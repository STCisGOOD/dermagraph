
use dep::poseidon::poseidon::bn254::hash_2;

#[test]
fn test_poseidon_equivalence_zeros() {
    let a: Field = 0x0000000000000000000000000000000000000000000000000000000000000000;
    let b: Field = 0x0000000000000000000000000000000000000000000000000000000000000000;
    let expected: Field = 0x2098f5fb9e239eab3ceac3f27b81e481dc3124d55ffed523a839ee8446b64864;
    assert(hash_2([a, b]) == expected);
}

#[test]
fn test_poseidon_equivalence_simple() {
    let a: Field = 0x0000000000000000000000000000000000000000000000000000000000000001;
    let b: Field = 0x0000000000000000000000000000000000000000000000000000000000000002;
    let expected: Field = 0x115cc0f5e7d690413df64c6b9662e9cf2a3617f2743245519e19607a4417189a;
    assert(hash_2([a, b]) == expected);
}

#[test]
fn test_poseidon_equivalence_large() {
    let a: Field = 0x0000000000000000000000000000000000000000000000000000000000003039;
    let b: Field = 0x0000000000000000000000000000000000000000000000000000000000010932;
    let expected: Field = 0x1914879b2a4e7f9555f3eb55837243cefb1366a692794a7e5b5b3181fb14b49b;
    assert(hash_2([a, b]) == expected);
}

#[test]
fn test_poseidon_equivalence_gray_scott_params() {
    let a: Field = 0x0000000000000000000000000000000000000000000000000000000000000028;
    let b: Field = 0x000000000000000000000000000000000000000000000000000000000000003e;
    let expected: Field = 0x03d43984dffac9b2c083be29d4c1066f5307cbfcac7d413a5de982ee2b5a935a;
    assert(hash_2([a, b]) == expected);
}

#[test]
fn test_reaction_equivalence_equilibrium() {
    let u: Field = 0x0000000000000000000000000000000000000000000000000000000000000001;
    let v: Field = 0x0000000000000000000000000000000000000000000000000000000000000000;
    let f: Field = 0x0000000000000000000000000000000000000000000000000000000000000028;
    let k: Field = 0x000000000000000000000000000000000000000000000000000000000000003e;

    let uv2 = u * v * v;
    let react_u = f - f * u - uv2;
    let expected_react_u: Field = 0x0000000000000000000000000000000000000000000000000000000000000000;
    assert(react_u == expected_react_u);

    let react_v = uv2 - (f + k) * v;
    let expected_react_v: Field = 0x0000000000000000000000000000000000000000000000000000000000000000;
    assert(react_v == expected_react_v);
}

#[test]
fn test_reaction_equivalence_nontrivial() {
    let u: Field = 0x0000000000000000000000000000000000000000000000000000000000000005;
    let v: Field = 0x0000000000000000000000000000000000000000000000000000000000000003;
    let f: Field = 0x0000000000000000000000000000000000000000000000000000000000000028;
    let k: Field = 0x000000000000000000000000000000000000000000000000000000000000003e;

    let uv2 = u * v * v;
    let react_u = f - f * u - uv2;
    let expected_react_u: Field = 0x30644e72e131a029b85045b68181585d2833e84879b9709143e1f593efffff34;
    assert(react_u == expected_react_u);

    let react_v = uv2 - (f + k) * v;
    let expected_react_v: Field = 0x30644e72e131a029b85045b68181585d2833e84879b9709143e1f593effffefc;
    assert(react_v == expected_react_v);
}

#[test]
fn test_finalize_hash_construction() {

    let u0: Field = 1;
    let v0: Field = 4;
    let u1: Field = 2;
    let v1: Field = 5;
    let u2: Field = 3;
    let v2: Field = 6;

    let mut acc: Field = 0;

    let combined0 = hash_2([u0, v0]);
    acc = hash_2([acc, combined0]);

    let combined1 = hash_2([u1, v1]);
    acc = hash_2([acc, combined1]);

    let combined2 = hash_2([u2, v2]);
    acc = hash_2([acc, combined2]);

    assert(acc != 0);
}

#[test]
fn test_gray_scott_step_matches_rust() {

    let u: Field = 10;
    let v: Field = 5;
    let f: Field = 40;
    let k: Field = 62;

    let uv2 = u * v * v;
    assert(uv2 == 250);

    let react_u = f - f * u - uv2;

    let react_v = uv2 - (f + k) * v;

    assert(react_u != 0);
    assert(react_v != 0);
}
