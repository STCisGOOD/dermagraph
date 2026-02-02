
use crate::field::Fr;
use crate::matrix::GraphLaplacian;
use crate::iterate::{MorphogenState, turing_iterate};
use crate::params::TuringParams;
use crate::poseidon::{hash_2, hash_many};
use crate::Result;

pub struct TuringHash;

impl TuringHash {
    pub fn compute(
        initial: &MorphogenState,
        laplacian: &GraphLaplacian,
        params: &TuringParams,
    ) -> Result<Fr> {
        let final_state = turing_iterate(initial, laplacian, params)?;

        Ok(final_state.hash())
    }

    pub fn from_biometric(
        x: &[f64],
        y: &[f64],
        theta: &[f64],
        laplacian: &GraphLaplacian,
        params: &TuringParams,
    ) -> Result<Fr> {
        let initial = MorphogenState::from_biometric(x, y, theta);
        Self::compute(&initial, laplacian, params)
    }

    pub fn compute_with_context(
        initial: &MorphogenState,
        laplacian: &GraphLaplacian,
        params: &TuringParams,
        context: &str,
    ) -> Result<Fr> {
        let context_hash = context_to_field(context);

        let mut modified = initial.clone();
        for (i, u) in modified.u.iter_mut().enumerate() {
            *u = *u + context_hash * Fr::from_u64(i as u64 + 1);
        }

        Self::compute(&modified, laplacian, params)
    }

    pub fn verify(
        initial: &MorphogenState,
        laplacian: &GraphLaplacian,
        params: &TuringParams,
        expected: Fr,
    ) -> Result<bool> {
        let computed = Self::compute(initial, laplacian, params)?;
        Ok(computed == expected)
    }
}

fn context_to_field(context: &str) -> Fr {
    let bytes = context.as_bytes();
    let elements: Vec<Fr> = bytes.iter()
        .map(|&b| Fr::from_u64(b as u64))
        .collect();
    hash_many(&elements)
}

pub fn hash_fields(
    elements: &[Fr],
    laplacian: &GraphLaplacian,
    params: &TuringParams,
) -> Result<Fr> {
    let n = laplacian.dim();

    let mut u = vec![Fr::zero(); n];
    let mut v = vec![Fr::zero(); n];

    for (i, &elem) in elements.iter().enumerate() {
        if i < n {
            u[i] = elem;
            v[i] = hash_2(elem, Fr::from_u64(i as u64));
        }
    }

    let state = MorphogenState::new(u, v)?;
    TuringHash::compute(&state, laplacian, params)
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
            (0, 2, Fr::one()),
        ];
        GraphLaplacian::from_edges(4, &edges, true)
    }

    #[test]
    fn test_deterministic() {
        let lap = test_laplacian();
        let params = TuringParams::fast();

        let initial = MorphogenState {
            u: vec![Fr::from_u64(1), Fr::from_u64(2), Fr::from_u64(3), Fr::from_u64(4)],
            v: vec![Fr::from_u64(4), Fr::from_u64(3), Fr::from_u64(2), Fr::from_u64(1)],
        };

        let hash1 = TuringHash::compute(&initial, &lap, &params).unwrap();
        let hash2 = TuringHash::compute(&initial, &lap, &params).unwrap();

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_different_inputs_different_hashes() {
        let lap = test_laplacian();
        let params = TuringParams::fast();

        let initial1 = MorphogenState {
            u: vec![Fr::from_u64(1), Fr::from_u64(2), Fr::from_u64(3), Fr::from_u64(4)],
            v: vec![Fr::from_u64(4), Fr::from_u64(3), Fr::from_u64(2), Fr::from_u64(1)],
        };

        let initial2 = MorphogenState {
            u: vec![Fr::from_u64(1), Fr::from_u64(2), Fr::from_u64(3), Fr::from_u64(5)],
            v: vec![Fr::from_u64(4), Fr::from_u64(3), Fr::from_u64(2), Fr::from_u64(1)],
        };

        let hash1 = TuringHash::compute(&initial1, &lap, &params).unwrap();
        let hash2 = TuringHash::compute(&initial2, &lap, &params).unwrap();

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_context_separation() {
        let lap = test_laplacian();
        let params = TuringParams::fast();

        let initial = MorphogenState {
            u: vec![Fr::from_u64(1), Fr::from_u64(2), Fr::from_u64(3), Fr::from_u64(4)],
            v: vec![Fr::from_u64(4), Fr::from_u64(3), Fr::from_u64(2), Fr::from_u64(1)],
        };

        let hash_a = TuringHash::compute_with_context(&initial, &lap, &params, "app_a").unwrap();
        let hash_b = TuringHash::compute_with_context(&initial, &lap, &params, "app_b").unwrap();

        assert_ne!(hash_a, hash_b);
    }

    #[test]
    fn test_verify() {
        let lap = test_laplacian();
        let params = TuringParams::fast();

        let initial = MorphogenState {
            u: vec![Fr::from_u64(5), Fr::from_u64(6), Fr::from_u64(7), Fr::from_u64(8)],
            v: vec![Fr::from_u64(1), Fr::from_u64(1), Fr::from_u64(1), Fr::from_u64(1)],
        };

        let hash = TuringHash::compute(&initial, &lap, &params).unwrap();

        assert!(TuringHash::verify(&initial, &lap, &params, hash).unwrap());
        assert!(!TuringHash::verify(&initial, &lap, &params, hash + Fr::one()).unwrap());
    }
}
