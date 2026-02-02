
pub mod field;
pub mod matrix;
pub mod reaction;
pub mod diffusion;
pub mod iterate;
pub mod hash;
pub mod commit;
pub mod kdf;
pub mod params;
pub mod poseidon;
pub mod equivalence;
pub mod sth;
pub mod sovereign;
pub mod person_identity;
pub mod person_circuit;
pub use field::Fr;
pub use matrix::{SparseMatrix, GraphLaplacian};
pub use reaction::{ReactionParams, reaction_u, reaction_v};
pub use diffusion::apply_laplacian;
pub use iterate::{TuringIterator, MorphogenState};
pub use hash::TuringHash;
pub use commit::{TuringCommit, TuringOpening};
pub use kdf::TuringKdf;
pub use params::TuringParams;

pub use sth::{SpectralTuringHash, STHParams, STHState, PersonalizedDiffusion};

pub use sovereign::{SovereignWallet, HardeningParams};

pub use person_identity::{PersonEmbedding, PersonIdentity, ScopedNullifier, MatchingStats};

pub use person_circuit::{
    CircuitEmbedding, PersonCommitment, PersonNullifier,
    PersonCircuitInputs, QuantizationConfig,
    would_match_nullifier, estimate_collision_probability,
};

#[derive(Debug, thiserror::Error)]
pub enum TuringError {
    #[error("Dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch { expected: usize, got: usize },

    #[error("Matrix is not square: {rows}x{cols}")]
    NotSquare { rows: usize, cols: usize },

    #[error("Iteration did not converge after {iterations} steps")]
    DidNotConverge { iterations: usize },

    #[error("Invalid parameter: {name} = {value}")]
    InvalidParameter { name: &'static str, value: String },

    #[error("Commitment verification failed")]
    VerificationFailed,
}

pub type Result<T> = std::result::Result<T, TuringError>;

#[cfg(test)]
mod tests {
    #[test]
    fn test_basic_iteration() {
        assert!(true);
    }
}
