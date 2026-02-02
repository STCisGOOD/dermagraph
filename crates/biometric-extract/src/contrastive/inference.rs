
use burn::prelude::*;
use burn::record::{FullPrecisionSettings, Recorder};
use std::path::Path;
use tracing::info;

use super::embedder::{FingerprintEmbedder, EmbedderConfig};
use super::center_features::{CenterFeatures, extract_center_features};
use super::core_detect::detect_core;
use super::{PersonEmbedding, ContrastiveConfig};
use super::fuzzy_extractor::{FuzzyNullifier, HelperData, XLockConfig, XLockError};
use super::safetensors_loader::load_embedder_from_safetensors;
use crate::orientation::OrientationField;
use crate::image_proc::FingerprintImage;

#[derive(Debug, Clone)]
pub struct IdentityResult {
    pub same_person: bool,

    pub confidence: f32,

    pub similarity: f32,

    pub threshold: f32,
}

impl IdentityResult {
    pub fn is_confident(&self) -> bool {
        self.confidence > 0.8
    }

    pub fn description(&self) -> String {
        let certainty = if self.confidence > 0.9 {
            "very likely"
        } else if self.confidence > 0.7 {
            "likely"
        } else if self.confidence > 0.5 {
            "possibly"
        } else {
            "uncertain"
        };

        if self.same_person {
            format!("Same person ({}, {:.0}% confidence)", certainty, self.confidence * 100.0)
        } else {
            format!("Different people ({}, {:.0}% confidence)", certainty, self.confidence * 100.0)
        }
    }
}

pub struct PersonIdentifier<B: Backend> {
    model: FingerprintEmbedder<B>,

    config: ContrastiveConfig,

    device: B::Device,

    fuzzy_nullifier: Option<FuzzyNullifier>,
}

impl<B: Backend> PersonIdentifier<B> {
    pub fn new(device: B::Device) -> Self {
        let embedder_config = EmbedderConfig::default();
        let model = FingerprintEmbedder::new(&device, embedder_config);

        let fuzzy_nullifier = FuzzyNullifier::default_settings().ok();

        Self {
            model,
            config: ContrastiveConfig::default(),
            device,
            fuzzy_nullifier,
        }
    }

    pub fn with_xlock_config(device: B::Device, xlock_config: XLockConfig) -> Self {
        let embedder_config = EmbedderConfig::default();
        let model = FingerprintEmbedder::new(&device, embedder_config);
        let fuzzy_nullifier = FuzzyNullifier::new(xlock_config).ok();

        Self {
            model,
            config: ContrastiveConfig::default(),
            device,
            fuzzy_nullifier,
        }
    }

    pub fn load(device: B::Device, path: &Path) -> anyhow::Result<Self> {
        let embedder_config = EmbedderConfig::default();
        let model = FingerprintEmbedder::new(&device, embedder_config);

        let record = burn::record::DefaultFileRecorder::<FullPrecisionSettings>::new()
            .load(path.to_path_buf(), &device)
            .map_err(|e| anyhow::anyhow!("Failed to load model: {:?}", e))?;

        let model = model.load_record(record);

        info!("Loaded person identifier model from {}", path.display());

        let fuzzy_nullifier = FuzzyNullifier::default_settings().ok();

        Ok(Self {
            model,
            config: ContrastiveConfig::default(),
            device,
            fuzzy_nullifier,
        })
    }

    pub fn load_safetensors(device: B::Device, path: &Path) -> anyhow::Result<Self> {
        let model = load_embedder_from_safetensors::<B>(path, &device)
            .map_err(|e| anyhow::anyhow!("Failed to load safetensors model: {}", e))?;

        info!("Loaded person identifier from safetensors: {}", path.display());

        let fuzzy_nullifier = FuzzyNullifier::default_settings().ok();

        Ok(Self {
            model,
            config: ContrastiveConfig::default(),
            device,
            fuzzy_nullifier,
        })
    }

    pub fn embed(&self, image_data: &[u8]) -> anyhow::Result<PersonEmbedding> {
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

        let classical_tensor: Tensor<B, 2> = Tensor::<B, 1>::from_floats(classical.to_vector().as_slice(), &self.device)
            .reshape([1, CenterFeatures::DIM]);

        let embedding = self.model.forward(image_tensor, classical_tensor);

        let embedding_data: Vec<f32> = embedding.into_data().to_vec()
            .map_err(|e| anyhow::anyhow!("Failed to extract embedding: {:?}", e))?;

        Ok(PersonEmbedding::from_vec(embedding_data))
    }

    pub fn embed_raw(
        &self,
        data: &[u8],
        width: u32,
        height: u32,
    ) -> anyhow::Result<PersonEmbedding> {
        let image = FingerprintImage::from_raw(width, height, data.to_vec())?;
        let normalized = image.normalize()?;

        let orientation = OrientationField::compute(&normalized)?;
        let cores = detect_core(&orientation, orientation.block_size());
        let classical = extract_center_features(&orientation, None, &cores);

        let pixels: Vec<f32> = data.iter().map(|&b| b as f32 / 255.0).collect();

        let image_tensor: Tensor<B, 4> = Tensor::<B, 1>::from_floats(pixels.as_slice(), &self.device)
            .reshape([1, 1, height as usize, width as usize]);

        let classical_tensor: Tensor<B, 2> = Tensor::<B, 1>::from_floats(classical.to_vector().as_slice(), &self.device)
            .reshape([1, CenterFeatures::DIM]);

        let embedding = self.model.forward(image_tensor, classical_tensor);
        let embedding_data: Vec<f32> = embedding.into_data().to_vec()
            .map_err(|e| anyhow::anyhow!("Failed to extract embedding: {:?}", e))?;

        Ok(PersonEmbedding::from_vec(embedding_data))
    }

    pub fn compare(
        &self,
        image1: &[u8],
        image2: &[u8],
    ) -> anyhow::Result<IdentityResult> {
        let embedding1 = self.embed(image1)?;
        let embedding2 = self.embed(image2)?;

        self.compare_embeddings(&embedding1, &embedding2)
    }

    pub fn compare_embeddings(
        &self,
        embedding1: &PersonEmbedding,
        embedding2: &PersonEmbedding,
    ) -> anyhow::Result<IdentityResult> {
        let similarity = embedding1.similarity(embedding2);
        let threshold = self.config.same_person_threshold;

        let confidence = if similarity > threshold {
            let excess = (similarity - threshold) / (1.0 - threshold);
            0.5 + 0.5 * excess.min(1.0)
        } else {
            let deficit = (threshold - similarity) / (threshold + 1.0);
            0.5 + 0.5 * deficit.min(1.0)
        };

        Ok(IdentityResult {
            same_person: similarity > threshold,
            confidence,
            similarity,
            threshold,
        })
    }

    pub fn generate_nullifier(
        &self,
        image: &[u8],
        scope: &str,
    ) -> anyhow::Result<[u8; 32]> {
        let embedding = self.embed(image)?;
        self.nullifier_from_embedding(&embedding, scope)
    }

    pub fn nullifier_from_embedding(
        &self,
        embedding: &PersonEmbedding,
        scope: &str,
    ) -> anyhow::Result<[u8; 32]> {
        use sha2::{Sha256, Digest};

        let quantized: Vec<i32> = embedding
            .vector
            .iter()
            .map(|&x| (x * 10.0).round() as i32)
            .collect();

        let mut hasher = Sha256::new();

        hasher.update(b"dermagraph-person-nullifier-v1");

        for val in &quantized {
            hasher.update(val.to_le_bytes());
        }

        hasher.update(scope.as_bytes());

        let result = hasher.finalize();
        let mut nullifier = [0u8; 32];
        nullifier.copy_from_slice(&result);

        Ok(nullifier)
    }

    pub fn enroll_fuzzy(
        &self,
        image: &[u8],
        scope: &str,
        password: Option<&str>,
    ) -> anyhow::Result<(HelperData, [u8; 32])> {
        let fuzzy = self.fuzzy_nullifier.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Fuzzy nullifier not initialized"))?;

        let embedding = self.embed(image)?;

        fuzzy.enroll(&embedding.vector, scope, password)
            .map_err(|e| anyhow::anyhow!("Fuzzy enrollment failed: {}", e))
    }

    pub fn enroll_fuzzy_embedding(
        &self,
        embedding: &PersonEmbedding,
        scope: &str,
        password: Option<&str>,
    ) -> anyhow::Result<(HelperData, [u8; 32])> {
        let fuzzy = self.fuzzy_nullifier.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Fuzzy nullifier not initialized"))?;

        fuzzy.enroll(&embedding.vector, scope, password)
            .map_err(|e| anyhow::anyhow!("Fuzzy enrollment failed: {}", e))
    }

    pub fn verify_fuzzy(
        &self,
        image: &[u8],
        helper_data: &HelperData,
        scope: &str,
        password: Option<&str>,
    ) -> anyhow::Result<[u8; 32]> {
        let fuzzy = self.fuzzy_nullifier.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Fuzzy nullifier not initialized"))?;

        let embedding = self.embed(image)?;

        fuzzy.verify(&embedding.vector, helper_data, scope, password)
            .map_err(|e| anyhow::anyhow!("Fuzzy verification failed: {}", e))
    }

    pub fn verify_fuzzy_embedding(
        &self,
        embedding: &PersonEmbedding,
        helper_data: &HelperData,
        scope: &str,
        password: Option<&str>,
    ) -> anyhow::Result<[u8; 32]> {
        let fuzzy = self.fuzzy_nullifier.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Fuzzy nullifier not initialized"))?;

        fuzzy.verify(&embedding.vector, helper_data, scope, password)
            .map_err(|e| anyhow::anyhow!("Fuzzy verification failed: {}", e))
    }

    pub fn has_fuzzy_nullifier(&self) -> bool {
        self.fuzzy_nullifier.is_some()
    }

    pub fn xlock_config(&self) -> Option<&XLockConfig> {
        self.fuzzy_nullifier.as_ref().map(|f| f.config())
    }

    pub fn set_threshold(&mut self, threshold: f32) {
        self.config.same_person_threshold = threshold;
    }

    pub fn threshold(&self) -> f32 {
        self.config.same_person_threshold
    }

    pub fn model(&self) -> &FingerprintEmbedder<B> {
        &self.model
    }
}

impl<B: Backend> PersonIdentifier<B> {
    pub fn embed_batch(&self, images: &[Vec<u8>]) -> anyhow::Result<Vec<PersonEmbedding>> {
        images
            .iter()
            .map(|img| self.embed(img))
            .collect()
    }

    pub fn find_duplicates(
        &self,
        images: &[Vec<u8>],
    ) -> anyhow::Result<Vec<(usize, usize, f32)>> {
        let embeddings = self.embed_batch(images)?;
        let mut duplicates = Vec::new();

        for i in 0..embeddings.len() {
            for j in (i + 1)..embeddings.len() {
                let result = self.compare_embeddings(&embeddings[i], &embeddings[j])?;
                if result.same_person {
                    duplicates.push((i, j, result.similarity));
                }
            }
        }

        duplicates.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        Ok(duplicates)
    }
}

pub struct ThresholdCalibrator {
    true_positives: Vec<f32>,
    true_negatives: Vec<f32>,
}

impl ThresholdCalibrator {
    pub fn new() -> Self {
        Self {
            true_positives: Vec::new(),
            true_negatives: Vec::new(),
        }
    }

    pub fn add_positive(&mut self, similarity: f32) {
        self.true_positives.push(similarity);
    }

    pub fn add_negative(&mut self, similarity: f32) {
        self.true_negatives.push(similarity);
    }

    pub fn optimal_threshold(&self, target_fpr: f32) -> f32 {
        if self.true_negatives.is_empty() {
            return 0.5;
        }

        let mut negatives = self.true_negatives.clone();
        negatives.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

        let target_idx = (self.true_negatives.len() as f32 * target_fpr) as usize;
        let threshold = negatives.get(target_idx).copied().unwrap_or(0.5);

        threshold
    }

    pub fn accuracy_at(&self, threshold: f32) -> (f32, f32, f32) {
        let tp = self.true_positives.iter().filter(|&&s| s > threshold).count();
        let fn_ = self.true_positives.iter().filter(|&&s| s <= threshold).count();
        let tn = self.true_negatives.iter().filter(|&&s| s <= threshold).count();
        let fp = self.true_negatives.iter().filter(|&&s| s > threshold).count();

        let tpr = tp as f32 / (tp + fn_).max(1) as f32;
        let fpr = fp as f32 / (fp + tn).max(1) as f32;
        let accuracy = (tp + tn) as f32 / (tp + fn_ + tn + fp).max(1) as f32;

        (tpr, fpr, accuracy)
    }

    pub fn roc_curve(&self, num_points: usize) -> Vec<(f32, f32, f32)> {
        let mut points = Vec::with_capacity(num_points);

        for i in 0..num_points {
            let threshold = i as f32 / (num_points - 1) as f32;
            let (tpr, fpr, _) = self.accuracy_at(threshold);
            points.push((threshold, fpr, tpr));
        }

        points
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_result() {
        let result = IdentityResult {
            same_person: true,
            confidence: 0.95,
            similarity: 0.85,
            threshold: 0.5,
        };

        assert!(result.is_confident());
        assert!(result.description().contains("Same person"));
    }

    #[test]
    fn test_nullifier_deterministic() {
        let embedding = PersonEmbedding::from_vec(vec![0.5, 0.3, -0.2, 0.8]);

        let n1 = nullifier_from_vec(&embedding.vector, "test-scope");
        let n2 = nullifier_from_vec(&embedding.vector, "test-scope");
        assert_eq!(n1, n2);

        let n3 = nullifier_from_vec(&embedding.vector, "other-scope");
        assert_ne!(n1, n3);
    }

    fn nullifier_from_vec(vec: &[f32], scope: &str) -> [u8; 32] {
        use sha2::{Sha256, Digest};

        let quantized: Vec<i32> = vec.iter().map(|&x| (x * 10.0).round() as i32).collect();

        let mut hasher = Sha256::new();
        hasher.update(b"dermagraph-person-nullifier-v1");
        for val in &quantized {
            hasher.update(val.to_le_bytes());
        }
        hasher.update(scope.as_bytes());

        let result = hasher.finalize();
        let mut nullifier = [0u8; 32];
        nullifier.copy_from_slice(&result);
        nullifier
    }

    #[test]
    fn test_threshold_calibrator() {
        let mut cal = ThresholdCalibrator::new();

        cal.add_positive(0.85);
        cal.add_positive(0.78);
        cal.add_positive(0.92);

        cal.add_negative(0.23);
        cal.add_negative(0.31);
        cal.add_negative(0.15);

        let (tpr, fpr, acc) = cal.accuracy_at(0.5);
        assert!(tpr > 0.9);
        assert!(fpr < 0.1);
        assert!(acc > 0.9);
    }
}
