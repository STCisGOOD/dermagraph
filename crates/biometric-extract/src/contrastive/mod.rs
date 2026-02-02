
mod core_detect;
mod center_features;
mod backbone;
mod embedder;
mod loss;
mod dataset;
mod train;
mod inference;
mod fuzzy_extractor;
mod enrollment;
mod safetensors_loader;

pub use core_detect::{CorePoint, CoreType, detect_core};
pub use center_features::{CenterFeatures, extract_center_features};
pub use backbone::ResNet18;
pub use embedder::{FingerprintEmbedder, EmbedderConfig};
pub use loss::{info_nce_loss, ContrastiveBatch};
pub use dataset::{FingerprintDataset, FingerprintSample};
pub use train::{ContrastiveTrainer, TrainingConfig, TrainingState};
pub use inference::{PersonIdentifier, IdentityResult, ThresholdCalibrator};
pub use fuzzy_extractor::{
    XLockConfig, XLockError, XLockExtractor, HelperData, FuzzyNullifier,
    quantize_embedding_binary, quantize_embedding_random_projection, expand_embedding_to_bits,
    FingerType, MultiFingerHelperData,
};
pub use enrollment::{
    MultiFingerEnrollment, SimpleEnrollment, EnrollmentResult, EnrollmentConfig,
};
pub use safetensors_loader::{load_embedder_from_safetensors, load_embedder_with_config};

use burn::prelude::*;

#[derive(Debug, Clone)]
pub struct PersonEmbedding {
    pub vector: Vec<f32>,
}

impl PersonEmbedding {
    pub fn from_vec(mut v: Vec<f32>) -> Self {
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-8 {
            for x in &mut v {
                *x /= norm;
            }
        }
        Self { vector: v }
    }

    pub fn similarity(&self, other: &PersonEmbedding) -> f32 {
        self.vector
            .iter()
            .zip(other.vector.iter())
            .map(|(a, b)| a * b)
            .sum()
    }

    pub fn same_person(&self, other: &PersonEmbedding, threshold: f32) -> bool {
        self.similarity(other) > threshold
    }

    pub fn dim(&self) -> usize {
        self.vector.len()
    }
}

#[derive(Debug, Clone)]
pub struct ContrastiveConfig {
    pub embedding_dim: usize,
    pub temperature: f32,
    pub image_size: usize,
    pub use_classical_features: bool,
    pub same_person_threshold: f32,
}

impl Default for ContrastiveConfig {
    fn default() -> Self {
        Self {
            embedding_dim: 128,
            temperature: 0.07,
            image_size: 192,
            use_classical_features: true,
            same_person_threshold: 0.5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_similarity() {
        let e1 = PersonEmbedding::from_vec(vec![1.0, 0.0, 0.0]);
        let e2 = PersonEmbedding::from_vec(vec![1.0, 0.0, 0.0]);
        let e3 = PersonEmbedding::from_vec(vec![0.0, 1.0, 0.0]);

        assert!((e1.similarity(&e2) - 1.0).abs() < 1e-5);

        assert!(e1.similarity(&e3).abs() < 1e-5);
    }

    #[test]
    fn test_l2_normalization() {
        let e = PersonEmbedding::from_vec(vec![3.0, 4.0]);
        let norm: f32 = e.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }
}
