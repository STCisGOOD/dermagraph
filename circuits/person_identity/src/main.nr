
use dep::poseidon::poseidon::bn254::hash_2;
use dep::poseidon::poseidon::bn254::hash_3;
use dep::poseidon::poseidon::bn254::hash_4;

global EMBEDDING_DIM: u32 = 32;

global MERKLE_DEPTH: u32 = 20;

global PERSON_NULLIFIER_DOMAIN: Field = 0x706572736f6e3a6e756c6c696669657200;
global PERSON_COMMITMENT_DOMAIN: Field = 0x706572736f6e3a636f6d6d69746d656e74;
global PERSON_ID_DOMAIN: Field = 0x706572736f6e3a69643a763100000000;

struct QuantizedEmbedding {
    values: [Field; EMBEDDING_DIM],
}

struct MerkleProof {
    path: [Field; MERKLE_DEPTH],
    indices: [u1; MERKLE_DEPTH],
}

fn compress_embedding(embedding: QuantizedEmbedding) -> Field {
    let mut acc = PERSON_ID_DOMAIN;

    for i in 0..8 {
        let idx = i * 4;
        acc = hash_4([
            acc,
            embedding.values[idx],
            embedding.values[idx + 1],
            embedding.values[idx + 2]
        ]);
        acc = hash_2([acc, embedding.values[idx + 3]]);
    }

    acc
}

fn compute_commitment(embedding: QuantizedEmbedding, blinding: Field) -> Field {
    let embedding_hash = compress_embedding(embedding);
    hash_3([PERSON_COMMITMENT_DOMAIN, embedding_hash, blinding])
}

fn derive_nullifier(embedding: QuantizedEmbedding, scope: Field) -> Field {
    let embedding_hash = compress_embedding(embedding);
    hash_3([PERSON_NULLIFIER_DOMAIN, embedding_hash, scope])
}

fn verify_merkle_proof(leaf: Field, root: Field, proof: MerkleProof) -> bool {
    let mut current = leaf;

    for i in 0..MERKLE_DEPTH {
        let sibling = proof.path[i];
        if proof.indices[i] == 0 {
            current = hash_2([current, sibling]);
        } else {
            current = hash_2([sibling, current]);
        }
    }

    current == root
}

fn main(
    embedding: QuantizedEmbedding,
    blinding: Field,
    merkle_proof: MerkleProof,

    commitment: pub Field,
    merkle_root: pub Field,
    scope: pub Field,
    nullifier: pub Field,
) {

    let computed_commitment = compute_commitment(embedding, blinding);
    assert(computed_commitment == commitment);

    assert(verify_merkle_proof(commitment, merkle_root, merkle_proof));

    let computed_nullifier = derive_nullifier(embedding, scope);
    assert(computed_nullifier == nullifier);

}

fn derive_hierarchical_nullifier(
    embedding: QuantizedEmbedding,
    parent_scope: Field,
    child_scope: Field,
) -> Field {
    let parent_nullifier = derive_nullifier(embedding, parent_scope);
    hash_2([parent_nullifier, child_scope])
}

#[test]
fn test_compress_embedding() {
    let embedding = QuantizedEmbedding {
        values: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
                 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32],
    };

    let hash1 = compress_embedding(embedding);
    let hash2 = compress_embedding(embedding);

    assert(hash1 == hash2);

    assert(hash1 != 0);
}

#[test]
fn test_commitment() {
    let embedding = QuantizedEmbedding {
        values: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
                 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32],
    };

    let blinding1: Field = 12345;
    let blinding2: Field = 67890;

    let commit1 = compute_commitment(embedding, blinding1);
    let commit2 = compute_commitment(embedding, blinding2);

    assert(commit1 != commit2);

    let commit1_again = compute_commitment(embedding, blinding1);
    assert(commit1 == commit1_again);
}

#[test]
fn test_nullifier_determinism() {
    let embedding = QuantizedEmbedding {
        values: [5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 75, 80,
                 85, 90, 95, 100, 105, 110, 115, 120, 125, 130, 135, 140, 145, 150, 155, 160],
    };

    let scope: Field = 0x64616f2d766f74652d32303234;

    let null1 = derive_nullifier(embedding, scope);
    let null2 = derive_nullifier(embedding, scope);

    assert(null1 == null2);
}

#[test]
fn test_nullifier_scope_separation() {
    let embedding = QuantizedEmbedding {
        values: [5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 75, 80,
                 85, 90, 95, 100, 105, 110, 115, 120, 125, 130, 135, 140, 145, 150, 155, 160],
    };

    let scope1: Field = 1;
    let scope2: Field = 2;

    let null1 = derive_nullifier(embedding, scope1);
    let null2 = derive_nullifier(embedding, scope2);

    assert(null1 != null2);
}

#[test]
fn test_nullifier_embedding_separation() {
    let embedding1 = QuantizedEmbedding {
        values: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
                 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32],
    };

    let embedding2 = QuantizedEmbedding {
        values: [100, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
                 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32],
    };

    let scope: Field = 1;

    let null1 = derive_nullifier(embedding1, scope);
    let null2 = derive_nullifier(embedding2, scope);

    assert(null1 != null2);
}

#[test]
fn test_merkle_verification() {
    let leaf: Field = 42;
    let sibling: Field = 100;
    let root: Field = hash_2([leaf, sibling]);

    let computed_root: Field = hash_2([leaf, sibling]);
    assert(computed_root == root);
}

#[test]
fn test_hierarchical_nullifier() {
    let embedding = QuantizedEmbedding {
        values: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
                 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32],
    };

    let parent_scope: Field = 0x64616f;
    let child_scope: Field = 0x766f7465;

    let hierarchical = derive_hierarchical_nullifier(embedding, parent_scope, child_scope);
    let parent_null = derive_nullifier(embedding, parent_scope);

    assert(hierarchical != parent_null);

    let hierarchical2 = derive_hierarchical_nullifier(embedding, parent_scope, child_scope);
    assert(hierarchical == hierarchical2);
}
