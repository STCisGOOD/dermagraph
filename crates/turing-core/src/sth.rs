
use crate::field::Fr;
use crate::matrix::GraphLaplacian;
use crate::reaction::ReactionParams;
use crate::poseidon::{hash_2, hash_many};
use crate::Result;
use crate::TuringError;
use tracing::{debug, info};

#[derive(Clone, Debug)]
pub struct STHParams {
    pub full_rounds: usize,

    pub partial_rounds: usize,

    pub reaction: ReactionParams,

    pub alpha: Fr,

    pub base_d_u: Fr,

    pub base_d_v: Fr,
}

impl Default for STHParams {
    fn default() -> Self {
        Self::standard_128bit()
    }
}

impl STHParams {
    pub fn standard_128bit() -> Self {
        Self {
            full_rounds: 8,
            partial_rounds: 22,
            reaction: ReactionParams::crypto(),
            alpha: Fr::from_u64(1),
            base_d_u: Fr::from_u64(2),
            base_d_v: Fr::from_u64(1),
        }
    }

    pub fn fast() -> Self {
        Self {
            full_rounds: 4,
            partial_rounds: 4,
            reaction: ReactionParams::crypto(),
            alpha: Fr::from_u64(1),
            base_d_u: Fr::from_u64(2),
            base_d_v: Fr::from_u64(1),
        }
    }

    pub fn total_rounds(&self) -> usize {
        self.full_rounds + self.partial_rounds
    }
}

#[derive(Clone, Debug)]
pub struct PersonalizedDiffusion {
    pub d_u: Vec<Fr>,

    pub d_v: Vec<Fr>,
}

impl PersonalizedDiffusion {
    pub fn from_quantized_spectrum(
        quantized: &[u64],
        base_d_u: Fr,
        base_d_v: Fr,
    ) -> Self {
        let n = quantized.len();

        let scaling = Fr::from_u64(7);

        let d_u: Vec<Fr> = quantized.iter()
            .map(|&q| {
                let offset = Fr::from_u64(q) * scaling;
                base_d_u + offset
            })
            .collect();

        let d_v: Vec<Fr> = quantized.iter()
            .map(|&q| {
                let offset = Fr::from_u64(q * 3 + 5) * scaling;
                base_d_v + offset
            })
            .collect();

        debug!("Created personalized diffusion coefficients");

        Self { d_u, d_v }
    }

    pub fn dim(&self) -> usize {
        self.d_u.len()
    }
}

#[derive(Clone, Debug)]
pub struct STHState {
    pub u: Vec<Fr>,

    pub v: Vec<Fr>,
}

impl STHState {
    pub fn from_minutiae(
        x: &[f64],
        y: &[f64],
        theta: &[f64],
        quantized: &[u64],
    ) -> Self {
        let n = x.len();

        let mut u = Vec::with_capacity(n);
        let mut v = Vec::with_capacity(n);

        for i in 0..n {
            let xi = Fr::from_u64((x[i] * 1000.0) as u64);
            let yi = Fr::from_u64((y[i] * 1000.0) as u64);
            let ti = Fr::from_u64((theta[i] * 1000.0) as u64);

            u.push(hash_many(&[xi, yi, Fr::from_u64(i as u64)]));

            let qi = if i < quantized.len() {
                Fr::from_u64(quantized[i])
            } else {
                Fr::zero()
            };
            v.push(hash_many(&[ti, qi, Fr::from_u64(i as u64 + n as u64)]));
        }

        Self { u, v }
    }

    pub fn dim(&self) -> usize {
        self.u.len()
    }

    pub fn hash(&self) -> Fr {
        let mut all = Vec::with_capacity(self.u.len() + self.v.len());
        all.extend(&self.u);
        all.extend(&self.v);
        hash_many(&all)
    }
}

pub struct SpectralTuringHash;

impl SpectralTuringHash {
    pub fn compute(
        x: &[f64],
        y: &[f64],
        theta: &[f64],
        quantized: &[u64],
        laplacian: &GraphLaplacian,
        params: &STHParams,
    ) -> Result<Fr> {
        let n = x.len();

        if y.len() != n || theta.len() != n {
            return Err(TuringError::DimensionMismatch {
                expected: n,
                got: y.len().min(theta.len()),
            });
        }

        if laplacian.dim() != n {
            return Err(TuringError::DimensionMismatch {
                expected: n,
                got: laplacian.dim(),
            });
        }

        info!("Computing STH identity");

        let mut state = STHState::from_minutiae(x, y, theta, quantized);

        let diffusion = PersonalizedDiffusion::from_quantized_spectrum(
            quantized,
            params.base_d_u,
            params.base_d_v,
        );

        let half_full = params.full_rounds / 2;

        for r in 0..half_full {
            state = sth_full_round(&state, laplacian, &diffusion, &params.reaction, params.alpha, r)?;
        }

        for r in 0..params.partial_rounds {
            state = sth_partial_round(&state, laplacian, &diffusion, &params.reaction, params.alpha, r)?;
        }

        for r in half_full..params.full_rounds {
            state = sth_full_round(&state, laplacian, &diffusion, &params.reaction, params.alpha, r)?;
        }

        let identity = state.hash();

        debug!("STH identity computed successfully");

        Ok(identity)
    }

    pub fn compute_with_scope(
        x: &[f64],
        y: &[f64],
        theta: &[f64],
        quantized: &[u64],
        laplacian: &GraphLaplacian,
        params: &STHParams,
        scope: &str,
    ) -> Result<Fr> {
        let scope_hash = scope_to_field(scope);

        let mut scoped_quantized: Vec<u64> = quantized.to_vec();
        for (i, q) in scoped_quantized.iter_mut().enumerate() {
            let scope_offset = (scope_hash + Fr::from_u64(i as u64)).into_repr_u64() % 1000;
            *q = q.wrapping_add(scope_offset);
        }

        Self::compute(x, y, theta, &scoped_quantized, laplacian, params)
    }

    pub fn derive_nullifier(identity: Fr, scope: &str) -> Fr {
        let scope_hash = scope_to_field(scope);
        hash_2(identity, scope_hash)
    }

    pub fn derive_commitment(identity: Fr, salt: Fr) -> Fr {
        hash_2(identity, salt)
    }
}

fn sth_full_round(
    state: &STHState,
    laplacian: &GraphLaplacian,
    diffusion: &PersonalizedDiffusion,
    reaction: &ReactionParams,
    alpha: Fr,
    round: usize,
) -> Result<STHState> {
    let n = state.dim();
    let mut new_u = vec![Fr::zero(); n];
    let mut new_v = vec![Fr::zero(); n];

    let lu = laplacian.apply(&state.u)?;
    let lv = laplacian.apply(&state.v)?;

    let rc = Fr::from_u64((round * 7 + 3) as u64);

    for i in 0..n {
        let u = state.u[i];
        let v = state.v[i];
        let d_u = diffusion.d_u.get(i).cloned().unwrap_or(Fr::from_u64(2));
        let d_v = diffusion.d_v.get(i).cloned().unwrap_or(Fr::from_u64(1));

        let uv2 = u * v * v;

        let f = reaction.f;
        let k = reaction.k;

        new_u[i] = u + d_u * lu[i] + alpha * (f - u - uv2) + rc;
        new_v[i] = v + d_v * lv[i] + alpha * (uv2 - v * (f + k)) + rc;
    }

    Ok(STHState { u: new_u, v: new_v })
}

fn sth_partial_round(
    state: &STHState,
    laplacian: &GraphLaplacian,
    diffusion: &PersonalizedDiffusion,
    reaction: &ReactionParams,
    alpha: Fr,
    round: usize,
) -> Result<STHState> {
    let n = state.dim();
    let mut new_u = vec![Fr::zero(); n];
    let mut new_v = vec![Fr::zero(); n];

    let lu = laplacian.apply(&state.u)?;
    let lv = laplacian.apply(&state.v)?;

    let rc = Fr::from_u64((round * 11 + 7) as u64);

    for i in 0..n {
        let u = state.u[i];
        let v = state.v[i];
        let d_u = diffusion.d_u.get(i).cloned().unwrap_or(Fr::from_u64(2));
        let d_v = diffusion.d_v.get(i).cloned().unwrap_or(Fr::from_u64(1));

        if i == 0 {
            let uv2 = u * v * v;
            let f = reaction.f;
            let k = reaction.k;

            new_u[i] = u + d_u * lu[i] + alpha * (f - u - uv2) + rc;
            new_v[i] = v + d_v * lv[i] + alpha * (uv2 - v * (f + k)) + rc;
        } else {
            new_u[i] = u + d_u * lu[i] + rc;
            new_v[i] = v + d_v * lv[i] + rc;
        }
    }

    Ok(STHState { u: new_u, v: new_v })
}

fn scope_to_field(scope: &str) -> Fr {
    let bytes = scope.as_bytes();
    let elements: Vec<Fr> = bytes.iter()
        .map(|&b| Fr::from_u64(b as u64))
        .collect();
    hash_many(&elements)
}

trait FrRepr {
    fn into_repr_u64(self) -> u64;
}

impl FrRepr for Fr {
    fn into_repr_u64(self) -> u64 {
        use ark_ff::PrimeField;
        let repr = self.0.into_bigint();
        repr.0[0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matrix::GraphLaplacian;

    fn test_laplacian(n: usize) -> GraphLaplacian {
        let mut edges = Vec::new();
        for i in 0..n {
            let j = (i + 1) % n;
            edges.push((i, j, Fr::one()));
        }
        GraphLaplacian::from_edges(n, &edges, true)
    }

    fn mock_biometric(n: usize) -> (Vec<f64>, Vec<f64>, Vec<f64>, Vec<u64>) {
        let x: Vec<f64> = (0..n).map(|i| 100.0 + i as f64 * 10.0).collect();
        let y: Vec<f64> = (0..n).map(|i| 100.0 + (i as f64 * 7.0) % 50.0).collect();
        let theta: Vec<f64> = (0..n).map(|i| (i as f64 * 0.5) % std::f64::consts::PI).collect();
        let quantized: Vec<u64> = (0..n).map(|i| (i as u64 * 3 + 5)).collect();
        (x, y, theta, quantized)
    }

    #[test]
    fn test_sth_determinism() {
        let n = 8;
        let (x, y, theta, quantized) = mock_biometric(n);
        let laplacian = test_laplacian(n);
        let params = STHParams::fast();

        let hash1 = SpectralTuringHash::compute(&x, &y, &theta, &quantized, &laplacian, &params)
            .unwrap();
        let hash2 = SpectralTuringHash::compute(&x, &y, &theta, &quantized, &laplacian, &params)
            .unwrap();

        assert_eq!(hash1, hash2, "STH must be deterministic");
    }

    #[test]
    fn test_sth_different_inputs() {
        let n = 8;
        let (x1, y1, theta1, quantized1) = mock_biometric(n);

        let mut x2 = x1.clone();
        x2[0] += 0.1;

        let laplacian = test_laplacian(n);
        let params = STHParams::fast();

        let hash1 = SpectralTuringHash::compute(&x1, &y1, &theta1, &quantized1, &laplacian, &params)
            .unwrap();
        let hash2 = SpectralTuringHash::compute(&x2, &y1, &theta1, &quantized1, &laplacian, &params)
            .unwrap();

        assert_ne!(hash1, hash2, "Different inputs must produce different hashes");
    }

    #[test]
    fn test_sth_scope_separation() {
        let n = 8;
        let (x, y, theta, quantized) = mock_biometric(n);
        let laplacian = test_laplacian(n);
        let params = STHParams::fast();

        let hash_a = SpectralTuringHash::compute_with_scope(
            &x, &y, &theta, &quantized, &laplacian, &params, "app_a"
        ).unwrap();

        let hash_b = SpectralTuringHash::compute_with_scope(
            &x, &y, &theta, &quantized, &laplacian, &params, "app_b"
        ).unwrap();

        assert_ne!(hash_a, hash_b, "Different scopes must produce different hashes");
    }

    #[test]
    fn test_nullifier_derivation() {
        let identity = Fr::from_u64(12345);

        let null_a = SpectralTuringHash::derive_nullifier(identity, "scope_a");
        let null_b = SpectralTuringHash::derive_nullifier(identity, "scope_b");

        assert_ne!(null_a, null_b, "Different scopes must produce different nullifiers");

        let null_a2 = SpectralTuringHash::derive_nullifier(identity, "scope_a");
        assert_eq!(null_a, null_a2, "Same scope must produce same nullifier");
    }

    #[test]
    fn test_personalized_diffusion() {
        let quantized = vec![5, 10, 15, 20];
        let base_u = Fr::from_u64(2);
        let base_v = Fr::from_u64(1);

        let diffusion = PersonalizedDiffusion::from_quantized_spectrum(&quantized, base_u, base_v);

        assert_eq!(diffusion.dim(), 4);

        assert_ne!(diffusion.d_u[0], diffusion.d_u[1]);
        assert_ne!(diffusion.d_v[0], diffusion.d_v[1]);
    }

    #[test]
    fn test_sth_quantized_spectrum_matters() {
        let n = 8;
        let (x, y, theta, quantized1) = mock_biometric(n);

        let quantized2: Vec<u64> = quantized1.iter().map(|&q| q + 10).collect();

        let laplacian = test_laplacian(n);
        let params = STHParams::fast();

        let hash1 = SpectralTuringHash::compute(&x, &y, &theta, &quantized1, &laplacian, &params)
            .unwrap();
        let hash2 = SpectralTuringHash::compute(&x, &y, &theta, &quantized2, &laplacian, &params)
            .unwrap();

        assert_ne!(hash1, hash2, "Different spectrum must produce different hash");
    }
}
