
use anyhow::{Context, Result};
use burn::prelude::*;
use burn_ndarray::NdArray;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use biometric_extract::contrastive::{
    FingerprintEmbedder,
    load_embedder_from_safetensors,
    PersonEmbedding,
    CenterFeatures,
};

pub type InferenceBackend = NdArray;

pub struct EmbedderModel {
    model: FingerprintEmbedder<InferenceBackend>,
    device: <InferenceBackend as burn::prelude::Backend>::Device,
}

impl EmbedderModel {
    pub fn load(weights_path: &Path) -> Result<Self> {
        info!("Loading cross-finger CNN model from: {}", weights_path.display());

        let device = Default::default();
        let model = load_embedder_from_safetensors::<InferenceBackend>(weights_path, &device)
            .context("Failed to load FingerprintEmbedder weights")?;

        info!("Cross-finger CNN model loaded successfully (128-dim embeddings)");

        Ok(Self { model, device })
    }

    pub fn embed(&self, image_data: &[u8], width: u32, height: u32) -> Result<PersonEmbedding> {
        if width != 192 || height != 192 {
            anyhow::bail!("Expected 192x192 image, got {}x{}", width, height);
        }

        let image_f32: Vec<f32> = image_data.iter()
            .map(|&b| b as f32 / 255.0)
            .collect();

        let image_tensor = Tensor::<InferenceBackend, 1>::from_floats(
            image_f32.as_slice(),
            &self.device,
        ).reshape([1, 1, 192, 192]);

        let center_features = extract_center_features_from_image(image_data, width, height);
        let classical_tensor = Tensor::<InferenceBackend, 1>::from_floats(
            center_features.as_slice(),
            &self.device,
        ).reshape([1, CenterFeatures::DIM]);

        let embedding_tensor = self.model.forward(image_tensor, classical_tensor);

        let embedding_vec: Vec<f32> = embedding_tensor
            .into_data()
            .to_vec()
            .context("Failed to extract embedding data")?;

        Ok(PersonEmbedding::from_vec(embedding_vec))
    }
}

fn extract_center_features_from_image(_image_data: &[u8], _width: u32, _height: u32) -> Vec<f32> {
    warn!("Using zero classical features (demo mode)");
    vec![0.0; CenterFeatures::DIM]
}

#[derive(Clone)]
pub struct ModelState {
    weights_path: Option<std::path::PathBuf>,
    weights_valid: bool,
}

impl ModelState {
    pub fn new() -> Self {
        Self {
            weights_path: None,
            weights_valid: false,
        }
    }

    pub fn with_weights_path(weights_path: impl AsRef<Path>) -> Self {
        let path = weights_path.as_ref().to_path_buf();
        let valid = path.exists();
        Self {
            weights_path: Some(path),
            weights_valid: valid,
        }
    }

    pub fn is_available(&self) -> bool {
        self.weights_valid
    }

    pub fn is_loaded(&self) -> bool {
        self.weights_valid
    }

    pub fn weights_path(&self) -> Option<&std::path::PathBuf> {
        self.weights_path.as_ref()
    }

    pub fn embedder(&self) -> Result<EmbedderModel> {
        let path = self.weights_path.as_ref()
            .context("No weights path configured")?;
        EmbedderModel::load(path)
    }

    pub fn embed(&self, image_data: &[u8], width: u32, height: u32) -> Result<PersonEmbedding> {
        let embedder = self.embedder()?;
        embedder.embed(image_data, width, height)
    }
}

impl Default for ModelState {
    fn default() -> Self {
        Self::new()
    }
}

pub type SharedModelState = Arc<RwLock<ModelState>>;

pub fn create_shared_model_state(weights_path: Option<impl AsRef<Path>>) -> SharedModelState {
    let state = match weights_path {
        Some(path) => ModelState::with_weights_path(path),
        None => ModelState::new(),
    };
    Arc::new(RwLock::new(state))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_state_creation() {
        let state = ModelState::new();
        assert!(!state.is_loaded());
    }
}
