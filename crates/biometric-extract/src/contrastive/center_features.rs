
use crate::orientation::OrientationField;
use crate::frequency::FrequencyImage;
use super::core_detect::{CorePoint, CoreType};
use std::f64::consts::PI;

#[derive(Debug, Clone)]
pub struct CenterFeatures {
    pub orientation_hist: [f32; 8],

    pub curvature_stats: [f32; 3],

    pub frequency_stats: [f32; 2],

    pub core_type: [f32; 3],

    pub core_position: [f32; 2],

    pub radial_profile: [f32; 32],
}

impl CenterFeatures {
    pub const DIM: usize = 8 + 3 + 2 + 3 + 2 + 32;

    pub fn to_vector(&self) -> Vec<f32> {
        let mut v = Vec::with_capacity(Self::DIM);
        v.extend_from_slice(&self.orientation_hist);
        v.extend_from_slice(&self.curvature_stats);
        v.extend_from_slice(&self.frequency_stats);
        v.extend_from_slice(&self.core_type);
        v.extend_from_slice(&self.core_position);
        v.extend_from_slice(&self.radial_profile);
        v
    }

    pub fn zeros() -> Self {
        Self {
            orientation_hist: [0.0; 8],
            curvature_stats: [0.0; 3],
            frequency_stats: [0.0; 2],
            core_type: [0.0; 3],
            core_position: [0.5, 0.5],
            radial_profile: [0.0; 32],
        }
    }
}

pub fn extract_center_features(
    orientation: &OrientationField,
    frequency: Option<&FrequencyImage>,
    cores: &[CorePoint],
) -> CenterFeatures {
    if cores.is_empty() {
        return extract_at_position(
            orientation,
            frequency,
            orientation.get_width() / 2,
            orientation.get_height() / 2,
            CoreType::Loop,
        );
    }

    let core = &cores[0];
    extract_at_position(orientation, frequency, core.x, core.y, core.core_type)
}

fn extract_at_position(
    orientation: &OrientationField,
    frequency: Option<&FrequencyImage>,
    cx: u32,
    cy: u32,
    core_type: CoreType,
) -> CenterFeatures {
    let width = orientation.get_width();
    let height = orientation.get_height();
    let block_size = orientation.block_size();

    let mut orientation_hist = [0.0f32; 8];
    let radius = 3 * block_size;

    let mut count = 0;
    for dy in -(radius as i32)..(radius as i32) {
        for dx in -(radius as i32)..(radius as i32) {
            let x = cx as i32 + dx;
            let y = cy as i32 + dy;

            if x > 0 && y > 0 && (x as u32) < width && (y as u32) < height {
                let r2 = dx * dx + dy * dy;
                if r2 <= (radius * radius) as i32 {
                    let theta = orientation.get(x as u32, y as u32);
                    let bin = ((theta / PI) * 8.0).floor() as usize;
                    let bin = bin.min(7);
                    orientation_hist[bin] += 1.0;
                    count += 1;
                }
            }
        }
    }

    if count > 0 {
        for h in &mut orientation_hist {
            *h /= count as f32;
        }
    }

    let mut curvatures = Vec::new();
    for by in 1..(orientation.blocks_y() - 1) {
        for bx in 1..(orientation.blocks_x() - 1) {
            let x = bx * block_size + block_size / 2;
            let y = by * block_size + block_size / 2;

            let dx = (x as i32 - cx as i32).abs();
            let dy = (y as i32 - cy as i32).abs();
            if (dx * dx + dy * dy) <= (radius * radius) as i32 {
                let curv = compute_curvature(orientation, x, y, block_size);
                curvatures.push(curv);
            }
        }
    }

    let curvature_stats = if !curvatures.is_empty() {
        let mean: f32 = curvatures.iter().sum::<f32>() / curvatures.len() as f32;
        let variance: f32 = curvatures.iter().map(|c| (c - mean).powi(2)).sum::<f32>()
            / curvatures.len() as f32;
        let std = variance.sqrt();
        let max = curvatures.iter().cloned().fold(0.0f32, f32::max);
        [mean, std, max]
    } else {
        [0.0, 0.0, 0.0]
    };

    let frequency_stats = if let Some(freq) = frequency {
        let mut freqs = Vec::new();
        for by in 0..orientation.blocks_y() {
            for bx in 0..orientation.blocks_x() {
                let x = bx * block_size + block_size / 2;
                let y = by * block_size + block_size / 2;

                let dx = (x as i32 - cx as i32).abs();
                let dy = (y as i32 - cy as i32).abs();
                if (dx * dx + dy * dy) <= (radius * radius) as i32 {
                    freqs.push(freq.get(x, y) as f32);
                }
            }
        }

        if !freqs.is_empty() {
            let mean = freqs.iter().sum::<f32>() / freqs.len() as f32;
            let variance = freqs.iter().map(|f| (f - mean).powi(2)).sum::<f32>()
                / freqs.len() as f32;
            [mean, variance.sqrt()]
        } else {
            [0.1, 0.02]
        }
    } else {
        [0.1, 0.02]
    };

    let core_type_vec = match core_type {
        CoreType::Loop => [1.0, 0.0, 0.0],
        CoreType::Whorl => [0.0, 1.0, 0.0],
        CoreType::Delta => [0.0, 0.0, 1.0],
    };

    let core_position = [
        cx as f32 / width as f32,
        cy as f32 / height as f32,
    ];

    let radial_profile = compute_radial_profile(orientation, cx, cy, block_size);

    CenterFeatures {
        orientation_hist,
        curvature_stats,
        frequency_stats,
        core_type: core_type_vec,
        core_position,
        radial_profile,
    }
}

fn compute_curvature(orientation: &OrientationField, x: u32, y: u32, block_size: u32) -> f32 {
    let theta_c = orientation.get(x, y);

    let neighbors: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];
    let mut total_change = 0.0f64;

    for (dx, dy) in neighbors {
        let nx = (x as i32 + dx * block_size as i32) as u32;
        let ny = (y as i32 + dy * block_size as i32) as u32;

        let theta_n = orientation.get(nx, ny);

        let mut diff = theta_n - theta_c;
        while diff > PI / 2.0 {
            diff -= PI;
        }
        while diff < -PI / 2.0 {
            diff += PI;
        }

        total_change += diff.abs();
    }

    (total_change / 4.0) as f32
}

fn compute_radial_profile(
    orientation: &OrientationField,
    cx: u32,
    cy: u32,
    block_size: u32,
) -> [f32; 32] {
    let mut profile = [0.0f32; 32];

    let directions = 8;
    let radii = [1, 2, 3, 4];

    for (r_idx, &r) in radii.iter().enumerate() {
        for d in 0..directions {
            let angle = (d as f64 / directions as f64) * 2.0 * PI;
            let dx = (r as f64 * angle.cos() * block_size as f64) as i32;
            let dy = (r as f64 * angle.sin() * block_size as f64) as i32;

            let x = (cx as i32 + dx) as u32;
            let y = (cy as i32 + dy) as u32;

            let theta = orientation.get(x, y);
            let idx = r_idx * 8 + d;

            profile[idx] = (2.0 * theta).sin() as f32;
        }
    }

    profile
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_dimension() {
        let features = CenterFeatures::zeros();
        assert_eq!(features.to_vector().len(), CenterFeatures::DIM);
    }

    #[test]
    fn test_histogram_normalization() {
        let mut hist = [1.0f32; 8];
        let sum: f32 = hist.iter().sum();
        for h in &mut hist {
            *h /= sum;
        }
        let normalized_sum: f32 = hist.iter().sum();
        assert!((normalized_sum - 1.0).abs() < 1e-5);
    }
}
