
use crate::field::Fr;
use crate::matrix::GraphLaplacian;
use crate::iterate::{MorphogenState, turing_iterate};
use crate::params::TuringParams;
use crate::poseidon::hash_many;
use crate::Result;

pub struct TuringKdf;

impl TuringKdf {
    pub fn derive(
        seed: Fr,
        context: &str,
        laplacian: &GraphLaplacian,
        params: &TuringParams,
    ) -> Result<Fr> {
        let n = laplacian.dim();

        let (u0, v0) = Self::expand_seed(seed, context, n);

        let initial = MorphogenState::new(u0, v0)?;

        let final_state = turing_iterate(&initial, laplacian, params)?;

        Ok(final_state.hash())
    }

    pub fn derive_many(
        seed: Fr,
        context: &str,
        count: usize,
        laplacian: &GraphLaplacian,
        params: &TuringParams,
    ) -> Result<Vec<Fr>> {
        let mut keys = Vec::with_capacity(count);

        for i in 0..count {
            let indexed_context = format!("{}:{}", context, i);
            let key = Self::derive(seed, &indexed_context, laplacian, params)?;
            keys.push(key);
        }

        Ok(keys)
    }

    pub fn derive_nullifier(
        seed: Fr,
        scope: &str,
        laplacian: &GraphLaplacian,
        params: &TuringParams,
    ) -> Result<Fr> {
        let context = format!("nullifier:{}", scope);
        Self::derive(seed, &context, laplacian, params)
    }

    pub fn derive_credential(
        seed: Fr,
        app_id: &str,
        laplacian: &GraphLaplacian,
        params: &TuringParams,
    ) -> Result<Fr> {
        let context = format!("credential:{}", app_id);
        Self::derive(seed, &context, laplacian, params)
    }

    fn expand_seed(seed: Fr, context: &str, n: usize) -> (Vec<Fr>, Vec<Fr>) {
        let context_hash = Self::hash_context(context);

        let mut u0 = Vec::with_capacity(n);
        let mut v0 = Vec::with_capacity(n);

        for i in 0..n {
            let u_i = hash_many(&[seed, context_hash, Fr::from_u64(i as u64), Fr::zero()]);
            let v_i = hash_many(&[seed, context_hash, Fr::from_u64(i as u64), Fr::one()]);

            u0.push(u_i);
            v0.push(v_i);
        }

        (u0, v0)
    }

    fn hash_context(context: &str) -> Fr {
        let elements: Vec<Fr> = context
            .bytes()
            .map(|b| Fr::from_u64(b as u64))
            .collect();
        hash_many(&elements)
    }
}

pub struct BiometricKdf;

impl BiometricKdf {
    pub fn derive_master(
        x: &[f64],
        y: &[f64],
        theta: &[f64],
        laplacian: &GraphLaplacian,
        params: &TuringParams,
    ) -> Result<Fr> {
        let initial = MorphogenState::from_biometric(x, y, theta);
        let final_state = turing_iterate(&initial, laplacian, params)?;
        Ok(final_state.hash())
    }

    pub fn derive_for_context(
        x: &[f64],
        y: &[f64],
        theta: &[f64],
        context: &str,
        laplacian: &GraphLaplacian,
        params: &TuringParams,
    ) -> Result<Fr> {
        let master = Self::derive_master(x, y, theta, laplacian, params)?;

        TuringKdf::derive(master, context, laplacian, params)
    }

    pub fn derive_nullifier(
        x: &[f64],
        y: &[f64],
        theta: &[f64],
        scope: &str,
        laplacian: &GraphLaplacian,
        params: &TuringParams,
    ) -> Result<Fr> {
        let master = Self::derive_master(x, y, theta, laplacian, params)?;
        TuringKdf::derive_nullifier(master, scope, laplacian, params)
    }
}

pub struct HierarchicalKdf;

impl HierarchicalKdf {
    pub fn derive_child(
        parent: Fr,
        child_index: u64,
        laplacian: &GraphLaplacian,
        params: &TuringParams,
    ) -> Result<Fr> {
        let context = format!("child:{}", child_index);
        TuringKdf::derive(parent, &context, laplacian, params)
    }

    pub fn derive_path(
        master: Fr,
        path: &[u64],
        laplacian: &GraphLaplacian,
        params: &TuringParams,
    ) -> Result<Fr> {
        let mut current = master;

        for &index in path {
            current = Self::derive_child(current, index, laplacian, params)?;
        }

        Ok(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_laplacian() -> GraphLaplacian {
        let edges = vec![
            (0, 1, Fr::one()),
            (1, 2, Fr::one()),
            (2, 3, Fr::one()),
            (3, 0, Fr::one()),
        ];
        GraphLaplacian::from_edges(4, &edges, true)
    }

    #[test]
    fn test_deterministic() {
        let lap = test_laplacian();
        let params = TuringParams::fast();
        let seed = Fr::from_u64(12345);

        let key1 = TuringKdf::derive(seed, "test", &lap, &params).unwrap();
        let key2 = TuringKdf::derive(seed, "test", &lap, &params).unwrap();

        assert_eq!(key1, key2);
    }

    #[test]
    fn test_context_separation() {
        let lap = test_laplacian();
        let params = TuringParams::fast();
        let seed = Fr::from_u64(12345);

        let key_a = TuringKdf::derive(seed, "app_a", &lap, &params).unwrap();
        let key_b = TuringKdf::derive(seed, "app_b", &lap, &params).unwrap();

        assert_ne!(key_a, key_b);
    }

    #[test]
    fn test_different_seeds() {
        let lap = test_laplacian();
        let params = TuringParams::fast();

        let key1 = TuringKdf::derive(Fr::from_u64(111), "test", &lap, &params).unwrap();
        let key2 = TuringKdf::derive(Fr::from_u64(222), "test", &lap, &params).unwrap();

        assert_ne!(key1, key2);
    }

    #[test]
    fn test_derive_many() {
        let lap = test_laplacian();
        let params = TuringParams::fast();
        let seed = Fr::from_u64(42);

        let keys = TuringKdf::derive_many(seed, "session", 5, &lap, &params).unwrap();

        assert_eq!(keys.len(), 5);

        for i in 0..keys.len() {
            for j in (i + 1)..keys.len() {
                assert_ne!(keys[i], keys[j]);
            }
        }
    }

    #[test]
    fn test_nullifier_unlinkable() {
        let lap = test_laplacian();
        let params = TuringParams::fast();
        let seed = Fr::from_u64(42);

        let null_a = TuringKdf::derive_nullifier(seed, "scope_a", &lap, &params).unwrap();
        let null_b = TuringKdf::derive_nullifier(seed, "scope_b", &lap, &params).unwrap();

        assert_ne!(null_a, null_b);
    }

    #[test]
    fn test_hierarchical() {
        let lap = test_laplacian();
        let params = TuringParams::fast();
        let master = Fr::from_u64(42);

        let key_0 = HierarchicalKdf::derive_child(master, 0, &lap, &params).unwrap();
        let key_1 = HierarchicalKdf::derive_child(master, 1, &lap, &params).unwrap();

        assert_ne!(key_0, key_1);

        let path_key = HierarchicalKdf::derive_path(master, &[0, 1, 2], &lap, &params).unwrap();
        assert!(!path_key.is_zero());
    }

    #[test]
    fn test_biometric_kdf() {
        let lap = test_laplacian();
        let params = TuringParams::fast();

        let x = vec![100.0, 200.0, 150.0, 250.0];
        let y = vec![100.0, 100.0, 200.0, 200.0];
        let theta = vec![0.0, 1.57, 3.14, 0.0];

        let master = BiometricKdf::derive_master(&x, &y, &theta, &lap, &params).unwrap();
        assert!(!master.is_zero());

        let null = BiometricKdf::derive_nullifier(&x, &y, &theta, "voting", &lap, &params).unwrap();
        assert!(!null.is_zero());
        assert_ne!(master, null);
    }
}
