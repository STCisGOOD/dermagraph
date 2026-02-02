
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WitnessError {
    #[error("Too many minutiae: {count} > {max}")]
    TooManyMinutiae { count: usize, max: usize },

    #[error("Too many Laplacian entries: {count} > {max}")]
    TooManyLaplacianEntries { count: usize, max: usize },

    #[error("Spectrum size mismatch: {count} vs expected {expected}")]
    SpectrumSizeMismatch { count: usize, expected: usize },

    #[error("Invalid Merkle proof: {reason}")]
    InvalidMerkleProof { reason: String },

    #[error("Merkle tree error: {reason}")]
    MerkleTreeError { reason: String },

    #[error("Field conversion error: {reason}")]
    FieldConversionError { reason: String },

    #[error("Identity hash error: {reason}")]
    IdentityHashError { reason: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML error: {0}")]
    Toml(#[from] toml::ser::Error),

    #[error("Biometric error: {0}")]
    Biometric(#[from] biometric_extract::ExtractError),
}

pub type Result<T> = std::result::Result<T, WitnessError>;
