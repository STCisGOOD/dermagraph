
use crate::field::Fr;
use crate::matrix::GraphLaplacian;
use crate::iterate::MorphogenState;
use crate::hash::TuringHash;
use crate::params::TuringParams;
use crate::poseidon::{hash_2, hash_many};
use crate::Result;
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TuringOpening {
    pub u0: Vec<Fr>,
    pub v0: Vec<Fr>,
}

impl TuringOpening {
    pub fn from_state(state: &MorphogenState) -> Self {
        Self {
            u0: state.u.clone(),
            v0: state.v.clone(),
        }
    }

    pub fn to_state(&self) -> Result<MorphogenState> {
        MorphogenState::new(self.u0.clone(), self.v0.clone())
    }
}

pub struct TuringCommit;

impl TuringCommit {
    pub fn commit(
        secret: &[Fr],
        laplacian: &GraphLaplacian,
        params: &TuringParams,
    ) -> Result<(Fr, TuringOpening)> {
        let n = laplacian.dim();

        let mut u0 = vec![Fr::zero(); n];
        let mut v0 = vec![Fr::zero(); n];

        for (i, &s) in secret.iter().enumerate() {
            if i < n {
                u0[i] = s;
                v0[i] = hash_2(s, Fr::from_u64(i as u64));
            }
        }

        for i in secret.len()..n {
            u0[i] = hash_many(&[Fr::from_u64(i as u64)]);
            v0[i] = hash_many(&[Fr::from_u64(i as u64 + n as u64)]);
        }

        let initial = MorphogenState::new(u0.clone(), v0.clone())?;

        let commitment = TuringHash::compute(&initial, laplacian, params)?;

        let opening = TuringOpening { u0, v0 };

        Ok((commitment, opening))
    }

    pub fn commit_state(
        initial: &MorphogenState,
        laplacian: &GraphLaplacian,
        params: &TuringParams,
    ) -> Result<(Fr, TuringOpening)> {
        let commitment = TuringHash::compute(initial, laplacian, params)?;
        let opening = TuringOpening::from_state(initial);
        Ok((commitment, opening))
    }

    pub fn verify(
        commitment: Fr,
        opening: &TuringOpening,
        laplacian: &GraphLaplacian,
        params: &TuringParams,
    ) -> Result<bool> {
        let state = opening.to_state()?;
        TuringHash::verify(&state, laplacian, params, commitment)
    }

    pub fn commit_randomized(
        secret: &[Fr],
        randomness: &[Fr],
        laplacian: &GraphLaplacian,
        params: &TuringParams,
    ) -> Result<(Fr, TuringOpening)> {
        let n = laplacian.dim();

        let mut u0 = vec![Fr::zero(); n];
        let mut v0 = vec![Fr::zero(); n];

        for i in 0..n {
            let s = secret.get(i).copied().unwrap_or(Fr::zero());
            let r = randomness.get(i).copied().unwrap_or(Fr::zero());

            u0[i] = s + r;
            v0[i] = hash_many(&[s, r, Fr::from_u64(i as u64)]);
        }

        let initial = MorphogenState::new(u0.clone(), v0.clone())?;
        let commitment = TuringHash::compute(&initial, laplacian, params)?;

        Ok((commitment, TuringOpening { u0, v0 }))
    }
}

pub struct TuringVectorCommit;

impl TuringVectorCommit {
    pub fn commit(
        values: &[Fr],
        laplacian: &GraphLaplacian,
        params: &TuringParams,
    ) -> Result<(Fr, TuringOpening)> {
        TuringCommit::commit(values, laplacian, params)
    }

    pub fn prove_element(
        opening: &TuringOpening,
        index: usize,
    ) -> Result<Fr> {
        let proof = hash_2(
            opening.u0.get(index).copied().unwrap_or(Fr::zero()),
            Fr::from_u64(index as u64),
        );
        Ok(proof)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_laplacian() -> GraphLaplacian {
        let edges = vec![
            (0, 1, Fr::one()),
            (1, 2, Fr::one()),
            (2, 0, Fr::one()),
        ];
        GraphLaplacian::from_edges(3, &edges, true)
    }

    #[test]
    fn test_commit_verify() {
        let lap = test_laplacian();
        let params = TuringParams::fast();

        let secret = vec![Fr::from_u64(42), Fr::from_u64(123)];
        let (commitment, opening) = TuringCommit::commit(&secret, &lap, &params).unwrap();

        assert!(TuringCommit::verify(commitment, &opening, &lap, &params).unwrap());
    }

    #[test]
    fn test_binding() {
        let lap = test_laplacian();
        let params = TuringParams::fast();

        let secret1 = vec![Fr::from_u64(42)];
        let secret2 = vec![Fr::from_u64(43)];

        let (c1, _) = TuringCommit::commit(&secret1, &lap, &params).unwrap();
        let (c2, _) = TuringCommit::commit(&secret2, &lap, &params).unwrap();

        assert_ne!(c1, c2);
    }

    #[test]
    fn test_cannot_open_to_wrong_value() {
        let lap = test_laplacian();
        let params = TuringParams::fast();

        let secret = vec![Fr::from_u64(42)];
        let (commitment, _) = TuringCommit::commit(&secret, &lap, &params).unwrap();

        let wrong_secret = vec![Fr::from_u64(43)];
        let (_, wrong_opening) = TuringCommit::commit(&wrong_secret, &lap, &params).unwrap();

        assert!(!TuringCommit::verify(commitment, &wrong_opening, &lap, &params).unwrap());
    }

    #[test]
    fn test_randomized_hiding() {
        let lap = test_laplacian();
        let params = TuringParams::fast();

        let secret = vec![Fr::from_u64(42)];
        let rand1 = vec![Fr::from_u64(1)];
        let rand2 = vec![Fr::from_u64(2)];

        let (c1, _) = TuringCommit::commit_randomized(&secret, &rand1, &lap, &params).unwrap();
        let (c2, _) = TuringCommit::commit_randomized(&secret, &rand2, &lap, &params).unwrap();

        assert_ne!(c1, c2);
    }
}
