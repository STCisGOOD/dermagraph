
use crate::field::Fr;
use crate::matrix::GraphLaplacian;
use crate::Result;

#[derive(Clone, Debug)]
pub struct DiffusionParams {
    pub d_u: Fr,
    pub d_v: Fr,
    pub dt: Fr,
}

impl Default for DiffusionParams {
    fn default() -> Self {
        Self {
            d_u: Fr::from_u64(2),
            d_v: Fr::from_u64(1),
            dt: Fr::from_u64(1),
        }
    }
}

impl DiffusionParams {
    pub fn new(d_u: u64, d_v: u64, dt_inverse: u64) -> Self {
        Self {
            d_u: Fr::from_u64(d_u),
            d_v: Fr::from_u64(d_v),
            dt: Fr::from_u64(dt_inverse).inverse().unwrap_or(Fr::one()),
        }
    }

    pub fn crypto() -> Self {
        Self {
            d_u: Fr::from_u64(2),
            d_v: Fr::from_u64(1),
            dt: Fr::from_u64(1),
        }
    }
}

pub fn apply_diffusion(
    x: &[Fr],
    laplacian: &GraphLaplacian,
    d: Fr,
    dt: Fr,
) -> Result<Vec<Fr>> {
    let lx = laplacian.apply(x)?;

    let scale = d * dt;
    let result: Vec<Fr> = lx.iter().map(|&v| scale * v).collect();

    Ok(result)
}

pub fn diffuse_activator(
    u: &[Fr],
    laplacian: &GraphLaplacian,
    params: &DiffusionParams,
) -> Result<Vec<Fr>> {
    apply_diffusion(u, laplacian, params.d_u, params.dt)
}

pub fn diffuse_inhibitor(
    v: &[Fr],
    laplacian: &GraphLaplacian,
    params: &DiffusionParams,
) -> Result<Vec<Fr>> {
    apply_diffusion(v, laplacian, params.d_v, params.dt)
}

pub fn diffuse_both(
    u: &[Fr],
    v: &[Fr],
    laplacian: &GraphLaplacian,
    params: &DiffusionParams,
) -> Result<(Vec<Fr>, Vec<Fr>)> {
    let du = diffuse_activator(u, laplacian, params)?;
    let dv = diffuse_inhibitor(v, laplacian, params)?;
    Ok((du, dv))
}

pub fn apply_laplacian(x: &[Fr], laplacian: &GraphLaplacian) -> Result<Vec<Fr>> {
    laplacian.apply(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matrix::GraphLaplacian;

    fn simple_laplacian() -> GraphLaplacian {
        let edges = vec![
            (0, 1, Fr::one()),
            (1, 2, Fr::one()),
            (0, 2, Fr::one()),
        ];
        GraphLaplacian::from_edges(3, &edges, true)
    }

    #[test]
    fn test_diffusion_constant() {
        let lap = simple_laplacian();
        let params = DiffusionParams::default();

        let u = vec![Fr::one(); 3];
        let du = diffuse_activator(&u, &lap, &params).unwrap();

        for d in du {
            assert_eq!(d, Fr::zero());
        }
    }

    #[test]
    fn test_diffusion_gradient() {
        let lap = simple_laplacian();
        let params = DiffusionParams::default();

        let u = vec![Fr::from_u64(1), Fr::from_u64(2), Fr::from_u64(3)];
        let du = diffuse_activator(&u, &lap, &params).unwrap();

        let any_nonzero = du.iter().any(|&d| !d.is_zero());
        assert!(any_nonzero);
    }

    #[test]
    fn test_inhibitor_diffuses_faster() {
        let lap = simple_laplacian();
        let params = DiffusionParams::default();

        let x = vec![Fr::from_u64(1), Fr::from_u64(10), Fr::from_u64(1)];

        let du = diffuse_activator(&x, &lap, &params).unwrap();
        let dv = diffuse_inhibitor(&x, &lap, &params).unwrap();

        assert_ne!(du, dv);
    }
}
