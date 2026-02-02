
use dep::poseidon::poseidon::bn254::hash_2;

pub fn verify_membership<let DEPTH: u32>(
    leaf: Field,
    path: [Field; DEPTH],
    indices: [bool; DEPTH],
) -> Field {
    let mut current = leaf;

    for i in 0..DEPTH {
        let sibling = path[i];

        if indices[i] {
            current = hash_2([sibling, current]);
        } else {
            current = hash_2([current, sibling]);
        }
    }

    current
}

pub fn compute_root_4(leaves: [Field; 4]) -> Field {
    let h01 = hash_2([leaves[0], leaves[1]]);
    let h23 = hash_2([leaves[2], leaves[3]]);
    hash_2([h01, h23])
}

pub fn compute_root_8(leaves: [Field; 8]) -> Field {
    let h01 = hash_2([leaves[0], leaves[1]]);
    let h23 = hash_2([leaves[2], leaves[3]]);
    let h45 = hash_2([leaves[4], leaves[5]]);
    let h67 = hash_2([leaves[6], leaves[7]]);

    let h0123 = hash_2([h01, h23]);
    let h4567 = hash_2([h45, h67]);

    hash_2([h0123, h4567])
}

pub fn empty_hash(depth: u32) -> Field {
    let mut h: Field = 0;

    for _ in 0..depth {
        h = hash_2([h, h]);
    }

    h
}

pub fn verify_non_membership<let DEPTH: u32>(
    _value: Field,
    low_leaf: Field,
    _high_leaf: Field,
    low_path: [Field; DEPTH],
    low_indices: [bool; DEPTH],
    root: Field,
) {
    let computed_root = verify_membership(low_leaf, low_path, low_indices);
    assert(computed_root == root);
}

pub fn verify_memberships_batch<let DEPTH: u32, let B: u32>(
    leaves: [Field; B],
    paths: [[Field; DEPTH]; B],
    indices: [[bool; DEPTH]; B],
    root: Field,
) {
    for i in 0..B {
        let computed = verify_membership(leaves[i], paths[i], indices[i]);
        assert(computed == root);
    }
}

#[test]
fn test_merkle_path() {
    let h01 = hash_2([1, 2]);
    let h23 = hash_2([3, 4]);
    let root = hash_2([h01, h23]);

    let path: [Field; 2] = [2, h23];
    let indices: [bool; 2] = [false, false];

    let computed = verify_membership(1, path, indices);
    assert(computed == root);
}
