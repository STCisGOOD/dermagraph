
use crate::error::{ExtractError, Result};
use crate::ridge_graph::RidgeGraph;
use nalgebra::{DMatrix, DVector, SymmetricEigen};
use tracing::{debug, info};

#[derive(Debug, Clone)]
pub struct SpectralSignature {
    pub eigenvalues: Vec<f64>,

    pub eigenvectors: DMatrix<f64>,

    pub dimension: usize,
}

impl SpectralSignature {
    pub fn from_graph(graph: &RidgeGraph) -> Result<Self> {
        let n = graph.node_count;

        if n < 3 {
            return Err(ExtractError::InsufficientMinutiae { found: n, minimum: 3 });
        }

        debug!("Computing spectral signature for graph with {} nodes", n);

        let adj = graph.adjacency_matrix();
        let degrees = graph.degrees();

        for (i, &d) in degrees.iter().enumerate() {
            if d < 1e-10 {
                debug!("Warning: Node {} has near-zero degree", i);
            }
        }

        let laplacian = compute_normalized_laplacian(&adj, &degrees);

        let eigen = SymmetricEigen::new(laplacian);

        let eigenvalues: Vec<f64> = eigen.eigenvalues.iter().cloned().collect();
        let eigenvectors_raw = eigen.eigenvectors;

        let mut indices: Vec<usize> = (0..n).collect();
        indices.sort_by(|&i, &j| {
            eigenvalues[i].partial_cmp(&eigenvalues[j]).unwrap()
        });

        let sorted_eigenvalues: Vec<f64> = indices.iter().map(|&i| eigenvalues[i]).collect();

        let mut sorted_eigenvectors = DMatrix::zeros(n, n);
        for (new_idx, &old_idx) in indices.iter().enumerate() {
            for row in 0..n {
                sorted_eigenvectors[(row, new_idx)] = eigenvectors_raw[(row, old_idx)];
            }
        }

        let eigenvalues: Vec<f64> = sorted_eigenvalues
            .iter()
            .map(|&v| if v.abs() < 1e-10 { 0.0 } else { v })
            .collect();

        info!("Computed spectral signature: {} eigenvalues", n);
        debug!("Eigenvalue range: [{:.6}, {:.6}]",
               eigenvalues.first().unwrap_or(&0.0),
               eigenvalues.last().unwrap_or(&0.0));

        Ok(Self {
            eigenvalues,
            eigenvectors: sorted_eigenvectors,
            dimension: n,
        })
    }

    pub fn top_k_eigenvalues(&self, k: usize) -> Vec<f64> {
        self.eigenvalues
            .iter()
            .skip(1)
            .take(k)
            .cloned()
            .collect()
    }

    pub fn distance(&self, other: &SpectralSignature) -> f64 {
        let n = self.eigenvalues.len().min(other.eigenvalues.len());

        let mut sum_sq = 0.0;
        for i in 0..n {
            let diff = self.eigenvalues[i] - other.eigenvalues[i];
            sum_sq += diff * diff;
        }

        sum_sq.sqrt()
    }

    pub fn is_similar(&self, other: &SpectralSignature, tolerance: f64) -> bool {
        if self.dimension != other.dimension {
            return false;
        }

        for (a, b) in self.eigenvalues.iter().zip(other.eigenvalues.iter()) {
            let max_val = a.abs().max(b.abs()).max(1e-10);
            let rel_diff = (a - b).abs() / max_val;

            if rel_diff > tolerance {
                return false;
            }
        }

        true
    }

    pub fn fiedler_value(&self) -> f64 {
        self.eigenvalues.get(1).cloned().unwrap_or(0.0)
    }

    pub fn spectral_gap(&self) -> f64 {
        if self.eigenvalues.len() >= 2 {
            self.eigenvalues[1] - self.eigenvalues[0]
        } else {
            0.0
        }
    }

    pub fn eigenvector(&self, index: usize) -> Option<DVector<f64>> {
        if index < self.dimension {
            Some(self.eigenvectors.column(index).into_owned())
        } else {
            None
        }
    }

    pub fn reconstruct_with_function<F>(&self, f: F) -> DMatrix<f64>
    where
        F: Fn(f64) -> f64,
    {
        let n = self.dimension;

        let mut result = DMatrix::zeros(n, n);

        for k in 0..n {
            let f_lambda = f(self.eigenvalues[k]);
            let v_k = self.eigenvectors.column(k);

            for i in 0..n {
                for j in 0..n {
                    result[(i, j)] += f_lambda * v_k[i] * v_k[j];
                }
            }
        }

        result
    }
}

fn compute_normalized_laplacian(adj: &[Vec<f64>], degrees: &[f64]) -> DMatrix<f64> {
    let n = degrees.len();
    let mut laplacian = DMatrix::zeros(n, n);

    let d_inv_sqrt: Vec<f64> = degrees
        .iter()
        .map(|&d| if d > 1e-10 { 1.0 / d.sqrt() } else { 0.0 })
        .collect();

    for i in 0..n {
        for j in 0..n {
            if i == j {
                laplacian[(i, i)] = if degrees[i] > 1e-10 { 1.0 } else { 0.0 };
            } else {
                laplacian[(i, j)] = -d_inv_sqrt[i] * adj[i][j] * d_inv_sqrt[j];
            }
        }
    }

    laplacian
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::minutiae::MinutiaeSet;

    #[test]
    fn test_spectral_signature_mock() {
        let minutiae = MinutiaeSet::mock();
        let graph = RidgeGraph::from_minutiae(&minutiae);

        let spectrum = SpectralSignature::from_graph(&graph).unwrap();

        assert_eq!(spectrum.eigenvalues.len(), 8);
        assert_eq!(spectrum.dimension, 8);

        assert!(spectrum.eigenvalues[0].abs() < 0.01);

        for &ev in &spectrum.eigenvalues {
            assert!(ev >= -0.01 && ev <= 2.01, "Eigenvalue {} out of range", ev);
        }

        for i in 1..spectrum.eigenvalues.len() {
            assert!(spectrum.eigenvalues[i] >= spectrum.eigenvalues[i-1] - 1e-10);
        }
    }

    #[test]
    fn test_spectral_stability() {
        let minutiae1 = MinutiaeSet::mock();
        let graph1 = RidgeGraph::from_minutiae(&minutiae1);
        let spectrum1 = SpectralSignature::from_graph(&graph1).unwrap();

        let graph2 = RidgeGraph::from_minutiae(&minutiae1);
        let spectrum2 = SpectralSignature::from_graph(&graph2).unwrap();

        let distance = spectrum1.distance(&spectrum2);
        assert!(distance < 1e-10, "Same input should give same spectrum");
    }

    #[test]
    fn test_fiedler_value() {
        let minutiae = MinutiaeSet::mock();
        let graph = RidgeGraph::from_minutiae(&minutiae);
        let spectrum = SpectralSignature::from_graph(&graph).unwrap();

        let fiedler = spectrum.fiedler_value();

        assert!(fiedler > 0.0, "Connected graph should have positive Fiedler value");
    }

    #[test]
    fn test_reconstruct_identity() {
        let minutiae = MinutiaeSet::mock();
        let graph = RidgeGraph::from_minutiae(&minutiae);
        let spectrum = SpectralSignature::from_graph(&graph).unwrap();

        let reconstructed = spectrum.reconstruct_with_function(|x| x);

        let n = spectrum.dimension;
        for i in 0..n {
            for j in 0..n {
                let diff = (reconstructed[(i, j)] - reconstructed[(j, i)]).abs();
                assert!(diff < 1e-10, "Reconstruction should be symmetric");
            }
        }
    }
}
