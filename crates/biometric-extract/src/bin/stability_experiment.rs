
use std::collections::HashMap;
use std::path::PathBuf;
use anyhow::{Result, Context};
use clap::Parser;

use hardware::{Sensor, SensorConfig, SensorType};
use biometric_extract::{BiometricData, QuantizationParams, QuantizedSpectrum, SpectralSignature};

#[derive(Parser, Debug)]
#[command(name = "stability_experiment")]
#[command(about = "Analyze eigenvalue stability across multiple fingerprint scans")]
struct Args {
    #[arg(short, long)]
    port: String,

    #[arg(short, long, default_value = "10")]
    scans: usize,

    #[arg(short, long, default_value = "2")]
    delay: u64,

    #[arg(short, long, default_value = "./experiment_results")]
    output: PathBuf,

    #[arg(long, default_value = "false")]
    test_deltas: bool,

    #[arg(long, default_value = "false")]
    mock: bool,
}

#[derive(Debug, Clone)]
struct ScanResult {
    scan_id: usize,
    num_minutiae: usize,
    num_edges: usize,
    raw_eigenvalues: Vec<f64>,
    quantized_default: Vec<i64>,
}

#[derive(Debug)]
struct StabilityAnalysis {
    num_scans: usize,
    eigenvalue_stats: Vec<EigenvalueStats>,
    stable_indices: Vec<usize>,
    unstable_indices: Vec<usize>,
    stability_mask: Vec<bool>,
    estimated_entropy_bits: f64,
}

#[derive(Debug, Clone)]
struct EigenvalueStats {
    position: usize,
    mean: f64,
    std_dev: f64,
    min: f64,
    max: f64,
    range: f64,
    coefficient_of_variation: f64,
    quantized_values: Vec<i64>,
    quantized_unique: usize,
    is_stable: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    let args = Args::parse();

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║     SPECTRAL EIGENVALUE STABILITY EXPERIMENT                 ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  Analyzing fingerprint eigenvalue stability for Dermagraph   ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    tokio::fs::create_dir_all(&args.output).await?;

    let config = if args.mock {
        println!("Using MOCK sensor (no hardware)");
        SensorConfig::default()
    } else {
        println!("Connecting to R503 sensor on {}...", args.port);
        SensorConfig {
            sensor_type: SensorType::R503,
            port: Some(args.port.clone()),
            baud_rate: 57600,
        }
    };

    let mut sensor = Sensor::connect(&config).await
        .context("Failed to connect to sensor")?;

    let info = sensor.info().await?;
    println!("✓ Connected: {} ({}x{} @ {}dpi)",
             info.model, info.image_width, info.image_height, info.resolution_dpi);
    println!();

    let scan_results = capture_scans(&mut sensor, args.scans, args.delay).await?;

    let analysis = analyze_stability(&scan_results);

    print_analysis(&analysis);

    if args.test_deltas {
        test_quantization_deltas(&scan_results);
    }

    save_results(&args.output, &scan_results, &analysis).await?;

    print_recommendations(&analysis);

    Ok(())
}

async fn capture_scans(
    sensor: &mut Sensor,
    num_scans: usize,
    delay_secs: u64,
) -> Result<Vec<ScanResult>> {
    let mut results = Vec::with_capacity(num_scans);

    println!("═══════════════════════════════════════════════════════════════");
    println!("  SCAN CAPTURE PHASE ({} scans requested)", num_scans);
    println!("═══════════════════════════════════════════════════════════════");
    println!();
    println!("INSTRUCTIONS:");
    println!("  1. Place your finger on the sensor");
    println!("  2. After each scan, LIFT and REPOSITION your finger");
    println!("  3. Try slight variations in position/pressure");
    println!("  4. This simulates real-world usage variance");
    println!();

    for i in 0..num_scans {
        println!("────────────────────────────────────────────────────────────────");
        println!("  Scan {}/{}: Place finger on sensor...", i + 1, num_scans);

        let image = sensor.capture().await
            .context(format!("Failed to capture scan {}", i + 1))?;

        println!("  ✓ Captured (quality: {})", image.quality);

        let biometric = BiometricData::from_raw(
            image.data,
            image.width,
            image.height,
        ).await.context("Failed to extract features")?;

        println!("  ✓ Extracted {} minutiae, {} edges",
                 biometric.minutiae.len(), biometric.graph.edge_count());

        results.push(ScanResult {
            scan_id: i,
            num_minutiae: biometric.minutiae.len(),
            num_edges: biometric.graph.edge_count(),
            raw_eigenvalues: biometric.spectrum.eigenvalues.clone(),
            quantized_default: biometric.quantized.quantized.clone(),
        });

        if i < num_scans - 1 {
            println!("  → Lift finger and reposition ({}s delay)...", delay_secs);
            tokio::time::sleep(tokio::time::Duration::from_secs(delay_secs)).await;
        }
    }

    println!();
    println!("✓ Captured {} scans successfully!", results.len());
    println!();

    Ok(results)
}

fn analyze_stability(scans: &[ScanResult]) -> StabilityAnalysis {
    if scans.is_empty() {
        return StabilityAnalysis {
            num_scans: 0,
            eigenvalue_stats: vec![],
            stable_indices: vec![],
            unstable_indices: vec![],
            stability_mask: vec![],
            estimated_entropy_bits: 0.0,
        };
    }

    let min_eigenvalues = scans.iter()
        .map(|s| s.raw_eigenvalues.len())
        .min()
        .unwrap_or(0);

    let num_positions = min_eigenvalues.saturating_sub(1).min(20);

    let mut eigenvalue_stats = Vec::with_capacity(num_positions);

    for pos in 0..num_positions {
        let values: Vec<f64> = scans.iter()
            .filter_map(|s| s.raw_eigenvalues.get(pos + 1).copied())
            .collect();

        let quantized: Vec<i64> = scans.iter()
            .filter_map(|s| s.quantized_default.get(pos).copied())
            .collect();

        let stats = compute_eigenvalue_stats(pos, &values, &quantized);
        eigenvalue_stats.push(stats);
    }

    let stability_threshold = 0.10;
    let quantized_stability_threshold = 0.8;

    let mut stable_indices = Vec::new();
    let mut unstable_indices = Vec::new();
    let mut stability_mask = Vec::new();

    for (i, stats) in eigenvalue_stats.iter().enumerate() {
        let raw_stable = stats.coefficient_of_variation < stability_threshold;

        let mut counts: HashMap<i64, usize> = HashMap::new();
        for &q in &stats.quantized_values {
            *counts.entry(q).or_insert(0) += 1;
        }
        let max_count = counts.values().max().copied().unwrap_or(0);
        let quantized_stable = max_count as f64 / scans.len() as f64 >= quantized_stability_threshold;

        let is_stable = raw_stable || quantized_stable;

        if is_stable {
            stable_indices.push(i);
            stability_mask.push(true);
        } else {
            unstable_indices.push(i);
            stability_mask.push(false);
        }
    }

    let estimated_entropy_bits = estimate_entropy(&eigenvalue_stats, &stable_indices);

    let eigenvalue_stats: Vec<EigenvalueStats> = eigenvalue_stats.into_iter()
        .enumerate()
        .map(|(i, mut s)| {
            s.is_stable = stability_mask.get(i).copied().unwrap_or(false);
            s
        })
        .collect();

    StabilityAnalysis {
        num_scans: scans.len(),
        eigenvalue_stats,
        stable_indices,
        unstable_indices,
        stability_mask,
        estimated_entropy_bits,
    }
}

fn compute_eigenvalue_stats(position: usize, values: &[f64], quantized: &[i64]) -> EigenvalueStats {
    let n = values.len() as f64;

    let mean = values.iter().sum::<f64>() / n;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
    let std_dev = variance.sqrt();

    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;

    let coefficient_of_variation = if mean.abs() > 1e-10 {
        std_dev / mean
    } else {
        f64::INFINITY
    };

    let quantized_unique = quantized.iter()
        .collect::<std::collections::HashSet<_>>()
        .len();

    EigenvalueStats {
        position,
        mean,
        std_dev,
        min,
        max,
        range,
        coefficient_of_variation,
        quantized_values: quantized.to_vec(),
        quantized_unique,
        is_stable: false,
    }
}

fn estimate_entropy(stats: &[EigenvalueStats], stable_indices: &[usize]) -> f64 {

    let mut total_entropy = 0.0;

    for &i in stable_indices {
        if let Some(stat) = stats.get(i) {
            let delta = 0.05;
            let possible_bins = (stat.range / delta).ceil().max(1.0);

            let max_entropy = possible_bins.log2();

            let effective_entropy = max_entropy * (1.0 - stat.coefficient_of_variation.min(1.0) * 0.5);

            total_entropy += effective_entropy.max(0.0);
        }
    }

    total_entropy
}

fn print_analysis(analysis: &StabilityAnalysis) {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  STABILITY ANALYSIS RESULTS");
    println!("═══════════════════════════════════════════════════════════════");
    println!();
    println!("Total scans analyzed: {}", analysis.num_scans);
    println!("Eigenvalues analyzed: {}", analysis.eigenvalue_stats.len());
    println!();

    println!("┌─────┬──────────┬──────────┬──────────┬────────┬─────────┬────────┐");
    println!("│ Pos │   Mean   │  StdDev  │   CV%    │ Range  │ Q.Uniq  │ Stable │");
    println!("├─────┼──────────┼──────────┼──────────┼────────┼─────────┼────────┤");

    for stat in &analysis.eigenvalue_stats {
        let stable_marker = if stat.is_stable { "  ✓  " } else { "  ✗  " };
        let cv_pct = stat.coefficient_of_variation * 100.0;

        println!("│ {:>3} │ {:>8.4} │ {:>8.4} │ {:>7.2}% │ {:>6.3} │   {:>2}    │{}│",
                 stat.position,
                 stat.mean,
                 stat.std_dev,
                 cv_pct,
                 stat.range,
                 stat.quantized_unique,
                 stable_marker);
    }

    println!("└─────┴──────────┴──────────┴──────────┴────────┴─────────┴────────┘");
    println!();

    println!("STABILITY SUMMARY:");
    println!("  Stable eigenvalues:   {} / {}",
             analysis.stable_indices.len(),
             analysis.eigenvalue_stats.len());
    println!("  Unstable eigenvalues: {} / {}",
             analysis.unstable_indices.len(),
             analysis.eigenvalue_stats.len());
    println!();

    println!("  Stable positions:   {:?}", analysis.stable_indices);
    println!("  Unstable positions: {:?}", analysis.unstable_indices);
    println!();

    println!("  Stability mask: {:?}",
             analysis.stability_mask.iter()
                 .map(|&b| if b { '1' } else { '0' })
                 .collect::<String>());
    println!();

    println!("ENTROPY ESTIMATE:");
    println!("  From stable eigenvalues: {:.1} bits", analysis.estimated_entropy_bits);
    println!("  Security level: {}",
             if analysis.estimated_entropy_bits >= 80.0 { "STRONG (80+ bits)" }
             else if analysis.estimated_entropy_bits >= 50.0 { "MODERATE (50-80 bits)" }
             else { "WEAK (<50 bits) - ADD PASSPHRASE" });
    println!();
}

fn test_quantization_deltas(scans: &[ScanResult]) {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  QUANTIZATION PARAMETER SWEEP");
    println!("═══════════════════════════════════════════════════════════════");
    println!();

    let deltas = [0.025, 0.05, 0.075, 0.1, 0.15, 0.2];

    println!("Testing delta values: {:?}", deltas);
    println!();

    println!("┌─────────┬───────────┬───────────┬───────────┬───────────────┐");
    println!("│  Delta  │ Stable EV │ Match Rate│  Entropy  │ Recommendation│");
    println!("├─────────┼───────────┼───────────┼───────────┼───────────────┤");

    for delta in deltas {
        let params = QuantizationParams {
            delta,
            num_eigenvalues: 16,
            min_eigenvalue: 0.01,
            max_eigenvalue: 2.0,
        };

        let mut match_count = 0;
        let mut total_comparisons = 0;

        let mut all_quantized: Vec<Vec<i64>> = Vec::new();

        for scan in scans {
            let quantized: Vec<i64> = scan.raw_eigenvalues.iter()
                .skip(1)
                .take(params.num_eigenvalues)
                .filter(|&&v| v >= params.min_eigenvalue && v <= params.max_eigenvalue)
                .map(|&v| (v / delta).floor() as i64)
                .collect();

            all_quantized.push(quantized);
        }

        for i in 0..all_quantized.len() {
            for j in (i + 1)..all_quantized.len() {
                let matches = all_quantized[i].iter()
                    .zip(all_quantized[j].iter())
                    .filter(|(a, b)| a == b)
                    .count();

                let len = all_quantized[i].len().min(all_quantized[j].len());
                if len > 0 {
                    match_count += matches;
                    total_comparisons += len;
                }
            }
        }

        let match_rate = if total_comparisons > 0 {
            match_count as f64 / total_comparisons as f64 * 100.0
        } else {
            0.0
        };

        let num_positions = all_quantized.first().map(|v| v.len()).unwrap_or(0);
        let mut stable_count = 0;

        for pos in 0..num_positions {
            let values: Vec<i64> = all_quantized.iter()
                .filter_map(|q| q.get(pos).copied())
                .collect();

            let mut counts: HashMap<i64, usize> = HashMap::new();
            for v in &values {
                *counts.entry(*v).or_insert(0) += 1;
            }

            let max_count = counts.values().max().copied().unwrap_or(0);
            if max_count as f64 / scans.len() as f64 >= 0.8 {
                stable_count += 1;
            }
        }

        let max_bin = (2.0 / delta).ceil() as usize;
        let entropy_per_stable = (max_bin as f64).log2();
        let estimated_entropy = stable_count as f64 * entropy_per_stable * 0.7;

        let recommendation = if match_rate > 90.0 && estimated_entropy > 50.0 {
            "★ OPTIMAL ★"
        } else if match_rate > 80.0 && estimated_entropy > 40.0 {
            "  Good   "
        } else if match_rate > 70.0 {
            " Usable  "
        } else {
            "  Poor   "
        };

        println!("│  {:>5.3}  │    {:>2}     │   {:>5.1}%  │   {:>5.1}   │{}│",
                 delta, stable_count, match_rate, estimated_entropy, recommendation);
    }

    println!("└─────────┴───────────┴───────────┴───────────┴───────────────┘");
    println!();
}

async fn save_results(
    output_dir: &PathBuf,
    scans: &[ScanResult],
    analysis: &StabilityAnalysis,
) -> Result<()> {
    use std::io::Write;

    let raw_data: Vec<serde_json::Value> = scans.iter()
        .map(|s| serde_json::json!({
            "scan_id": s.scan_id,
            "num_minutiae": s.num_minutiae,
            "num_edges": s.num_edges,
            "raw_eigenvalues": s.raw_eigenvalues,
            "quantized_default": s.quantized_default,
        }))
        .collect();

    let json_path = output_dir.join("scan_data.json");
    let json_content = serde_json::to_string_pretty(&raw_data)?;
    tokio::fs::write(&json_path, json_content).await?;
    println!("✓ Raw data saved to: {:?}", json_path);

    let summary_path = output_dir.join("analysis_summary.txt");
    let mut summary = String::new();

    summary.push_str("STABILITY EXPERIMENT RESULTS\n");
    summary.push_str("============================\n\n");
    summary.push_str(&format!("Scans: {}\n", analysis.num_scans));
    summary.push_str(&format!("Stable eigenvalues: {:?}\n", analysis.stable_indices));
    summary.push_str(&format!("Stability mask: {:?}\n", analysis.stability_mask));
    summary.push_str(&format!("Estimated entropy: {:.1} bits\n", analysis.estimated_entropy_bits));

    tokio::fs::write(&summary_path, summary).await?;
    println!("✓ Analysis saved to: {:?}", summary_path);

    let csv_path = output_dir.join("eigenvalues.csv");
    let mut csv_content = String::new();

    csv_content.push_str("scan_id");
    for i in 0..20 {
        csv_content.push_str(&format!(",lambda_{}", i + 1));
    }
    csv_content.push('\n');

    for scan in scans {
        csv_content.push_str(&format!("{}", scan.scan_id));
        for i in 0..20 {
            let value = scan.raw_eigenvalues.get(i + 1).copied().unwrap_or(0.0);
            csv_content.push_str(&format!(",{:.6}", value));
        }
        csv_content.push('\n');
    }

    tokio::fs::write(&csv_path, csv_content).await?;
    println!("✓ CSV data saved to: {:?}", csv_path);

    Ok(())
}

fn print_recommendations(analysis: &StabilityAnalysis) {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  RECOMMENDATIONS");
    println!("═══════════════════════════════════════════════════════════════");
    println!();

    let stable_count = analysis.stable_indices.len();
    let total_count = analysis.eigenvalue_stats.len();
    let stability_rate = stable_count as f64 / total_count as f64 * 100.0;

    if stability_rate >= 70.0 {
        println!("✓ GOOD: {}% eigenvalues are stable", stability_rate as u32);
        println!("  → Stable Core Hash approach is viable");
        println!();
    } else if stability_rate >= 50.0 {
        println!("⚠ MODERATE: Only {}% eigenvalues are stable", stability_rate as u32);
        println!("  → Consider wider quantization bins (delta=0.1)");
        println!("  → Or use fuzzy commitment with BCH codes");
        println!();
    } else {
        println!("✗ POOR: Only {}% eigenvalues are stable", stability_rate as u32);
        println!("  → Recommend fuzzy commitment architecture");
        println!("  → Consider alternative feature extraction");
        println!();
    }

    if analysis.estimated_entropy_bits >= 80.0 {
        println!("✓ ENTROPY: {:.0} bits is STRONG", analysis.estimated_entropy_bits);
        println!("  → Sufficient for standalone biometric authentication");
    } else if analysis.estimated_entropy_bits >= 50.0 {
        println!("⚠ ENTROPY: {:.0} bits is MODERATE", analysis.estimated_entropy_bits);
        println!("  → Recommend adding passphrase for production use");
    } else {
        println!("✗ ENTROPY: {:.0} bits is WEAK", analysis.estimated_entropy_bits);
        println!("  → MUST add passphrase for any security application");
    }

    println!();
    println!("SUGGESTED PARAMETERS:");
    println!("  stability_mask = {:?}",
             analysis.stability_mask.iter()
                 .map(|&b| if b { 1u8 } else { 0u8 })
                 .collect::<Vec<_>>());
    println!("  num_stable_eigenvalues = {}", stable_count);
    println!("  stable_positions = {:?}", analysis.stable_indices);
    println!();
}
