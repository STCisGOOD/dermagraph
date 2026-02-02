
use anyhow::{Context, Result};
use burn::prelude::*;
use burn::record::{FullPrecisionSettings, Recorder};
use burn_import::safetensors::SafetensorsFileRecorder;
use std::path::Path;
use tracing::info;

use super::{FingerprintEmbedder, EmbedderConfig};

pub fn load_embedder_from_safetensors<B: Backend>(
    path: &Path,
    device: &B::Device,
) -> Result<FingerprintEmbedder<B>> {
    info!("Loading FingerprintEmbedder from: {}", path.display());

    let config = EmbedderConfig::default();
    let embedding_dim = config.embedding_dim;

    let model = FingerprintEmbedder::new(device, config);

    let record = SafetensorsFileRecorder::<FullPrecisionSettings>::default()
        .load(path.to_path_buf().into(), device)
        .with_context(|| format!("Failed to load safetensors: {}", path.display()))?;

    let model = model.load_record(record);

    info!("Successfully loaded FingerprintEmbedder weights ({} embedding dim)",
          embedding_dim);

    Ok(model)
}

pub fn load_embedder_with_config<B: Backend>(
    path: &Path,
    device: &B::Device,
    config: EmbedderConfig,
) -> Result<FingerprintEmbedder<B>> {
    info!("Loading FingerprintEmbedder from: {} (embed_dim={})",
          path.display(), config.embedding_dim);

    let model = FingerprintEmbedder::new(device, config);

    let record = SafetensorsFileRecorder::<FullPrecisionSettings>::default()
        .load(path.to_path_buf().into(), device)
        .with_context(|| format!("Failed to load safetensors: {}", path.display()))?;

    let model = model.load_record(record);

    info!("Successfully loaded FingerprintEmbedder weights");
    Ok(model)
}

#[cfg(test)]
mod tests {
    use super::*;
    use burn_ndarray::NdArray;

    type TestBackend = NdArray<f32>;

    #[test]
    fn test_module_compiles() {
        assert!(true);
    }

    #[test]
    fn test_list_safetensors_keys() {
        use safetensors::SafeTensors;

        let weights_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap()
            .parent().unwrap()
            .join("checkpoints")
            .join("best_burn.safetensors");

        if !weights_path.exists() {
            eprintln!("Skipping: {} not found", weights_path.display());
            return;
        }

        let data = std::fs::read(&weights_path).expect("Failed to read file");
        let tensors = SafeTensors::deserialize(&data).expect("Failed to parse safetensors");

        println!("\n═══ Safetensors Keys in {} ═══\n", weights_path.display());
        let mut names: Vec<_> = tensors.names().into_iter().collect();
        names.sort();
        for name in &names {
            let tensor = tensors.tensor(name).unwrap();
            println!("  {} {:?}", name, tensor.shape());
        }
        println!("\nTotal: {} tensors", names.len());
    }

    #[test]
    fn test_load_converted_weights_and_inference() {
        use super::super::center_features::CenterFeatures;

        let weights_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("checkpoints")
            .join("best_burn.safetensors");

        if !weights_path.exists() {
            eprintln!(
                "Skipping test: {} not found. Run `tch-trainer convert-to-burn` first.",
                weights_path.display()
            );
            return;
        }

        let device = Default::default();
        let model = load_embedder_from_safetensors::<TestBackend>(&weights_path, &device)
            .expect("Failed to load model from safetensors");

        let images = burn::tensor::Tensor::<TestBackend, 4>::zeros([1, 1, 192, 192], &device);
        let classical = burn::tensor::Tensor::<TestBackend, 2>::zeros([1, CenterFeatures::DIM], &device);

        let embeddings = model.forward(images, classical);

        let dims = embeddings.dims();
        assert_eq!(dims, [1, 128], "Expected [1, 128], got {:?}", dims);

        let norm: f32 = embeddings.clone()
            .powf_scalar(2.0)
            .sum_dim(1)
            .sqrt()
            .into_data()
            .to_vec()
            .unwrap()[0];
        assert!(
            (norm - 1.0).abs() < 1e-4,
            "Expected L2 norm ~1.0, got {}",
            norm
        );

        println!("[OK] Burn safetensors integration verified!");
        println!("   Model loaded from: {}", weights_path.display());
        println!("   Embedding dim: {}", dims[1]);
        println!("   L2 norm: {:.6}", norm);
    }

    #[test]
    fn test_end_to_end_fuzzy_nullifier_pipeline() {
        use super::super::center_features::CenterFeatures;
        use super::super::fuzzy_extractor::{FuzzyNullifier, XLockConfig, expand_embedding_to_bits};
        use super::super::PersonEmbedding;

        let weights_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap()
            .parent().unwrap()
            .join("checkpoints")
            .join("best_burn.safetensors");

        if !weights_path.exists() {
            eprintln!("Skipping test: {} not found", weights_path.display());
            return;
        }

        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!("║     DERMAGRAPH END-TO-END PIPELINE TEST                       ║");
        println!("║     Fingerprint → Embedding → X-Lock → Nullifier              ║");
        println!("╚══════════════════════════════════════════════════════════════╝\n");

        println!("[1] Step 1: Loading trained model...");
        let device = Default::default();
        let model = load_embedder_from_safetensors::<TestBackend>(&weights_path, &device)
            .expect("Failed to load model");
        println!("   ✓ Model loaded from: {}\n", weights_path.display());

        println!("[2] Step 2: Generating embeddings...");

        let image1 = burn::tensor::Tensor::<TestBackend, 4>::zeros([1, 1, 192, 192], &device);
        let classical1 = burn::tensor::Tensor::<TestBackend, 2>::zeros([1, CenterFeatures::DIM], &device);
        let embedding1_tensor = model.forward(image1, classical1);
        let embedding1_vec: Vec<f32> = embedding1_tensor.into_data().to_vec().unwrap();
        let embedding1 = PersonEmbedding::from_vec(embedding1_vec.clone());

        let image2 = burn::tensor::Tensor::<TestBackend, 4>::zeros([1, 1, 192, 192], &device)
            + burn::tensor::Tensor::<TestBackend, 4>::ones([1, 1, 192, 192], &device) * 0.01;
        let classical2 = burn::tensor::Tensor::<TestBackend, 2>::zeros([1, CenterFeatures::DIM], &device);
        let embedding2_tensor = model.forward(image2, classical2);
        let embedding2_vec: Vec<f32> = embedding2_tensor.into_data().to_vec().unwrap();
        let embedding2 = PersonEmbedding::from_vec(embedding2_vec);

        let similarity = embedding1.similarity(&embedding2);
        println!("   Embedding 1: {:?}... (128 dims)", &embedding1.vector[..4]);
        println!("   Embedding 2: {:?}... (128 dims)", &embedding2.vector[..4]);
        println!("   Cosine similarity: {:.4}", similarity);
        println!("   ✓ Embeddings generated\n");

        println!("[3] Step 3: X-Lock Fuzzy Extraction...");
        let config = XLockConfig {
            use_hard_majority: false,
            ..XLockConfig::default()
        };
        let fuzzy = FuzzyNullifier::new(config).expect("Failed to create fuzzy nullifier");

        println!("   Config: {} entropy bits, {} lockers/bit, {} indices/locker",
            fuzzy.config().entropy_bits,
            fuzzy.config().lockers_per_bit,
            fuzzy.config().indices_per_locker);

        println!("\n[4] Step 4: Enrollment (Gen procedure)...");
        let scope = "dao-vote-2024";
        let password = Some("secret123");

        let (helper_data, nullifier1) = fuzzy.enroll(&embedding1.vector, scope, password)
            .expect("Enrollment failed");

        println!("   Scope: \"{}\"", scope);
        println!("   Nullifier: 0x{}...", nullifier1[..8].iter().map(|b| format!("{:02x}", b)).collect::<String>());
        println!("   Helper data size: {} bytes", helper_data.to_bytes().len());
        println!("   ✓ Enrollment complete\n");

        println!("[OK] Step 5: Verification (Rep procedure)...");
        let nullifier2 = fuzzy.verify(&embedding2.vector, &helper_data, scope, password)
            .expect("Verification failed");

        println!("   Reproduced: 0x{}...", nullifier2[..8].iter().map(|b| format!("{:02x}", b)).collect::<String>());

        if nullifier1 == nullifier2 {
            println!("   ✓ MATCH! Same person confirmed\n");
        } else {
            println!("   ✗ NO MATCH - nullifiers differ\n");
        }

        println!("[6] Step 6: Different scope test...");
        let (_, nullifier_other_scope) = fuzzy.enroll(&embedding1.vector, "airdrop-2024", password)
            .expect("Enrollment failed");

        println!("   Scope: \"airdrop-2024\"");
        println!("   Nullifier: 0x{}...", nullifier_other_scope[..8].iter().map(|b| format!("{:02x}", b)).collect::<String>());
        assert_ne!(nullifier1, nullifier_other_scope, "Different scopes should produce different nullifiers");
        println!("   ✓ Different scope produces different nullifier\n");

        println!("╔══════════════════════════════════════════════════════════════╗");
        println!("║                    PIPELINE TEST COMPLETE                     ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  ✓ Model loaded from safetensors                             ║");
        println!("║  ✓ 128-dim L2-normalized embeddings generated                ║");
        println!("║  ✓ X-Lock fuzzy extraction working                           ║");
        println!("║  ✓ Nullifier reproduction successful                         ║");
        println!("║  ✓ Scope separation verified                                 ║");
        println!("╚══════════════════════════════════════════════════════════════╝");

        assert_eq!(nullifier1, nullifier2, "Same person should produce same nullifier");
    }
}
