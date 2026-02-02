
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use biometric_extract::contrastive::{
    ContrastiveTrainer, TrainingConfig, FingerprintDataset,
    PersonIdentifier,
};

use burn_ndarray::{NdArray, NdArrayDevice};

#[cfg(feature = "libtorch")]
use burn_tch::{LibTorch, LibTorchDevice};

#[cfg(feature = "gpu")]
use burn_wgpu::{Wgpu, WgpuDevice};

#[cfg(feature = "libtorch")]
type TrainBackend = burn::backend::Autodiff<LibTorch>;

#[cfg(all(feature = "gpu", not(feature = "libtorch")))]
type TrainBackend = burn::backend::Autodiff<Wgpu>;

#[cfg(all(not(feature = "gpu"), not(feature = "libtorch")))]
type TrainBackend = burn::backend::Autodiff<NdArray<f32>>;

type InferenceBackend = NdArray<f32>;

#[derive(Parser)]
#[command(name = "train-embedder")]
#[command(about = "Train person-level fingerprint embedding model")]
struct Cli {
    #[arg(short, long)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Train {
        #[arg(short, long)]
        dataset: PathBuf,

        #[arg(short, long, default_value = "./checkpoints")]
        output: PathBuf,

        #[arg(short, long, default_value = "100")]
        epochs: usize,

        #[arg(short, long, default_value = "32")]
        batch_size: usize,

        #[arg(long, default_value = "0.001")]
        lr: f64,

        #[arg(long, default_value = "0.2")]
        val_split: f64,

        #[arg(long, default_value = "15")]
        patience: usize,

        #[arg(long, default_value = "42")]
        seed: u64,
    },

    Evaluate {
        #[arg(short, long)]
        model: PathBuf,

        #[arg(short, long)]
        dataset: PathBuf,

        #[arg(long, default_value = "0.5")]
        threshold: f32,
    },

    Calibrate {
        #[arg(short, long)]
        model: PathBuf,

        #[arg(short, long)]
        dataset: PathBuf,

        #[arg(long, default_value = "0.01")]
        target_fpr: f32,
    },

    Compare {
        #[arg(short, long)]
        model: PathBuf,

        #[arg(long)]
        image1: PathBuf,

        #[arg(long)]
        image2: PathBuf,
    },

    Stats {
        #[arg(short, long)]
        dataset: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let level = if cli.verbose { Level::DEBUG } else { Level::INFO };
    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_target(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    match cli.command {
        Commands::Train {
            dataset,
            output,
            epochs,
            batch_size,
            lr,
            val_split,
            patience,
            seed,
        } => {
            train(dataset, output, epochs, batch_size, lr, val_split, patience, seed)?;
        }

        Commands::Evaluate {
            model,
            dataset,
            threshold,
        } => {
            evaluate(model, dataset, threshold)?;
        }

        Commands::Calibrate {
            model,
            dataset,
            target_fpr,
        } => {
            calibrate(model, dataset, target_fpr)?;
        }

        Commands::Compare {
            model,
            image1,
            image2,
        } => {
            compare(model, image1, image2)?;
        }

        Commands::Stats { dataset } => {
            show_stats(dataset)?;
        }
    }

    Ok(())
}

fn train(
    dataset_path: PathBuf,
    output: PathBuf,
    epochs: usize,
    batch_size: usize,
    lr: f64,
    val_split: f64,
    patience: usize,
    seed: u64,
) -> Result<()> {
    info!("Loading dataset from {}", dataset_path.display());

    let dataset = FingerprintDataset::from_directory(&dataset_path)?;
    let stats = dataset.stats();

    info!("Dataset loaded:");
    info!("  Total samples: {}", stats.total_samples);
    info!("  Num persons: {}", stats.num_persons);
    info!("  Avg samples/person: {:.1}", stats.avg_samples_per_person);
    info!("  Avg fingers/person: {:.1}", stats.avg_fingers_per_person);

    if stats.num_persons < 5 {
        anyhow::bail!("Need at least 5 persons for training (have {})", stats.num_persons);
    }

    let (train_indices, val_indices) = dataset.train_val_split(val_split);
    info!("Train/val split: {}/{} samples", train_indices.len(), val_indices.len());

    let config = TrainingConfig {
        epochs,
        batch_size,
        learning_rate: lr,
        patience,
        seed,
        output_dir: output.clone(),
        ..Default::default()
    };

    #[cfg(feature = "libtorch")]
    let device = LibTorchDevice::Cuda(0);

    #[cfg(all(feature = "gpu", not(feature = "libtorch")))]
    let device = WgpuDevice::default();

    #[cfg(all(not(feature = "gpu"), not(feature = "libtorch")))]
    let device = NdArrayDevice::default();

    #[cfg(feature = "libtorch")]
    info!("Using backend: LibTorch (PyTorch autodiff)");
    #[cfg(all(feature = "gpu", not(feature = "libtorch")))]
    info!("Using backend: GPU (WGPU)");
    #[cfg(all(not(feature = "gpu"), not(feature = "libtorch")))]
    info!("Using backend: CPU (NdArray)");

    let mut trainer = ContrastiveTrainer::<TrainBackend>::new(device, config);

    trainer.train(&dataset, Some(&dataset))?;

    info!("Training complete!");
    info!("Best model saved to {}/best.mpk", output.display());

    Ok(())
}

fn evaluate(model_path: PathBuf, dataset_path: PathBuf, threshold: f32) -> Result<()> {
    info!("Loading model from {}", model_path.display());
    info!("Loading test dataset from {}", dataset_path.display());

    let device = NdArrayDevice::default();
    let mut identifier = PersonIdentifier::<InferenceBackend>::load(device, &model_path)?;
    identifier.set_threshold(threshold);

    let dataset = FingerprintDataset::from_directory(&dataset_path)?;
    let stats = dataset.stats();

    info!("Test dataset: {} samples, {} persons", stats.total_samples, stats.num_persons);

    let mut rng = rand::thread_rng();
    let mut true_positives = 0;
    let mut true_negatives = 0;
    let mut false_positives = 0;
    let mut false_negatives = 0;

    let num_tests = 100;

    for _ in 0..num_tests {
        if let Some((idx1, idx2, _)) = dataset.sample_positive_pair(&mut rng) {
            let sample1 = dataset.get(idx1).unwrap();
            let sample2 = dataset.get(idx2).unwrap();

            let img1 = std::fs::read(&sample1.image_path)?;
            let img2 = std::fs::read(&sample2.image_path)?;

            let result = identifier.compare(&img1, &img2)?;

            if result.same_person {
                true_positives += 1;
            } else {
                false_negatives += 1;
            }
        }
    }

    let accuracy = true_positives as f32 / num_tests as f32;

    info!("Results:");
    info!("  Same-person accuracy: {:.1}%", accuracy * 100.0);
    info!("  Threshold: {}", threshold);

    Ok(())
}

fn calibrate(model_path: PathBuf, dataset_path: PathBuf, target_fpr: f32) -> Result<()> {
    info!("Calibrating threshold for {:.1}% FPR", target_fpr * 100.0);

    let device = NdArrayDevice::default();
    let identifier = PersonIdentifier::<InferenceBackend>::load(device, &model_path)?;

    let dataset = FingerprintDataset::from_directory(&dataset_path)?;

    let mut calibrator = biometric_extract::contrastive::ThresholdCalibrator::new();
    let mut rng = rand::thread_rng();

    info!("Collecting positive pairs...");
    for _ in 0..50 {
        if let Some((idx1, idx2, _)) = dataset.sample_positive_pair(&mut rng) {
            let sample1 = dataset.get(idx1).unwrap();
            let sample2 = dataset.get(idx2).unwrap();

            let img1 = std::fs::read(&sample1.image_path)?;
            let img2 = std::fs::read(&sample2.image_path)?;

            let emb1 = identifier.embed(&img1)?;
            let emb2 = identifier.embed(&img2)?;

            calibrator.add_positive(emb1.similarity(&emb2));
        }
    }

    info!("Note: Full calibration requires negative pairs (different persons)");

    let optimal = calibrator.optimal_threshold(target_fpr);
    let (tpr, fpr, acc) = calibrator.accuracy_at(optimal);

    info!("Optimal threshold: {:.3}", optimal);
    info!("  TPR: {:.1}%", tpr * 100.0);
    info!("  FPR: {:.1}%", fpr * 100.0);
    info!("  Accuracy: {:.1}%", acc * 100.0);

    Ok(())
}

fn compare(model_path: PathBuf, image1: PathBuf, image2: PathBuf) -> Result<()> {
    info!("Loading model...");

    let device = NdArrayDevice::default();
    let identifier = PersonIdentifier::<InferenceBackend>::load(device, &model_path)?;

    info!("Loading images...");
    let img1 = std::fs::read(&image1)?;
    let img2 = std::fs::read(&image2)?;

    info!("Computing embeddings...");
    let result = identifier.compare(&img1, &img2)?;

    println!();
    println!("═══ Comparison Result ═══");
    println!();
    println!("Image 1: {}", image1.display());
    println!("Image 2: {}", image2.display());
    println!();
    println!("Similarity: {:.3}", result.similarity);
    println!("Threshold:  {:.3}", result.threshold);
    println!();
    println!("{}", result.description());

    Ok(())
}

fn show_stats(dataset_path: PathBuf) -> Result<()> {
    let dataset = FingerprintDataset::from_directory(&dataset_path)?;
    let stats = dataset.stats();

    println!();
    println!("═══ Dataset Statistics ═══");
    println!();
    println!("Total samples:        {}", stats.total_samples);
    println!("Number of persons:    {}", stats.num_persons);
    println!("Avg samples/person:   {:.1}", stats.avg_samples_per_person);
    println!("Avg fingers/person:   {:.1}", stats.avg_fingers_per_person);
    println!("Min samples/person:   {}", stats.min_samples_per_person);
    println!("Max samples/person:   {}", stats.max_samples_per_person);

    println!();
    if stats.num_persons < 10 {
        println!("⚠ Warning: Recommend at least 10 persons for reliable training");
    }
    if stats.avg_fingers_per_person < 2.0 {
        println!("⚠ Warning: Need multiple fingers per person for contrastive learning");
    }
    if stats.avg_samples_per_person < 10.0 {
        println!("⚠ Warning: Recommend at least 10 samples per person");
    }

    Ok(())
}
