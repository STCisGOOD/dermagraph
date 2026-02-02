
use anyhow::{Context, Result};
use hardware::{Sensor, SensorConfig};
use std::path::Path;
use tokio::sync::mpsc;
use tracing::{info, warn};

use biometric_extract::contrastive::{
    FuzzyNullifier, XLockConfig, MultiFingerHelperData, FingerType, PersonEmbedding,
};

use crate::model::EmbedderModel;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "event", content = "data")]
pub enum EnrollmentProgress {
    #[serde(rename = "ready")]
    Ready { finger: String },
    #[serde(rename = "captured")]
    Captured { finger: String, quality: u8 },
    #[serde(rename = "lift")]
    Lift { finger: String },
    #[serde(rename = "processing")]
    Processing { step: String, percent: u8 },
    #[serde(rename = "complete")]
    Complete { nullifier: String, similarity: f32 },
    #[serde(rename = "error")]
    Error { message: String },
}

#[derive(Debug, Clone)]
pub struct XLockEnrollmentResult {
    pub nullifier: [u8; 32],
    pub helper_data: MultiFingerHelperData,
    pub intra_similarity: f32,
    pub representative_embedding: PersonEmbedding,
    pub embedding_key: [u8; 32],
}

#[derive(Debug, Clone)]
pub struct XLockVerifyResult {
    pub nullifier: [u8; 32],
    pub embedding_key: [u8; 32],
    pub matched_finger: FingerType,
}

struct CapturedImage {
    data: Vec<u8>,
    width: u32,
    height: u32,
    quality: u8,
}

async fn capture_fingerprint(
    sensor: &mut Sensor,
    finger_name: &str,
) -> Result<CapturedImage> {
    info!("Waiting for {} finger...", finger_name);

    let image = sensor.capture().await
        .with_context(|| format!("Failed to capture {} finger", finger_name))?;

    info!("{} captured (quality: {})", finger_name, image.quality);

    if image.quality < 50 {
        warn!("{} quality is low ({}). Consider rescanning.", finger_name, image.quality);
    }

    Ok(CapturedImage {
        data: image.data,
        width: image.width as u32,
        height: image.height as u32,
        quality: image.quality,
    })
}

async fn capture_fingerprint_no_wait(
    sensor: &mut Sensor,
    finger_name: &str,
) -> Result<CapturedImage> {
    info!("Waiting for {} finger...", finger_name);

    let _ = sensor.set_led_waiting().await;

    loop {
        match sensor.capture_no_wait().await {
            Ok(image) => {
                let _ = sensor.set_led_success().await;

                info!("{} captured (quality: {})", finger_name, image.quality);

                if image.quality < 50 {
                    warn!("{} quality is low ({}). Consider rescanning.", finger_name, image.quality);
                }

                return Ok(CapturedImage {
                    data: image.data,
                    width: image.width as u32,
                    height: image.height as u32,
                    quality: image.quality,
                });
            }
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("no finger") || err_str.contains("nofinger") {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    continue;
                }
                let _ = sensor.set_led(false).await;
                return Err(anyhow::anyhow!("Failed to capture {} finger: {}", finger_name, e));
            }
        }
    }
}

async fn wait_for_finger_removal(sensor: &mut Sensor) -> Result<()> {
    info!("Waiting for finger removal...");
    while sensor.finger_present().await.unwrap_or(false) {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
    Ok(())
}

fn generate_embedding(
    embedder: &EmbedderModel,
    image: &CapturedImage,
    finger_name: &str,
) -> Result<PersonEmbedding> {
    embedder.embed(&image.data, image.width, image.height)
        .with_context(|| format!("Failed to generate embedding for {}", finger_name))
}

fn compute_representative_embedding(
    thumb: &PersonEmbedding,
    index: &PersonEmbedding,
    middle: &PersonEmbedding,
) -> PersonEmbedding {
    let avg_vector: Vec<f32> = (0..128)
        .map(|i| (thumb.vector[i] + index.vector[i] + middle.vector[i]) / 3.0)
        .collect();
    PersonEmbedding::from_vec(avg_vector)
}

pub async fn enroll_three_fingers(
    sensor_config: &SensorConfig,
    weights_path: &Path,
    scope: &str,
    password: Option<&str>,
) -> Result<XLockEnrollmentResult> {
    info!("Starting 3-finger enrollment with cross-finger CNN + X-Lock");

    let mut sensor = Sensor::connect(sensor_config).await
        .context("Failed to connect to fingerprint sensor")?;

    info!("Sensor connected. Will capture: thumb, index, middle");

    let thumb_image = capture_fingerprint(&mut sensor, "thumb").await?;

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let index_image = capture_fingerprint(&mut sensor, "index").await?;

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let middle_image = capture_fingerprint(&mut sensor, "middle").await?;

    info!("All three fingers captured. Generating embeddings...");

    let weights_path = weights_path.to_path_buf();
    let scope = scope.to_string();
    let password = password.map(|s| s.to_string());

    let result = tokio::task::spawn_blocking(move || {
        let embedder = EmbedderModel::load(&weights_path)
            .context("Failed to load CNN model")?;

        let thumb_embed = generate_embedding(&embedder, &thumb_image, "thumb")?;
        let index_embed = generate_embedding(&embedder, &index_image, "index")?;
        let middle_embed = generate_embedding(&embedder, &middle_image, "middle")?;

        let sim_ti = thumb_embed.similarity(&index_embed);
        let sim_tm = thumb_embed.similarity(&middle_embed);
        let sim_im = index_embed.similarity(&middle_embed);
        let avg_similarity = (sim_ti + sim_tm + sim_im) / 3.0;

        info!("Intra-person similarities: T-I={:.3}, T-M={:.3}, I-M={:.3}, avg={:.3}",
              sim_ti, sim_tm, sim_im, avg_similarity);

        if avg_similarity < 0.5 {
            warn!("Low cross-finger similarity ({:.3}). Are these from the same person?", avg_similarity);
        }

        let config = XLockConfig {
            use_hard_majority: false,
            min_avg_confidence: 0.15,
            ..Default::default()
        };
        let fuzzy = FuzzyNullifier::new(config)
            .context("Failed to create X-Lock fuzzy nullifier")?;

        let (helper_data, embedding_key) = fuzzy.enroll_three_fingers(
            &thumb_embed.vector,
            &index_embed.vector,
            &middle_embed.vector,
            &scope,
            password.as_deref(),
        ).context("X-Lock multi-finger enrollment failed")?;

        info!("Multi-finger enrollment complete. Nullifier: 0x{}...",
              hex::encode(&helper_data.nullifier[..8]));

        let representative_embedding = compute_representative_embedding(
            &thumb_embed, &index_embed, &middle_embed
        );

        Ok::<_, anyhow::Error>(XLockEnrollmentResult {
            nullifier: helper_data.nullifier,
            helper_data,
            intra_similarity: avg_similarity,
            representative_embedding,
            embedding_key,
        })
    }).await.context("Blocking task panicked")??;

    Ok(result)
}

pub async fn enroll_three_fingers_with_progress(
    sensor_config: &SensorConfig,
    weights_path: &Path,
    scope: &str,
    password: Option<&str>,
    progress_tx: mpsc::Sender<EnrollmentProgress>,
) -> Result<XLockEnrollmentResult> {
    info!("Starting 3-finger enrollment with progress updates");

    let mut sensor = Sensor::connect(sensor_config).await
        .context("Failed to connect to fingerprint sensor")?;

    info!("Sensor connected. Will capture: thumb, index, middle");

    let send_progress = |tx: &mpsc::Sender<EnrollmentProgress>, event: EnrollmentProgress| {
        if tx.try_send(event).is_err() {
            warn!("Failed to send progress event (receiver dropped?)");
        }
    };

    send_progress(&progress_tx, EnrollmentProgress::Ready { finger: "thumb".to_string() });
    let thumb_image = capture_fingerprint_no_wait(&mut sensor, "thumb").await?;
    send_progress(&progress_tx, EnrollmentProgress::Captured {
        finger: "thumb".to_string(),
        quality: thumb_image.quality
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    send_progress(&progress_tx, EnrollmentProgress::Lift { finger: "thumb".to_string() });
    wait_for_finger_removal(&mut sensor).await?;

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    send_progress(&progress_tx, EnrollmentProgress::Ready { finger: "index".to_string() });
    let index_image = capture_fingerprint_no_wait(&mut sensor, "index").await?;
    send_progress(&progress_tx, EnrollmentProgress::Captured {
        finger: "index".to_string(),
        quality: index_image.quality
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    send_progress(&progress_tx, EnrollmentProgress::Lift { finger: "index".to_string() });
    wait_for_finger_removal(&mut sensor).await?;

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    send_progress(&progress_tx, EnrollmentProgress::Ready { finger: "middle".to_string() });
    let middle_image = capture_fingerprint_no_wait(&mut sensor, "middle").await?;
    send_progress(&progress_tx, EnrollmentProgress::Captured {
        finger: "middle".to_string(),
        quality: middle_image.quality
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    send_progress(&progress_tx, EnrollmentProgress::Lift { finger: "middle".to_string() });
    wait_for_finger_removal(&mut sensor).await?;

    let _ = sensor.set_led(true).await;

    send_progress(&progress_tx, EnrollmentProgress::Processing {
        step: "loading_model".to_string(),
        percent: 0
    });

    info!("All three fingers captured. Generating embeddings...");

    let weights_path = weights_path.to_path_buf();
    let scope = scope.to_string();
    let password = password.map(|s| s.to_string());
    let progress_tx_clone = progress_tx.clone();

    let result = tokio::task::spawn_blocking(move || {
        let embedder = EmbedderModel::load(&weights_path)
            .context("Failed to load CNN model")?;
        let _ = progress_tx_clone.try_send(EnrollmentProgress::Processing {
            step: "thumb_embedding".to_string(),
            percent: 10
        });

        let thumb_embed = generate_embedding(&embedder, &thumb_image, "thumb")?;
        let _ = progress_tx_clone.try_send(EnrollmentProgress::Processing {
            step: "index_embedding".to_string(),
            percent: 35
        });

        let index_embed = generate_embedding(&embedder, &index_image, "index")?;
        let _ = progress_tx_clone.try_send(EnrollmentProgress::Processing {
            step: "middle_embedding".to_string(),
            percent: 60
        });

        let middle_embed = generate_embedding(&embedder, &middle_image, "middle")?;
        let _ = progress_tx_clone.try_send(EnrollmentProgress::Processing {
            step: "xlock_enrollment".to_string(),
            percent: 85
        });

        let sim_ti = thumb_embed.similarity(&index_embed);
        let sim_tm = thumb_embed.similarity(&middle_embed);
        let sim_im = index_embed.similarity(&middle_embed);
        let avg_similarity = (sim_ti + sim_tm + sim_im) / 3.0;

        info!("Intra-person similarities: T-I={:.3}, T-M={:.3}, I-M={:.3}, avg={:.3}",
              sim_ti, sim_tm, sim_im, avg_similarity);

        if avg_similarity < 0.5 {
            warn!("Low cross-finger similarity ({:.3}). Are these from the same person?", avg_similarity);
        }

        let config = XLockConfig {
            use_hard_majority: false,
            min_avg_confidence: 0.15,
            ..Default::default()
        };
        let fuzzy = FuzzyNullifier::new(config)
            .context("Failed to create X-Lock fuzzy nullifier")?;

        let (helper_data, embedding_key) = fuzzy.enroll_three_fingers(
            &thumb_embed.vector,
            &index_embed.vector,
            &middle_embed.vector,
            &scope,
            password.as_deref(),
        ).context("X-Lock multi-finger enrollment failed")?;

        let _ = progress_tx_clone.try_send(EnrollmentProgress::Processing {
            step: "finalizing".to_string(),
            percent: 95
        });

        info!("Multi-finger enrollment complete. Nullifier: 0x{}...",
              hex::encode(&helper_data.nullifier[..8]));

        let representative_embedding = compute_representative_embedding(
            &thumb_embed, &index_embed, &middle_embed
        );

        let _ = progress_tx_clone.try_send(EnrollmentProgress::Complete {
            nullifier: format!("0x{}", hex::encode(&helper_data.nullifier)),
            similarity: avg_similarity,
        });

        Ok::<_, anyhow::Error>(XLockEnrollmentResult {
            nullifier: helper_data.nullifier,
            helper_data,
            intra_similarity: avg_similarity,
            representative_embedding,
            embedding_key,
        })
    }).await.context("Blocking task panicked")??;

    Ok(result)
}

pub fn enroll_mock(
    scope: &str,
    password: Option<&str>,
) -> Result<XLockEnrollmentResult> {
    info!("Mock 3-finger enrollment (no hardware)");

    let thumb: Vec<f32> = (0..128).map(|i| ((i as f32) / 128.0) * 2.0 - 1.0).collect();
    let index: Vec<f32> = (0..128).map(|i| ((i as f32 + 5.0) / 128.0) * 2.0 - 1.0).collect();
    let middle: Vec<f32> = (0..128).map(|i| ((i as f32 + 10.0) / 128.0) * 2.0 - 1.0).collect();

    let thumb_embed = PersonEmbedding::from_vec(thumb);
    let index_embed = PersonEmbedding::from_vec(index);
    let middle_embed = PersonEmbedding::from_vec(middle);

    let avg_similarity = (
        thumb_embed.similarity(&index_embed) +
        thumb_embed.similarity(&middle_embed) +
        index_embed.similarity(&middle_embed)
    ) / 3.0;

    let config = XLockConfig {
        use_hard_majority: false,
        min_avg_confidence: 0.15,
        ..Default::default()
    };
    let fuzzy = FuzzyNullifier::new(config)?;

    let (helper_data, embedding_key) = fuzzy.enroll_three_fingers(
        &thumb_embed.vector,
        &index_embed.vector,
        &middle_embed.vector,
        scope,
        password,
    )?;

    let representative_embedding = compute_representative_embedding(
        &thumb_embed, &index_embed, &middle_embed
    );

    Ok(XLockEnrollmentResult {
        nullifier: helper_data.nullifier,
        helper_data,
        intra_similarity: avg_similarity,
        representative_embedding,
        embedding_key,
    })
}

pub async fn verify_finger(
    sensor_config: &SensorConfig,
    weights_path: &Path,
    helper_data: &MultiFingerHelperData,
    scope: &str,
    password: Option<&str>,
) -> Result<XLockVerifyResult> {
    info!("Starting finger verification with cross-finger CNN + X-Lock");

    let mut sensor = Sensor::connect(sensor_config).await
        .context("Failed to connect to fingerprint sensor")?;

    info!("Sensor connected. Scan ANY finger to verify...");

    let image = capture_fingerprint(&mut sensor, "verification").await?;

    info!("Fingerprint captured (quality: {})", image.quality);

    let weights_path = weights_path.to_path_buf();
    let helper_data = helper_data.clone();
    let scope = scope.to_string();
    let password = password.map(|s| s.to_string());

    let result = tokio::task::spawn_blocking(move || {
        let embedder = EmbedderModel::load(&weights_path)
            .context("Failed to load CNN model")?;

        let embedding = embedder.embed(&image.data, image.width, image.height)
            .context("Failed to generate embedding")?;

        let config = XLockConfig {
            use_hard_majority: false,
            min_avg_confidence: 0.05,
            ..Default::default()
        };
        let fuzzy = FuzzyNullifier::new(config)?;

        info!("Verifying with scope='{}', password={}", scope, password.is_some());
        info!("Helper data has {} enrolled fingers", helper_data.num_fingers());
        info!("Expected nullifier: 0x{}...", hex::encode(&helper_data.nullifier[..8]));

        let result = fuzzy.verify_against_multiple(
            &embedding.vector,
            &helper_data,
            &scope,
            password.as_deref(),
        );

        let (nullifier, embedding_key, matched_finger) = match result {
            Ok(r) => r,
            Err(e) => {
                warn!("X-Lock verification error details: {:?}", e);
                if let biometric_extract::contrastive::XLockError::ReproductionFailed { confidence } = &e {
                    if *confidence > 0.9 {
                        warn!("Nullifier mismatch! Voting succeeded but key derivation differs.");
                        warn!("Check: scope and password must match enrollment exactly.");
                    }
                }
                return Err(anyhow::anyhow!("X-Lock verification failed: {}", e));
            }
        };

        info!("Verification successful! Matched {} finger. Nullifier: 0x{}...",
              matched_finger, hex::encode(&nullifier[..8]));

        Ok::<_, anyhow::Error>(XLockVerifyResult {
            nullifier,
            embedding_key,
            matched_finger,
        })
    }).await.context("Blocking task panicked")??;

    Ok(result)
}

pub fn verify_mock(
    helper_data: &MultiFingerHelperData,
    scope: &str,
    password: Option<&str>,
) -> Result<XLockVerifyResult> {
    info!("Mock finger verification (no hardware)");

    let test_finger: Vec<f32> = (0..128).map(|i| ((i as f32 + 0.5) / 128.0) * 2.0 - 1.0).collect();

    let config = XLockConfig {
        use_hard_majority: false,
        min_avg_confidence: 0.15,
        ..Default::default()
    };
    let fuzzy = FuzzyNullifier::new(config)?;

    let (nullifier, embedding_key, matched_finger) = fuzzy.verify_against_multiple(
        &test_finger,
        helper_data,
        scope,
        password,
    )?;

    Ok(XLockVerifyResult {
        nullifier,
        embedding_key,
        matched_finger,
    })
}

mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_enrollment_and_verify() {
        let scope = "test-scope";
        let password = Some("test-password");

        let result = enroll_mock(scope, password).unwrap();
        assert_eq!(result.helper_data.num_fingers(), 3);

        let verify_result = verify_mock(&result.helper_data, scope, password).unwrap();
        assert_eq!(verify_result.nullifier, result.nullifier);
    }

    #[test]
    fn test_representative_embedding_computed() {
        let scope = "test-scope";

        let result = enroll_mock(scope, None).unwrap();

        assert_eq!(result.representative_embedding.vector.len(), 128);

        let norm: f32 = result.representative_embedding.vector.iter()
            .map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01, "Embedding should be L2-normalized, got norm {}", norm);
    }

    #[test]
    fn test_embedding_key_security_fix() {
        let scope = "test-scope";
        let password = Some("secure-password");

        let enroll_result = enroll_mock(scope, password).unwrap();

        assert_ne!(enroll_result.embedding_key, [0u8; 32],
            "embedding_key should be derived from β, not zero");

        let serialized = enroll_result.helper_data.to_bytes();

        let verify_result = verify_mock(&enroll_result.helper_data, scope, password).unwrap();

        assert_eq!(verify_result.embedding_key, enroll_result.embedding_key,
            "Re-derived embedding_key must match enrollment key - \
             this is the core of the security model");

        println!("Helper data size (without embedding_key): {} bytes", serialized.len());

        let restored = MultiFingerHelperData::from_bytes(&serialized).unwrap();
        assert_eq!(restored.nullifier, enroll_result.helper_data.nullifier,
            "Deserialized helper_data should have correct nullifier");
    }
}
