
use crate::field::Fr;
use crate::person_identity::PersonEmbedding;
use crate::poseidon::{hash_2, hash_many};
use rand::Rng;

pub mod domains {
    use super::Fr;

    pub fn person_nullifier() -> Fr {
        Fr::from_be_bytes_mod_order(&hex::decode("706572736f6e3a6e756c6c696669657200").unwrap())
    }

    pub fn person_commitment() -> Fr {
        Fr::from_be_bytes_mod_order(&hex::decode("706572736f6e3a636f6d6d69746d656e74").unwrap())
    }

    pub fn person_id() -> Fr {
        Fr::from_be_bytes_mod_order(&hex::decode("706572736f6e3a69643a763100000000").unwrap())
    }
}

#[derive(Debug, Clone)]
pub struct QuantizationConfig {
    pub precision: f32,

    pub output_dim: usize,
}

impl Default for QuantizationConfig {
    fn default() -> Self {
        Self {
            precision: 0.1,
            output_dim: 32,
        }
    }
}

fn fr_to_hex(fr: &Fr) -> String {
    hex::encode(fr.to_be_bytes())
}

#[derive(Debug, Clone)]
pub struct CircuitEmbedding {
    pub values: Vec<Fr>,
    pub quantized: Vec<i32>,
}

impl CircuitEmbedding {
    pub fn from_embedding(embedding: &PersonEmbedding, config: &QuantizationConfig) -> Self {
        let quantized: Vec<i32> = embedding.vector
            .iter()
            .map(|&x| (x / config.precision).round() as i32)
            .collect();

        let field_values: Vec<Fr> = quantized
            .iter()
            .map(|&q| {
                if q >= 0 {
                    Fr::from_u64(q as u64)
                } else {
                    -Fr::from_u64((-q) as u64)
                }
            })
            .collect();

        let values = compress_to_circuit_format(&field_values, config.output_dim);

        Self { values, quantized }
    }

    pub fn to_hex_values(&self) -> Vec<String> {
        self.values.iter().map(|fr| format!("0x{}", fr_to_hex(fr))).collect()
    }
}

fn compress_to_circuit_format(field_values: &[Fr], output_dim: usize) -> Vec<Fr> {
    let mut padded = field_values.to_vec();
    padded.resize(128, Fr::zero());

    let mut result = Vec::with_capacity(output_dim);

    for chunk_idx in 0..output_dim {
        let base = chunk_idx * 4;
        if base + 3 < padded.len() {
            let chunk_hash = hash_many(&[
                domains::person_id(),
                Fr::from_u64(chunk_idx as u64),
                padded[base],
                padded[base + 1],
                padded[base + 2],
                padded[base + 3],
            ]);
            result.push(chunk_hash);
        } else {
            result.push(Fr::zero());
        }
    }

    result
}

fn compress_embedding_to_single(values: &[Fr]) -> Fr {
    assert_eq!(values.len(), 32, "Circuit embedding must have exactly 32 values");

    let mut acc = domains::person_id();

    for i in 0..8 {
        let idx = i * 4;
        let h1 = hash_many(&[acc, values[idx], values[idx + 1], values[idx + 2]]);
        acc = hash_2(h1, values[idx + 3]);
    }

    acc
}

#[derive(Debug, Clone)]
pub struct PersonCommitment {
    pub value: Fr,
    pub blinding: Fr,
}

impl PersonCommitment {
    pub fn new<R: Rng>(embedding: &CircuitEmbedding, rng: &mut R) -> Self {
        let blinding = Fr::random(rng);
        let value = Self::compute(&embedding.values, &blinding);
        Self { value, blinding }
    }

    pub fn with_blinding(embedding: &CircuitEmbedding, blinding: Fr) -> Self {
        let value = Self::compute(&embedding.values, &blinding);
        Self { value, blinding }
    }

    fn compute(values: &[Fr], blinding: &Fr) -> Fr {
        let embedding_hash = compress_embedding_to_single(values);
        hash_many(&[domains::person_commitment(), embedding_hash, *blinding])
    }

    pub fn to_hex(&self) -> String {
        format!("0x{}", fr_to_hex(&self.value))
    }
}

#[derive(Debug, Clone)]
pub struct PersonNullifier {
    pub value: Fr,
    pub scope: Fr,
}

impl PersonNullifier {
    pub fn derive(embedding: &CircuitEmbedding, scope: &str) -> Self {
        let scope_field = Self::scope_to_field(scope);
        let value = Self::compute(&embedding.values, &scope_field);
        Self { value, scope: scope_field }
    }

    pub fn derive_with_field(embedding: &CircuitEmbedding, scope: Fr) -> Self {
        let value = Self::compute(&embedding.values, &scope);
        Self { value, scope }
    }

    fn compute(values: &[Fr], scope: &Fr) -> Fr {
        let embedding_hash = compress_embedding_to_single(values);
        hash_many(&[domains::person_nullifier(), embedding_hash, *scope])
    }

    pub fn scope_to_field(scope: &str) -> Fr {
        let scope_bytes = scope.as_bytes();
        let mut field_elements = Vec::new();

        for chunk in scope_bytes.chunks(31) {
            let mut padded = [0u8; 32];
            padded[1..1 + chunk.len()].copy_from_slice(chunk);
            field_elements.push(Fr::from_be_bytes_mod_order(&padded));
        }

        if field_elements.is_empty() {
            Fr::zero()
        } else if field_elements.len() == 1 {
            field_elements[0]
        } else {
            hash_many(&field_elements)
        }
    }

    pub fn to_hex(&self) -> String {
        format!("0x{}", fr_to_hex(&self.value))
    }
}

#[derive(Debug, Clone)]
pub struct PersonCircuitInputs {
    pub embedding: CircuitEmbedding,
    pub blinding: Fr,
    pub merkle_path: Vec<Fr>,
    pub merkle_indices: Vec<bool>,
    pub commitment: Fr,
    pub merkle_root: Fr,
    pub scope: Fr,
    pub nullifier: Fr,
}

impl PersonCircuitInputs {
    pub fn generate<R: Rng>(
        embedding: &PersonEmbedding,
        scope: &str,
        merkle_path: Vec<Fr>,
        merkle_indices: Vec<bool>,
        merkle_root: Fr,
        rng: &mut R,
    ) -> Self {
        let config = QuantizationConfig::default();
        let circuit_embedding = CircuitEmbedding::from_embedding(embedding, &config);
        let commitment = PersonCommitment::new(&circuit_embedding, rng);
        let nullifier = PersonNullifier::derive(&circuit_embedding, scope);

        Self {
            blinding: commitment.blinding,
            commitment: commitment.value,
            merkle_path,
            merkle_indices,
            merkle_root,
            scope: nullifier.scope,
            nullifier: nullifier.value,
            embedding: circuit_embedding,
        }
    }

    pub fn generate_simple<R: Rng>(
        embedding: &PersonEmbedding,
        scope: &str,
        rng: &mut R,
    ) -> Self {
        let config = QuantizationConfig::default();
        let circuit_embedding = CircuitEmbedding::from_embedding(embedding, &config);
        let commitment = PersonCommitment::new(&circuit_embedding, rng);
        let nullifier = PersonNullifier::derive(&circuit_embedding, scope);

        let merkle_path = vec![Fr::zero(); 20];
        let merkle_indices = vec![false; 20];

        Self {
            blinding: commitment.blinding,
            commitment: commitment.value,
            merkle_path,
            merkle_indices,
            merkle_root: Fr::zero(),
            scope: nullifier.scope,
            nullifier: nullifier.value,
            embedding: circuit_embedding,
        }
    }

    pub fn to_prover_toml(&self) -> String {
        let mut toml = String::new();

        toml.push_str("# Auto-generated prover inputs for person_identity circuit\n");
        toml.push_str("# Generated by turing-core::person_circuit\n\n");

        toml.push_str("[embedding]\n");
        toml.push_str("values = [\n");
        for (i, val) in self.embedding.values.iter().enumerate() {
            if i > 0 {
                toml.push_str(",\n");
            }
            toml.push_str(&format!("    \"0x{}\"", fr_to_hex(val)));
        }
        toml.push_str("\n]\n\n");

        toml.push_str(&format!("blinding = \"0x{}\"\n\n", fr_to_hex(&self.blinding)));

        toml.push_str("[merkle_proof]\n");
        toml.push_str("path = [\n");
        for (i, val) in self.merkle_path.iter().enumerate() {
            if i > 0 {
                toml.push_str(",\n");
            }
            toml.push_str(&format!("    \"0x{}\"", fr_to_hex(val)));
        }
        toml.push_str("\n]\n");
        toml.push_str(&format!("indices = {:?}\n\n",
            self.merkle_indices.iter().map(|&b| if b { 1 } else { 0 }).collect::<Vec<_>>()));

        toml.push_str(&format!("commitment = \"0x{}\"\n", fr_to_hex(&self.commitment)));
        toml.push_str(&format!("merkle_root = \"0x{}\"\n", fr_to_hex(&self.merkle_root)));
        toml.push_str(&format!("scope = \"0x{}\"\n", fr_to_hex(&self.scope)));
        toml.push_str(&format!("nullifier = \"0x{}\"\n", fr_to_hex(&self.nullifier)));

        toml
    }
}

pub fn would_match_nullifier(
    embedding1: &PersonEmbedding,
    embedding2: &PersonEmbedding,
    config: &QuantizationConfig,
) -> bool {
    let q1 = CircuitEmbedding::from_embedding(embedding1, config);
    let q2 = CircuitEmbedding::from_embedding(embedding2, config);

    q1.quantized == q2.quantized
}

pub fn estimate_collision_probability(
    embedding1: &PersonEmbedding,
    embedding2: &PersonEmbedding,
    _config: &QuantizationConfig,
) -> f32 {
    let similarity = embedding1.similarity(embedding2);

    if similarity > 0.95 {
        0.95
    } else if similarity > 0.85 {
        0.85 - 0.5 * (0.95 - similarity) / 0.1
    } else if similarity > 0.5 {
        0.5 * (similarity - 0.5) / 0.35
    } else {
        0.01
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_separators_non_zero() {
        assert!(!domains::person_nullifier().is_zero());
        assert!(!domains::person_commitment().is_zero());
        assert!(!domains::person_id().is_zero());
    }

    #[test]
    fn test_domain_separators_distinct() {
        assert_ne!(domains::person_nullifier(), domains::person_commitment());
        assert_ne!(domains::person_nullifier(), domains::person_id());
        assert_ne!(domains::person_commitment(), domains::person_id());
    }

    #[test]
    fn test_circuit_embedding_creation() {
        let embedding = PersonEmbedding::new(vec![0.51, 0.32, -0.19, 0.78]);
        let config = QuantizationConfig::default();
        let circuit = CircuitEmbedding::from_embedding(&embedding, &config);

        assert_eq!(circuit.values.len(), 32, "Must produce 32 field elements");
        assert!(!circuit.quantized.is_empty());
    }

    #[test]
    fn test_quantization_deterministic() {
        let embedding = PersonEmbedding::new(vec![0.51, 0.32, -0.19, 0.78]);
        let config = QuantizationConfig::default();

        let c1 = CircuitEmbedding::from_embedding(&embedding, &config);
        let c2 = CircuitEmbedding::from_embedding(&embedding, &config);

        assert_eq!(c1.quantized, c2.quantized, "Quantization must be deterministic");
        assert_eq!(c1.values, c2.values, "Compression must be deterministic");
    }

    #[test]
    fn test_commitment_hiding() {
        let embedding = PersonEmbedding::new(vec![0.5, 0.3, -0.2, 0.8]);
        let config = QuantizationConfig::default();
        let circuit = CircuitEmbedding::from_embedding(&embedding, &config);

        let blinding1 = Fr::from_u64(12345);
        let blinding2 = Fr::from_u64(67890);

        let commit1 = PersonCommitment::with_blinding(&circuit, blinding1);
        let commit2 = PersonCommitment::with_blinding(&circuit, blinding2);

        assert_ne!(commit1.value, commit2.value);
    }

    #[test]
    fn test_commitment_binding() {
        let e1 = PersonEmbedding::new(vec![0.5, 0.3, -0.2, 0.8]);
        let e2 = PersonEmbedding::new(vec![0.9, 0.1, -0.5, 0.2]);
        let config = QuantizationConfig::default();

        let c1 = CircuitEmbedding::from_embedding(&e1, &config);
        let c2 = CircuitEmbedding::from_embedding(&e2, &config);

        let blinding = Fr::from_u64(12345);

        let commit1 = PersonCommitment::with_blinding(&c1, blinding);
        let commit2 = PersonCommitment::with_blinding(&c2, blinding);

        assert_ne!(commit1.value, commit2.value);
    }

    #[test]
    fn test_nullifier_determinism() {
        let embedding = PersonEmbedding::new(vec![0.5, 0.3, -0.2, 0.8]);
        let config = QuantizationConfig::default();
        let circuit = CircuitEmbedding::from_embedding(&embedding, &config);

        let null1 = PersonNullifier::derive(&circuit, "test-scope");
        let null2 = PersonNullifier::derive(&circuit, "test-scope");

        assert_eq!(null1.value, null2.value, "Nullifier must be deterministic");
    }

    #[test]
    fn test_nullifier_scope_separation() {
        let embedding = PersonEmbedding::new(vec![0.5, 0.3, -0.2, 0.8]);
        let config = QuantizationConfig::default();
        let circuit = CircuitEmbedding::from_embedding(&embedding, &config);

        let null1 = PersonNullifier::derive(&circuit, "scope1");
        let null2 = PersonNullifier::derive(&circuit, "scope2");

        assert_ne!(null1.value, null2.value);
    }

    #[test]
    fn test_nullifier_embedding_separation() {
        let e1 = PersonEmbedding::new(vec![0.5, 0.3, -0.2, 0.8]);
        let e2 = PersonEmbedding::new(vec![0.9, 0.1, -0.5, 0.2]);
        let config = QuantizationConfig::default();

        let c1 = CircuitEmbedding::from_embedding(&e1, &config);
        let c2 = CircuitEmbedding::from_embedding(&e2, &config);

        let null1 = PersonNullifier::derive(&c1, "same-scope");
        let null2 = PersonNullifier::derive(&c2, "same-scope");

        assert_ne!(null1.value, null2.value);
    }

    #[test]
    fn test_cross_finger_matching_similar() {
        let thumb = PersonEmbedding::new(vec![0.51, 0.32, -0.19, 0.78]);
        let index = PersonEmbedding::new(vec![0.52, 0.31, -0.18, 0.79]);

        let config = QuantizationConfig { precision: 0.1, output_dim: 32 };

        let would_match = would_match_nullifier(&thumb, &index, &config);

        assert!(would_match, "Similar embeddings should produce same nullifier");
    }

    #[test]
    fn test_cross_finger_not_matching_different() {
        let alice = PersonEmbedding::new(vec![0.51, 0.32, -0.19, 0.78]);
        let bob = PersonEmbedding::new(vec![-0.32, 0.91, 0.45, -0.11]);

        let config = QuantizationConfig { precision: 0.1, output_dim: 32 };

        let would_match = would_match_nullifier(&alice, &bob, &config);

        assert!(!would_match, "Different people should produce different nullifiers");
    }

    #[test]
    fn test_prover_toml_generation() {
        let embedding = PersonEmbedding::new(vec![0.5; 128]);
        let mut rng = rand::thread_rng();
        let inputs = PersonCircuitInputs::generate_simple(&embedding, "test", &mut rng);

        let toml = inputs.to_prover_toml();

        assert!(toml.contains("[embedding]"));
        assert!(toml.contains("values = ["));
        assert!(toml.contains("blinding = \"0x"));
        assert!(toml.contains("[merkle_proof]"));
        assert!(toml.contains("commitment = \"0x"));
        assert!(toml.contains("nullifier = \"0x"));
    }

    #[test]
    fn test_full_pipeline_consistency() {
        let embedding = PersonEmbedding::new(vec![0.5; 128]);
        let config = QuantizationConfig::default();
        let circuit = CircuitEmbedding::from_embedding(&embedding, &config);

        let blinding = Fr::from_u64(42);
        let scope = "test-scope";

        let commit1 = PersonCommitment::with_blinding(&circuit, blinding);
        let commit2 = PersonCommitment::with_blinding(&circuit, blinding);

        let null1 = PersonNullifier::derive(&circuit, scope);
        let null2 = PersonNullifier::derive(&circuit, scope);

        assert_eq!(commit1.value, commit2.value);
        assert_eq!(null1.value, null2.value);
    }

    #[test]
    fn test_print_prover_toml() {
        let embedding_values: Vec<f32> = (0..128).map(|i| (i as f32 / 128.0) - 0.5).collect();
        let embedding = PersonEmbedding::new(embedding_values);
        let config = QuantizationConfig::default();
        let circuit = CircuitEmbedding::from_embedding(&embedding, &config);

        let blinding = Fr::from_u64(0x1234567890abcdef);
        let scope = "dao-vote-2024";

        let commitment = PersonCommitment::with_blinding(&circuit, blinding);
        let nullifier = PersonNullifier::derive(&circuit, scope);

        let inputs = PersonCircuitInputs {
            embedding: circuit,
            blinding,
            merkle_path: (0..20).map(|i| Fr::from_u64(i as u64 + 1)).collect(),
            merkle_indices: vec![false; 20],
            commitment: commitment.value,
            merkle_root: Fr::zero(),
            scope: nullifier.scope,
            nullifier: nullifier.value,
        };

        println!("\n=== GENERATED PROVER.TOML ===\n");
        println!("{}", inputs.to_prover_toml());
        println!("=== END PROVER.TOML ===\n");
    }
}
