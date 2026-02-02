
use crate::field::Fr;
use crate::reaction::ReactionParams;
use crate::diffusion::DiffusionParams;
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TuringParams {
    pub reaction: ReactionParams,
    #[serde(skip)]
    pub diffusion: DiffusionParams,
    pub iterations: usize,
}

impl Default for TuringParams {
    fn default() -> Self {
        Self {
            reaction: ReactionParams::default(),
            diffusion: DiffusionParams::default(),
            iterations: 64,
        }
    }
}

impl TuringParams {
    pub fn crypto() -> Self {
        Self {
            reaction: ReactionParams::crypto(),
            diffusion: DiffusionParams::crypto(),
            iterations: 64,
        }
    }

    pub fn fast() -> Self {
        Self {
            reaction: ReactionParams::fast(),
            diffusion: DiffusionParams::default(),
            iterations: 16,
        }
    }

    pub fn with_iterations(mut self, n: usize) -> Self {
        self.iterations = n;
        self
    }

    pub fn f(&self) -> Fr {
        self.reaction.f
    }

    pub fn k(&self) -> Fr {
        self.reaction.k
    }

    pub fn d_u(&self) -> Fr {
        self.diffusion.d_u
    }

    pub fn d_v(&self) -> Fr {
        self.diffusion.d_v
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.iterations == 0 {
            return Err("iterations must be > 0");
        }
        if self.diffusion.d_u.is_zero() {
            return Err("d_u must be > 0");
        }
        if self.diffusion.d_v.is_zero() {
            return Err("d_v must be > 0");
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct BiometricParams {
    pub turing: TuringParams,
    pub salt: Fr,
    pub version: u32,
}

impl Default for BiometricParams {
    fn default() -> Self {
        Self {
            turing: TuringParams::crypto(),
            salt: Fr::from_u64(0x44455244494E4F),
            version: 1,
        }
    }
}

impl BiometricParams {
    pub fn with_salt(mut self, salt: Fr) -> Self {
        self.salt = salt;
        self
    }
}

#[derive(Clone, Debug)]
pub struct NullifierParams {
    pub turing: TuringParams,
    pub domain: Fr,
}

impl Default for NullifierParams {
    fn default() -> Self {
        Self {
            turing: TuringParams::crypto(),
            domain: Fr::from_u64(0x6e756c6c69666965),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_valid() {
        let params = TuringParams::default();
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_crypto_valid() {
        let params = TuringParams::crypto();
        assert!(params.validate().is_ok());
        assert_eq!(params.iterations, 64);
    }

    #[test]
    fn test_gray_scott_params() {
        let params = TuringParams::crypto();

        assert_eq!(params.f(), Fr::from_u64(40));
        assert_eq!(params.k(), Fr::from_u64(62));
        assert_eq!(params.d_u(), Fr::from_u64(2));
        assert_eq!(params.d_v(), Fr::from_u64(1));
    }

    #[test]
    fn test_builder() {
        let params = TuringParams::default()
            .with_iterations(128);
        assert_eq!(params.iterations, 128);
    }
}
