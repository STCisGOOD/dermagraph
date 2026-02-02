
use burn::optim::{AdamWConfig, GradientsParams, Optimizer};
use burn::prelude::*;
use burn::module::AutodiffModule;
use burn::record::{FullPrecisionSettings, Recorder};
use burn::tensor::backend::AutodiffBackend;

use std::path::{Path, PathBuf};
use rand::SeedableRng;
use tracing::{info, warn};

use super::dataset::{FingerprintDataset, FingerprintSample, load_fingerprint_image};
use super::embedder::{FingerprintEmbedder, EmbedderConfig};
use super::center_features::{CenterFeatures, extract_center_features};
use super::core_detect::detect_core;
use super::loss::{info_nce_loss, contrastive_accuracy};
use crate::orientation::OrientationField;
use crate::image_proc::FingerprintImage;

#[derive(Debug, Clone)]
pub struct TrainingConfig {
    pub epochs: usize,

    pub batch_size: usize,

    pub learning_rate: f64,

    pub weight_decay: f64,

    pub temperature: f32,

    pub min_lr: f64,

    pub warmup_epochs: usize,

    pub val_every: usize,

    pub checkpoint_every: usize,

    pub output_dir: PathBuf,

    pub patience: usize,

    pub image_size: (u32, u32),

    pub seed: u64,
}

impl Default for TrainingConfig {
    fn default() -> Self {
        Self {
            epochs: 100,
            batch_size: 32,
            learning_rate: 1e-3,
            weight_decay: 1e-4,
            temperature: 0.07,
            min_lr: 1e-6,
            warmup_epochs: 5,
            val_every: 5,
            checkpoint_every: 10,
            output_dir: PathBuf::from("./checkpoints"),
            patience: 15,
            image_size: (192, 192),
            seed: 42,
        }
    }
}

#[derive(Debug)]
pub struct TrainingState {
    pub epoch: usize,

    pub best_val_acc: f32,

    pub epochs_without_improvement: usize,

    pub train_losses: Vec<f32>,

    pub val_accuracies: Vec<f32>,
}

impl TrainingState {
    pub fn new() -> Self {
        Self {
            epoch: 0,
            best_val_acc: 0.0,
            epochs_without_improvement: 0,
            train_losses: Vec::new(),
            val_accuracies: Vec::new(),
        }
    }
}

pub struct ContrastiveTrainer<B: AutodiffBackend> {
    model: FingerprintEmbedder<B>,

    optimizer_config: AdamWConfig,

    config: TrainingConfig,

    state: TrainingState,

    device: B::Device,
}

impl<B: AutodiffBackend> ContrastiveTrainer<B> {
    pub fn new(device: B::Device, config: TrainingConfig) -> Self {
        let embedder_config = EmbedderConfig::default();
        let model = FingerprintEmbedder::new(&device, embedder_config);

        let optimizer_config = AdamWConfig::new()
            .with_weight_decay(config.weight_decay as f32);

        Self {
            model,
            optimizer_config,
            config,
            state: TrainingState::new(),
            device,
        }
    }

    pub fn train(
        &mut self,
        train_dataset: &FingerprintDataset,
        val_dataset: Option<&FingerprintDataset>,
    ) -> anyhow::Result<()> {
        info!("Starting training for {} epochs", self.config.epochs);
        info!("Training samples: {}", train_dataset.len());
        if let Some(val) = val_dataset {
            info!("Validation samples: {}", val.len());
        }

        std::fs::create_dir_all(&self.config.output_dir)?;

        let mut rng = rand::rngs::StdRng::seed_from_u64(self.config.seed);

        let mut optim = self.optimizer_config.clone().init();

        for epoch in 0..self.config.epochs {
            self.state.epoch = epoch;

            let train_loss = self.train_epoch(train_dataset, &mut rng, &mut optim)?;
            self.state.train_losses.push(train_loss);

            info!("Epoch {}/{}: train_loss = {:.4}", epoch + 1, self.config.epochs, train_loss);

            if (epoch + 1) % self.config.val_every == 0 {
                if let Some(val_ds) = val_dataset {
                    let val_acc = self.validate(val_ds, &mut rng)?;
                    self.state.val_accuracies.push(val_acc);

                    info!("Epoch {}/{}: val_acc = {:.2}%", epoch + 1, self.config.epochs, val_acc * 100.0);

                    if val_acc > self.state.best_val_acc {
                        self.state.best_val_acc = val_acc;
                        self.state.epochs_without_improvement = 0;

                        self.save_checkpoint("best")?;
                        info!("New best model saved! (acc = {:.2}%)", val_acc * 100.0);
                    } else {
                        self.state.epochs_without_improvement += 1;
                    }

                    if self.state.epochs_without_improvement >= self.config.patience {
                        warn!("Early stopping triggered after {} epochs without improvement", self.config.patience);
                        break;
                    }
                }
            }

            if (epoch + 1) % self.config.checkpoint_every == 0 {
                self.save_checkpoint(&format!("epoch_{}", epoch + 1))?;
            }

            let lr = self.compute_lr(epoch);
            if (epoch + 1) % 10 == 0 {
                info!("Current learning rate: {:.6}", lr);
            }
        }

        self.save_checkpoint("final")?;

        info!("Training complete!");
        info!("Best validation accuracy: {:.2}%", self.state.best_val_acc * 100.0);

        Ok(())
    }

    fn train_epoch<R: rand::Rng, O: Optimizer<FingerprintEmbedder<B>, B>>(
        &mut self,
        dataset: &FingerprintDataset,
        rng: &mut R,
        optim: &mut O,
    ) -> anyhow::Result<f32> {
        let num_batches = (dataset.len() / self.config.batch_size).max(1);
        let mut total_loss = 0.0;
        let mut valid_batches = 0;

        for batch_idx in 0..num_batches {
            let pairs = dataset.sample_batch(self.config.batch_size, rng);

            if pairs.len() < 2 {
                warn!("Batch {} has only {} pairs, skipping", batch_idx, pairs.len());
                continue;
            }

            let (anchor_images, anchor_classical, positive_images, positive_classical) =
                match self.prepare_batch(&pairs, dataset) {
                    Ok(batch) => batch,
                    Err(e) => {
                        warn!("Failed to prepare batch {}: {}", batch_idx, e);
                        continue;
                    }
                };

            if batch_idx == 0 {
                info!("First batch shapes:");
                info!("  anchor_images: {:?}", anchor_images.dims());
                info!("  anchor_classical: {:?}", anchor_classical.dims());
            }

            if batch_idx == 0 {
                info!("Running forward pass for anchor...");
            }
            let anchor_embeddings = self.model.forward(anchor_images.clone(), anchor_classical.clone());
            if batch_idx == 0 {
                info!("  anchor_embeddings: {:?}", anchor_embeddings.dims());
                info!("Running forward pass for positive...");
            }
            let positive_embeddings = self.model.forward(positive_images, positive_classical);
            if batch_idx == 0 {
                info!("  positive_embeddings: {:?}", positive_embeddings.dims());
                info!("[DEBUG] Computing loss...");
            }

            let loss = info_nce_loss(
                anchor_embeddings,
                positive_embeddings,
                self.config.temperature,
            );
            if batch_idx == 0 {
                info!("[DEBUG] Loss tensor created");
            }

            let loss_val: f32 = loss.clone().into_scalar().elem();
            if batch_idx == 0 {
                info!("[DEBUG] Loss value extracted: {:.4}", loss_val);
            }

            if batch_idx == 0 {
                info!("[DEBUG] Starting backward pass...");
            }
            let grads = loss.backward();
            if batch_idx == 0 {
                info!("[DEBUG] Backward pass complete");
            }
            let grads = GradientsParams::from_grads(grads, &self.model);
            if batch_idx == 0 {
                info!("[DEBUG] Gradients extracted");
            }

            let lr = self.compute_lr(self.state.epoch);
            self.model = optim.step(lr, self.model.clone(), grads);
            if batch_idx == 0 {
                info!("[DEBUG] Optimizer step complete");
            }

            total_loss += loss_val;
            valid_batches += 1;

            if (batch_idx + 1) % 10 == 0 {
                info!(
                    "  Batch {}/{}: loss = {:.4}",
                    batch_idx + 1,
                    num_batches,
                    loss_val
                );
            }
        }

        Ok(total_loss / valid_batches.max(1) as f32)
    }

    fn validate<R: rand::Rng>(
        &self,
        dataset: &FingerprintDataset,
        rng: &mut R,
    ) -> anyhow::Result<f32> {
        let num_batches = (dataset.len() / self.config.batch_size).max(1);
        let mut total_acc = 0.0;

        let model_valid = self.model.valid();

        for _ in 0..num_batches {
            let pairs = dataset.sample_batch(self.config.batch_size, rng);

            if pairs.len() < 2 {
                continue;
            }

            let (anchor_images, anchor_classical, positive_images, positive_classical) =
                self.prepare_batch(&pairs, dataset)?;

            let anchor_embeddings = model_valid.forward(anchor_images.inner(), anchor_classical.inner());
            let positive_embeddings = model_valid.forward(positive_images.inner(), positive_classical.inner());

            let acc = contrastive_accuracy(
                anchor_embeddings,
                positive_embeddings,
            );
            total_acc += acc;
        }

        Ok(total_acc / num_batches as f32)
    }

    fn prepare_batch(
        &self,
        pairs: &[(usize, usize, String)],
        dataset: &FingerprintDataset,
    ) -> anyhow::Result<(Tensor<B, 4>, Tensor<B, 2>, Tensor<B, 4>, Tensor<B, 2>)> {
        let batch_size = pairs.len();
        if batch_size == 0 {
            anyhow::bail!("Empty batch");
        }

        let (w, h) = self.config.image_size;
        let img_size = (w * h) as usize;

        let mut anchor_pixels = Vec::with_capacity(batch_size * img_size);
        let mut positive_pixels = Vec::with_capacity(batch_size * img_size);
        let mut anchor_feats = Vec::with_capacity(batch_size * CenterFeatures::DIM);
        let mut positive_feats = Vec::with_capacity(batch_size * CenterFeatures::DIM);

        for (i, (anchor_idx, positive_idx, _)) in pairs.iter().enumerate() {
            let anchor_sample = dataset.get(*anchor_idx)
                .ok_or_else(|| anyhow::anyhow!("Invalid anchor index: {}", anchor_idx))?;
            let positive_sample = dataset.get(*positive_idx)
                .ok_or_else(|| anyhow::anyhow!("Invalid positive index: {}", positive_idx))?;

            let anchor_img = load_fingerprint_image(anchor_sample, self.config.image_size)
                .map_err(|e| anyhow::anyhow!("Failed to load anchor image {}: {}", anchor_sample.image_path.display(), e))?;
            let positive_img = load_fingerprint_image(positive_sample, self.config.image_size)
                .map_err(|e| anyhow::anyhow!("Failed to load positive image {}: {}", positive_sample.image_path.display(), e))?;

            if anchor_img.len() != img_size {
                anyhow::bail!("Anchor image {} has wrong size: {} vs expected {}", i, anchor_img.len(), img_size);
            }
            if positive_img.len() != img_size {
                anyhow::bail!("Positive image {} has wrong size: {} vs expected {}", i, positive_img.len(), img_size);
            }

            let anchor_classical = extract_classical_features(&anchor_img, w, h)?;
            let positive_classical = extract_classical_features(&positive_img, w, h)?;

            anchor_pixels.extend(anchor_img);
            positive_pixels.extend(positive_img);
            anchor_feats.extend(anchor_classical.to_vector());
            positive_feats.extend(positive_classical.to_vector());
        }

        let anchor_images: Tensor<B, 4> = Tensor::<B, 1>::from_floats(anchor_pixels.as_slice(), &self.device)
            .reshape([batch_size, 1, h as usize, w as usize]);

        let positive_images: Tensor<B, 4> = Tensor::<B, 1>::from_floats(positive_pixels.as_slice(), &self.device)
            .reshape([batch_size, 1, h as usize, w as usize]);

        let anchor_classical: Tensor<B, 2> = Tensor::<B, 1>::from_floats(anchor_feats.as_slice(), &self.device)
            .reshape([batch_size, CenterFeatures::DIM]);

        let positive_classical: Tensor<B, 2> = Tensor::<B, 1>::from_floats(positive_feats.as_slice(), &self.device)
            .reshape([batch_size, CenterFeatures::DIM]);

        Ok((anchor_images, anchor_classical, positive_images, positive_classical))
    }

    fn compute_lr(&self, epoch: usize) -> f64 {
        if epoch < self.config.warmup_epochs {
            let warmup_factor = (epoch + 1) as f64 / self.config.warmup_epochs as f64;
            self.config.learning_rate * warmup_factor
        } else {
            let progress = (epoch - self.config.warmup_epochs) as f64
                / (self.config.epochs - self.config.warmup_epochs) as f64;
            let cosine_factor = 0.5 * (1.0 + (std::f64::consts::PI * progress).cos());
            self.config.min_lr + (self.config.learning_rate - self.config.min_lr) * cosine_factor
        }
    }

    fn save_checkpoint(&self, name: &str) -> anyhow::Result<()> {
        let path = self.config.output_dir.join(name);

        burn::record::DefaultFileRecorder::<FullPrecisionSettings>::new()
            .record(self.model.clone().into_record(), path.clone())
            .map_err(|e| anyhow::anyhow!("Failed to save checkpoint: {:?}", e))?;

        info!("Saved checkpoint to {}", path.display());
        Ok(())
    }

    pub fn load_checkpoint(&mut self, path: &Path) -> anyhow::Result<()> {
        let record = burn::record::DefaultFileRecorder::<FullPrecisionSettings>::new()
            .load(path.to_path_buf(), &self.device)
            .map_err(|e| anyhow::anyhow!("Failed to load checkpoint: {:?}", e))?;

        self.model = self.model.clone().load_record(record);
        info!("Loaded checkpoint from {}", path.display());
        Ok(())
    }

    pub fn into_model(self) -> FingerprintEmbedder<B> {
        self.model
    }

    pub fn model(&self) -> &FingerprintEmbedder<B> {
        &self.model
    }

    pub fn state(&self) -> &TrainingState {
        &self.state
    }
}

fn extract_classical_features(
    pixels: &[f32],
    width: u32,
    height: u32,
) -> anyhow::Result<CenterFeatures> {
    let bytes: Vec<u8> = pixels.iter().map(|&p| (p * 255.0) as u8).collect();

    let image = FingerprintImage::from_raw(width, height, bytes)?;
    let normalized = image.normalize()?;

    let orientation = OrientationField::compute(&normalized)?;

    let cores = detect_core(&orientation, orientation.block_size());

    let features = extract_center_features(&orientation, None, &cores);

    Ok(features)
}

pub fn quick_train_test<B: AutodiffBackend>(device: B::Device) -> anyhow::Result<()> {
    info!("Running quick training test...");

    let config = TrainingConfig {
        epochs: 2,
        batch_size: 4,
        val_every: 1,
        checkpoint_every: 1,
        output_dir: PathBuf::from("./test_checkpoints"),
        ..Default::default()
    };

    let samples: Vec<FingerprintSample> = (0..20)
        .flat_map(|person| {
            (0..5).map(move |finger| FingerprintSample {
                image_path: PathBuf::from(format!("test/p{}/f{}.png", person, finger)),
                person_id: format!("person_{}", person),
                finger_id: format!("finger_{}", finger),
                session: 1,
                width: 192,
                height: 192,
            })
        })
        .collect();

    let dataset = FingerprintDataset::from_samples(samples)?;
    let (train_idx, val_idx) = dataset.train_val_split(0.2);

    info!("Test dataset: {} train, {} val", train_idx.len(), val_idx.len());

    info!("Quick test setup complete (actual training requires real images)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lr_schedule() {
        let config = TrainingConfig {
            epochs: 100,
            warmup_epochs: 5,
            learning_rate: 1e-3,
            min_lr: 1e-6,
            ..Default::default()
        };

        assert!((compute_lr_test(&config, 0) - 0.0002).abs() < 1e-6);
        assert!((compute_lr_test(&config, 4) - 0.001).abs() < 1e-6);

        let lr_start = compute_lr_test(&config, 5);
        let lr_mid = compute_lr_test(&config, 52);
        let lr_end = compute_lr_test(&config, 99);

        assert!(lr_start > lr_mid);
        assert!(lr_mid > lr_end);
        assert!(lr_end > config.min_lr * 0.9);
    }

    fn compute_lr_test(config: &TrainingConfig, epoch: usize) -> f64 {
        if epoch < config.warmup_epochs {
            config.learning_rate * (epoch + 1) as f64 / config.warmup_epochs as f64
        } else {
            let progress = (epoch - config.warmup_epochs) as f64
                / (config.epochs - config.warmup_epochs) as f64;
            let cosine = 0.5 * (1.0 + (std::f64::consts::PI * progress).cos());
            config.min_lr + (config.learning_rate - config.min_lr) * cosine
        }
    }

    #[test]
    fn test_training_state() {
        let mut state = TrainingState::new();
        assert_eq!(state.epoch, 0);
        assert_eq!(state.best_val_acc, 0.0);

        state.epoch = 10;
        state.best_val_acc = 0.85;
        state.train_losses.push(0.5);

        assert_eq!(state.epoch, 10);
        assert_eq!(state.train_losses.len(), 1);
    }
}
