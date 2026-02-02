
use crate::field::Fr;
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReactionParams {
    pub f: Fr,
    pub k: Fr,
}

impl Default for ReactionParams {
    fn default() -> Self {
        Self {
            f: Fr::from_u64(40),
            k: Fr::from_u64(62),
        }
    }
}

impl ReactionParams {
    pub fn new(f: u64, k: u64) -> Self {
        Self {
            f: Fr::from_u64(f),
            k: Fr::from_u64(k),
        }
    }

    pub fn crypto() -> Self {
        Self {
            f: Fr::from_u64(40),
            k: Fr::from_u64(62),
        }
    }

    pub fn fast() -> Self {
        Self {
            f: Fr::from_u64(35),
            k: Fr::from_u64(60),
        }
    }
}

#[inline]
pub fn reaction_u(u: Fr, v: Fr, params: &ReactionParams) -> Fr {
    let uv2 = u * v * v;
    params.f - params.f * u - uv2
}

#[inline]
pub fn reaction_v(u: Fr, v: Fr, params: &ReactionParams) -> Fr {
    let uv2 = u * v * v;
    let decay = (params.f + params.k) * v;
    uv2 - decay
}

pub fn apply_reaction(u: &[Fr], v: &[Fr], params: &ReactionParams) -> (Vec<Fr>, Vec<Fr>) {
    let n = u.len();
    debug_assert_eq!(n, v.len(), "u and v must have same length");

    let mut du = Vec::with_capacity(n);
    let mut dv = Vec::with_capacity(n);

    for i in 0..n {
        du.push(reaction_u(u[i], v[i], params));
        dv.push(reaction_v(u[i], v[i], params));
    }

    (du, dv)
}

pub fn reaction_jacobian(u: Fr, v: Fr, params: &ReactionParams) -> [[Fr; 2]; 2] {
    let v2 = v * v;
    let two_uv = Fr::from_u64(2) * u * v;

    let df_u_du = -v2 - params.f;
    let df_u_dv = -two_uv;
    let df_v_du = v2;
    let df_v_dv = two_uv - (params.f + params.k);

    [[df_u_du, df_u_dv], [df_v_du, df_v_dv]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reaction_u_equilibrium() {
        let params = ReactionParams::default();

        let result = reaction_u(Fr::one(), Fr::zero(), &params);
        assert_eq!(result, Fr::zero());
    }

    #[test]
    fn test_reaction_v_zero_u() {
        let params = ReactionParams::default();

        let v = Fr::from_u64(100);
        let result = reaction_v(Fr::zero(), v, &params);
        let expected = -(params.f + params.k) * v;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_bivariate_asymmetry() {
        let u1 = Fr::from_u64(2);
        let v1 = Fr::from_u64(3);
        let u2 = Fr::from_u64(3);
        let v2 = Fr::from_u64(2);

        let product1 = u1 * v1 * v1;
        let product2 = u2 * v2 * v2;

        assert_ne!(product1, product2);
    }

    #[test]
    fn test_apply_reaction_vectors() {
        let params = ReactionParams::default();
        let u = vec![Fr::from_u64(1), Fr::from_u64(2)];
        let v = vec![Fr::from_u64(1), Fr::from_u64(1)];

        let (du, dv) = apply_reaction(&u, &v, &params);

        assert_eq!(du.len(), 2);
        assert_eq!(dv.len(), 2);
    }
}
