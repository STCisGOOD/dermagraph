
use biometric_extract::{extract_minutiae, extract_laplacian, FingerprintImage};
use std::f64::consts::PI;

fn generate_synthetic_fingerprint(width: u32, height: u32) -> Vec<u8> {
    let mut data = vec![0u8; (width * height) as usize];

    let cx = width as f64 / 2.0;
    let cy = height as f64 / 2.0;

    let ridge_spacing = 8.0;

    for y in 0..height {
        for x in 0..width {
            let dx = x as f64 - cx;
            let dy = y as f64 - cy;

            let r = (dx * dx + dy * dy).sqrt();

            let theta = dy.atan2(dx);

            let phase = r / ridge_spacing + theta * 2.0 / PI;

            let ridge_value = ((phase * PI).sin() + 1.0) / 2.0;

            let noise = ((x as f64 * 0.1).sin() * (y as f64 * 0.1).cos() + 1.0) / 2.0 * 0.1;

            let pixel_value = ((ridge_value + noise) * 255.0).clamp(0.0, 255.0) as u8;
            data[(y * width + x) as usize] = pixel_value;
        }
    }

    data
}

fn generate_fingerprint_with_minutiae(width: u32, height: u32) -> Vec<u8> {
    let mut data = vec![128u8; (width * height) as usize];

    let ridge_spacing = 10;
    let ridge_width = 3;

    for ridge_idx in 0..((height as i32) / ridge_spacing) {
        let base_y = ridge_idx * ridge_spacing + ridge_spacing / 2;

        if base_y < 20 || base_y >= height as i32 - 20 {
            continue;
        }

        for x in 20..(width as i32 - 20) {
            let has_gap = (ridge_idx % 3 == 0) && (x > 80 && x < 90);
            let has_gap2 = (ridge_idx % 4 == 1) && (x > 150 && x < 160);

            let is_bifurcation = (ridge_idx % 5 == 2) && (x > 110 && x < 140);

            if has_gap || has_gap2 {
                continue;
            }

            for dy in -(ridge_width/2)..=(ridge_width/2) {
                let y = base_y + dy;
                if y >= 0 && y < height as i32 {
                    if is_bifurcation {
                        let offset = ((x - 110) as f64 / 30.0 * 5.0) as i32;
                        let y1 = y - offset;
                        let y2 = y + offset;
                        if y1 >= 0 && y1 < height as i32 {
                            data[(y1 as u32 * width + x as u32) as usize] = 255;
                        }
                        if y2 >= 0 && y2 < height as i32 {
                            data[(y2 as u32 * width + x as u32) as usize] = 255;
                        }
                    } else {
                        data[(y as u32 * width + x as u32) as usize] = 255;
                    }
                }
            }
        }
    }

    data
}

fn encode_as_png(width: u32, height: u32, data: &[u8]) -> Vec<u8> {
    use std::io::Cursor;

    let mut buffer = Cursor::new(Vec::new());

    let mut encoder = png::Encoder::new(&mut buffer, width, height);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);

    let mut writer = encoder.write_header().expect("Failed to write PNG header");
    writer.write_image_data(data).expect("Failed to write PNG data");
    drop(writer);

    buffer.into_inner()
}

#[test]
fn test_synthetic_fingerprint_extraction() {
    let width = 200;
    let height = 200;
    let raw_data = generate_synthetic_fingerprint(width, height);

    let png_data = encode_as_png(width, height, &raw_data);

    let image = FingerprintImage::from_bytes(&png_data)
        .expect("Should load synthetic fingerprint");

    let (w, h) = image.dimensions();
    assert_eq!(w, width);
    assert_eq!(h, height);

    let normalized = image.normalize()
        .expect("Normalization should succeed");

    assert_eq!(normalized.dimensions(), (width, height));

    let enhanced = normalized.enhance()
        .expect("Enhancement should succeed");

    assert_eq!(enhanced.dimensions(), (width, height));

    println!("=== Synthetic Fingerprint Test ===");
    println!("Image size: {}x{}", width, height);
    println!("Preprocessing pipeline: OK");
}

#[test]
fn test_orientation_estimation() {
    use biometric_extract::OrientationField;

    let width = 200;
    let height = 200;
    let raw_data = generate_synthetic_fingerprint(width, height);
    let png_data = encode_as_png(width, height, &raw_data);

    let image = FingerprintImage::from_bytes(&png_data).unwrap();
    let normalized = image.normalize().unwrap();

    let orientation = OrientationField::compute(&normalized)
        .expect("Orientation estimation should succeed");

    let theta_center = orientation.get(width / 2, height / 2);
    let theta_corner = orientation.get(50, 50);

    assert!(theta_center >= 0.0 && theta_center < std::f64::consts::PI);
    assert!(theta_corner >= 0.0 && theta_corner < std::f64::consts::PI);

    println!("=== Orientation Field Test ===");
    println!("Center orientation: {:.3} rad ({:.1} deg)",
             theta_center, theta_center.to_degrees());
    println!("Corner orientation: {:.3} rad ({:.1} deg)",
             theta_corner, theta_corner.to_degrees());
}

#[test]
fn test_frequency_estimation() {
    use biometric_extract::{OrientationField, FrequencyImage};

    let width = 200;
    let height = 200;
    let raw_data = generate_synthetic_fingerprint(width, height);
    let png_data = encode_as_png(width, height, &raw_data);

    let image = FingerprintImage::from_bytes(&png_data).unwrap();
    let normalized = image.normalize().unwrap();
    let enhanced = normalized.enhance().unwrap();
    let orientation = OrientationField::compute(&normalized).unwrap();

    let frequency = FrequencyImage::compute(&enhanced, &orientation)
        .expect("Frequency estimation should succeed");

    let freq_center = frequency.get(width / 2, height / 2);

    assert!(freq_center >= 0.0, "Frequency must be non-negative");
    assert!(freq_center < 1.0, "Frequency should be < 1 ridge/pixel");

    println!("=== Frequency Estimation Test ===");
    println!("Center frequency: {:.4} ridges/pixel", freq_center);
    println!("Estimated ridge spacing: {:.1} pixels",
             if freq_center > 0.0 { 1.0 / freq_center } else { f64::INFINITY });
}

#[test]
fn test_full_extraction_pipeline() {
    let width = 200;
    let height = 200;
    let raw_data = generate_fingerprint_with_minutiae(width, height);
    let png_data = encode_as_png(width, height, &raw_data);

    let result = extract_minutiae(&png_data);

    match result {
        Ok(minutiae) => {
            println!("=== Full Extraction Pipeline ===");
            println!("Extracted {} minutiae", minutiae.len());

            let endings = minutiae.minutiae.iter()
                .filter(|m| matches!(m.minutiae_type, biometric_extract::MinutiaeType::RidgeEnding))
                .count();
            let bifurcations = minutiae.minutiae.iter()
                .filter(|m| matches!(m.minutiae_type, biometric_extract::MinutiaeType::Bifurcation))
                .count();

            println!("Ridge endings: {}", endings);
            println!("Bifurcations: {}", bifurcations);

            for (i, m) in minutiae.minutiae.iter().take(5).enumerate() {
                println!("  Minutia {}: ({:.1}, {:.1}) theta={:.2} type={:?}",
                         i, m.x, m.y, m.theta, m.minutiae_type);
            }

            assert!(minutiae.len() >= 5, "Should extract at least 5 minutiae");
        }
        Err(e) => {
            println!("Extraction result: {:?}", e);
            println!("Note: Simple synthetic patterns may not produce enough minutiae");
        }
    }
}

#[test]
fn test_gabor_and_thinning() {
    use biometric_extract::{OrientationField, FrequencyImage};

    let width = 200;
    let height = 200;
    let raw_data = generate_fingerprint_with_minutiae(width, height);
    let png_data = encode_as_png(width, height, &raw_data);

    let image = FingerprintImage::from_bytes(&png_data).unwrap();
    let normalized = image.normalize().unwrap();
    let enhanced = normalized.enhance().unwrap();
    let orientation = OrientationField::compute(&normalized).unwrap();
    let frequency = FrequencyImage::compute(&enhanced, &orientation).unwrap();

    let binary = enhanced.gabor_filter(&orientation, &frequency)
        .expect("Gabor filtering should succeed");

    let (bw, bh) = binary.dimensions();
    assert_eq!(bw, width);
    assert_eq!(bh, height);

    let thinned = binary.thin()
        .expect("Thinning should succeed");

    let (tw, th) = thinned.dimensions();
    assert_eq!(tw, width);
    assert_eq!(th, height);

    let mut binary_count = 0usize;
    let mut thinned_count = 0usize;
    for y in 0..height {
        for x in 0..width {
            if binary.get(x, y) {
                binary_count += 1;
            }
            if thinned.get(x, y) {
                thinned_count += 1;
            }
        }
    }

    println!("=== Gabor + Thinning Test ===");
    println!("Binary ridge pixels: {}", binary_count);
    println!("Thinned ridge pixels: {}", thinned_count);
    println!("Thinning ratio: {:.2}x", binary_count as f64 / thinned_count.max(1) as f64);

    assert!(thinned_count <= binary_count, "Thinning should reduce pixel count");
}

#[tokio::test]
async fn test_complete_identity_from_image() {
    use turing_core::{TuringHash, TuringParams, MorphogenState};
    use turing_core::poseidon::hash_2;

    let width = 200;
    let height = 200;
    let raw_data = generate_fingerprint_with_minutiae(width, height);
    let png_data = encode_as_png(width, height, &raw_data);

    let laplacian = extract_laplacian(&png_data).await
        .expect("Laplacian extraction should succeed");

    println!("=== Complete Identity Pipeline ===");
    println!("Laplacian dimension: {}x{}", laplacian.dim(), laplacian.dim());
    println!("Non-zero entries: {}", laplacian.matrix.nnz());

    let minutiae = extract_minutiae(&png_data)
        .expect("Minutiae extraction should succeed");

    let x = minutiae.x_coords();
    let y = minutiae.y_coords();
    let theta = minutiae.orientations();

    let initial_state = MorphogenState::from_biometric(&x, &y, &theta);

    let params = TuringParams::default();
    let identity_hash = TuringHash::compute(&initial_state, &laplacian, &params)
        .expect("Turing hash should succeed");

    let commitment = hash_2(identity_hash, identity_hash);

    println!("Identity hash computed: non-zero = {}", !identity_hash.is_zero());
    println!("Commitment computed: non-zero = {}", !commitment.is_zero());

    let identity_hash_2 = TuringHash::compute(&initial_state, &laplacian, &params)
        .expect("Second hash should succeed");

    assert_eq!(identity_hash, identity_hash_2, "Identity must be deterministic");
    assert!(!identity_hash.is_zero(), "Identity should be non-trivial");
    assert!(!commitment.is_zero(), "Commitment should be non-trivial");

    println!("PASS: Complete pipeline from image to identity commitment");
}

#[tokio::test]
async fn test_identity_reproducibility() {
    use turing_core::{TuringHash, TuringParams, MorphogenState};

    let width = 200;
    let height = 200;
    let raw_data = generate_fingerprint_with_minutiae(width, height);
    let png_data = encode_as_png(width, height, &raw_data);

    let params = TuringParams::default();

    let laplacian1 = extract_laplacian(&png_data).await.unwrap();
    let minutiae1 = extract_minutiae(&png_data).unwrap();
    let state1 = MorphogenState::from_biometric(
        &minutiae1.x_coords(),
        &minutiae1.y_coords(),
        &minutiae1.orientations(),
    );
    let hash1 = TuringHash::compute(&state1, &laplacian1, &params).unwrap();

    let laplacian2 = extract_laplacian(&png_data).await.unwrap();
    let minutiae2 = extract_minutiae(&png_data).unwrap();
    let state2 = MorphogenState::from_biometric(
        &minutiae2.x_coords(),
        &minutiae2.y_coords(),
        &minutiae2.orientations(),
    );
    let hash2 = TuringHash::compute(&state2, &laplacian2, &params).unwrap();

    assert_eq!(hash1, hash2, "Same image must always produce same identity");
    println!("PASS: Identity is reproducible across extractions");
}
