
use crate::error::Result;
use crate::spectral::SpectralSignature;
use tracing::{debug, info};

#[derive(Debug, Clone)]
pub struct QuantizationParams {
    pub delta: f64,

    pub num_eigenvalues: usize,

    pub min_eigenvalue: f64,

    pub max_eigenvalue: f64,
}

impl Default for QuantizationParams {
    fn default() -> Self {
        Self {
            delta: 0.05,

            num_eigenvalues: 16,

            min_eigenvalue: 0.01,

            max_eigenvalue: 2.0,
        }
    }
}

impl QuantizationParams {
    pub fn high_security() -> Self {
        Self {
            delta: 0.025,
            num_eigenvalues: 20,
            min_eigenvalue: 0.005,
            max_eigenvalue: 2.0,
        }
    }

    pub fn high_tolerance() -> Self {
        Self {
            delta: 0.1,
            num_eigenvalues: 12,
            min_eigenvalue: 0.02,
            max_eigenvalue: 2.0,
        }
    }

    pub fn zk_optimized() -> Self {
        Self {
            delta: 0.05,
            num_eigenvalues: 8,
            min_eigenvalue: 0.01,
            max_eigenvalue: 2.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuantizedSpectrum {
    pub quantized: Vec<i64>,

    pub delta_scaled: u64,

    pub original_count: usize,
}

impl QuantizedSpectrum {
    pub fn from_values(values: Vec<u64>) -> Self {
        let offset = 100i64;
        let quantized: Vec<i64> = values
            .into_iter()
            .map(|v| v as i64 - offset)
            .collect();

        Self {
            quantized,
            delta_scaled: 50,
            original_count: 0,
        }
    }

    pub fn from_spectrum(spectrum: &SpectralSignature, params: &QuantizationParams) -> Self {
        let mut quantized = Vec::with_capacity(params.num_eigenvalues);

        for &eigenvalue in spectrum.eigenvalues.iter().skip(1).take(params.num_eigenvalues) {
            if eigenvalue >= params.min_eigenvalue && eigenvalue <= params.max_eigenvalue {
                let q = (eigenvalue / params.delta).floor() as i64;
                quantized.push(q);
            }
        }

        info!("Quantized {} eigenvalues with Δ={}", quantized.len(), params.delta);
        debug!("Quantized values: {:?}", quantized);

        Self {
            quantized,
            delta_scaled: (params.delta * 1000.0) as u64,
            original_count: spectrum.eigenvalues.len(),
        }
    }

    pub fn matches(&self, other: &QuantizedSpectrum) -> bool {
        self.quantized == other.quantized
    }

    pub fn hamming_distance(&self, other: &QuantizedSpectrum) -> usize {
        self.quantized
            .iter()
            .zip(other.quantized.iter())
            .filter(|(a, b)| a != b)
            .count()
    }

    pub fn matches_with_tolerance(&self, other: &QuantizedSpectrum, max_mismatches: usize) -> bool {
        self.hamming_distance(other) <= max_mismatches
    }

    pub fn len(&self) -> usize {
        self.quantized.len()
    }

    pub fn is_empty(&self) -> bool {
        self.quantized.is_empty()
    }

    pub fn to_field_elements(&self) -> Vec<u64> {
        let offset = 100i64;

        self.quantized
            .iter()
            .map(|&q| (q + offset) as u64)
            .collect()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        bytes.extend_from_slice(&(self.quantized.len() as u32).to_le_bytes());

        for &q in &self.quantized {
            bytes.extend_from_slice(&q.to_le_bytes());
        }

        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        use crate::error::ExtractError;

        if bytes.len() < 4 {
            return Err(ExtractError::ProcessingError("Quantized spectrum too short".into()));
        }

        let len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;

        if bytes.len() < 4 + len * 8 {
            return Err(ExtractError::ProcessingError("Quantized spectrum truncated".into()));
        }

        let mut quantized = Vec::with_capacity(len);
        for i in 0..len {
            let offset = 4 + i * 8;
            let q = i64::from_le_bytes([
                bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3],
                bytes[offset + 4], bytes[offset + 5], bytes[offset + 6], bytes[offset + 7],
            ]);
            quantized.push(q);
        }

        Ok(Self {
            quantized,
            delta_scaled: 50,
            original_count: len + 1,
        })
    }
}

pub fn dequantize(quantized: &QuantizedSpectrum, params: &QuantizationParams) -> Vec<f64> {
    quantized
        .quantized
        .iter()
        .map(|&q| (q as f64 + 0.5) * params.delta)
        .collect()
}

#[derive(Debug)]
pub struct QuantizationStats {
    pub unique_bins: usize,

    pub min_q: i64,

    pub max_q: i64,

    pub entropy_bits: f64,
}

impl QuantizedSpectrum {
    pub fn stats(&self) -> QuantizationStats {
        use std::collections::HashSet;

        let unique: HashSet<i64> = self.quantized.iter().cloned().collect();
        let min_q = self.quantized.iter().min().cloned().unwrap_or(0);
        let max_q = self.quantized.iter().max().cloned().unwrap_or(0);

        let entropy_bits = (unique.len() as f64).log2() * self.quantized.len() as f64;

        QuantizationStats {
            unique_bins: unique.len(),
            min_q,
            max_q,
            entropy_bits,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::minutiae::MinutiaeSet;
    use crate::ridge_graph::RidgeGraph;

    #[test]
    fn test_quantization_determinism() {
        let minutiae = MinutiaeSet::mock();
        let graph = RidgeGraph::from_minutiae(&minutiae);
        let spectrum = SpectralSignature::from_graph(&graph).unwrap();

        let params = QuantizationParams::default();
        let q1 = QuantizedSpectrum::from_spectrum(&spectrum, &params);
        let q2 = QuantizedSpectrum::from_spectrum(&spectrum, &params);

        assert!(q1.matches(&q2), "Same spectrum must give same quantization");
    }

    #[test]
    fn test_quantization_range() {
        let minutiae = MinutiaeSet::mock();
        let graph = RidgeGraph::from_minutiae(&minutiae);
        let spectrum = SpectralSignature::from_graph(&graph).unwrap();

        let params = QuantizationParams::default();
        let quantized = QuantizedSpectrum::from_spectrum(&spectrum, &params);

        for &q in &quantized.quantized {
            assert!(q >= 0, "Quantized eigenvalue {} should be non-negative", q);
        }

        let max_expected = (params.max_eigenvalue / params.delta).ceil() as i64;
        for &q in &quantized.quantized {
            assert!(q <= max_expected, "Quantized value {} exceeds max {}", q, max_expected);
        }
    }

    #[test]
    fn test_field_element_conversion() {
        let minutiae = MinutiaeSet::mock();
        let graph = RidgeGraph::from_minutiae(&minutiae);
        let spectrum = SpectralSignature::from_graph(&graph).unwrap();

        let params = QuantizationParams::default();
        let quantized = QuantizedSpectrum::from_spectrum(&spectrum, &params);

        let field_elems = quantized.to_field_elements();

        for &fe in &field_elems {
            assert!(fe > 0, "Field element should be positive");
        }
    }

    #[test]
    fn test_serialization_roundtrip() {
        let minutiae = MinutiaeSet::mock();
        let graph = RidgeGraph::from_minutiae(&minutiae);
        let spectrum = SpectralSignature::from_graph(&graph).unwrap();

        let params = QuantizationParams::default();
        let original = QuantizedSpectrum::from_spectrum(&spectrum, &params);

        let bytes = original.to_bytes();
        let recovered = QuantizedSpectrum::from_bytes(&bytes).unwrap();

        assert_eq!(original.quantized, recovered.quantized);
    }

    #[test]
    fn test_stats() {
        let minutiae = MinutiaeSet::mock();
        let graph = RidgeGraph::from_minutiae(&minutiae);
        let spectrum = SpectralSignature::from_graph(&graph).unwrap();

        let params = QuantizationParams::default();
        let quantized = QuantizedSpectrum::from_spectrum(&spectrum, &params);

        let stats = quantized.stats();

        println!("Quantization stats:");
        println!("  Unique bins: {}", stats.unique_bins);
        println!("  Range: [{}, {}]", stats.min_q, stats.max_q);
        println!("  Entropy estimate: {:.2} bits", stats.entropy_bits);

        assert!(stats.entropy_bits > 10.0, "Should have reasonable entropy");
    }
}
