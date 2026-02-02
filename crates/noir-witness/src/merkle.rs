
use std::collections::HashMap;
use turing_core::Fr;
use tracing::debug;

use crate::constants::MERKLE_DEPTH;
use crate::error::{Result, WitnessError};
use crate::field::FieldFormatter;

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
                FieldFormatter::poseidon_hash_2(sibling, &current)
            } else {
                FieldFormatter::poseidon_hash_2(&current, sibling)
            };
        }

        current == self.root
    }

    pub fn path_to_noir(&self) -> Vec<String> {
        let mut result: Vec<String> = self.path.iter()
            .map(|fr| FieldFormatter::from_tc_fr(fr))
            .collect();

        while result.len() < MERKLE_DEPTH {
            result.push(FieldFormatter::zero());
        }

        result
    }

    pub fn indices_to_noir(&self) -> Vec<String> {
        let mut result: Vec<String> = self.indices.iter()
            .map(|&b| FieldFormatter::from_bool(b))
            .collect();

        while result.len() < MERKLE_DEPTH {
            result.push(FieldFormatter::from_bool(false));
        }

        result
    }

    pub fn root_to_noir(&self) -> String {
        FieldFormatter::from_tc_fr(&self.root)
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
            let next = FieldFormatter::poseidon_hash_2(&prev, &prev);
            defaults.push(next);
        }

        Self {
            leaves: HashMap::new(),
            next_index: 0,
            defaults,
        }
    }

    pub fn insert(&mut self, identity: Fr) -> Result<usize> {
        let index = self.next_index;
        if index >= (1 << MERKLE_DEPTH) {
            return Err(WitnessError::MerkleTreeError {
                reason: "Tree is full".to_string(),
            });
        }

        self.leaves.insert(index, identity);
        self.next_index += 1;
        debug!("Inserted identity at index {}", index);

        Ok(index)
    }

    pub fn insert_at(&mut self, index: usize, identity: Fr) -> Result<()> {
        if index >= (1 << MERKLE_DEPTH) {
            return Err(WitnessError::MerkleTreeError {
                reason: format!("Index {} out of bounds", index),
            });
        }

        self.leaves.insert(index, identity);
        if index >= self.next_index {
            self.next_index = index + 1;
        }
        Ok(())
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

                let left = current_level
                    .get(&left_idx)
                    .copied()
                    .unwrap_or(self.defaults[level]);
                let right = current_level
                    .get(&right_idx)
                    .copied()
                    .unwrap_or(self.defaults[level]);

                next_level.insert(parent_idx, FieldFormatter::poseidon_hash_2(&left, &right));
            }

            current_level = next_level;
        }

        current_level.get(&0).copied().unwrap_or(self.defaults[MERKLE_DEPTH])
    }

    pub fn prove(&self, index: usize) -> Result<MerkleProof> {
        let leaf = *self.leaves.get(&index).ok_or_else(|| WitnessError::InvalidMerkleProof {
            reason: format!("No leaf at index {}", index),
        })?;

        let mut path = Vec::with_capacity(MERKLE_DEPTH);
        let mut indices = Vec::with_capacity(MERKLE_DEPTH);

        let mut level_state: HashMap<usize, Fr> = self.leaves.clone();
        let mut current_idx = index;

        for level in 0..MERKLE_DEPTH {
            let is_right = (current_idx & 1) == 1;
            indices.push(is_right);

            let sibling_idx = if is_right {
                current_idx - 1
            } else {
                current_idx + 1
            };

            let sibling = level_state
                .get(&sibling_idx)
                .copied()
                .unwrap_or(self.defaults[level]);
            path.push(sibling);

            let mut next_level_state: HashMap<usize, Fr> = HashMap::new();
            let parent_indices: std::collections::HashSet<usize> = level_state
                .keys()
                .chain(std::iter::once(&current_idx))
                .chain(std::iter::once(&sibling_idx))
                .map(|&idx| idx >> 1)
                .collect();

            for parent_idx in parent_indices {
                let left_idx = parent_idx << 1;
                let right_idx = left_idx + 1;

                let left = level_state
                    .get(&left_idx)
                    .copied()
                    .unwrap_or(self.defaults[level]);
                let right = level_state
                    .get(&right_idx)
                    .copied()
                    .unwrap_or(self.defaults[level]);

                next_level_state.insert(parent_idx, FieldFormatter::poseidon_hash_2(&left, &right));
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

    pub fn count(&self) -> usize {
        self.leaves.len()
    }

    pub fn contains(&self, identity: &Fr) -> bool {
        self.leaves.values().any(|&v| v == *identity)
    }

    pub fn find(&self, identity: &Fr) -> Option<usize> {
        self.leaves.iter()
            .find(|(_, &v)| v == *identity)
            .map(|(&k, _)| k)
    }
}

impl Default for MerkleTree {
    fn default() -> Self {
        Self::new()
    }
}

impl MerkleTree {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = Vec::new();

        data.extend(&(self.next_index as u64).to_le_bytes());

        data.extend(&(self.leaves.len() as u64).to_le_bytes());

        for (&index, value) in &self.leaves {
            data.extend(&(index as u64).to_le_bytes());
            data.extend(&value.to_be_bytes());
        }

        data
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 16 {
            return Err(WitnessError::MerkleTreeError {
                reason: "Data too short for merkle tree".to_string(),
            });
        }

        let next_index = u64::from_le_bytes(data[0..8].try_into().unwrap()) as usize;

        let num_leaves = u64::from_le_bytes(data[8..16].try_into().unwrap()) as usize;

        let expected_len = 16 + num_leaves * 40;
        if data.len() < expected_len {
            return Err(WitnessError::MerkleTreeError {
                reason: format!("Data too short: expected {} bytes, got {}", expected_len, data.len()),
            });
        }

        let mut leaves = HashMap::new();
        let mut offset = 16;
        for _ in 0..num_leaves {
            let index = u64::from_le_bytes(data[offset..offset+8].try_into().unwrap()) as usize;
            let value = Fr::from_be_bytes_mod_order(&data[offset+8..offset+40]);
            leaves.insert(index, value);
            offset += 40;
        }

        let mut defaults = Vec::with_capacity(MERKLE_DEPTH + 1);
        defaults.push(Fr::zero());
        for i in 0..MERKLE_DEPTH {
            let prev = defaults[i];
            let next = FieldFormatter::poseidon_hash_2(&prev, &prev);
            defaults.push(next);
        }

        Ok(Self {
            leaves,
            next_index,
            defaults,
        })
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

        let id1 = Fr::from_u64(12345);
        let idx1 = tree.insert(id1).unwrap();
        assert_eq!(idx1, 0);

        let proof = tree.prove(idx1).unwrap();
        assert!(proof.verify());
        assert_eq!(proof.leaf, id1);
    }

    #[test]
    fn test_multiple_inserts() {
        let mut tree = MerkleTree::new();

        let ids: Vec<Fr> = (0u64..10).map(Fr::from_u64).collect();

        for (i, &id) in ids.iter().enumerate() {
            let idx = tree.insert(id).unwrap();
            assert_eq!(idx, i);
        }

        assert_eq!(tree.count(), 10);

        for i in 0..10 {
            let proof = tree.prove(i).unwrap();
            assert!(proof.verify(), "Proof {} failed", i);
        }
    }

    #[test]
    fn test_proof_format() {
        let mut tree = MerkleTree::new();
        tree.insert(Fr::from_u64(42)).unwrap();

        let proof = tree.prove(0).unwrap();

        let path = proof.path_to_noir();
        let indices = proof.indices_to_noir();

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

    #[test]
    fn test_find_and_contains() {
        let mut tree = MerkleTree::new();

        let id1 = Fr::from_u64(12345);
        let id2 = Fr::from_u64(67890);
        let id_not_present = Fr::from_u64(99999);

        tree.insert(id1).unwrap();
        tree.insert(id2).unwrap();

        assert!(tree.contains(&id1));
        assert!(tree.contains(&id2));
        assert!(!tree.contains(&id_not_present));

        assert_eq!(tree.find(&id1), Some(0));
        assert_eq!(tree.find(&id2), Some(1));
        assert_eq!(tree.find(&id_not_present), None);
    }
}
