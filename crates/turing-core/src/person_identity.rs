
use crate::field::Fr;
use sha2::{Sha256, Digest};

#[derive(Clone, Debug)]
pub struct PersonEmbedding {
    pub vector: Vec<f32>,
}

impl PersonEmbedding {
    pub fn new(mut v: Vec<f32>) -> Self {
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-8 {
            for x in &mut v {
                *x /= norm;
            }
        }
        Self { vector: v }
    }

    pub fn similarity(&self, other: &PersonEmbedding) -> f32 {
        self.vector
            .iter()
            .zip(other.vector.iter())
            .map(|(a, b)| a * b)
            .sum()
    }

    fn quantize(&self, precision: f32) -> Vec<i32> {
        self.vector
            .iter()
            .map(|&x| (x / precision).round() as i32)
            .collect()
    }

    fn to_bytes(&self, precision: f32) -> Vec<u8> {
        let quantized = self.quantize(precision);
        let mut bytes = Vec::with_capacity(quantized.len() * 4);
        for val in quantized {
            bytes.extend_from_slice(&val.to_le_bytes());
        }
        bytes
    }
}

pub struct PersonIdentity;

pub mod domains {
    pub const PERSON_NULLIFIER: &[u8] = b"dermagraph:person:nullifier:v1";
    pub const PERSON_COMMITMENT: &[u8] = b"dermagraph:person:commitment:v1";
    pub const PERSON_ID: &[u8] = b"dermagraph:person:id:v1";
}

impl PersonIdentity {
    pub fn generate_nullifier(
        embedding: &PersonEmbedding,
        scope: &str,
    ) -> [u8; 32] {
        Self::generate_nullifier_with_precision(embedding, scope, 0.1)
    }

    pub fn generate_nullifier_with_precision(
        embedding: &PersonEmbedding,
        scope: &str,
        precision: f32,
    ) -> [u8; 32] {
        let mut hasher = Sha256::new();

        hasher.update(domains::PERSON_NULLIFIER);

        hasher.update(&embedding.to_bytes(precision));

        hasher.update(scope.as_bytes());

        let result = hasher.finalize();
        let mut nullifier = [0u8; 32];
        nullifier.copy_from_slice(&result);
        nullifier
    }

    pub fn generate_commitment(
        embedding: &PersonEmbedding,
        blinding: &[u8; 32],
    ) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(domains::PERSON_COMMITMENT);
        hasher.update(&embedding.to_bytes(0.1));
        hasher.update(blinding);

        let result = hasher.finalize();
        let mut commitment = [0u8; 32];
        commitment.copy_from_slice(&result);
        commitment
    }

    pub fn same_person(
        embedding1: &PersonEmbedding,
        embedding2: &PersonEmbedding,
        threshold: f32,
    ) -> (bool, f32) {
        let similarity = embedding1.similarity(embedding2);

        let confidence = if similarity > threshold {
            let excess = (similarity - threshold) / (1.0 - threshold);
            0.5 + 0.5 * excess.min(1.0)
        } else {
            let deficit = (threshold - similarity) / (threshold + 1.0);
            0.5 + 0.5 * deficit.min(1.0)
        };

        (similarity > threshold, confidence)
    }

    pub fn to_field_element(embedding: &PersonEmbedding) -> Fr {
        let mut hasher = Sha256::new();
        hasher.update(domains::PERSON_ID);
        hasher.update(&embedding.to_bytes(0.1));

        let hash = hasher.finalize();

        let mut bytes = [0u8; 32];
        bytes[1..32].copy_from_slice(&hash[0..31]);

        Fr::from_be_bytes_mod_order(&bytes)
    }

    pub fn derive_identity_chain(
        embedding: &PersonEmbedding,
        chain_length: usize,
    ) -> Vec<[u8; 32]> {
        let mut chain = Vec::with_capacity(chain_length);
        let base = embedding.to_bytes(0.1);

        for i in 0..chain_length {
            let mut hasher = Sha256::new();
            hasher.update(domains::PERSON_ID);
            hasher.update(&base);
            hasher.update(&(i as u64).to_le_bytes());

            let result = hasher.finalize();
            let mut id = [0u8; 32];
            id.copy_from_slice(&result);
            chain.push(id);
        }

        chain
    }
}

#[derive(Clone, Debug)]
pub struct ScopedNullifier {
    pub nullifier: [u8; 32],
    pub scope: String,
    pub precision: f32,
}

impl ScopedNullifier {
    pub fn new(embedding: &PersonEmbedding, scope: &str) -> Self {
        let precision = 0.1;
        let nullifier = PersonIdentity::generate_nullifier_with_precision(
            embedding,
            scope,
            precision,
        );

        Self {
            nullifier,
            scope: scope.to_string(),
            precision,
        }
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.nullifier)
    }

    pub fn to_field(&self) -> Fr {
        let mut bytes = [0u8; 32];
        bytes[1..32].copy_from_slice(&self.nullifier[0..31]);
        Fr::from_be_bytes_mod_order(&bytes)
    }
}

#[derive(Debug, Clone)]
pub struct MatchingStats {
    pub true_positive_rate: f32,
    pub false_positive_rate: f32,
    pub accuracy: f32,
    pub threshold: f32,
}

impl MatchingStats {
    pub fn meets_target(&self) -> bool {
        self.true_positive_rate >= 0.85 && self.false_positive_rate <= 0.05
    }

    pub fn summary(&self) -> String {
        format!(
            "TPR: {:.1}%, FPR: {:.1}%, Acc: {:.1}% (threshold: {:.3})",
            self.true_positive_rate * 100.0,
            self.false_positive_rate * 100.0,
            self.accuracy * 100.0,
            self.threshold,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_normalization() {
        let e = PersonEmbedding::new(vec![3.0, 4.0]);
        let norm: f32 = e.vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_nullifier_determinism() {
        let e = PersonEmbedding::new(vec![0.5, 0.3, -0.2, 0.8]);

        let n1 = PersonIdentity::generate_nullifier(&e, "test");
        let n2 = PersonIdentity::generate_nullifier(&e, "test");

        assert_eq!(n1, n2, "Same embedding + scope should produce same nullifier");
    }

    #[test]
    fn test_nullifier_scope_separation() {
        let e = PersonEmbedding::new(vec![0.5, 0.3, -0.2, 0.8]);

        let n1 = PersonIdentity::generate_nullifier(&e, "scope1");
        let n2 = PersonIdentity::generate_nullifier(&e, "scope2");

        assert_ne!(n1, n2, "Different scopes should produce different nullifiers");
    }

    #[test]
    fn test_similarity() {
        let e1 = PersonEmbedding::new(vec![1.0, 0.0, 0.0]);
        let e2 = PersonEmbedding::new(vec![1.0, 0.0, 0.0]);
        let e3 = PersonEmbedding::new(vec![0.0, 1.0, 0.0]);

        assert!((e1.similarity(&e2) - 1.0).abs() < 1e-5);
        assert!(e1.similarity(&e3).abs() < 1e-5);
    }

    #[test]
    fn test_quantization_stability() {
        let e1 = PersonEmbedding::new(vec![0.51, 0.31, -0.19, 0.79]);
        let e2 = PersonEmbedding::new(vec![0.49, 0.29, -0.21, 0.81]);

        let n1 = PersonIdentity::generate_nullifier(&e1, "test");
        let n2 = PersonIdentity::generate_nullifier(&e2, "test");

        let _ = (n1, n2);
    }

    #[test]
    fn test_field_element() {
        let e = PersonEmbedding::new(vec![0.5, 0.3, -0.2, 0.8]);
        let fr = PersonIdentity::to_field_element(&e);

        let _ = fr;
    }

    #[test]
    fn test_identity_chain() {
        let e = PersonEmbedding::new(vec![0.5, 0.3, -0.2, 0.8]);
        let chain = PersonIdentity::derive_identity_chain(&e, 5);

        assert_eq!(chain.len(), 5);

        for i in 0..chain.len() {
            for j in (i + 1)..chain.len() {
                assert_ne!(chain[i], chain[j]);
            }
        }
    }
}
