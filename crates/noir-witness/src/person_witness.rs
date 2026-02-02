
use std::path::Path;
use std::fs;

use turing_core::Fr;
use turing_core::person_circuit::{
    CircuitEmbedding, PersonCommitment, PersonNullifier,
    QuantizationConfig,
};
use turing_core::person_identity::PersonEmbedding;
use rand::Rng;
use tracing::{info, debug};

use crate::constants::MERKLE_DEPTH;
use crate::error::Result;
use crate::field::FieldFormatter;
use crate::merkle::MerkleTree;

#[derive(Debug, Clone)]
pub struct PersonCircuitWitness {
    pub embedding: CircuitEmbedding,
    pub blinding: Fr,
    pub merkle_path: Vec<Fr>,
    pub merkle_indices: Vec<bool>,
    pub commitment: Fr,
    pub merkle_root: Fr,
    pub scope: Fr,
    pub nullifier: Fr,
}

pub struct PersonWitnessGenerator {
    merkle_tree: MerkleTree,
}

impl PersonWitnessGenerator {
    pub fn new() -> Self {
        Self {
            merkle_tree: MerkleTree::new(),
        }
    }

    pub fn with_tree(tree: MerkleTree) -> Self {
        Self { merkle_tree: tree }
    }

    pub fn merkle_root(&self) -> Fr {
        self.merkle_tree.root()
    }

    pub fn tree(&self) -> &MerkleTree {
        &self.merkle_tree
    }

    pub fn register_commitment(&mut self, commitment: Fr) -> Result<usize> {
        self.merkle_tree.insert(commitment)
    }

    pub fn find_commitment(&self, commitment: &Fr) -> Option<usize> {
        self.merkle_tree.find(commitment)
    }

    pub fn generate_with_stored_commitment(
        &self,
        embedding: &PersonEmbedding,
        stored_blinding: Fr,
        scope: &str,
    ) -> Result<PersonCircuitWitness> {
        info!("Generating person_identity witness with stored commitment for scope: {}", scope);

        let config = QuantizationConfig::default();
        let circuit_embedding = CircuitEmbedding::from_embedding(embedding, &config);
        debug!("Quantized embedding to {} field elements", circuit_embedding.values.len());

        let commitment_obj = PersonCommitment::with_blinding(&circuit_embedding, stored_blinding);
        let commitment = commitment_obj.value;
        let blinding = commitment_obj.blinding;
        debug!("Recreated commitment with stored blinding");

        let index = self.merkle_tree.find(&commitment)
            .ok_or_else(|| crate::error::WitnessError::MerkleTreeError {
                reason: "Commitment not found in tree - was it registered during enrollment?".to_string(),
            })?;
        debug!("Found commitment at Merkle index {}", index);

        let merkle_proof = self.merkle_tree.prove(index)?;
        assert!(merkle_proof.verify(), "Generated Merkle proof must be valid");
        debug!("Generated valid Merkle proof");

        let nullifier_obj = PersonNullifier::derive(&circuit_embedding, scope);
        let scope_field = nullifier_obj.scope;
        let nullifier = nullifier_obj.value;
        debug!("Derived nullifier for scope");

        info!(
            "Person witness (stored commitment) complete: index {}, tree has {} entries, root stable",
            index,
            self.merkle_tree.count()
        );

        Ok(PersonCircuitWitness {
            embedding: circuit_embedding,
            blinding,
            merkle_path: merkle_proof.path.clone(),
            merkle_indices: merkle_proof.indices.clone(),
            commitment,
            merkle_root: merkle_proof.root,
            scope: scope_field,
            nullifier,
        })
    }

    pub fn generate<R: Rng>(
        &mut self,
        embedding: &PersonEmbedding,
        scope: &str,
        rng: &mut R,
    ) -> Result<PersonCircuitWitness> {
        info!("Generating person_identity witness for scope: {}", scope);

        let config = QuantizationConfig::default();
        let circuit_embedding = CircuitEmbedding::from_embedding(embedding, &config);
        debug!("Quantized embedding to {} field elements", circuit_embedding.values.len());

        let commitment_obj = PersonCommitment::new(&circuit_embedding, rng);
        let commitment = commitment_obj.value;
        let blinding = commitment_obj.blinding;
        debug!("Created commitment");

        let index = self.merkle_tree.find(&commitment)
            .unwrap_or_else(|| {
                self.merkle_tree.insert(commitment).expect("Merkle tree insert failed")
            });
        debug!("Commitment at Merkle index {}", index);

        let merkle_proof = self.merkle_tree.prove(index)?;
        assert!(merkle_proof.verify(), "Generated Merkle proof must be valid");
        debug!("Generated valid Merkle proof");

        let nullifier_obj = PersonNullifier::derive(&circuit_embedding, scope);
        let scope_field = nullifier_obj.scope;
        let nullifier = nullifier_obj.value;
        debug!("Derived nullifier for scope");

        info!(
            "Person witness complete: commitment registered at index {}, tree has {} entries",
            index,
            self.merkle_tree.count()
        );

        Ok(PersonCircuitWitness {
            embedding: circuit_embedding,
            blinding,
            merkle_path: merkle_proof.path.clone(),
            merkle_indices: merkle_proof.indices.clone(),
            commitment,
            merkle_root: merkle_proof.root,
            scope: scope_field,
            nullifier,
        })
    }
}

impl Default for PersonWitnessGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl PersonCircuitWitness {
    pub fn write_prover_toml(&self, path: &Path) -> Result<()> {
        let content = self.to_prover_toml();
        fs::write(path, content)?;
        info!("Wrote Prover.toml to {:?}", path);
        Ok(())
    }

    pub fn to_prover_toml(&self) -> String {
        let mut toml = String::new();

        toml.push_str("# Auto-generated prover inputs for person_identity circuit\n");
        toml.push_str("# Generated by noir-witness::person_witness\n");
        toml.push_str("#\n");
        toml.push_str("# All values are mathematically consistent:\n");
        toml.push_str("# - commitment = Poseidon(COMMITMENT_DOMAIN, compress(embedding), blinding)\n");
        toml.push_str("# - nullifier = Poseidon(NULLIFIER_DOMAIN, compress(embedding), scope)\n");
        toml.push_str("# - merkle_root is computed from real tree with commitment as leaf\n");
        toml.push_str("# - merkle_proof is valid path from commitment to root\n\n");

        toml.push_str(&format!("blinding = \"{}\"\n", FieldFormatter::from_tc_fr(&self.blinding)));
        toml.push_str(&format!("commitment = \"{}\"\n", FieldFormatter::from_tc_fr(&self.commitment)));
        toml.push_str(&format!("merkle_root = \"{}\"\n", FieldFormatter::from_tc_fr(&self.merkle_root)));
        toml.push_str(&format!("scope = \"{}\"\n", FieldFormatter::from_tc_fr(&self.scope)));
        toml.push_str(&format!("nullifier = \"{}\"\n\n", FieldFormatter::from_tc_fr(&self.nullifier)));

        toml.push_str("[embedding]\n");
        toml.push_str("values = [\n");
        for (i, val) in self.embedding.values.iter().enumerate() {
            if i > 0 {
                toml.push_str(",\n");
            }
            toml.push_str(&format!("    \"{}\"", FieldFormatter::from_tc_fr(val)));
        }
        toml.push_str("\n]\n\n");

        toml.push_str("[merkle_proof]\n");
        toml.push_str("path = [\n");
        for (i, val) in self.merkle_path.iter().enumerate() {
            if i > 0 {
                toml.push_str(",\n");
            }
            toml.push_str(&format!("    \"{}\"", FieldFormatter::from_tc_fr(val)));
        }
        for i in self.merkle_path.len()..MERKLE_DEPTH {
            if i > 0 {
                toml.push_str(",\n");
            }
            toml.push_str(&format!("    \"{}\"", FieldFormatter::zero()));
        }
        toml.push_str("\n]\n");

        toml.push_str("indices = [");
        for (i, &is_right) in self.merkle_indices.iter().enumerate() {
            if i > 0 {
                toml.push_str(", ");
            }
            toml.push_str(if is_right { "1" } else { "0" });
        }
        for i in self.merkle_indices.len()..MERKLE_DEPTH {
            if i > 0 || !self.merkle_indices.is_empty() {
                toml.push_str(", ");
            }
            toml.push_str("0");
        }
        toml.push_str("]\n");

        toml
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_person_witness_generation() {
        let embedding_values: Vec<f32> = (0..128).map(|i| (i as f32 / 128.0) - 0.5).collect();
        let embedding = PersonEmbedding::new(embedding_values);

        let mut generator = PersonWitnessGenerator::new();
        let mut rng = rand::thread_rng();

        let witness = generator.generate(&embedding, "test-scope", &mut rng).unwrap();

        assert_eq!(witness.embedding.values.len(), 32);
        assert!(!witness.blinding.is_zero());
        assert_eq!(witness.merkle_path.len(), MERKLE_DEPTH);
        assert_eq!(witness.merkle_indices.len(), MERKLE_DEPTH);
        assert!(!witness.commitment.is_zero());
        assert!(!witness.merkle_root.is_zero());
        assert!(!witness.nullifier.is_zero());
    }

    #[test]
    fn test_merkle_root_not_zero() {
        let embedding = PersonEmbedding::new(vec![0.5; 128]);
        let mut generator = PersonWitnessGenerator::new();
        let mut rng = rand::thread_rng();

        let witness = generator.generate(&embedding, "scope", &mut rng).unwrap();

        assert!(!witness.merkle_root.is_zero(), "Merkle root must not be zero!");
    }

    #[test]
    fn test_prover_toml_format() {
        let embedding = PersonEmbedding::new(vec![0.5; 128]);
        let mut generator = PersonWitnessGenerator::new();
        let mut rng = rand::thread_rng();

        let witness = generator.generate(&embedding, "test", &mut rng).unwrap();
        let toml = witness.to_prover_toml();

        assert!(toml.contains("[embedding]"));
        assert!(toml.contains("values = ["));
        assert!(toml.contains("blinding = \"0x"));
        assert!(toml.contains("[merkle_proof]"));
        assert!(toml.contains("path = ["));
        assert!(toml.contains("indices = ["));
        assert!(toml.contains("commitment = \"0x"));
        assert!(toml.contains("merkle_root = \"0x"));
        assert!(toml.contains("scope = \"0x"));
        assert!(toml.contains("nullifier = \"0x"));

        assert!(!toml.contains("merkle_root = \"0x0000000000000000000000000000000000000000000000000000000000000000\""));
    }

    #[test]
    fn test_nullifier_determinism() {
        let embedding = PersonEmbedding::new(vec![0.5; 128]);

        let mut gen1 = PersonWitnessGenerator::new();
        let mut gen2 = PersonWitnessGenerator::new();
        let mut rng1 = rand::thread_rng();
        let mut rng2 = rand::thread_rng();

        let w1 = gen1.generate(&embedding, "same-scope", &mut rng1).unwrap();
        let w2 = gen2.generate(&embedding, "same-scope", &mut rng2).unwrap();

        assert_eq!(w1.nullifier, w2.nullifier);
    }

    #[test]
    fn test_different_scopes_different_nullifiers() {
        let embedding = PersonEmbedding::new(vec![0.5; 128]);
        let mut generator = PersonWitnessGenerator::new();
        let mut rng = rand::thread_rng();

        let w1 = generator.generate(&embedding, "scope-a", &mut rng).unwrap();
        let w2 = generator.generate(&embedding, "scope-b", &mut rng).unwrap();

        assert_ne!(w1.nullifier, w2.nullifier);
    }
}
