
use crate::field::Fr;
use crate::matrix::GraphLaplacian;
use crate::reaction::{ReactionParams, apply_reaction};
use crate::diffusion::{DiffusionParams, diffuse_both};
use crate::params::TuringParams;
use crate::poseidon::hash_2;
use crate::{TuringError, Result};
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MorphogenState {
    pub u: Vec<Fr>,
    pub v: Vec<Fr>,
}

impl MorphogenState {
    pub fn new(u: Vec<Fr>, v: Vec<Fr>) -> Result<Self> {
        if u.len() != v.len() {
            return Err(TuringError::DimensionMismatch {
                expected: u.len(),
                got: v.len(),
            });
        }
        Ok(Self { u, v })
    }

    pub fn zeros(n: usize) -> Self {
        Self {
            u: vec![Fr::zero(); n],
            v: vec![Fr::zero(); n],
        }
    }

    pub fn from_biometric(
        x: &[f64],
        y: &[f64],
        theta: &[f64],
    ) -> Self {
        let n = x.len();
        assert_eq!(y.len(), n);
        assert_eq!(theta.len(), n);

        let u: Vec<Fr> = (0..n).map(|i| {
            let x_scaled = (x[i] * 1000.0) as i64;
            let y_scaled = (y[i] * 1000.0) as i64;
            hash_2(Fr::from(x_scaled), Fr::from(y_scaled))
        }).collect();

        let v: Vec<Fr> = (0..n).map(|i| {
            let theta_scaled = (theta[i] * 1000.0) as i64;
            hash_2(Fr::from(theta_scaled), Fr::from_u64(i as u64))
        }).collect();

        Self { u, v }
    }

    pub fn len(&self) -> usize {
        self.u.len()
    }

    pub fn is_empty(&self) -> bool {
        self.u.is_empty()
    }

    pub fn distance(&self, other: &Self) -> Fr {
        assert_eq!(self.len(), other.len());

        let mut sum = Fr::zero();
        for i in 0..self.len() {
            let du = self.u[i] - other.u[i];
            let dv = self.v[i] - other.v[i];
            sum = sum + du.square() + dv.square();
        }
        sum
    }

    pub fn hash(&self) -> Fr {
        let mut acc = Fr::zero();
        for i in 0..self.len() {
            let combined = hash_2(self.u[i], self.v[i]);
            acc = hash_2(acc, combined);
        }
        acc
    }
}

pub struct TuringIterator<'a> {
    laplacian: &'a GraphLaplacian,
    reaction: &'a ReactionParams,
    diffusion: &'a DiffusionParams,
}

impl<'a> TuringIterator<'a> {
    pub fn new(
        laplacian: &'a GraphLaplacian,
        reaction: &'a ReactionParams,
        diffusion: &'a DiffusionParams,
    ) -> Self {
        Self { laplacian, reaction, diffusion }
    }

    pub fn from_params(
        laplacian: &'a GraphLaplacian,
        params: &'a TuringParams,
    ) -> Self {
        Self {
            laplacian,
            reaction: &params.reaction,
            diffusion: &params.diffusion,
        }
    }

    pub fn step(&self, state: &MorphogenState) -> Result<MorphogenState> {
        let n = state.len();

        let (diff_u, diff_v) = diffuse_both(
            &state.u,
            &state.v,
            self.laplacian,
            self.diffusion,
        )?;

        let (react_u, react_v) = apply_reaction(
            &state.u,
            &state.v,
            self.reaction,
        );

        let dt = self.diffusion.dt;

        let u_new: Vec<Fr> = (0..n).map(|i| {
            state.u[i] + diff_u[i] + dt * react_u[i]
        }).collect();

        let v_new: Vec<Fr> = (0..n).map(|i| {
            state.v[i] + diff_v[i] + dt * react_v[i]
        }).collect();

        MorphogenState::new(u_new, v_new)
    }

    pub fn iterate(&self, initial: &MorphogenState, steps: usize) -> Result<MorphogenState> {
        let mut state = initial.clone();

        for _ in 0..steps {
            state = self.step(&state)?;
        }

        Ok(state)
    }

    pub fn iterate_until_convergence(
        &self,
        initial: &MorphogenState,
        max_steps: usize,
        _threshold: Fr,
    ) -> Result<(MorphogenState, usize)> {
        let mut state = initial.clone();
        let mut prev_state = state.clone();

        for step in 0..max_steps {
            state = self.step(&state)?;

            if step % 10 == 9 {
                let dist = state.distance(&prev_state);
                if dist.is_zero() {
                    return Ok((state, step + 1));
                }
                prev_state = state.clone();
            }
        }

        Ok((state, max_steps))
    }
}

pub fn turing_iterate(
    initial: &MorphogenState,
    laplacian: &GraphLaplacian,
    params: &TuringParams,
) -> Result<MorphogenState> {
    let iterator = TuringIterator::from_params(laplacian, params);
    iterator.iterate(initial, params.iterations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matrix::GraphLaplacian;

    fn triangle_laplacian() -> GraphLaplacian {
        let edges = vec![
            (0, 1, Fr::one()),
            (1, 2, Fr::one()),
            (0, 2, Fr::one()),
        ];
        GraphLaplacian::from_edges(3, &edges, true)
    }

    #[test]
    fn test_single_step() {
        let lap = triangle_laplacian();
        let params = TuringParams::default();
        let iterator = TuringIterator::from_params(&lap, &params);

        let initial = MorphogenState {
            u: vec![Fr::from_u64(1), Fr::from_u64(2), Fr::from_u64(3)],
            v: vec![Fr::from_u64(1), Fr::from_u64(1), Fr::from_u64(1)],
        };

        let next = iterator.step(&initial).unwrap();

        assert_ne!(next.u, initial.u);
    }

    #[test]
    fn test_multiple_iterations() {
        let lap = triangle_laplacian();
        let params = TuringParams::fast();
        let iterator = TuringIterator::from_params(&lap, &params);

        let initial = MorphogenState {
            u: vec![Fr::from_u64(1), Fr::from_u64(10), Fr::from_u64(1)],
            v: vec![Fr::from_u64(1), Fr::from_u64(1), Fr::from_u64(1)],
        };

        let final_state = iterator.iterate(&initial, 16).unwrap();

        assert_eq!(final_state.len(), initial.len());
    }

    #[test]
    fn test_deterministic() {
        let lap = triangle_laplacian();
        let params = TuringParams::fast();

        let initial = MorphogenState {
            u: vec![Fr::from_u64(5), Fr::from_u64(7), Fr::from_u64(3)],
            v: vec![Fr::from_u64(2), Fr::from_u64(4), Fr::from_u64(6)],
        };

        let result1 = turing_iterate(&initial, &lap, &params).unwrap();
        let result2 = turing_iterate(&initial, &lap, &params).unwrap();

        assert_eq!(result1.u, result2.u);
        assert_eq!(result1.v, result2.v);
    }

    #[test]
    fn test_sensitive_to_input() {
        let lap = triangle_laplacian();
        let params = TuringParams::fast();

        let initial1 = MorphogenState {
            u: vec![Fr::from_u64(5), Fr::from_u64(7), Fr::from_u64(3)],
            v: vec![Fr::from_u64(2), Fr::from_u64(4), Fr::from_u64(6)],
        };

        let initial2 = MorphogenState {
            u: vec![Fr::from_u64(5), Fr::from_u64(8), Fr::from_u64(3)],
            v: vec![Fr::from_u64(2), Fr::from_u64(4), Fr::from_u64(6)],
        };

        let result1 = turing_iterate(&initial1, &lap, &params).unwrap();
        let result2 = turing_iterate(&initial2, &lap, &params).unwrap();

        assert_ne!(result1.u, result2.u);
    }

    #[test]
    fn test_from_biometric() {
        let x = vec![100.0, 200.0, 150.0];
        let y = vec![100.0, 100.0, 200.0];
        let theta = vec![0.0, 1.57, 3.14];

        let state = MorphogenState::from_biometric(&x, &y, &theta);

        assert_eq!(state.len(), 3);
        assert_ne!(state.u[0], state.u[1]);
        assert_ne!(state.v[0], state.v[1]);
    }
}
