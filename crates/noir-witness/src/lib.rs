
mod error;
mod field;
mod merkle;
mod person_witness;
mod witness;

pub use error::{WitnessError, Result};
pub use field::FieldFormatter;
pub use merkle::{MerkleTree, MerkleProof};
pub use person_witness::{PersonCircuitWitness, PersonWitnessGenerator};
pub use witness::{CircuitWitness, WitnessGenerator, ProverToml};

pub mod constants {
    pub const MAX_MINUTIAE: usize = 32;

    pub const MAX_LAPLACIAN_ENTRIES: usize = 128;

    pub const QUANTIZED_SPECTRUM_SIZE: usize = 16;

    pub const MERKLE_DEPTH: usize = 20;

    pub const COORDINATE_SCALE: u64 = 1000;

    pub const ANGLE_SCALE: u64 = 1_000_000;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants_match_noir() {
        assert_eq!(constants::MAX_MINUTIAE, 32);
        assert_eq!(constants::MAX_LAPLACIAN_ENTRIES, 128);
        assert_eq!(constants::QUANTIZED_SPECTRUM_SIZE, 16);
        assert_eq!(constants::MERKLE_DEPTH, 20);
    }
}
