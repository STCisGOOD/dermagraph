
use ark_bn254::Fr as ArkFr;
use ark_ff::{Field, PrimeField};
use ark_std::{One, Zero, UniformRand};
use std::ops::{Add, Sub, Mul, Neg};
use serde::{Serialize, Deserialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Fr(pub(crate) ArkFr);

impl Fr {
    pub const MODULUS: &'static str =
        "21888242871839275222246405745257275088548364400416034343698204186575808495617";

    #[inline]
    pub fn zero() -> Self {
        Fr(ArkFr::zero())
    }

    #[inline]
    pub fn one() -> Self {
        Fr(ArkFr::one())
    }

    #[inline]
    pub fn from_u64(val: u64) -> Self {
        Fr(ArkFr::from(val))
    }

    pub fn from_le_bytes(bytes: &[u8]) -> Option<Self> {
        ArkFr::from_random_bytes(bytes).map(Fr)
    }

    pub fn from_be_bytes_mod_order(bytes: &[u8]) -> Self {
        Fr(ArkFr::from_be_bytes_mod_order(bytes))
    }

    pub fn to_le_bytes(&self) -> [u8; 32] {
        let bigint = self.0.into_bigint();
        let mut bytes = [0u8; 32];
        for (i, limb) in bigint.0.iter().enumerate() {
            bytes[i * 8..(i + 1) * 8].copy_from_slice(&limb.to_le_bytes());
        }
        bytes
    }

    pub fn to_be_bytes(&self) -> [u8; 32] {
        let mut bytes = self.to_le_bytes();
        bytes.reverse();
        bytes
    }

    #[inline]
    pub fn inverse(&self) -> Option<Self> {
        self.0.inverse().map(Fr)
    }

    #[inline]
    pub fn square(&self) -> Self {
        Fr(self.0.square())
    }

    #[inline]
    pub fn cube(&self) -> Self {
        Fr(self.0.square() * self.0)
    }

    #[inline]
    pub fn pow(&self, exp: u64) -> Self {
        Fr(self.0.pow([exp]))
    }

    #[inline]
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    pub fn random<R: rand::Rng + ?Sized>(rng: &mut R) -> Self {
        Fr(ArkFr::rand(rng))
    }

    #[deprecated(since = "0.1.0", note = "Use crate::poseidon::hash_many for cryptographic security")]
    pub fn hash_many(elements: &[Fr]) -> Fr {
        let mut state = Fr::zero();
        for (i, elem) in elements.iter().enumerate() {
            state = state * Fr::from_u64(31) + *elem + Fr::from_u64(i as u64);
            state = state.cube() + Fr::from_u64(5);
        }
        state
    }
}

impl Add for Fr {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Fr(self.0 + rhs.0)
    }
}

impl Sub for Fr {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Fr(self.0 - rhs.0)
    }
}

impl Mul for Fr {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        Fr(self.0 * rhs.0)
    }
}

impl Neg for Fr {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Fr(-self.0)
    }
}

impl std::iter::Sum for Fr {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Fr::zero(), |acc, x| acc + x)
    }
}

impl Default for Fr {
    fn default() -> Self {
        Fr::zero()
    }
}

impl From<u64> for Fr {
    fn from(val: u64) -> Self {
        Fr::from_u64(val)
    }
}

impl From<i64> for Fr {
    fn from(val: i64) -> Self {
        if val >= 0 {
            Fr::from_u64(val as u64)
        } else {
            -Fr::from_u64((-val) as u64)
        }
    }
}

impl Serialize for Fr {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let bytes = self.to_be_bytes();
        serializer.serialize_bytes(&bytes)
    }
}

impl<'de> Deserialize<'de> for Fr {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes: Vec<u8> = Deserialize::deserialize(deserializer)?;
        Ok(Fr::from_be_bytes_mod_order(&bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_arithmetic() {
        let a = Fr::from_u64(5);
        let b = Fr::from_u64(7);

        assert_eq!(a + b, Fr::from_u64(12));
        assert_eq!(b - a, Fr::from_u64(2));
        assert_eq!(a * b, Fr::from_u64(35));
    }

    #[test]
    fn test_inverse() {
        let a = Fr::from_u64(5);
        let a_inv = a.inverse().unwrap();
        assert_eq!(a * a_inv, Fr::one());
    }

    #[test]
    fn test_zero_inverse() {
        assert!(Fr::zero().inverse().is_none());
    }

    #[test]
    fn test_cube() {
        let a = Fr::from_u64(3);
        assert_eq!(a.cube(), Fr::from_u64(27));
    }

    #[test]
    fn test_serialization_roundtrip() {
        let a = Fr::from_u64(12345678);
        let bytes = a.to_be_bytes();
        let b = Fr::from_be_bytes_mod_order(&bytes);
        assert_eq!(a, b);
    }

}
