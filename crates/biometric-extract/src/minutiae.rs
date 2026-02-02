
use crate::error::{ExtractError, Result};
use crate::image_proc::BinaryImage;
use serde::{Deserialize, Serialize};
use tracing::debug;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MinutiaeType {
    RidgeEnding,
    Bifurcation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Minutia {
    pub x: f64,
    pub y: f64,
    pub theta: f64,
    pub minutiae_type: MinutiaeType,
    pub quality: f64,
}

impl Minutia {
    pub fn new(x: f64, y: f64, theta: f64, minutiae_type: MinutiaeType) -> Self {
        Self {
            x,
            y,
            theta: theta.rem_euclid(std::f64::consts::TAU),
            minutiae_type,
            quality: 1.0,
        }
    }

    pub fn distance_to(&self, other: &Minutia) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }

    pub fn angle_diff(&self, other: &Minutia) -> f64 {
        let diff = (self.theta - other.theta).abs();
        diff.min(std::f64::consts::TAU - diff)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinutiaeSet {
    pub minutiae: Vec<Minutia>,
    pub image_width: u32,
    pub image_height: u32,
}

impl MinutiaeSet {
    pub fn extract(image: &BinaryImage) -> Result<Self> {
        let mut minutiae = Vec::new();

        let (width, height) = image.dimensions();

        for y in 1..(height - 1) {
            for x in 1..(width - 1) {
                if !image.get(x, y) {
                    continue;
                }

                let neighbors = [
                    image.get(x + 1, y),
                    image.get(x + 1, y - 1),
                    image.get(x, y - 1),
                    image.get(x - 1, y - 1),
                    image.get(x - 1, y),
                    image.get(x - 1, y + 1),
                    image.get(x, y + 1),
                    image.get(x + 1, y + 1),
                ];

                let cn: u32 = (0..8)
                    .map(|i| {
                        let curr = neighbors[i] as u32;
                        let next = neighbors[(i + 1) % 8] as u32;
                        (curr as i32 - next as i32).unsigned_abs()
                    })
                    .sum::<u32>()
                    / 2;

                let minutiae_type = match cn {
                    1 => Some(MinutiaeType::RidgeEnding),
                    3 => Some(MinutiaeType::Bifurcation),
                    _ => None,
                };

                if let Some(mtype) = minutiae_type {
                    let theta = compute_local_orientation(&neighbors);

                    minutiae.push(Minutia::new(
                        x as f64,
                        y as f64,
                        theta,
                        mtype,
                    ));
                }
            }
        }

        debug!("Raw minutiae count: {}", minutiae.len());

        let filtered = filter_minutiae(minutiae, 10.0);

        debug!("Filtered minutiae count: {}", filtered.len());

        if filtered.len() < 5 {
            return Err(ExtractError::InsufficientMinutiae {
                found: filtered.len(),
                minimum: 5,
            });
        }

        Ok(Self {
            minutiae: filtered,
            image_width: width,
            image_height: height,
        })
    }

    pub fn from_coords(x: &[f64], y: &[f64], theta: &[f64]) -> Self {
        let n = x.len().min(y.len()).min(theta.len());
        let minutiae = (0..n)
            .map(|i| Minutia::new(
                x[i],
                y[i],
                theta[i],
                if i % 2 == 0 { MinutiaeType::RidgeEnding } else { MinutiaeType::Bifurcation },
            ))
            .collect();

        Self {
            minutiae,
            image_width: 300,
            image_height: 300,
        }
    }

    pub fn mock() -> Self {
        let minutiae = vec![
            Minutia::new(100.0, 100.0, 0.0, MinutiaeType::RidgeEnding),
            Minutia::new(150.0, 120.0, 0.5, MinutiaeType::Bifurcation),
            Minutia::new(200.0, 180.0, 1.0, MinutiaeType::RidgeEnding),
            Minutia::new(250.0, 160.0, 1.5, MinutiaeType::Bifurcation),
            Minutia::new(180.0, 220.0, 2.0, MinutiaeType::RidgeEnding),
            Minutia::new(120.0, 200.0, 2.5, MinutiaeType::Bifurcation),
            Minutia::new(160.0, 150.0, 3.0, MinutiaeType::RidgeEnding),
            Minutia::new(220.0, 140.0, 3.5, MinutiaeType::Bifurcation),
        ];

        Self {
            minutiae,
            image_width: 300,
            image_height: 300,
        }
    }

    pub fn len(&self) -> usize {
        self.minutiae.len()
    }

    pub fn is_empty(&self) -> bool {
        self.minutiae.is_empty()
    }

    pub fn x_coords(&self) -> Vec<f64> {
        self.minutiae.iter().map(|m| m.x).collect()
    }

    pub fn y_coords(&self) -> Vec<f64> {
        self.minutiae.iter().map(|m| m.y).collect()
    }

    pub fn orientations(&self) -> Vec<f64> {
        self.minutiae.iter().map(|m| m.theta).collect()
    }

    pub fn normalized(&self) -> Self {
        let minutiae = self
            .minutiae
            .iter()
            .map(|m| Minutia {
                x: m.x / self.image_width as f64,
                y: m.y / self.image_height as f64,
                theta: m.theta,
                minutiae_type: m.minutiae_type,
                quality: m.quality,
            })
            .collect();

        Self {
            minutiae,
            image_width: 1,
            image_height: 1,
        }
    }
}

fn compute_local_orientation(neighbors: &[bool; 8]) -> f64 {
    let angles = [
        0.0,
        std::f64::consts::FRAC_PI_4,
        std::f64::consts::FRAC_PI_2,
        3.0 * std::f64::consts::FRAC_PI_4,
        std::f64::consts::PI,
        5.0 * std::f64::consts::FRAC_PI_4,
        3.0 * std::f64::consts::FRAC_PI_2,
        7.0 * std::f64::consts::FRAC_PI_4,
    ];

    let mut sum_sin = 0.0;
    let mut sum_cos = 0.0;

    for (i, &is_ridge) in neighbors.iter().enumerate() {
        if is_ridge {
            sum_sin += angles[i].sin();
            sum_cos += angles[i].cos();
        }
    }

    sum_sin.atan2(sum_cos).rem_euclid(std::f64::consts::TAU)
}

fn filter_minutiae(mut minutiae: Vec<Minutia>, min_distance: f64) -> Vec<Minutia> {
    minutiae.sort_by(|a, b| b.quality.partial_cmp(&a.quality).unwrap());

    let mut result = Vec::new();

    for m in minutiae {
        let too_close = result.iter().any(|existing: &Minutia| {
            m.distance_to(existing) < min_distance
        });

        if !too_close {
            result.push(m);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_minutiae() {
        let set = MinutiaeSet::mock();
        assert_eq!(set.len(), 8);
    }

    #[test]
    fn test_distance() {
        let m1 = Minutia::new(0.0, 0.0, 0.0, MinutiaeType::RidgeEnding);
        let m2 = Minutia::new(3.0, 4.0, 0.0, MinutiaeType::RidgeEnding);
        assert!((m1.distance_to(&m2) - 5.0).abs() < 0.001);
    }
}
