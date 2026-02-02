
use std::collections::HashMap;
use crate::field::Fr;
use crate::poseidon::hash_2;

pub const MERKLE_DEPTH: usize = 20;

#[derive(Clone, Debug)]
pub struct MerkleProof {
    pub path: Vec<Fr>,
    pub indices: Vec<bool>,
    pub root: Fr,
    pub leaf: Fr,
}

impl MerkleProof {
    pub fn verify(&self) -> bool {
        let mut current = self.leaf;

        for (sibling, &is_right) in self.path.iter().zip(&self.indices) {
            current = if is_right {
                hash_2(*sibling, current)
            } else {
                hash_2(current, *sibling)
            };
        }

        current == self.root
    }

    pub fn path_to_hex(&self) -> Vec<String> {
        let mut result: Vec<String> = self.path.iter()
            .map(|fr| format!("0x{}", hex::encode(fr.to_be_bytes())))
            .collect();

        while result.len() < MERKLE_DEPTH {
            result.push(format!("0x{}", hex::encode(Fr::zero().to_be_bytes())));
        }

        result
    }

    pub fn indices_to_array(&self) -> Vec<u8> {
        let mut result: Vec<u8> = self.indices.iter()
            .map(|&b| if b { 1 } else { 0 })
            .collect();

        while result.len() < MERKLE_DEPTH {
            result.push(0);
        }

        result
    }

    pub fn root_to_hex(&self) -> String {
        format!("0x{}", hex::encode(self.root.to_be_bytes()))
    }
}

pub struct MerkleTree {
    leaves: HashMap<usize, Fr>,
    next_index: usize,
    defaults: Vec<Fr>,
}

impl MerkleTree {
    pub fn new() -> Self {
        let mut defaults = Vec::with_capacity(MERKLE_DEPTH + 1);
        defaults.push(Fr::zero());

        for i in 0..MERKLE_DEPTH {
            let prev = defaults[i];
            let next = hash_2(prev, prev);
            defaults.push(next);
        }

        Self {
            leaves: HashMap::new(),
            next_index: 0,
            defaults,
        }
    }

    pub fn insert(&mut self, commitment: Fr) -> Result<usize, &'static str> {
        let index = self.next_index;
        if index >= (1 << MERKLE_DEPTH) {
            return Err("Tree is full");
        }

        self.leaves.insert(index, commitment);
        self.next_index += 1;
        Ok(index)
    }

    fn get_leaf(&self, index: usize) -> Fr {
        self.leaves.get(&index).copied().unwrap_or(self.defaults[0])
    }

    pub fn root(&self) -> Fr {

        if self.leaves.is_empty() {
            return self.defaults[MERKLE_DEPTH];
        }

        let mut current_level: HashMap<usize, Fr> = self.leaves.clone();

        for level in 0..MERKLE_DEPTH {
            let mut next_level: HashMap<usize, Fr> = HashMap::new();

            let parent_indices: std::collections::HashSet<usize> = current_level
                .keys()
                .map(|&idx| idx >> 1)
                .collect();

            for parent_idx in parent_indices {
                let left_idx = parent_idx << 1;
                let right_idx = left_idx + 1;

                let left = current_level.get(&left_idx)
                    .copied()
                    .unwrap_or(self.defaults[level]);
                let right = current_level.get(&right_idx)
                    .copied()
                    .unwrap_or(self.defaults[level]);

                next_level.insert(parent_idx, hash_2(left, right));
            }

            current_level = next_level;
        }

        current_level.get(&0).copied().unwrap_or(self.defaults[MERKLE_DEPTH])
    }

    pub fn prove(&self, index: usize) -> Result<MerkleProof, &'static str> {
        let leaf = *self.leaves.get(&index).ok_or("No leaf at index")?;

        let mut path = Vec::with_capacity(MERKLE_DEPTH);
        let mut indices = Vec::with_capacity(MERKLE_DEPTH);

        let mut current_idx = index;

        let mut level_state: HashMap<usize, Fr> = self.leaves.clone();

        for level in 0..MERKLE_DEPTH {
            let is_right = (current_idx & 1) == 1;
            indices.push(is_right);

            let sibling_idx = if is_right {
                current_idx - 1
            } else {
                current_idx + 1
            };

            let sibling = level_state.get(&sibling_idx)
                .copied()
                .unwrap_or(self.defaults[level]);
            path.push(sibling);

            let mut next_level_state: HashMap<usize, Fr> = HashMap::new();
            let parent_indices: std::collections::HashSet<usize> = level_state
                .keys()
                .map(|&idx| idx >> 1)
                .collect();
            let parents_needed: std::collections::HashSet<usize> = {
                let mut s = parent_indices;
                s.insert(current_idx >> 1);
                s.insert(sibling_idx >> 1);
                s
            };

            for parent_idx in parents_needed {
                let left_idx = parent_idx << 1;
                let right_idx = left_idx + 1;

                let left = level_state.get(&left_idx)
                    .copied()
                    .unwrap_or(self.defaults[level]);
                let right = level_state.get(&right_idx)
                    .copied()
                    .unwrap_or(self.defaults[level]);

                next_level_state.insert(parent_idx, hash_2(left, right));
            }

            level_state = next_level_state;
            current_idx >>= 1;
        }

        let root = self.root();

        Ok(MerkleProof {
            path,
            indices,
            root,
            leaf,
        })
    }

    pub fn find(&self, commitment: &Fr) -> Option<usize> {
        self.leaves.iter()
            .find(|(_, &v)| v == *commitment)
            .map(|(&k, _)| k)
    }

    pub fn count(&self) -> usize {
        self.leaves.len()
    }
}

impl Default for MerkleTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_tree() {
        let tree = MerkleTree::new();
        assert_eq!(tree.count(), 0);
        let root = tree.root();
        assert!(!root.is_zero());
    }

    #[test]
    fn test_insert_and_prove() {
        let mut tree = MerkleTree::new();

        let commitment = Fr::from_u64(12345);
        let index = tree.insert(commitment).unwrap();
        assert_eq!(index, 0);

        let proof = tree.prove(index).unwrap();
        assert!(proof.verify());
        assert_eq!(proof.leaf, commitment);
    }

    #[test]
    fn test_proof_format() {
        let mut tree = MerkleTree::new();
        tree.insert(Fr::from_u64(42)).unwrap();

        let proof = tree.prove(0).unwrap();

        let path = proof.path_to_hex();
        let indices = proof.indices_to_array();

        assert_eq!(path.len(), MERKLE_DEPTH);
        assert_eq!(indices.len(), MERKLE_DEPTH);

        for p in &path {
            assert!(p.starts_with("0x"));
        }
    }

    #[test]
    fn test_root_changes_on_insert() {
        let mut tree = MerkleTree::new();
        let root1 = tree.root();

        tree.insert(Fr::from_u64(123)).unwrap();
        let root2 = tree.root();

        assert_ne!(root1, root2);
    }
}
