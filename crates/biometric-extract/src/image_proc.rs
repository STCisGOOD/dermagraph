
use crate::error::{ExtractError, Result};
use crate::orientation::OrientationField;
use crate::frequency::FrequencyImage;
use image::GrayImage;
use tracing::debug;

#[derive(Clone)]
pub struct FingerprintImage {
    image: GrayImage,
}

impl FingerprintImage {
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        let img = image::load_from_memory(data)
            .map_err(|e| ExtractError::ImageLoadError(e.to_string()))?
            .into_luma8();

        let (width, height) = img.dimensions();
        debug!("Loaded {}x{} fingerprint image", width, height);

        if width < 100 || height < 100 {
            return Err(ExtractError::ImageTooSmall {
                width,
                height,
                min_size: 100,
            });
        }

        Ok(Self { image: img })
    }

    pub fn from_raw(width: u32, height: u32, data: Vec<u8>) -> Result<Self> {
        let img = GrayImage::from_raw(width, height, data)
            .ok_or_else(|| ExtractError::ImageLoadError("Invalid raw image data".into()))?;

        Ok(Self { image: img })
    }

    pub fn dimensions(&self) -> (u32, u32) {
        self.image.dimensions()
    }

    pub fn normalize(&self) -> Result<NormalizedImage> {
        let (width, height) = self.dimensions();
        let pixels: Vec<f64> = self.image.pixels().map(|p| p.0[0] as f64).collect();

        let mean: f64 = pixels.iter().sum::<f64>() / pixels.len() as f64;

        let variance: f64 = pixels.iter().map(|p| (p - mean).powi(2)).sum::<f64>()
            / pixels.len() as f64;
        let std_dev = variance.sqrt();

        let target_mean = 100.0;
        let target_var = 100.0;

        let normalized: Vec<u8> = pixels
            .iter()
            .map(|&p| {
                let n = if p > mean {
                    target_mean + ((target_var * (p - mean).powi(2) / variance).sqrt())
                } else {
                    target_mean - ((target_var * (p - mean).powi(2) / variance).sqrt())
                };
                n.clamp(0.0, 255.0) as u8
            })
            .collect();

        let image = GrayImage::from_raw(width, height, normalized)
            .ok_or_else(|| ExtractError::ProcessingError("Normalization failed".into()))?;

        debug!("Normalized image: mean={:.1}, std={:.1}", mean, std_dev);

        Ok(NormalizedImage { image })
    }
}

#[derive(Clone)]
pub struct NormalizedImage {
    image: GrayImage,
}

impl NormalizedImage {
    pub fn dimensions(&self) -> (u32, u32) {
        self.image.dimensions()
    }

    pub fn get(&self, x: u32, y: u32) -> u8 {
        self.image.get_pixel(x, y).0[0]
    }

    pub fn enhance(&self) -> Result<EnhancedImage> {
        let (width, height) = self.dimensions();

        let mut histogram = [0u32; 256];
        for p in self.image.pixels() {
            histogram[p.0[0] as usize] += 1;
        }

        let total = (width * height) as f64;
        let mut cdf = [0.0f64; 256];
        let mut cumulative = 0u32;
        for i in 0..256 {
            cumulative += histogram[i];
            cdf[i] = cumulative as f64 / total;
        }

        let equalized: Vec<u8> = self
            .image
            .pixels()
            .map(|p| (cdf[p.0[0] as usize] * 255.0) as u8)
            .collect();

        let image = GrayImage::from_raw(width, height, equalized)
            .ok_or_else(|| ExtractError::ProcessingError("Enhancement failed".into()))?;

        debug!("Enhanced image via histogram equalization");

        Ok(EnhancedImage { image })
    }

    pub fn as_image(&self) -> &GrayImage {
        &self.image
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.image.as_raw()
    }
}

#[derive(Clone)]
pub struct EnhancedImage {
    image: GrayImage,
}

impl EnhancedImage {
    pub fn dimensions(&self) -> (u32, u32) {
        self.image.dimensions()
    }

    pub fn get(&self, x: u32, y: u32) -> u8 {
        self.image.get_pixel(x, y).0[0]
    }

    pub fn as_image(&self) -> &GrayImage {
        &self.image
    }

    pub fn gabor_filter(
        &self,
        orientation: &OrientationField,
        frequency: &FrequencyImage,
    ) -> Result<BinaryImage> {
        let (width, height) = self.dimensions();
        let mut result = vec![0u8; (width * height) as usize];

        let ksize = 16;
        let half = ksize / 2;

        for y in half..(height - half) {
            for x in half..(width - half) {
                let theta = orientation.get(x, y);
                let freq = frequency.get(x, y);

                if freq < 0.01 {
                    continue;
                }

                let mut sum = 0.0;
                for ky in 0..ksize {
                    for kx in 0..ksize {
                        let dx = (kx as i32 - half as i32) as f64;
                        let dy = (ky as i32 - half as i32) as f64;

                        let x_theta = dx * theta.cos() + dy * theta.sin();
                        let y_theta = -dx * theta.sin() + dy * theta.cos();

                        let sigma_x: f64 = 4.0;
                        let sigma_y: f64 = 4.0;
                        let gaussian = (-0.5 * (x_theta.powi(2) / sigma_x.powi(2)
                            + y_theta.powi(2) / sigma_y.powi(2)))
                        .exp();
                        let sinusoid = (2.0 * std::f64::consts::PI * freq * x_theta).cos();
                        let kernel = gaussian * sinusoid;

                        let px = (x as i32 + kx as i32 - half as i32) as u32;
                        let py = (y as i32 + ky as i32 - half as i32) as u32;
                        let pixel = self.get(px, py) as f64;

                        sum += pixel * kernel;
                    }
                }

                let idx = (y * width + x) as usize;
                result[idx] = if sum > 0.0 { 255 } else { 0 };
            }
        }

        debug!("Applied Gabor filter");

        Ok(BinaryImage::from_raw(width, height, result))
    }
}

#[derive(Clone)]
pub struct BinaryImage {
    width: u32,
    height: u32,
    data: Vec<bool>,
}

impl BinaryImage {
    pub fn from_raw(width: u32, height: u32, data: Vec<u8>) -> Self {
        let data: Vec<bool> = data.iter().map(|&v| v > 127).collect();
        Self { width, height, data }
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn get(&self, x: u32, y: u32) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        self.data[(y * self.width + x) as usize]
    }

    pub fn set(&mut self, x: u32, y: u32, value: bool) {
        if x < self.width && y < self.height {
            self.data[(y * self.width + x) as usize] = value;
        }
    }

    pub fn thin(&self) -> Result<Self> {
        let mut current = self.clone();
        let mut changed = true;

        while changed {
            changed = false;

            let mut to_remove = Vec::new();
            for y in 1..(self.height - 1) {
                for x in 1..(self.width - 1) {
                    if current.get(x, y) && should_remove_1(&current, x, y) {
                        to_remove.push((x, y));
                    }
                }
            }
            for (x, y) in &to_remove {
                current.set(*x, *y, false);
                changed = true;
            }

            let mut to_remove = Vec::new();
            for y in 1..(self.height - 1) {
                for x in 1..(self.width - 1) {
                    if current.get(x, y) && should_remove_2(&current, x, y) {
                        to_remove.push((x, y));
                    }
                }
            }
            for (x, y) in &to_remove {
                current.set(*x, *y, false);
                changed = true;
            }
        }

        debug!("Applied thinning");

        Ok(current)
    }
}

fn should_remove_1(img: &BinaryImage, x: u32, y: u32) -> bool {
    let p2 = img.get(x, y - 1) as u8;
    let p3 = img.get(x + 1, y - 1) as u8;
    let p4 = img.get(x + 1, y) as u8;
    let p5 = img.get(x + 1, y + 1) as u8;
    let p6 = img.get(x, y + 1) as u8;
    let p7 = img.get(x - 1, y + 1) as u8;
    let p8 = img.get(x - 1, y) as u8;
    let p9 = img.get(x - 1, y - 1) as u8;

    let b = p2 + p3 + p4 + p5 + p6 + p7 + p8 + p9;
    if b < 2 || b > 6 {
        return false;
    }

    let neighbors = [p2, p3, p4, p5, p6, p7, p8, p9, p2];
    let a: u8 = neighbors.windows(2).map(|w| if w[0] == 0 && w[1] == 1 { 1 } else { 0 }).sum();
    if a != 1 {
        return false;
    }

    p2 * p4 * p6 == 0 && p4 * p6 * p8 == 0
}

fn should_remove_2(img: &BinaryImage, x: u32, y: u32) -> bool {
    let p2 = img.get(x, y - 1) as u8;
    let p3 = img.get(x + 1, y - 1) as u8;
    let p4 = img.get(x + 1, y) as u8;
    let p5 = img.get(x + 1, y + 1) as u8;
    let p6 = img.get(x, y + 1) as u8;
    let p7 = img.get(x - 1, y + 1) as u8;
    let p8 = img.get(x - 1, y) as u8;
    let p9 = img.get(x - 1, y - 1) as u8;

    let b = p2 + p3 + p4 + p5 + p6 + p7 + p8 + p9;
    if b < 2 || b > 6 {
        return false;
    }

    let neighbors = [p2, p3, p4, p5, p6, p7, p8, p9, p2];
    let a: u8 = neighbors.windows(2).map(|w| if w[0] == 0 && w[1] == 1 { 1 } else { 0 }).sum();
    if a != 1 {
        return false;
    }

    p2 * p4 * p8 == 0 && p2 * p6 * p8 == 0
}
