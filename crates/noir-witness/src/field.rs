
use turing_core::Fr as TcFr;
use sha2::{Sha256, Digest};

use crate::constants::{COORDINATE_SCALE, ANGLE_SCALE};

pub use turing_core::Fr;

pub struct FieldFormatter;

impl FieldFormatter {
    pub fn from_u64(value: u64) -> String {
        format!("0x{:064x}", value)
    }

    pub fn from_u32(value: u32) -> String {
        Self::from_u64(value as u64)
    }

    pub fn from_i64(value: i64) -> String {
        if value >= 0 {
            Self::from_u64(value as u64)
        } else {
            let abs_val = (-value) as u64;
            let fr = -TcFr::from_u64(abs_val);
            Self::from_tc_fr(&fr)
        }
    }

    pub fn from_tc_fr(fr: &TcFr) -> String {
        let bytes = fr.to_be_bytes();
        format!("0x{}", hex::encode(bytes))
    }

    pub fn from_coordinate(coord: f64) -> String {
        let scaled = (coord * COORDINATE_SCALE as f64).round() as i64;
        Self::from_i64(scaled)
    }

    pub fn from_angle(radians: f64) -> String {
        let normalized = radians.rem_euclid(std::f64::consts::TAU);
        let scaled = (normalized * ANGLE_SCALE as f64).round() as u64;
        Self::from_u64(scaled)
    }

    pub fn hash_to_field(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let hash = hasher.finalize();

        let mut bytes = [0u8; 32];
        bytes[1..].copy_from_slice(&hash[..31]);

        let fr = TcFr::from_be_bytes_mod_order(&bytes);
        Self::from_tc_fr(&fr)
    }

    pub fn from_string(s: &str) -> String {
        Self::hash_to_field(s.as_bytes())
    }

    pub fn from_bool(b: bool) -> String {
        if b {
            Self::from_u64(1)
        } else {
            Self::from_u64(0)
        }
    }

    pub fn zero() -> String {
        Self::from_u64(0)
    }

    pub fn one() -> String {
        Self::from_u64(1)
    }

    pub fn poseidon_hash_2(a: &TcFr, b: &TcFr) -> TcFr {
        use turing_core::poseidon::hash_2;
        hash_2(*a, *b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_u64() {
        assert_eq!(
            FieldFormatter::from_u64(42),
            "0x000000000000000000000000000000000000000000000000000000000000002a"
        );
    }

    #[test]
    fn test_from_coordinate() {
        let s = FieldFormatter::from_coordinate(100.5);
        assert!(s.starts_with("0x"));
        assert!(s.ends_with(&format!("{:x}", 100500)));
    }

    #[test]
    fn test_from_angle() {
        let s = FieldFormatter::from_angle(std::f64::consts::FRAC_PI_2);
        assert!(s.starts_with("0x"));
    }

    #[test]
    fn test_negative_field() {
        let s = FieldFormatter::from_i64(-1);
        assert!(s.starts_with("0x"));
        assert!(s.len() > 50);
    }

    #[test]
    fn test_hash_to_field() {
        let h1 = FieldFormatter::hash_to_field(b"test");
        let h2 = FieldFormatter::hash_to_field(b"test");
        let h3 = FieldFormatter::hash_to_field(b"different");

        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_poseidon_hash() {
        let a = TcFr::from_u64(1);
        let b = TcFr::from_u64(2);

        let h1 = FieldFormatter::poseidon_hash_2(&a, &b);
        let h2 = FieldFormatter::poseidon_hash_2(&a, &b);

        assert_eq!(h1, h2);
        assert!(!h1.is_zero());
    }
}
