
use crate::field::Fr;
use crate::poseidon::hash_2;
use crate::iterate::MorphogenState;
use crate::reaction::{reaction_u, reaction_v, ReactionParams};

#[derive(Debug, Clone)]
pub struct Hash2TestVector {
    pub a: Fr,
    pub b: Fr,
    pub expected: Fr,
}

pub fn generate_hash2_test_vectors() -> Vec<Hash2TestVector> {
    vec![
        Hash2TestVector {
            a: Fr::zero(),
            b: Fr::zero(),
            expected: hash_2(Fr::zero(), Fr::zero()),
        },
        Hash2TestVector {
            a: Fr::from_u64(1),
            b: Fr::from_u64(2),
            expected: hash_2(Fr::from_u64(1), Fr::from_u64(2)),
        },
        Hash2TestVector {
            a: Fr::from_u64(12345),
            b: Fr::from_u64(67890),
            expected: hash_2(Fr::from_u64(12345), Fr::from_u64(67890)),
        },
        Hash2TestVector {
            a: Fr::from_u64(40),
            b: Fr::from_u64(62),
            expected: hash_2(Fr::from_u64(40), Fr::from_u64(62)),
        },
    ]
}

#[derive(Debug, Clone)]
pub struct ReactionTestVector {
    pub u: Fr,
    pub v: Fr,
    pub f: Fr,
    pub k: Fr,
    pub expected_reaction_u: Fr,
    pub expected_reaction_v: Fr,
}

pub fn generate_reaction_test_vectors() -> Vec<ReactionTestVector> {
    let params = ReactionParams::crypto();

    vec![
        ReactionTestVector {
            u: Fr::from_u64(1),
            v: Fr::zero(),
            f: params.f,
            k: params.k,
            expected_reaction_u: reaction_u(Fr::from_u64(1), Fr::zero(), &params),
            expected_reaction_v: reaction_v(Fr::from_u64(1), Fr::zero(), &params),
        },
        ReactionTestVector {
            u: Fr::from_u64(5),
            v: Fr::from_u64(3),
            f: params.f,
            k: params.k,
            expected_reaction_u: reaction_u(Fr::from_u64(5), Fr::from_u64(3), &params),
            expected_reaction_v: reaction_v(Fr::from_u64(5), Fr::from_u64(3), &params),
        },
    ]
}

#[derive(Debug, Clone)]
pub struct FinalizeHashTestVector {
    pub u: Vec<Fr>,
    pub v: Vec<Fr>,
    pub expected_hash: Fr,
}

pub fn generate_finalize_hash_test_vectors() -> Vec<FinalizeHashTestVector> {
    let small_u = vec![Fr::from_u64(1), Fr::from_u64(2), Fr::from_u64(3)];
    let small_v = vec![Fr::from_u64(4), Fr::from_u64(5), Fr::from_u64(6)];
    let small_state = MorphogenState {
        u: small_u.clone(),
        v: small_v.clone(),
    };

    vec![
        FinalizeHashTestVector {
            u: small_u,
            v: small_v,
            expected_hash: small_state.hash(),
        },
    ]
}

pub fn print_noir_test_vectors() {
    println!("
    println!("
    println!("
    println!("
    println!();

    println!("
    for (i, v) in generate_hash2_test_vectors().iter().enumerate() {
        println!("#[test]");
        println!("fn test_poseidon_equivalence_{}() {{", i);
        println!("    use dep::poseidon::poseidon::bn254::hash_2;");
        println!("    let a: Field = {};", fr_to_noir_literal(&v.a));
        println!("    let b: Field = {};", fr_to_noir_literal(&v.b));
        println!("    let expected: Field = {};", fr_to_noir_literal(&v.expected));
        println!("    assert(hash_2([a, b]) == expected);");
        println!("}}");
        println!();
    }

    println!("
    for (i, v) in generate_reaction_test_vectors().iter().enumerate() {
        println!("#[test]");
        println!("fn test_reaction_equivalence_{}() {{", i);
        println!("    let u: Field = {};", fr_to_noir_literal(&v.u));
        println!("    let v: Field = {};", fr_to_noir_literal(&v.v));
        println!("    let f: Field = {};", fr_to_noir_literal(&v.f));
        println!("    let k: Field = {};", fr_to_noir_literal(&v.k));
        println!("    ");
        println!("
        println!("    let uv2 = u * v * v;");
        println!("    let react_u = f - f * u - uv2;");
        println!("    let expected_react_u: Field = {};", fr_to_noir_literal(&v.expected_reaction_u));
        println!("    assert(react_u == expected_react_u);");
        println!("    ");
        println!("
        println!("    let react_v = uv2 - (f + k) * v;");
        println!("    let expected_react_v: Field = {};", fr_to_noir_literal(&v.expected_reaction_v));
        println!("    assert(react_v == expected_react_v);");
        println!("}}");
        println!();
    }
}

fn fr_to_noir_literal(f: &Fr) -> String {
    let bytes = f.to_be_bytes();
    format!("0x{}", hex::encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash2_vectors_deterministic() {
        let vectors = generate_hash2_test_vectors();
        for v in &vectors {
            assert_eq!(hash_2(v.a, v.b), v.expected);
        }
    }

    #[test]
    fn test_reaction_vectors_deterministic() {
        let vectors = generate_reaction_test_vectors();
        let params = ReactionParams::crypto();
        for v in &vectors {
            assert_eq!(reaction_u(v.u, v.v, &params), v.expected_reaction_u);
            assert_eq!(reaction_v(v.u, v.v, &params), v.expected_reaction_v);
        }
    }

    #[test]
    fn test_finalize_hash_vectors_deterministic() {
        let vectors = generate_finalize_hash_test_vectors();
        for v in &vectors {
            let state = MorphogenState {
                u: v.u.clone(),
                v: v.v.clone(),
            };
            assert_eq!(state.hash(), v.expected_hash);
        }
    }

    #[test]
    #[ignore]
    fn print_noir_tests() {
        print_noir_test_vectors();
    }
}
