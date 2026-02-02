
use ark_bn254::Fr as ArkFr;
use light_poseidon::{Poseidon, PoseidonHasher, PoseidonError};
use crate::field::Fr;

pub struct PoseidonBn254 {
    hasher_2: Poseidon<ArkFr>,
    #[allow(dead_code)]
    hasher_var: Poseidon<ArkFr>,
}

impl PoseidonBn254 {
    pub fn new() -> Result<Self, PoseidonError> {
        Ok(Self {
            hasher_2: Poseidon::<ArkFr>::new_circom(2)?,
            hasher_var: Poseidon::<ArkFr>::new_circom(12)?,
        })
    }

    pub fn hash_2(&mut self, a: Fr, b: Fr) -> Result<Fr, PoseidonError> {
        let result = self.hasher_2.hash(&[a.0, b.0])?;
        Ok(Fr(result))
    }

    pub fn hash_many(&mut self, inputs: &[Fr]) -> Result<Fr, PoseidonError> {
        if inputs.is_empty() {
            return Ok(Fr::zero());
        }

        if inputs.len() == 1 {
            let result = self.hasher_2.hash(&[inputs[0].0, ArkFr::from(0u64)])?;
            return Ok(Fr(result));
        }

        if inputs.len() == 2 {
            return self.hash_2(inputs[0], inputs[1]);
        }

        let ark_inputs: Vec<ArkFr> = inputs.iter().map(|f| f.0).collect();

        let result = if ark_inputs.len() <= 12 {
            let mut exact_hasher = Poseidon::<ArkFr>::new_circom(ark_inputs.len())?;
            exact_hasher.hash(&ark_inputs)?
        } else {
            self.hash_chain(&ark_inputs)?
        };

        Ok(Fr(result))
    }

    fn hash_chain(&mut self, inputs: &[ArkFr]) -> Result<ArkFr, PoseidonError> {
        let mut acc = inputs[0];
        for input in inputs.iter().skip(1) {
            acc = self.hasher_2.hash(&[acc, *input])?;
        }
        Ok(acc)
    }
}

impl Default for PoseidonBn254 {
    fn default() -> Self {
        Self::new().expect("Failed to initialize Poseidon")
    }
}

thread_local! {
    static POSEIDON: std::cell::RefCell<PoseidonBn254> =
        std::cell::RefCell::new(PoseidonBn254::default());
}

pub fn hash_2(a: Fr, b: Fr) -> Fr {
    POSEIDON.with(|p| {
        p.borrow_mut().hash_2(a, b).expect("Poseidon hash_2 failed")
    })
}

pub fn hash_many(inputs: &[Fr]) -> Fr {
    POSEIDON.with(|p| {
        p.borrow_mut().hash_many(inputs).expect("Poseidon hash_many failed")
    })
}

pub fn hash_with_domain(domain: Fr, inputs: &[Fr]) -> Fr {
    let mut acc = domain;
    for input in inputs {
        acc = hash_2(acc, *input);
    }
    acc
}

pub fn commit(value: Fr, blinding: Fr) -> Fr {
    hash_2(value, blinding)
}

pub fn derive_nullifier(identity: Fr, scope: Fr) -> Fr {
    const NULLIFIER_DOMAIN: u64 = 0x6e756c6c69666965;
    let domain_hash = hash_2(Fr::from_u64(NULLIFIER_DOMAIN), scope);
    hash_2(domain_hash, identity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_2_deterministic() {
        let a = Fr::from_u64(1);
        let b = Fr::from_u64(2);

        let h1 = hash_2(a, b);
        let h2 = hash_2(a, b);

        assert_eq!(h1, h2, "Poseidon must be deterministic");
    }

    #[test]
    fn test_hash_2_different_inputs() {
        let h1 = hash_2(Fr::from_u64(1), Fr::from_u64(2));
        let h2 = hash_2(Fr::from_u64(1), Fr::from_u64(3));
        let h3 = hash_2(Fr::from_u64(2), Fr::from_u64(2));

        assert_ne!(h1, h2);
        assert_ne!(h1, h3);
        assert_ne!(h2, h3);
    }

    #[test]
    fn test_hash_2_non_commutative() {
        let a = Fr::from_u64(5);
        let b = Fr::from_u64(7);

        let h1 = hash_2(a, b);
        let h2 = hash_2(b, a);

        assert_ne!(h1, h2, "Poseidon hash_2 is not commutative");
    }

    #[test]
    fn test_hash_many_empty() {
        let h = hash_many(&[]);
        assert_eq!(h, Fr::zero());
    }

    #[test]
    fn test_hash_many_single() {
        let h1 = hash_many(&[Fr::from_u64(42)]);
        let h2 = hash_many(&[Fr::from_u64(42)]);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_many_multiple() {
        let inputs: Vec<Fr> = (0..5).map(|i| Fr::from_u64(i)).collect();

        let h1 = hash_many(&inputs);
        let h2 = hash_many(&inputs);

        assert_eq!(h1, h2);
    }

    #[test]
    fn test_domain_separation() {
        let input = Fr::from_u64(100);

        let domain_a = Fr::from_u64(1);
        let domain_b = Fr::from_u64(2);

        let h_a = hash_with_domain(domain_a, &[input]);
        let h_b = hash_with_domain(domain_b, &[input]);

        assert_ne!(h_a, h_b, "Different domains must produce different hashes");
    }

    #[test]
    fn test_nullifier_unlinkable() {
        let identity = Fr::from_u64(12345);

        let scope_a = Fr::from_u64(1);
        let scope_b = Fr::from_u64(2);

        let null_a = derive_nullifier(identity, scope_a);
        let null_b = derive_nullifier(identity, scope_b);

        assert_ne!(null_a, null_b, "Different scopes must produce different nullifiers");
    }

    #[test]
    fn test_nullifier_unique() {
        let identity_a = Fr::from_u64(111);
        let identity_b = Fr::from_u64(222);
        let scope = Fr::from_u64(1);

        let null_a = derive_nullifier(identity_a, scope);
        let null_b = derive_nullifier(identity_b, scope);

        assert_ne!(null_a, null_b, "Different identities must produce different nullifiers");
    }

    #[test]
    fn test_commitment_hiding() {
        let value = Fr::from_u64(1000);
        let blind_a = Fr::from_u64(111);
        let blind_b = Fr::from_u64(222);

        let c_a = commit(value, blind_a);
        let c_b = commit(value, blind_b);

        assert_ne!(c_a, c_b, "Different blindings must hide the same value");
    }
}
