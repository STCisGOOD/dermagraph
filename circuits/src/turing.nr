
use dep::poseidon::poseidon::bn254::hash_2;

pub struct GrayScottParams {
    pub f: Field,
    pub k: Field,
    pub d_u: Field,
    pub d_v: Field,
    pub dt: Field,
}

impl GrayScottParams {
    pub fn crypto() -> Self {
        Self {
            f: 40,
            k: 62,
            d_u: 2,
            d_v: 1,
            dt: 1,
        }
    }
}

pub struct MorphogenState<let N: u32> {
    pub u: [Field; N],
    pub v: [Field; N],
}

pub struct LaplacianEntry {
    pub row: u32,
    pub col: u32,
    pub value: Field,
}

fn reaction_u(u: Field, v: Field, f: Field) -> Field {
    let uv2 = u * v * v;
    f - f * u - uv2
}

fn reaction_v(u: Field, v: Field, f: Field, k: Field) -> Field {
    let uv2 = u * v * v;
    uv2 - (f + k) * v
}

pub fn apply_laplacian<let N: u32, let M: u32>(
    x: [Field; N],
    entries: [LaplacianEntry; M],
    n_entries: u32,
) -> [Field; N] {
    let mut result: [Field; N] = [0; N];

    for i in 0..M {
        if i < n_entries {
            let entry = entries[i];
            if (entry.row < N) & (entry.col < N) {
                result[entry.row] = result[entry.row] + entry.value * x[entry.col];
            }
        }
    }

    result
}

pub fn gray_scott_step<let N: u32, let M: u32>(
    state: MorphogenState<N>,
    laplacian: [LaplacianEntry; M],
    n_entries: u32,
    params: GrayScottParams,
) -> MorphogenState<N> {
    let lap_u = apply_laplacian(state.u, laplacian, n_entries);
    let lap_v = apply_laplacian(state.v, laplacian, n_entries);

    let mut new_u: [Field; N] = [0; N];
    let mut new_v: [Field; N] = [0; N];

    for i in 0..N {
        let react_u = reaction_u(state.u[i], state.v[i], params.f);
        let react_v = reaction_v(state.u[i], state.v[i], params.f, params.k);

        let diff_u = params.d_u * lap_u[i];
        let diff_v = params.d_v * lap_v[i];

        new_u[i] = state.u[i] + params.dt * (react_u + diff_u);
        new_v[i] = state.v[i] + params.dt * (react_v + diff_v);
    }

    MorphogenState { u: new_u, v: new_v }
}

pub fn gray_scott_iterate<let N: u32, let M: u32>(
    initial: MorphogenState<N>,
    laplacian: [LaplacianEntry; M],
    n_entries: u32,
    params: GrayScottParams,
) -> MorphogenState<N> {
    let mut state = initial;

    for _ in 0..64 {
        state = gray_scott_step(state, laplacian, n_entries, params);
    }

    state
}

pub fn finalize_hash<let N: u32>(state: MorphogenState<N>) -> Field {
    let mut acc: Field = 0;

    for i in 0..N {
        let combined = hash_2([state.u[i], state.v[i]]);
        acc = hash_2([acc, combined]);
    }

    acc
}

pub fn gray_scott_hash<let N: u32, let M: u32>(
    initial: MorphogenState<N>,
    laplacian: [LaplacianEntry; M],
    n_entries: u32,
    params: GrayScottParams,
) -> Field {
    let final_state = gray_scott_iterate(initial, laplacian, n_entries, params);
    finalize_hash(final_state)
}

pub fn gray_scott_hash_with_context<let N: u32, let M: u32>(
    initial: MorphogenState<N>,
    laplacian: [LaplacianEntry; M],
    n_entries: u32,
    params: GrayScottParams,
    context: Field,
) -> Field {
    let base_hash = gray_scott_hash(initial, laplacian, n_entries, params);
    hash_2([base_hash, context])
}

#[test]
fn test_reaction_symmetry() {
    let params = GrayScottParams::crypto();

    let r_u = reaction_u(1, 0, params.f);
    assert(r_u == 0);

    let r_v = reaction_v(0, 100, params.f, params.k);
    let expected = 0 - (params.f + params.k) * 100;
    assert(r_v == expected);
}

#[test]
fn test_bivariate_nonlinearity() {
    let u1: Field = 2;
    let v1: Field = 3;
    let u2: Field = 3;
    let v2: Field = 2;

    let product1 = u1 * v1 * v1;
    let product2 = u2 * v2 * v2;

    assert(product1 != product2);
}
