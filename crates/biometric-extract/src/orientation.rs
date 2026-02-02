
use crate::error::Result;
use crate::image_proc::NormalizedImage;
use tracing::debug;

pub struct OrientationField {
    width: u32,
    height: u32,
    block_size: u32,
    orientations: Vec<f64>,
}

impl OrientationField {
    pub fn compute(image: &NormalizedImage) -> Result<Self> {
        let (width, height) = image.dimensions();
        let block_size = 16u32;

        let blocks_x = width / block_size;
        let blocks_y = height / block_size;

        let mut orientations = vec![0.0; (blocks_x * blocks_y) as usize];

        let sobel_x: [[i32; 3]; 3] = [[-1, 0, 1], [-2, 0, 2], [-1, 0, 1]];
        let sobel_y: [[i32; 3]; 3] = [[-1, -2, -1], [0, 0, 0], [1, 2, 1]];

        for by in 0..blocks_y {
            for bx in 0..blocks_x {
                let mut vx = 0.0;
                let mut vy = 0.0;

                for dy in 1..(block_size - 1) {
                    for dx in 1..(block_size - 1) {
                        let x = bx * block_size + dx;
                        let y = by * block_size + dy;

                        let mut gx = 0i32;
                        let mut gy = 0i32;

                        for ky in 0..3 {
                            for kx in 0..3 {
                                let px = (x as i32 + kx as i32 - 1) as u32;
                                let py = (y as i32 + ky as i32 - 1) as u32;
                                let pixel = image.get(px, py) as i32;

                                gx += pixel * sobel_x[ky][kx];
                                gy += pixel * sobel_y[ky][kx];
                            }
                        }

                        let gx = gx as f64;
                        let gy = gy as f64;
                        vx += 2.0 * gx * gy;
                        vy += gx * gx - gy * gy;
                    }
                }

                let theta = 0.5 * vx.atan2(vy);
                let theta = theta.rem_euclid(std::f64::consts::PI);

                orientations[(by * blocks_x + bx) as usize] = theta;
            }
        }

        let smoothed = smooth_orientations(&orientations, blocks_x as usize, blocks_y as usize);

        debug!("Computed orientation field: {}x{} blocks", blocks_x, blocks_y);

        Ok(Self {
            width,
            height,
            block_size,
            orientations: smoothed,
        })
    }

    pub fn get(&self, x: u32, y: u32) -> f64 {
        let blocks_x = self.width / self.block_size;

        let bx = (x / self.block_size).min(blocks_x - 1);
        let by = (y / self.block_size).min(self.height / self.block_size - 1);

        self.orientations[(by * blocks_x + bx) as usize]
    }

    pub fn block_size(&self) -> u32 {
        self.block_size
    }

    pub fn get_width(&self) -> u32 {
        self.width
    }

    pub fn get_height(&self) -> u32 {
        self.height
    }

    pub fn blocks_x(&self) -> u32 {
        self.width / self.block_size
    }

    pub fn blocks_y(&self) -> u32 {
        self.height / self.block_size
    }

    pub fn as_vector(&self) -> &[f64] {
        &self.orientations
    }

    pub fn to_sin_cos(&self) -> (Vec<f32>, Vec<f32>) {
        let cos_vals: Vec<f32> = self.orientations
            .iter()
            .map(|&t| (2.0 * t).cos() as f32)
            .collect();
        let sin_vals: Vec<f32> = self.orientations
            .iter()
            .map(|&t| (2.0 * t).sin() as f32)
            .collect();
        (cos_vals, sin_vals)
    }
}

fn smooth_orientations(orientations: &[f64], width: usize, height: usize) -> Vec<f64> {
    let phi_x: Vec<f64> = orientations.iter().map(|&t| (2.0 * t).cos()).collect();
    let phi_y: Vec<f64> = orientations.iter().map(|&t| (2.0 * t).sin()).collect();

    let kernel_size = 3;
    let half = kernel_size / 2;

    let mut smoothed = vec![0.0; width * height];

    for y in 0..height {
        for x in 0..width {
            let mut sum_x = 0.0;
            let mut sum_y = 0.0;
            let mut count = 0;

            for ky in 0..kernel_size {
                for kx in 0..kernel_size {
                    let nx = x as i32 + kx as i32 - half as i32;
                    let ny = y as i32 + ky as i32 - half as i32;

                    if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
                        let idx = ny as usize * width + nx as usize;
                        sum_x += phi_x[idx];
                        sum_y += phi_y[idx];
                        count += 1;
                    }
                }
            }

            let avg_x = sum_x / count as f64;
            let avg_y = sum_y / count as f64;

            smoothed[y * width + x] = 0.5 * avg_y.atan2(avg_x);
        }
    }

    smoothed
}
