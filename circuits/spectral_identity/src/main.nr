
use std::hash::poseidon;

global MAX_MINUTIAE: u32 = 32;

global MAX_LAPLACIAN_ENTRIES: u32 = 128;

global QUANTIZED_SPECTRUM_SIZE: u32 = 16;

global FULL_ROUNDS: u32 = 8;
global PARTIAL_ROUNDS: u32 = 22;

global MERKLE_DEPTH: u32 = 20;

struct QuantizedSpectrum {
    values: [Field; QUANTIZED_SPECTRUM_SIZE],
    count: u32,
}

struct Minutiae {
    x: [Field; MAX_MINUTIAE],
    y: [Field; MAX_MINUTIAE],
    theta: [Field; MAX_MINUTIAE],
    count: u32,
}

struct LaplacianEntry {
    row: u32,
    col: u32,
    value: Field,
}

struct STHState {
    u: [Field; MAX_MINUTIAE],
    v: [Field; MAX_MINUTIAE],
}

struct PersonalizedDiffusion {
    d_u: [Field; QUANTIZED_SPECTRUM_SIZE],
    d_v: [Field; QUANTIZED_SPECTRUM_SIZE],
}

struct ReactionParams {
    f: Field,
    k: Field,
}

struct MerkleProof {
    path: [Field; MERKLE_DEPTH],
    indices: [u1; MERKLE_DEPTH],
}

fn hash2(a: Field, b: Field) -> Field {
    poseidon::bn254::hash_2([a, b])
}

fn hash_many<let N: u32>(inputs: [Field; N], count: u32) -> Field {
    let mut acc: Field = 0;
    for i in 0..N {
        if i < count {
            acc = hash2(acc, inputs[i]);
        }
    }
    acc
}

fn create_personalized_diffusion(
    quantized: QuantizedSpectrum,
    base_d_u: Field,
    base_d_v: Field,
) -> PersonalizedDiffusion {
    let mut d_u: [Field; QUANTIZED_SPECTRUM_SIZE] = [0; QUANTIZED_SPECTRUM_SIZE];
    let mut d_v: [Field; QUANTIZED_SPECTRUM_SIZE] = [0; QUANTIZED_SPECTRUM_SIZE];

    let scaling: Field = 7;

    for i in 0..QUANTIZED_SPECTRUM_SIZE {
        if i < quantized.count {
            let q = quantized.values[i];
            d_u[i] = base_d_u + q * scaling;
            d_v[i] = base_d_v + (q * 3 + 5) * scaling;
        } else {
            d_u[i] = base_d_u;
            d_v[i] = base_d_v;
        }
    }

    PersonalizedDiffusion { d_u, d_v }
}

fn create_initial_state(
    minutiae: Minutiae,
    quantized: QuantizedSpectrum,
) -> STHState {
    let mut u: [Field; MAX_MINUTIAE] = [0; MAX_MINUTIAE];
    let mut v: [Field; MAX_MINUTIAE] = [0; MAX_MINUTIAE];

    for i in 0..MAX_MINUTIAE {
        if i < minutiae.count {
            u[i] = poseidon::bn254::hash_3([
                minutiae.x[i],
                minutiae.y[i],
                i as Field,
            ]);

            let q = if i < quantized.count { quantized.values[i] } else { 0 };
            v[i] = poseidon::bn254::hash_3([
                minutiae.theta[i],
                q,
                (i + minutiae.count) as Field,
            ]);
        }
    }

    STHState { u, v }
}

fn apply_laplacian(
    x: [Field; MAX_MINUTIAE],
    entries: [LaplacianEntry; MAX_LAPLACIAN_ENTRIES],
    n_entries: u32,
) -> [Field; MAX_MINUTIAE] {
    let mut result: [Field; MAX_MINUTIAE] = [0; MAX_MINUTIAE];

    for i in 0..MAX_LAPLACIAN_ENTRIES {
        if i < n_entries {
            let entry = entries[i];
            if (entry.row < MAX_MINUTIAE) & (entry.col < MAX_MINUTIAE) {
                result[entry.row] = result[entry.row] + entry.value * x[entry.col];
            }
        }
    }

    result
}

fn sth_full_round(
    state: STHState,
    laplacian: [LaplacianEntry; MAX_LAPLACIAN_ENTRIES],
    n_entries: u32,
    diffusion: PersonalizedDiffusion,
    reaction: ReactionParams,
    alpha: Field,
    round: u32,
    n_minutiae: u32,
) -> STHState {
    let lu = apply_laplacian(state.u, laplacian, n_entries);
    let lv = apply_laplacian(state.v, laplacian, n_entries);

    let rc = round as Field * 7 + 3;

    let mut new_u: [Field; MAX_MINUTIAE] = [0; MAX_MINUTIAE];
    let mut new_v: [Field; MAX_MINUTIAE] = [0; MAX_MINUTIAE];

    for i in 0..MAX_MINUTIAE {
        if i < n_minutiae {
            let u = state.u[i];
            let v = state.v[i];

            let d_u_i = if i < QUANTIZED_SPECTRUM_SIZE { diffusion.d_u[i] } else { diffusion.d_u[0] };
            let d_v_i = if i < QUANTIZED_SPECTRUM_SIZE { diffusion.d_v[i] } else { diffusion.d_v[0] };

            let uv2 = u * v * v;

            new_u[i] = u + d_u_i * lu[i] + alpha * (reaction.f - u - uv2) + rc;
            new_v[i] = v + d_v_i * lv[i] + alpha * (uv2 - v * (reaction.f + reaction.k)) + rc;
        }
    }

    STHState { u: new_u, v: new_v }
}

fn sth_partial_round(
    state: STHState,
    laplacian: [LaplacianEntry; MAX_LAPLACIAN_ENTRIES],
    n_entries: u32,
    diffusion: PersonalizedDiffusion,
    reaction: ReactionParams,
    alpha: Field,
    round: u32,
    n_minutiae: u32,
) -> STHState {
    let lu = apply_laplacian(state.u, laplacian, n_entries);
    let lv = apply_laplacian(state.v, laplacian, n_entries);

    let rc = round as Field * 11 + 7;

    let mut new_u: [Field; MAX_MINUTIAE] = [0; MAX_MINUTIAE];
    let mut new_v: [Field; MAX_MINUTIAE] = [0; MAX_MINUTIAE];

    for i in 0..MAX_MINUTIAE {
        if i < n_minutiae {
            let u = state.u[i];
            let v = state.v[i];
            let d_u_i = if i < QUANTIZED_SPECTRUM_SIZE { diffusion.d_u[i] } else { diffusion.d_u[0] };
            let d_v_i = if i < QUANTIZED_SPECTRUM_SIZE { diffusion.d_v[i] } else { diffusion.d_v[0] };

            if i == 0 {
                let uv2 = u * v * v;
                new_u[i] = u + d_u_i * lu[i] + alpha * (reaction.f - u - uv2) + rc;
                new_v[i] = v + d_v_i * lv[i] + alpha * (uv2 - v * (reaction.f + reaction.k)) + rc;
            } else {
                new_u[i] = u + d_u_i * lu[i] + rc;
                new_v[i] = v + d_v_i * lv[i] + rc;
            }
        }
    }

    STHState { u: new_u, v: new_v }
}

fn compute_sth(
    minutiae: Minutiae,
    quantized: QuantizedSpectrum,
    laplacian: [LaplacianEntry; MAX_LAPLACIAN_ENTRIES],
    n_entries: u32,
) -> Field {
    let reaction = ReactionParams { f: 40, k: 62 };
    let alpha: Field = 1;
    let base_d_u: Field = 2;
    let base_d_v: Field = 1;

    let diffusion = create_personalized_diffusion(quantized, base_d_u, base_d_v);

    let mut state = create_initial_state(minutiae, quantized);

    let half_full = FULL_ROUNDS / 2;

    for r in 0..4 {
        state = sth_full_round(
            state, laplacian, n_entries, diffusion, reaction, alpha, r, minutiae.count
        );
    }

    for r in 0..22 {
        state = sth_partial_round(
            state, laplacian, n_entries, diffusion, reaction, alpha, r + 4, minutiae.count
        );
    }

    for r in 0..4 {
        state = sth_full_round(
            state, laplacian, n_entries, diffusion, reaction, alpha, r + 26, minutiae.count
        );
    }

    let mut hash_input: [Field; 64] = [0; 64];
    for i in 0..MAX_MINUTIAE {
        hash_input[i] = state.u[i];
        hash_input[i + MAX_MINUTIAE] = state.v[i];
    }

    let mut acc: Field = 0;
    for i in 0..64 {
        acc = hash2(acc, hash_input[i]);
    }
    acc
}

fn derive_nullifier(identity: Field, scope: Field) -> Field {
    hash2(identity, scope)
}

fn verify_merkle_proof(leaf: Field, root: Field, proof: MerkleProof) -> bool {
    let mut current = leaf;

    for i in 0..MERKLE_DEPTH {
        let sibling = proof.path[i];
        if proof.indices[i] == 0 {
            current = hash2(current, sibling);
        } else {
            current = hash2(sibling, current);
        }
    }

    current == root
}

fn main(
    minutiae: Minutiae,
    quantized: QuantizedSpectrum,
    laplacian: [LaplacianEntry; MAX_LAPLACIAN_ENTRIES],
    n_laplacian_entries: u32,
    merkle_proof: MerkleProof,

    merkle_root: pub Field,
    nullifier_scope: pub Field,
    nullifier: pub Field,
) {

    let identity = compute_sth(minutiae, quantized, laplacian, n_laplacian_entries);

    assert(verify_merkle_proof(identity, merkle_root, merkle_proof));

    let expected_nullifier = derive_nullifier(identity, nullifier_scope);
    assert(nullifier == expected_nullifier);

}

fn verify_simple(
    minutiae: Minutiae,
    quantized: QuantizedSpectrum,
    laplacian: [LaplacianEntry; MAX_LAPLACIAN_ENTRIES],
    n_entries: u32,

    expected_identity: pub Field,
    nullifier_scope: pub Field,
    nullifier: pub Field,
) {
    let identity = compute_sth(minutiae, quantized, laplacian, n_entries);
    assert(identity == expected_identity);

    let expected_nullifier = derive_nullifier(identity, nullifier_scope);
    assert(nullifier == expected_nullifier);
}

#[test]
fn test_personalized_diffusion() {
    let quantized = QuantizedSpectrum {
        values: [5, 10, 15, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        count: 4,
    };

    let diffusion = create_personalized_diffusion(quantized, 2, 1);

    assert(diffusion.d_u[0] != diffusion.d_u[1]);
    assert(diffusion.d_v[0] != diffusion.d_v[1]);
}

#[test]
fn test_hash2_works() {
    let h = hash2(1, 2);
    assert(h != 0);

    let h2 = hash2(1, 2);
    assert(h == h2);

    let h3 = hash2(1, 3);
    assert(h != h3);
}

#[test]
fn test_nullifier_derivation() {
    let identity: Field = 12345;
    let scope1: Field = 1;
    let scope2: Field = 2;

    let null1 = derive_nullifier(identity, scope1);
    let null2 = derive_nullifier(identity, scope2);

    assert(null1 != null2);

    let null1_again = derive_nullifier(identity, scope1);
    assert(null1 == null1_again);
}

#[test]
fn test_apply_laplacian() {
    let entries: [LaplacianEntry; MAX_LAPLACIAN_ENTRIES] = [
        LaplacianEntry { row: 0, col: 0, value: 2 },
        LaplacianEntry { row: 0, col: 1, value: -1 as Field },
        LaplacianEntry { row: 0, col: 2, value: -1 as Field },
        LaplacianEntry { row: 1, col: 0, value: -1 as Field },
        LaplacianEntry { row: 1, col: 1, value: 2 },
        LaplacianEntry { row: 1, col: 2, value: -1 as Field },
        LaplacianEntry { row: 2, col: 0, value: -1 as Field },
        LaplacianEntry { row: 2, col: 1, value: -1 as Field },
        LaplacianEntry { row: 2, col: 2, value: 2 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
        LaplacianEntry { row: 0, col: 0, value: 0 },
    ];

    let x: [Field; MAX_MINUTIAE] = [1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

    let result = apply_laplacian(x, entries, 9);

    assert(result[0] == 0);
    assert(result[1] == 0);
    assert(result[2] == 0);
}
