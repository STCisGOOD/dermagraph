
use dep::poseidon::poseidon::bn254::hash_2;
use crate::turing::{MorphogenState, GrayScottParams, LaplacianEntry, gray_scott_hash_with_context};

global NULLIFIER_DOMAIN: Field = 0x6e756c6c69666965723a3a7630;
global HIERARCHY_DOMAIN: Field = 0x6869657261726368793a3a7630;

pub fn derive_nullifier<let N: u32, let M: u32>(
    identity: Field,
    scope_hash: Field,
    laplacian: [LaplacianEntry; M],
    n_entries: u32,
    params: GrayScottParams,
) -> Field {
    let context = hash_2([NULLIFIER_DOMAIN, scope_hash]);

    let initial: MorphogenState<N> = expand_to_state(identity);

    gray_scott_hash_with_context(
        initial,
        laplacian,
        n_entries,
        params,
        context,
    )
}

pub fn expand_to_state<let N: u32>(seed: Field) -> MorphogenState<N> {
    let mut u: [Field; N] = [0; N];
    let mut v: [Field; N] = [0; N];

    let mut acc = seed;
    for i in 0..N {
        u[i] = acc;
        acc = hash_2([acc, i as Field]);
        v[i] = acc;
        acc = hash_2([acc, (i + N) as Field]);
    }

    MorphogenState { u, v }
}

pub fn derive_hierarchical<let N: u32, let M: u32>(
    parent_nullifier: Field,
    subscope_hash: Field,
    laplacian: [LaplacianEntry; M],
    n_entries: u32,
    params: GrayScottParams,
) -> Field {
    let context = hash_2([HIERARCHY_DOMAIN, subscope_hash]);
    let initial: MorphogenState<N> = expand_to_state(parent_nullifier);

    gray_scott_hash_with_context(
        initial,
        laplacian,
        n_entries,
        params,
        context,
    )
}

pub fn derive_batch<let N: u32, let M: u32, let B: u32>(
    identity: Field,
    scope_hashes: [Field; B],
    laplacian: [LaplacianEntry; M],
    n_entries: u32,
    params: GrayScottParams,
) -> [Field; B] {
    let mut nullifiers: [Field; B] = [0; B];

    for i in 0..B {
        nullifiers[i] = derive_nullifier::<N, M>(
            identity,
            scope_hashes[i],
            laplacian,
            n_entries,
            params,
        );
    }

    nullifiers
}

pub fn verify_nullifier<let N: u32, let M: u32>(
    claimed_nullifier: Field,
    identity: Field,
    scope_hash: Field,
    laplacian: [LaplacianEntry; M],
    n_entries: u32,
) {
    let params = GrayScottParams::crypto();

    let computed = derive_nullifier::<N, M>(
        identity,
        scope_hash,
        laplacian,
        n_entries,
        params,
    );

    assert(computed == claimed_nullifier);
}
