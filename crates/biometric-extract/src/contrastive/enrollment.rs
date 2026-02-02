
use burn::prelude::*;
use burn::optim::{AdamWConfig, Optimizer, GradientsParams};
use burn::tensor::backend::AutodiffBackend;
use tracing::{info, debug};

use super::embedder::{FingerprintEmbedder, EmbedderConfig};
use super::center_features::{CenterFeatures, extract_center_features};
use super::core_detect::detect_core;
use super::fuzzy_extractor::{FuzzyNullifier, HelperData, MultiFingerHelperData, FingerType, XLockConfig};
use super::loss::info_nce_loss;
use super::PersonEmbedding;
use crate::orientation::OrientationField;
use crate::image_proc::FingerprintImage;

#[derive(Debug, Clone)]
pub struct EnrollmentResult {
    pub multi_helper: MultiFingerHelperData,

    pub nullifier: [u8; 32],

    pub embedding_key: [u8; 32],

    pub training_pairs_contributed: usize,

    pub intra_person_similarity: f32,

    pub fingers_enrolled: usize,
}

impl EnrollmentResult {
    #[deprecated(since = "0.2.0", note = "Use multi_helper for proper multi-finger support")]
    pub fn helper_data(&self) -> &HelperData {
        &self.multi_helper.finger_helpers[0].1
    }
}

#[derive(Debug, Clone)]
pub struct EnrollmentConfig {
    pub min_intra_similarity: f32,

    pub enable_online_training: bool,

    pub xlock_config: XLockConfig,

    pub learning_rate: f64,

    pub temperature: f32,
}

impl Default for EnrollmentConfig {
    fn default() -> Self {
        Self {
            min_intra_similarity: 0.3,
            enable_online_training: true,
            xlock_config: XLockConfig::default(),
            learning_rate: 0.0001,
            temperature: 0.07,
        }
    }
}

struct TrainingBatch<B: Backend> {
    anchors: Vec<Tensor<B, 1>>,

    positives: Vec<Tensor<B, 1>>,
}

impl<B: Backend> TrainingBatch<B> {
    fn new() -> Self {
        Self {
            anchors: Vec::new(),
            positives: Vec::new(),
        }
    }

    fn len(&self) -> usize {
        self.anchors.len()
    }

    fn clear(&mut self) {
        self.anchors.clear();
        self.positives.clear();
    }
}

pub struct MultiFingerEnrollment<B: AutodiffBackend> {
    model: FingerprintEmbedder<B>,

    fuzzy_nullifier: FuzzyNullifier,

    config: EnrollmentConfig,

    device: B::Device,

    training_batch: TrainingBatch<B>,

    min_batch_size: usize,

    optimizer_config: AdamWConfig,

    total_pairs_trained: usize,
}

impl<B: AutodiffBackend> MultiFingerEnrollment<B> {
    pub fn new(device: B::Device) -> anyhow::Result<Self> {
        Self::with_config(device, EnrollmentConfig::default())
    }

    pub fn with_config(device: B::Device, config: EnrollmentConfig) -> anyhow::Result<Self> {
        let embedder_config = EmbedderConfig::default();
        let model = FingerprintEmbedder::new(&device, embedder_config);

        let fuzzy_nullifier = FuzzyNullifier::new(config.xlock_config.clone())
            .map_err(|e| anyhow::anyhow!("Failed to create fuzzy nullifier: {}", e))?;

        let optimizer_config = AdamWConfig::new()
            .with_weight_decay(0.01);

        Ok(Self {
            model,
            fuzzy_nullifier,
            config,
            device,
            training_batch: TrainingBatch::new(),
            min_batch_size: 8,
            optimizer_config,
            total_pairs_trained: 0,
        })
    }

    pub fn enroll_three_fingers(
        &mut self,
        thumb: &[u8],
        index: &[u8],
        middle: &[u8],
        scope: &str,
        password: Option<&str>,
    ) -> anyhow::Result<EnrollmentResult> {
        let embed_thumb = self.compute_embedding(thumb)?;
        let embed_index = self.compute_embedding(index)?;
        let embed_middle = self.compute_embedding(middle)?;

        let sim_ti = self.cosine_similarity(&embed_thumb, &embed_index);
        let sim_tm = self.cosine_similarity(&embed_thumb, &embed_middle);
        let sim_im = self.cosine_similarity(&embed_index, &embed_middle);
        let avg_similarity = (sim_ti + sim_tm + sim_im) / 3.0;

        debug!(
            "Intra-person similarities: thumb-index={:.3}, thumb-middle={:.3}, index-middle={:.3}, avg={:.3}",
            sim_ti, sim_tm, sim_im, avg_similarity
        );

        if avg_similarity < self.config.min_intra_similarity {
            return Err(anyhow::anyhow!(
                "Fingers don't appear to be from the same person (similarity: {:.2}%, threshold: {:.2}%)",
                avg_similarity * 100.0,
                self.config.min_intra_similarity * 100.0
            ));
        }

        let (multi_helper, embedding_key) = self.fuzzy_nullifier
            .enroll_three_fingers(
                &embed_thumb.vector,
                &embed_index.vector,
                &embed_middle.vector,
                scope,
                password,
            )
            .map_err(|e| anyhow::anyhow!("Multi-finger enrollment failed: {}", e))?;

        let nullifier = multi_helper.nullifier;

        let mut pairs_contributed = 0;
        if self.config.enable_online_training {
            let t_thumb = self.embedding_to_tensor(&embed_thumb);
            let t_index = self.embedding_to_tensor(&embed_index);
            let t_middle = self.embedding_to_tensor(&embed_middle);

            self.training_batch.anchors.push(t_thumb.clone());
            self.training_batch.positives.push(t_index.clone());

            self.training_batch.anchors.push(t_thumb);
            self.training_batch.positives.push(t_middle.clone());

            self.training_batch.anchors.push(t_index);
            self.training_batch.positives.push(t_middle);

            pairs_contributed = 3;

            self.maybe_train()?;
        }

        info!(
            "Enrolled user with {} fingers: similarity={:.1}%, pairs_contributed={}",
            multi_helper.num_fingers(),
            avg_similarity * 100.0,
            pairs_contributed
        );

        Ok(EnrollmentResult {
            multi_helper,
            nullifier,
            embedding_key,
            training_pairs_contributed: pairs_contributed,
            intra_person_similarity: avg_similarity,
            fingers_enrolled: 3,
        })
    }

    pub fn verify_any_finger(
        &self,
        finger: &[u8],
        multi_helper: &MultiFingerHelperData,
        scope: &str,
        password: Option<&str>,
    ) -> anyhow::Result<([u8; 32], [u8; 32], FingerType)> {
        let embedding = self.compute_embedding(finger)?;

        self.fuzzy_nullifier
            .verify_against_multiple(&embedding.vector, multi_helper, scope, password)
            .map_err(|e| anyhow::anyhow!("Verification failed: {}", e))
    }

    #[deprecated(since = "0.2.0", note = "Use verify_any_finger with MultiFingerHelperData")]
    pub fn verify_single_helper(
        &self,
        finger: &[u8],
        helper_data: &HelperData,
        scope: &str,
        password: Option<&str>,
    ) -> anyhow::Result<[u8; 32]> {
        let embedding = self.compute_embedding(finger)?;

        self.fuzzy_nullifier
            .verify(&embedding.vector, helper_data, scope, password)
            .map_err(|e| anyhow::anyhow!("Verification failed: {}", e))
    }

    fn compute_embedding(&self, image_data: &[u8]) -> anyhow::Result<PersonEmbedding> {
        let image = FingerprintImage::from_bytes(image_data)?;
        let normalized = image.normalize()?;

        let (width, height) = normalized.dimensions();

        let orientation = OrientationField::compute(&normalized)?;
        let cores = detect_core(&orientation, orientation.block_size());

        let classical = extract_center_features(&orientation, None, &cores);

        let pixels: Vec<f32> = normalized
            .as_bytes()
            .iter()
            .map(|&b| b as f32 / 255.0)
            .collect();

        let image_tensor: Tensor<B, 4> = Tensor::<B, 1>::from_floats(pixels.as_slice(), &self.device)
            .reshape([1, 1, height as usize, width as usize]);

        let classical_tensor: Tensor<B, 2> = Tensor::<B, 1>::from_floats(
            classical.to_vector().as_slice(),
            &self.device
        ).reshape([1, CenterFeatures::DIM]);

        let embedding = self.model.forward(image_tensor, classical_tensor);

        let embedding_data: Vec<f32> = embedding.into_data().to_vec()
            .map_err(|e| anyhow::anyhow!("Failed to extract embedding: {:?}", e))?;

        Ok(PersonEmbedding::from_vec(embedding_data))
    }

    fn cosine_similarity(&self, a: &PersonEmbedding, b: &PersonEmbedding) -> f32 {
        a.similarity(b)
    }

    fn average_embeddings(&self, embeddings: &[&PersonEmbedding]) -> PersonEmbedding {
        let dim = embeddings[0].vector.len();
        let mut avg = vec![0.0f32; dim];

        for emb in embeddings {
            for (i, &v) in emb.vector.iter().enumerate() {
                avg[i] += v;
            }
        }

        let n = embeddings.len() as f32;
        for v in &mut avg {
            *v /= n;
        }

        PersonEmbedding::from_vec(avg)
    }

    fn embedding_to_tensor(&self, embedding: &PersonEmbedding) -> Tensor<B, 1> {
        Tensor::from_floats(embedding.vector.as_slice(), &self.device)
    }

    fn maybe_train(&mut self) -> anyhow::Result<()> {
        if self.training_batch.len() < self.min_batch_size {
            return Ok(());
        }

        info!(
            "Running online training step with {} pairs",
            self.training_batch.len()
        );

        let batch_size = self.training_batch.len();
        let embed_dim = self.training_batch.anchors[0].dims()[0];

        let anchors: Vec<Tensor<B, 2>> = self.training_batch.anchors
            .iter()
            .map(|t| t.clone().unsqueeze())
            .collect();
        let anchor_batch = Tensor::cat(anchors, 0);

        let positives: Vec<Tensor<B, 2>> = self.training_batch.positives
            .iter()
            .map(|t| t.clone().unsqueeze())
            .collect();
        let positive_batch = Tensor::cat(positives, 0);

        let loss = info_nce_loss(anchor_batch, positive_batch, self.config.temperature);
        let loss_value: f32 = loss.clone().into_data().to_vec().unwrap_or(vec![0.0])[0];

        debug!("Contrastive loss: {:.4}", loss_value);

        let gradients = loss.backward();
        let grads = GradientsParams::from_grads(gradients, &self.model);
        let mut optim = self.optimizer_config.clone().init();
        self.model = optim.step(self.config.learning_rate, self.model.clone(), grads);

        self.total_pairs_trained += batch_size;
        self.training_batch.clear();

        info!(
            "Training step complete. Total pairs trained: {}",
            self.total_pairs_trained
        );

        Ok(())
    }

    pub fn total_pairs_trained(&self) -> usize {
        self.total_pairs_trained
    }

    pub fn pending_batch_size(&self) -> usize {
        self.training_batch.len()
    }

    pub fn flush_training(&mut self) -> anyhow::Result<()> {
        if self.training_batch.len() >= 2 {
            self.min_batch_size = 2;
            self.maybe_train()?;
            self.min_batch_size = 8;
        }
        Ok(())
    }

    pub fn model(&self) -> &FingerprintEmbedder<B> {
        &self.model
    }

    pub fn save_model(&self, path: &std::path::Path) -> anyhow::Result<()> {
        use burn::record::{FullPrecisionSettings, Recorder};

        let recorder = burn::record::DefaultFileRecorder::<FullPrecisionSettings>::new();
        recorder.record(self.model.clone().into_record(), path.to_path_buf())
            .map_err(|e| anyhow::anyhow!("Failed to save model: {:?}", e))?;

        info!("Saved model to {}", path.display());
        Ok(())
    }

    pub fn load_model(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        use burn::record::{FullPrecisionSettings, Recorder};

        let recorder = burn::record::DefaultFileRecorder::<FullPrecisionSettings>::new();
        let record = recorder.load(path.to_path_buf(), &self.device)
            .map_err(|e| anyhow::anyhow!("Failed to load model: {:?}", e))?;

        self.model = self.model.clone().load_record(record);

        info!("Loaded model from {}", path.display());
        Ok(())
    }
}

pub struct SimpleEnrollment<B: Backend> {
    model: FingerprintEmbedder<B>,
    fuzzy_nullifier: FuzzyNullifier,
    device: B::Device,
    min_intra_similarity: f32,
}

impl<B: Backend> SimpleEnrollment<B> {
    pub fn new(device: B::Device) -> anyhow::Result<Self> {
        let model = FingerprintEmbedder::new(&device, EmbedderConfig::default());
        let fuzzy_nullifier = FuzzyNullifier::default_settings()
            .map_err(|e| anyhow::anyhow!("Failed to create fuzzy nullifier: {}", e))?;

        Ok(Self {
            model,
            fuzzy_nullifier,
            device,
            min_intra_similarity: 0.3,
        })
    }

    pub fn load(device: B::Device, model_path: &std::path::Path) -> anyhow::Result<Self> {
        use burn::record::{FullPrecisionSettings, Recorder};

        let model = FingerprintEmbedder::new(&device, EmbedderConfig::default());

        let recorder = burn::record::DefaultFileRecorder::<FullPrecisionSettings>::new();
        let record = recorder.load(model_path.to_path_buf(), &device)
            .map_err(|e| anyhow::anyhow!("Failed to load model: {:?}", e))?;

        let model = model.load_record(record);

        let fuzzy_nullifier = FuzzyNullifier::default_settings()
            .map_err(|e| anyhow::anyhow!("Failed to create fuzzy nullifier: {}", e))?;

        Ok(Self {
            model,
            fuzzy_nullifier,
            device,
            min_intra_similarity: 0.3,
        })
    }

    pub fn enroll(
        &self,
        thumb: &[u8],
        index: &[u8],
        middle: &[u8],
        scope: &str,
        password: Option<&str>,
    ) -> anyhow::Result<EnrollmentResult> {
        let embed_thumb = self.compute_embedding(thumb)?;
        let embed_index = self.compute_embedding(index)?;
        let embed_middle = self.compute_embedding(middle)?;

        let sim_ti = embed_thumb.similarity(&embed_index);
        let sim_tm = embed_thumb.similarity(&embed_middle);
        let sim_im = embed_index.similarity(&embed_middle);
        let avg_similarity = (sim_ti + sim_tm + sim_im) / 3.0;

        if avg_similarity < self.min_intra_similarity {
            return Err(anyhow::anyhow!(
                "Fingers don't appear to be from the same person (similarity: {:.2}%)",
                avg_similarity * 100.0
            ));
        }

        let (multi_helper, embedding_key) = self.fuzzy_nullifier
            .enroll_three_fingers(
                &embed_thumb.vector,
                &embed_index.vector,
                &embed_middle.vector,
                scope,
                password,
            )
            .map_err(|e| anyhow::anyhow!("Multi-finger enrollment failed: {}", e))?;

        let nullifier = multi_helper.nullifier;

        Ok(EnrollmentResult {
            multi_helper,
            nullifier,
            embedding_key,
            training_pairs_contributed: 0,
            intra_person_similarity: avg_similarity,
            fingers_enrolled: 3,
        })
    }

    pub fn verify(
        &self,
        finger: &[u8],
        multi_helper: &MultiFingerHelperData,
        scope: &str,
        password: Option<&str>,
    ) -> anyhow::Result<([u8; 32], [u8; 32], FingerType)> {
        let embedding = self.compute_embedding(finger)?;

        self.fuzzy_nullifier
            .verify_against_multiple(&embedding.vector, multi_helper, scope, password)
            .map_err(|e| anyhow::anyhow!("Verification failed: {}", e))
    }

    pub fn model(&self) -> &FingerprintEmbedder<B> {
        &self.model
    }

    pub fn compute_embedding(&self, image_data: &[u8]) -> anyhow::Result<PersonEmbedding> {
        let image = FingerprintImage::from_bytes(image_data)?;
        let normalized = image.normalize()?;
        let (width, height) = normalized.dimensions();

        let orientation = OrientationField::compute(&normalized)?;
        let cores = detect_core(&orientation, orientation.block_size());
        let classical = extract_center_features(&orientation, None, &cores);

        let pixels: Vec<f32> = normalized.as_bytes().iter()
            .map(|&b| b as f32 / 255.0).collect();

        let image_tensor: Tensor<B, 4> = Tensor::<B, 1>::from_floats(pixels.as_slice(), &self.device)
            .reshape([1, 1, height as usize, width as usize]);

        let classical_tensor: Tensor<B, 2> = Tensor::<B, 1>::from_floats(
            classical.to_vector().as_slice(), &self.device
        ).reshape([1, CenterFeatures::DIM]);

        let embedding = self.model.forward(image_tensor, classical_tensor);
        let data: Vec<f32> = embedding.into_data().to_vec()
            .map_err(|e| anyhow::anyhow!("Failed to extract embedding: {:?}", e))?;

        Ok(PersonEmbedding::from_vec(data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enrollment_config_default() {
        let config = EnrollmentConfig::default();
        assert!(config.min_intra_similarity > 0.0);
        assert!(config.enable_online_training);
    }

    #[test]
    fn test_enrollment_result() {
        let helper = HelperData {
            indices: vec![],
            vault: vec![],
            config: XLockConfig::default(),
            version: 1,
        };

        let multi_helper = MultiFingerHelperData {
            finger_helpers: vec![
                (FingerType::Thumb, helper.clone()),
                (FingerType::Index, helper.clone()),
                (FingerType::Middle, helper),
            ],
            nullifier: [0u8; 32],
            version: 3,
        };

        let result = EnrollmentResult {
            multi_helper,
            nullifier: [0u8; 32],
            embedding_key: [1u8; 32],
            training_pairs_contributed: 3,
            intra_person_similarity: 0.75,
            fingers_enrolled: 3,
        };

        assert_eq!(result.training_pairs_contributed, 3);
        assert!(result.intra_person_similarity > 0.5);
        assert_eq!(result.fingers_enrolled, 3);
    }

    #[test]
    fn test_multi_finger_helper_data_serialization() {
        let helper = HelperData {
            indices: vec![vec![vec![1u16, 2, 3, 4, 5]; 15]; 48],
            vault: vec![vec![false; 15]; 48],
            config: XLockConfig::default(),
            version: 1,
        };

        let multi_helper = MultiFingerHelperData {
            finger_helpers: vec![
                (FingerType::Thumb, helper.clone()),
                (FingerType::Index, helper),
            ],
            nullifier: [42u8; 32],
            version: 3,
        };

        let bytes = multi_helper.to_bytes();
        let restored = MultiFingerHelperData::from_bytes(&bytes).unwrap();

        assert_eq!(restored.num_fingers(), 2);
        assert_eq!(restored.nullifier, [42u8; 32]);
        assert_eq!(restored.finger_helpers[0].0, FingerType::Thumb);
        assert_eq!(restored.finger_helpers[1].0, FingerType::Index);
    }
}
