
pub mod turing;
pub mod auth;
pub mod nullifier;
pub mod merkle;

use crate::turing::{TuringState, TuringParams, LaplacianEntry, turing_hash};
use crate::auth::AuthPublicInputs;
use crate::nullifier::derive_nullifier;
use crate::merkle::verify_membership;

global N: u32 = 16;
global M: u32 = 64;
global DEPTH: u32 = 20;

fn main(
    commitment: pub Field,
    nullifier: pub Field,
    scope_hash: pub Field,
    merkle_root: pub Field,

    initial_u: [Field; 16],
    initial_v: [Field; 16],

    laplacian_rows: [u32; 64],
    laplacian_cols: [u32; 64],
    laplacian_vals: [Field; 64],
    n_laplacian_entries: u32,

    merkle_path: [Field; 20],
    merkle_indices: [bool; 20],
) {
    let initial_state: TuringState<16> = TuringState {
        u: initial_u,
        v: initial_v,
    };

    let mut entries: [LaplacianEntry; 64] = [LaplacianEntry { row: 0, col: 0, value: 0 }; 64];
    for i in 0..64 {
        entries[i] = LaplacianEntry {
            row: laplacian_rows[i],
            col: laplacian_cols[i],
            value: laplacian_vals[i],
        };
    }

    let params = TuringParams::crypto();

    let computed_identity = turing_hash(
        initial_state,
        entries,
        n_laplacian_entries,
        params,
    );

    use std::hash::poseidon2::Poseidon2;
    let computed_commitment = Poseidon2::hash([computed_identity, computed_identity], 2);
    assert(computed_commitment == commitment);

    let computed_root = verify_membership(commitment, merkle_path, merkle_indices);
    assert(computed_root == merkle_root);

    let computed_nullifier: Field = derive_nullifier(
        computed_identity,
        scope_hash,
        entries,
        n_laplacian_entries,
        params,
    );
    assert(computed_nullifier == nullifier);
}

#[test]
fn test_basic_flow() {
    let params = TuringParams::crypto();
    assert(params.iterations == 64);
}
