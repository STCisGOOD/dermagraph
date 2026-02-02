
use std::path::Path;
use std::fs;

use turing_core::Fr;
use turing_core::{SpectralTuringHash, STHParams};
use serde::{Serialize, Deserialize};
use tracing::{info, debug};

use biometric_extract::{BiometricData, MinutiaeSet};
use turing_core::GraphLaplacian;

use crate::constants::*;
use crate::error::{Result, WitnessError};
use crate::field::FieldFormatter;
use crate::merkle::MerkleTree;

#[derive(Clone, Debug)]
pub struct CircuitWitness {
    pub minutiae_x: Vec<String>,
    pub minutiae_y: Vec<String>,
    pub minutiae_theta: Vec<String>,
    pub minutiae_count: u32,

    pub quantized_values: Vec<String>,
    pub quantized_count: u32,

    pub laplacian_rows: Vec<String>,
    pub laplacian_cols: Vec<String>,
    pub laplacian_vals: Vec<String>,
    pub n_laplacian_entries: u32,

    pub merkle_path: Vec<String>,
    pub merkle_indices: Vec<String>,

    pub merkle_root: String,
    pub nullifier_scope: String,
    pub nullifier: String,

    pub identity: Fr,
}

#[derive(Serialize, Deserialize)]
pub struct ProverToml {
    #[serde(rename = "minutiae.x")]
    pub minutiae_x: Vec<String>,
    #[serde(rename = "minutiae.y")]
    pub minutiae_y: Vec<String>,
    #[serde(rename = "minutiae.theta")]
    pub minutiae_theta: Vec<String>,
    #[serde(rename = "minutiae.count")]
    pub minutiae_count: String,

    #[serde(rename = "quantized.values")]
    pub quantized_values: Vec<String>,
    #[serde(rename = "quantized.count")]
    pub quantized_count: String,

    pub laplacian: Vec<LaplacianEntryToml>,
    pub n_laplacian_entries: String,

    #[serde(rename = "merkle_proof.path")]
    pub merkle_path: Vec<String>,
    #[serde(rename = "merkle_proof.indices")]
    pub merkle_indices: Vec<String>,

    pub merkle_root: String,
    pub nullifier_scope: String,
    pub nullifier: String,
}

#[derive(Serialize, Deserialize)]
pub struct LaplacianEntryToml {
    pub row: String,
    pub col: String,
    pub value: String,
}

pub struct WitnessGenerator {
    merkle_tree: MerkleTree,
}

impl WitnessGenerator {
    pub fn new() -> Self {
        Self {
            merkle_tree: MerkleTree::new(),
        }
    }

    pub fn with_tree(tree: MerkleTree) -> Self {
        Self { merkle_tree: tree }
    }

    pub fn register_identity(&mut self, identity: Fr) -> Result<usize> {
        self.merkle_tree.insert(identity)
    }

    pub fn merkle_root(&self) -> Fr {
        self.merkle_tree.root()
    }

    pub fn generate(
        &mut self,
        biometric: &BiometricData,
        scope: &str,
    ) -> Result<CircuitWitness> {
        info!("Generating circuit witness for scope: {}", scope);

        let (minutiae_x, minutiae_y, minutiae_theta, minutiae_count) =
            self.convert_minutiae(&biometric.minutiae)?;

        let (quantized_values, quantized_count) =
            self.convert_quantized(&biometric.quantized)?;

        let (laplacian_rows, laplacian_cols, laplacian_vals, n_laplacian_entries) =
            self.convert_laplacian(&biometric.laplacian)?;

        let identity = self.compute_identity_hash(biometric)?;
        debug!("Computed identity hash: {:?}", identity);

        let index = self.merkle_tree.find(&identity)
            .unwrap_or_else(|| {
                self.merkle_tree.insert(identity).expect("Failed to insert identity")
            });

        let merkle_proof = self.merkle_tree.prove(index)?;
        assert!(merkle_proof.verify(), "Merkle proof verification failed");

        let scope_hash = scope_to_poseidon_field(scope);
        let nullifier = SpectralTuringHash::derive_nullifier(identity, scope);

        info!("Generated witness: {} minutiae, {} laplacian entries",
              minutiae_count, n_laplacian_entries);

        Ok(CircuitWitness {
            minutiae_x,
            minutiae_y,
            minutiae_theta,
            minutiae_count,
            quantized_values,
            quantized_count,
            laplacian_rows,
            laplacian_cols,
            laplacian_vals,
            n_laplacian_entries,
            merkle_path: merkle_proof.path_to_noir(),
            merkle_indices: merkle_proof.indices_to_noir(),
            merkle_root: merkle_proof.root_to_noir(),
            nullifier_scope: FieldFormatter::from_tc_fr(&scope_hash),
            nullifier: FieldFormatter::from_tc_fr(&nullifier),
            identity,
        })
    }

    fn convert_minutiae(&self, minutiae: &MinutiaeSet) -> Result<(Vec<String>, Vec<String>, Vec<String>, u32)> {
        let count = minutiae.len();
        if count > MAX_MINUTIAE {
            return Err(WitnessError::TooManyMinutiae {
                count,
                max: MAX_MINUTIAE,
            });
        }

        let x_coords = minutiae.x_coords();
        let y_coords = minutiae.y_coords();
        let thetas = minutiae.orientations();

        let mut x_out = Vec::with_capacity(MAX_MINUTIAE);
        let mut y_out = Vec::with_capacity(MAX_MINUTIAE);
        let mut theta_out = Vec::with_capacity(MAX_MINUTIAE);

        for i in 0..count {
            x_out.push(FieldFormatter::from_coordinate(x_coords[i]));
            y_out.push(FieldFormatter::from_coordinate(y_coords[i]));
            theta_out.push(FieldFormatter::from_angle(thetas[i]));
        }

        while x_out.len() < MAX_MINUTIAE {
            x_out.push(FieldFormatter::zero());
            y_out.push(FieldFormatter::zero());
            theta_out.push(FieldFormatter::zero());
        }

        Ok((x_out, y_out, theta_out, count as u32))
    }

    fn convert_quantized(&self, quantized: &biometric_extract::QuantizedSpectrum) -> Result<(Vec<String>, u32)> {
        let values = quantized.to_field_elements();
        let count = values.len().min(QUANTIZED_SPECTRUM_SIZE);

        let mut out = Vec::with_capacity(QUANTIZED_SPECTRUM_SIZE);

        for i in 0..count {
            out.push(FieldFormatter::from_u64(values[i]));
        }

        while out.len() < QUANTIZED_SPECTRUM_SIZE {
            out.push(FieldFormatter::zero());
        }

        Ok((out, count as u32))
    }

    fn convert_laplacian(&self, laplacian: &GraphLaplacian) -> Result<(Vec<String>, Vec<String>, Vec<String>, u32)> {
        let mut rows = Vec::new();
        let mut cols = Vec::new();
        let mut vals = Vec::new();

        for (r, c, v) in laplacian.matrix.iter() {
            if rows.len() >= MAX_LAPLACIAN_ENTRIES {
                return Err(WitnessError::TooManyLaplacianEntries {
                    count: laplacian.matrix.nnz(),
                    max: MAX_LAPLACIAN_ENTRIES,
                });
            }

            rows.push(FieldFormatter::from_u32(r as u32));
            cols.push(FieldFormatter::from_u32(c as u32));
            vals.push(FieldFormatter::from_tc_fr(&v));
        }

        let n_entries = rows.len() as u32;

        while rows.len() < MAX_LAPLACIAN_ENTRIES {
            rows.push(FieldFormatter::zero());
            cols.push(FieldFormatter::zero());
            vals.push(FieldFormatter::zero());
        }

        Ok((rows, cols, vals, n_entries))
    }

    fn compute_identity_hash(&self, biometric: &BiometricData) -> Result<Fr> {
        let witness = biometric.to_sth_witness();

        let params = STHParams::standard_128bit();

        let identity = SpectralTuringHash::compute(
            &witness.minutiae_x,
            &witness.minutiae_y,
            &witness.minutiae_theta,
            &witness.quantized_spectrum,
            &biometric.laplacian,
            &params,
        ).map_err(|e| WitnessError::IdentityHashError {
            reason: format!("STH computation failed: {}", e),
        })?;

        debug!(
            "Computed STH identity: {} minutiae, {} spectrum values",
            witness.num_minutiae,
            witness.quantized_spectrum.len()
        );

        Ok(identity)
    }
}

impl Default for WitnessGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl CircuitWitness {
    pub fn write_prover_toml(&self, path: &Path) -> Result<()> {
        let toml = self.to_prover_toml();
        let content = toml::to_string_pretty(&toml)?;
        fs::write(path, content)?;
        info!("Wrote Prover.toml to {:?}", path);
        Ok(())
    }

    pub fn to_prover_toml(&self) -> ProverToml {
        let laplacian: Vec<LaplacianEntryToml> = (0..MAX_LAPLACIAN_ENTRIES)
            .map(|i| LaplacianEntryToml {
                row: self.laplacian_rows[i].clone(),
                col: self.laplacian_cols[i].clone(),
                value: self.laplacian_vals[i].clone(),
            })
            .collect();

        ProverToml {
            minutiae_x: self.minutiae_x.clone(),
            minutiae_y: self.minutiae_y.clone(),
            minutiae_theta: self.minutiae_theta.clone(),
            minutiae_count: format!("{}", self.minutiae_count),

            quantized_values: self.quantized_values.clone(),
            quantized_count: format!("{}", self.quantized_count),

            laplacian,
            n_laplacian_entries: format!("{}", self.n_laplacian_entries),

            merkle_path: self.merkle_path.clone(),
            merkle_indices: self.merkle_indices.clone(),

            merkle_root: self.merkle_root.clone(),
            nullifier_scope: self.nullifier_scope.clone(),
            nullifier: self.nullifier.clone(),
        }
    }

    pub fn write_flat_toml(&self, path: &Path) -> Result<()> {
        let mut content = String::new();

        content.push_str(&format!("minutiae_x = {:?}\n", self.minutiae_x));
        content.push_str(&format!("minutiae_y = {:?}\n", self.minutiae_y));
        content.push_str(&format!("minutiae_theta = {:?}\n", self.minutiae_theta));
        content.push_str(&format!("minutiae_count = \"{}\"\n", self.minutiae_count));

        content.push_str(&format!("quantized_values = {:?}\n", self.quantized_values));
        content.push_str(&format!("quantized_count = \"{}\"\n", self.quantized_count));

        content.push_str(&format!("laplacian_rows = {:?}\n", self.laplacian_rows));
        content.push_str(&format!("laplacian_cols = {:?}\n", self.laplacian_cols));
        content.push_str(&format!("laplacian_vals = {:?}\n", self.laplacian_vals));
        content.push_str(&format!("n_laplacian_entries = \"{}\"\n", self.n_laplacian_entries));

        content.push_str(&format!("merkle_path = {:?}\n", self.merkle_path));
        content.push_str(&format!("merkle_indices = {:?}\n", self.merkle_indices));

        content.push_str(&format!("merkle_root = \"{}\"\n", self.merkle_root));
        content.push_str(&format!("nullifier_scope = \"{}\"\n", self.nullifier_scope));
        content.push_str(&format!("nullifier = \"{}\"\n", self.nullifier));

        fs::write(path, content)?;
        info!("Wrote flat Prover.toml to {:?}", path);
        Ok(())
    }
}

fn scope_to_poseidon_field(scope: &str) -> Fr {
    use turing_core::poseidon::hash_many;

    let bytes = scope.as_bytes();
    let elements: Vec<Fr> = bytes.iter()
        .map(|&b| Fr::from_u64(b as u64))
        .collect();
    hash_many(&elements)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_witness_generation() {
        let biometric = BiometricData::mock();
        let mut generator = WitnessGenerator::new();

        let witness = generator.generate(&biometric, "test-scope").unwrap();

        assert_eq!(witness.minutiae_x.len(), MAX_MINUTIAE);
        assert_eq!(witness.quantized_values.len(), QUANTIZED_SPECTRUM_SIZE);
        assert_eq!(witness.laplacian_rows.len(), MAX_LAPLACIAN_ENTRIES);
        assert_eq!(witness.merkle_path.len(), MERKLE_DEPTH);

        assert!(witness.minutiae_count > 0);
        assert!(witness.quantized_count > 0);
    }

    #[test]
    fn test_nullifier_determinism() {
        let biometric = BiometricData::mock();
        let mut gen1 = WitnessGenerator::new();
        let mut gen2 = WitnessGenerator::new();

        let w1 = gen1.generate(&biometric, "scope-a").unwrap();
        let w2 = gen2.generate(&biometric, "scope-a").unwrap();

        assert_eq!(w1.nullifier, w2.nullifier);
    }

    #[test]
    fn test_different_scopes() {
        let biometric = BiometricData::mock();
        let mut generator = WitnessGenerator::new();

        let w1 = generator.generate(&biometric, "scope-a").unwrap();
        let w2 = generator.generate(&biometric, "scope-b").unwrap();

        assert_ne!(w1.nullifier, w2.nullifier);
    }
}
