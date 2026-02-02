
use dep::poseidon::poseidon::bn254::hash_2;
use crate::turing::{MorphogenState, GrayScottParams, LaplacianEntry, gray_scott_hash};
use crate::nullifier::derive_nullifier;
use crate::merkle::verify_membership;

pub struct AuthPublicInputs {
    pub commitment: Field,
    pub nullifier: Field,
    pub scope_hash: Field,
    pub merkle_root: Field,
}

pub struct AuthWitness<let N: u32, let M: u32, let DEPTH: u32> {
    pub initial_state: MorphogenState<N>,
    pub laplacian_entries: [LaplacianEntry; M],
    pub n_laplacian_entries: u32,
    pub merkle_path: [Field; DEPTH],
    pub merkle_indices: [bool; DEPTH],
}

pub fn auth_proof<let N: u32, let M: u32, let DEPTH: u32>(
    public: AuthPublicInputs,
    witness: AuthWitness<N, M, DEPTH>,
) {
    let params = GrayScottParams::crypto();

    let computed_identity = gray_scott_hash(
        witness.initial_state,
        witness.laplacian_entries,
        witness.n_laplacian_entries,
        params,
    );

    let computed_commitment = hash_2([computed_identity, computed_identity]);

    assert(computed_commitment == public.commitment);

    let computed_root = verify_membership(
        public.commitment,
        witness.merkle_path,
        witness.merkle_indices,
    );
    assert(computed_root == public.merkle_root);

    let computed_nullifier: Field = derive_nullifier::<N, M>(
        computed_identity,
        public.scope_hash,
        witness.laplacian_entries,
        witness.n_laplacian_entries,
        params,
    );
    assert(computed_nullifier == public.nullifier);
}

pub fn auth_proof_simple<let N: u32, let M: u32>(
    commitment: Field,
    nullifier: Field,
    scope_hash: Field,
    initial_state: MorphogenState<N>,
    laplacian_entries: [LaplacianEntry; M],
    n_laplacian_entries: u32,
) {
    let params = GrayScottParams::crypto();

    let identity = gray_scott_hash(
        initial_state,
        laplacian_entries,
        n_laplacian_entries,
        params,
    );

    let computed_commitment = hash_2([identity, identity]);
    assert(computed_commitment == commitment);

    let computed_nullifier: Field = derive_nullifier::<N, M>(
        identity,
        scope_hash,
        laplacian_entries,
        n_laplacian_entries,
        params,
    );
    assert(computed_nullifier == nullifier);
}

pub fn auth_proof_with_challenge<let N: u32, let M: u32>(
    commitment: Field,
    nullifier: Field,
    scope_hash: Field,
    challenge: Field,
    response: Field,
    initial_state: MorphogenState<N>,
    laplacian_entries: [LaplacianEntry; M],
    n_laplacian_entries: u32,
) {
    let params = GrayScottParams::crypto();

    let identity = gray_scott_hash(
        initial_state,
        laplacian_entries,
        n_laplacian_entries,
        params,
    );

    let computed_commitment = hash_2([identity, identity]);
    assert(computed_commitment == commitment);

    let computed_response = hash_2([identity, challenge]);
    assert(computed_response == response);

    let computed_nullifier: Field = derive_nullifier::<N, M>(
        identity,
        scope_hash,
        laplacian_entries,
        n_laplacian_entries,
        params,
    );
    assert(computed_nullifier == nullifier);
}
