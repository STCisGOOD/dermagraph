
mod error;
mod image_proc;
mod minutiae;
mod ridge_graph;
mod orientation;
mod frequency;
mod spectral;
mod quantize;

#[cfg(feature = "contrastive")]
pub mod contrastive;

pub use error::{ExtractError, Result};
pub use minutiae::{Minutia, MinutiaeSet, MinutiaeType};
pub use ridge_graph::{RidgeGraph, RidgeEdge};
pub use image_proc::FingerprintImage;
pub use orientation::OrientationField;
pub use frequency::FrequencyImage;
pub use spectral::SpectralSignature;
pub use quantize::{QuantizedSpectrum, QuantizationParams};

pub use turing_core::GraphLaplacian;
use tracing::info;

pub async fn extract_laplacian(image_data: &[u8]) -> Result<GraphLaplacian> {
    info!("Starting fingerprint feature extraction");

    let image = FingerprintImage::from_bytes(image_data)?;
    let normalized = image.normalize()?;
    let enhanced = normalized.enhance()?;

    let orientation = OrientationField::compute(&normalized)?;

    let frequency = FrequencyImage::compute(&enhanced, &orientation)?;

    let binary = enhanced.gabor_filter(&orientation, &frequency)?;
    let thinned = binary.thin()?;

    let minutiae = MinutiaeSet::extract(&thinned)?;
    info!("Extracted {} minutiae", minutiae.len());

    let graph = RidgeGraph::build(&minutiae, &thinned)?;
    info!("Built ridge graph with {} edges", graph.edge_count());

    let laplacian = graph.to_laplacian();
    info!("Computed {0}x{0} graph Laplacian", laplacian.dim());

    Ok(laplacian)
}

pub fn extract_minutiae(image_data: &[u8]) -> Result<MinutiaeSet> {
    let image = FingerprintImage::from_bytes(image_data)?;
    let normalized = image.normalize()?;
    let enhanced = normalized.enhance()?;
    let orientation = OrientationField::compute(&normalized)?;
    let frequency = FrequencyImage::compute(&enhanced, &orientation)?;
    let binary = enhanced.gabor_filter(&orientation, &frequency)?;
    let thinned = binary.thin()?;

    MinutiaeSet::extract(&thinned)
}

pub fn mock_extraction() -> (MinutiaeSet, GraphLaplacian) {
    info!("Using mock biometric data for development");

    let minutiae = MinutiaeSet::mock();

    let graph = RidgeGraph::from_minutiae(&minutiae);
    let laplacian = graph.to_laplacian();

    (minutiae, laplacian)
}

#[derive(Debug, Clone)]
pub struct BiometricData {
    pub minutiae: MinutiaeSet,

    pub graph: RidgeGraph,

    pub laplacian: GraphLaplacian,

    pub spectrum: SpectralSignature,

    pub quantized: QuantizedSpectrum,
}

impl BiometricData {
    pub async fn extract(image_data: &[u8]) -> Result<Self> {
        Self::extract_with_params(image_data, &QuantizationParams::default()).await
    }

    pub async fn extract_with_params(
        image_data: &[u8],
        quant_params: &QuantizationParams,
    ) -> Result<Self> {
        info!("Starting Spectral Turing Hash biometric extraction");

        let image = FingerprintImage::from_bytes(image_data)?;
        let normalized = image.normalize()?;
        let enhanced = normalized.enhance()?;

        let orientation = OrientationField::compute(&normalized)?;
        let frequency = FrequencyImage::compute(&enhanced, &orientation)?;

        let binary = enhanced.gabor_filter(&orientation, &frequency)?;
        let thinned = binary.thin()?;

        let minutiae = MinutiaeSet::extract(&thinned)?;
        info!("Extracted {} minutiae", minutiae.len());

        let graph = RidgeGraph::from_minutiae_with_orientation(&minutiae);
        info!("Built ridge graph with {} edges", graph.edge_count());

        let laplacian = graph.to_laplacian();

        let spectrum = SpectralSignature::from_graph(&graph)?;
        info!(
            "Computed spectral signature: {} eigenvalues, Fiedler={:.4}",
            spectrum.eigenvalues.len(),
            spectrum.fiedler_value()
        );

        let quantized = QuantizedSpectrum::from_spectrum(&spectrum, quant_params);
        let stats = quantized.stats();
        info!(
            "Quantized to {} values, entropy={:.1} bits",
            quantized.len(),
            stats.entropy_bits
        );

        Ok(Self {
            minutiae,
            graph,
            laplacian,
            spectrum,
            quantized,
        })
    }

    pub fn mock() -> Self {
        Self::mock_with_params(&QuantizationParams::default())
    }

    pub fn mock_with_params(quant_params: &QuantizationParams) -> Self {
        info!("Creating mock biometric data for STH testing");

        let minutiae = MinutiaeSet::mock();
        let graph = RidgeGraph::from_minutiae_with_orientation(&minutiae);
        let laplacian = graph.to_laplacian();
        let spectrum = SpectralSignature::from_graph(&graph)
            .expect("Mock graph should have valid spectrum");
        let quantized = QuantizedSpectrum::from_spectrum(&spectrum, quant_params);

        Self {
            minutiae,
            graph,
            laplacian,
            spectrum,
            quantized,
        }
    }

    pub async fn from_raw(data: Vec<u8>, width: u32, height: u32) -> Result<Self> {
        Self::from_raw_with_params(data, width, height, &QuantizationParams::default()).await
    }

    pub async fn from_raw_with_params(
        data: Vec<u8>,
        width: u32,
        height: u32,
        quant_params: &QuantizationParams,
    ) -> Result<Self> {
        info!("Extracting biometric data from {}x{} raw image", width, height);

        let image = FingerprintImage::from_raw(width, height, data)?;
        let normalized = image.normalize()?;
        let enhanced = normalized.enhance()?;

        let orientation = OrientationField::compute(&normalized)?;
        let frequency = FrequencyImage::compute(&enhanced, &orientation)?;

        let binary = enhanced.gabor_filter(&orientation, &frequency)?;
        let thinned = binary.thin()?;

        let minutiae = MinutiaeSet::extract(&thinned)?;
        info!("Extracted {} minutiae from raw image", minutiae.len());

        let graph = RidgeGraph::from_minutiae_with_orientation(&minutiae);
        info!("Built ridge graph with {} edges", graph.edge_count());

        let laplacian = graph.to_laplacian();

        let spectrum = SpectralSignature::from_graph(&graph)?;
        info!(
            "Computed spectral signature: {} eigenvalues, Fiedler={:.4}",
            spectrum.eigenvalues.len(),
            spectrum.fiedler_value()
        );

        let quantized = QuantizedSpectrum::from_spectrum(&spectrum, quant_params);
        let stats = quantized.stats();
        info!(
            "Quantized to {} values, entropy={:.1} bits",
            quantized.len(),
            stats.entropy_bits
        );

        Ok(Self {
            minutiae,
            graph,
            laplacian,
            spectrum,
            quantized,
        })
    }

    pub fn matches(&self, other: &BiometricData) -> bool {
        self.quantized.matches(&other.quantized)
    }

    pub fn matches_with_tolerance(&self, other: &BiometricData, tolerance: usize) -> bool {
        self.quantized.matches_with_tolerance(&other.quantized, tolerance)
    }

    pub fn to_sth_witness(&self) -> STHWitness {
        STHWitness {
            minutiae_x: self.minutiae.x_coords(),
            minutiae_y: self.minutiae.y_coords(),
            minutiae_theta: self.minutiae.orientations(),
            quantized_spectrum: self.quantized.to_field_elements(),
            num_minutiae: self.minutiae.len(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct STHWitness {
    pub minutiae_x: Vec<f64>,

    pub minutiae_y: Vec<f64>,

    pub minutiae_theta: Vec<f64>,

    pub quantized_spectrum: Vec<u64>,

    pub num_minutiae: usize,
}
