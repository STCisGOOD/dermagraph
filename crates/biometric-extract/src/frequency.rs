
use crate::error::Result;
use crate::image_proc::EnhancedImage;
use crate::orientation::OrientationField;
use tracing::debug;

pub struct FrequencyImage {
    width: u32,
    height: u32,
    block_size: u32,
    frequencies: Vec<f64>,
}

impl FrequencyImage {
    pub fn compute(image: &EnhancedImage, orientation: &OrientationField) -> Result<Self> {
        let (width, height) = image.dimensions();
        let block_size = orientation.block_size();

        let blocks_x = width / block_size;
        let blocks_y = height / block_size;

        let mut frequencies = vec![0.0; (blocks_x * blocks_y) as usize];

        let window_size = 32;
        let half_window = window_size / 2;

        for by in 0..blocks_y {
            for bx in 0..blocks_x {
                let cx = bx * block_size + block_size / 2;
                let cy = by * block_size + block_size / 2;

                if cx < half_window || cy < half_window
                    || cx >= width - half_window
                    || cy >= height - half_window
                {
                    continue;
                }

                let theta = orientation.get(cx, cy);

                let mut projection = vec![0.0; window_size as usize];

                for i in 0..window_size {
                    let offset = i as f64 - half_window as f64;
                    let mut sum = 0.0;

                    for j in -(half_window as i32)..(half_window as i32) {
                        let x = cx as f64 + offset * (theta + std::f64::consts::FRAC_PI_2).cos()
                            + j as f64 * theta.cos();
                        let y = cy as f64 + offset * (theta + std::f64::consts::FRAC_PI_2).sin()
                            + j as f64 * theta.sin();

                        let x = x.round() as u32;
                        let y = y.round() as u32;

                        if x < width && y < height {
                            sum += image.get(x, y) as f64;
                        }
                    }

                    projection[i as usize] = sum;
                }

                let freq = estimate_frequency(&projection);
                frequencies[(by * blocks_x + bx) as usize] = freq;
            }
        }

        interpolate_frequencies(&mut frequencies, blocks_x as usize, blocks_y as usize);

        debug!("Computed frequency image");

        Ok(Self {
            width,
            height,
            block_size,
            frequencies,
        })
    }

    pub fn get(&self, x: u32, y: u32) -> f64 {
        let blocks_x = self.width / self.block_size;

        let bx = (x / self.block_size).min(blocks_x - 1);
        let by = (y / self.block_size).min(self.height / self.block_size - 1);

        self.frequencies[(by * blocks_x + bx) as usize]
    }
}

fn estimate_frequency(projection: &[f64]) -> f64 {
    if projection.is_empty() {
        return 0.0;
    }

    let mut peaks = Vec::new();

    for i in 1..(projection.len() - 1) {
        if projection[i] > projection[i - 1] && projection[i] > projection[i + 1] {
            peaks.push(i);
        }
    }

    if peaks.len() < 2 {
        return 0.0;
    }

    let mut total_dist = 0;
    for i in 1..peaks.len() {
        total_dist += peaks[i] - peaks[i - 1];
    }

    let avg_dist = total_dist as f64 / (peaks.len() - 1) as f64;

    if avg_dist > 0.0 {
        1.0 / avg_dist
    } else {
        0.0
    }
}

fn interpolate_frequencies(frequencies: &mut [f64], _width: usize, _height: usize) {
    let valid_freq: f64 = frequencies.iter().filter(|&&f| f > 0.01).sum::<f64>()
        / frequencies.iter().filter(|&&f| f > 0.01).count().max(1) as f64;

    let default_freq = if valid_freq > 0.01 { valid_freq } else { 0.1 };

    for freq in frequencies.iter_mut() {
        if *freq < 0.01 || *freq > 0.5 {
            *freq = default_freq;
        }
    }
}
