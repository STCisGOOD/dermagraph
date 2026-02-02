
pub mod turing;
pub mod auth;
pub mod nullifier;
pub mod merkle;
pub mod equivalence;

use dep::poseidon::poseidon::bn254::hash_2;
use crate::turing::{MorphogenState, GrayScottParams, LaplacianEntry, gray_scott_hash};
use crate::nullifier::derive_nullifier;
use crate::merkle::verify_membership;

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
    let initial_state: MorphogenState<16> = MorphogenState {
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

    let params = GrayScottParams::crypto();

    let computed_identity = gray_scott_hash(
        initial_state,
        entries,
        n_laplacian_entries,
        params,
    );

    let computed_commitment = hash_2([computed_identity, computed_identity]);
    assert(computed_commitment == commitment);

    let computed_root = verify_membership(commitment, merkle_path, merkle_indices);
    assert(computed_root == merkle_root);

    let computed_nullifier: Field = derive_nullifier::<16, 64>(
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
    let params = GrayScottParams::crypto();
    assert(params.f == 40);
    assert(params.k == 62);
}
