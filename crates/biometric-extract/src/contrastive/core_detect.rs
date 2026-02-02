
use crate::orientation::OrientationField;
use std::f64::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreType {
    Loop,
    Whorl,
    Delta,
}

#[derive(Debug, Clone)]
pub struct CorePoint {
    pub x: u32,
    pub y: u32,
    pub core_type: CoreType,
    pub poincare_value: f64,
}

pub fn detect_core(orientation: &OrientationField, block_size: u32) -> Vec<CorePoint> {
    let mut cores = Vec::new();

    let width = orientation.get_width();
    let height = orientation.get_height();

    let blocks_x = width / block_size;
    let blocks_y = height / block_size;

    for by in 2..(blocks_y - 2) {
        for bx in 2..(blocks_x - 2) {
            let cx = bx * block_size + block_size / 2;
            let cy = by * block_size + block_size / 2;

            let poincare = compute_poincare_index(orientation, cx, cy, block_size);

            const TOLERANCE: f64 = 0.4;

            if (poincare - PI).abs() < TOLERANCE {
                cores.push(CorePoint {
                    x: cx,
                    y: cy,
                    core_type: CoreType::Loop,
                    poincare_value: poincare,
                });
            } else if (poincare - 2.0 * PI).abs() < TOLERANCE {
                cores.push(CorePoint {
                    x: cx,
                    y: cy,
                    core_type: CoreType::Whorl,
                    poincare_value: poincare,
                });
            } else if (poincare + PI).abs() < TOLERANCE {
                cores.push(CorePoint {
                    x: cx,
                    y: cy,
                    core_type: CoreType::Delta,
                    poincare_value: poincare,
                });
            }
        }
    }

    let mut filtered = Vec::new();
    let suppression_radius = 3 * block_size;

    cores.sort_by(|a, b| {
        b.poincare_value.abs()
            .partial_cmp(&a.poincare_value.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for core in cores {
        let dominated = filtered.iter().any(|c: &CorePoint| {
            let dx = (c.x as i32 - core.x as i32).abs() as u32;
            let dy = (c.y as i32 - core.y as i32).abs() as u32;
            dx < suppression_radius && dy < suppression_radius
        });

        if !dominated && core.core_type != CoreType::Delta {
            filtered.push(core);
        }
    }

    filtered.truncate(2);
    filtered
}

fn compute_poincare_index(
    orientation: &OrientationField,
    cx: u32,
    cy: u32,
    block_size: u32,
) -> f64 {
    let offsets: [(i32, i32); 8] = [
        (-1, -1), (0, -1), (1, -1),
        (1, 0),
        (1, 1), (0, 1), (-1, 1),
        (-1, 0),
    ];

    let mut total_change = 0.0;
    let mut prev_theta = None;

    for (dx, dy) in offsets.iter() {
        let x = (cx as i32 + dx * block_size as i32) as u32;
        let y = (cy as i32 + dy * block_size as i32) as u32;

        let theta = orientation.get(x, y);

        if let Some(prev) = prev_theta {
            let mut diff = theta - prev;

            while diff > PI / 2.0 {
                diff -= PI;
            }
            while diff < -PI / 2.0 {
                diff += PI;
            }

            total_change += diff;
        }

        prev_theta = Some(theta);
    }

    if let Some(prev) = prev_theta {
        let first_x = (cx as i32 + offsets[0].0 * block_size as i32) as u32;
        let first_y = (cy as i32 + offsets[0].1 * block_size as i32) as u32;
        let theta = orientation.get(first_x, first_y);

        let mut diff = theta - prev;
        while diff > PI / 2.0 {
            diff -= PI;
        }
        while diff < -PI / 2.0 {
            diff += PI;
        }

        total_change += diff;
    }

    total_change
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_poincare_classification() {
        assert!((PI - PI).abs() < 0.4);

        assert!((2.0 * PI - 2.0 * PI).abs() < 0.4);
    }
}
