
use crate::field::Fr;
use crate::matrix::GraphLaplacian;
use crate::sth::{SpectralTuringHash, STHParams};
use crate::Result;
use crate::TuringError;

use sha2::{Sha256, Sha512, Digest};
use argon2::{Argon2, Algorithm, Version, Params};
use hkdf::Hkdf;

pub mod domains {
    pub const MASTER: &[u8] = b"dermagraph:v1:master";
    pub const HARDENING: &[u8] = b"dermagraph:v1:hardening";
    pub const SIGNING: &[u8] = b"secp256k1:signing:v1";
    pub const NULLIFIER: &[u8] = b"dermagraph:v1:nullifier";
    pub const COMMITMENT: &[u8] = b"dermagraph:v1:commitment";
}

#[derive(Clone, Debug)]
pub struct HardeningParams {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
    pub output_len: usize,
}

impl Default for HardeningParams {
    fn default() -> Self {
        Self::standard()
    }
}

impl HardeningParams {
    pub fn standard() -> Self {
        Self {
            memory_kib: 1_048_576,
            iterations: 3,
            parallelism: 4,
            output_len: 32,
        }
    }

    pub fn paranoid() -> Self {
        Self {
            memory_kib: 2_097_152,
            iterations: 4,
            parallelism: 4,
            output_len: 32,
        }
    }

    pub fn fast() -> Self {
        Self {
            memory_kib: 262_144,
            iterations: 2,
            parallelism: 4,
            output_len: 32,
        }
    }

    pub fn attack_cost_estimate(&self, entropy_bits: f64) -> f64 {
        let attempts = 2_f64.powf(entropy_bits);
        let memory_gb = self.memory_kib as f64 / 1_048_576.0;
        let time_hours = (self.iterations as f64 * 0.5) / 3600.0;
        let cost_per_attempt = memory_gb * time_hours * 10.0;
        attempts * cost_per_attempt
    }
}

#[derive(Clone)]
pub struct SovereignWallet {
    master_seed: [u8; 32],
    signing_key: [u8; 32],
    public_key: [u8; 33],
    address: [u8; 20],
    chain_id: u64,
}

impl SovereignWallet {
    pub fn derive(
        minutiae_x: &[f64],
        minutiae_y: &[f64],
        minutiae_theta: &[f64],
        quantized_spectrum: &[u64],
        laplacian: &GraphLaplacian,
        passphrase: Option<&str>,
        chain_id: u64,
        account_index: u32,
        hardening: &HardeningParams,
    ) -> Result<Self> {
        let sth_params = STHParams::standard_128bit();
        let identity = SpectralTuringHash::compute(
            minutiae_x,
            minutiae_y,
            minutiae_theta,
            quantized_spectrum,
            laplacian,
            &sth_params,
        )?;

        let identity_bytes = Fr::to_be_bytes(&identity);

        let master_input = if let Some(pass) = passphrase {
            let pass_salt = &identity_bytes[0..16];

            let pass_params = Params::new(
                262_144,
                4,
                4,
                Some(32),
            ).map_err(|e| TuringError::InvalidParameter {
                name: "argon2_params",
                value: e.to_string(),
            })?;

            let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, pass_params);
            let mut passphrase_key = [0u8; 32];
            argon2.hash_password_into(pass.as_bytes(), pass_salt, &mut passphrase_key)
                .map_err(|e| TuringError::InvalidParameter {
                    name: "passphrase_hash",
                    value: e.to_string(),
                })?;

            let mut combined = [0u8; 32];
            for i in 0..32 {
                combined[i] = identity_bytes[i] ^ passphrase_key[i];
            }
            combined
        } else {
            identity_bytes
        };

        let hardening_params = Params::new(
            hardening.memory_kib,
            hardening.iterations,
            hardening.parallelism,
            Some(hardening.output_len),
        ).map_err(|e| TuringError::InvalidParameter {
            name: "hardening_params",
            value: e.to_string(),
        })?;

        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, hardening_params);
        let mut hardened = [0u8; 32];
        argon2.hash_password_into(&master_input, domains::HARDENING, &mut hardened)
            .map_err(|e| TuringError::InvalidParameter {
                name: "hardening",
                value: e.to_string(),
            })?;

        let mut info = Vec::with_capacity(12);
        info.extend_from_slice(&chain_id.to_be_bytes());
        info.extend_from_slice(&account_index.to_be_bytes());

        let hk = Hkdf::<Sha512>::new(Some(domains::MASTER), &hardened);
        let mut master_seed = [0u8; 32];
        hk.expand(&info, &mut master_seed)
            .map_err(|_| TuringError::InvalidParameter {
                name: "hkdf_expand",
                value: "output too long".to_string(),
            })?;

        let signing_key = derive_secp256k1_key(&master_seed)?;
        let public_key = compute_public_key(&signing_key);
        let address = compute_ethereum_address(&public_key);

        Ok(Self {
            master_seed,
            signing_key,
            public_key,
            address,
            chain_id,
        })
    }

    pub fn address(&self) -> &[u8; 20] {
        &self.address
    }

    pub fn address_checksum(&self) -> String {
        eip55_checksum(&self.address)
    }

    pub fn public_key(&self) -> &[u8; 33] {
        &self.public_key
    }

    pub fn public_key_uncompressed(&self) -> [u8; 65] {
        decompress_public_key(&self.public_key)
    }

    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    pub fn sign_hash(&self, message_hash: &[u8; 32]) -> Result<Signature> {
        ecdsa_sign_deterministic(&self.signing_key, message_hash, self.chain_id)
    }

    pub fn sign_personal(&self, message: &[u8]) -> Result<Signature> {
        let hash = eip191_hash(message);
        self.sign_hash(&hash)
    }

    pub fn sign_typed_data(
        &self,
        domain_separator: &[u8; 32],
        struct_hash: &[u8; 32],
    ) -> Result<Signature> {
        let hash = eip712_hash(domain_separator, struct_hash);
        self.sign_hash(&hash)
    }

    pub fn derive_nullifier(&self, scope: &str) -> [u8; 32] {
        let hk = Hkdf::<Sha256>::new(Some(domains::NULLIFIER), &self.master_seed);
        let mut nullifier = [0u8; 32];
        hk.expand(scope.as_bytes(), &mut nullifier).expect("valid output length");
        nullifier
    }

    pub fn derive_commitment(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(&self.master_seed);
        hasher.update(domains::COMMITMENT);
        hasher.finalize().into()
    }

    pub fn derive_child(&self, child_index: u32) -> Result<SovereignWallet> {
        let hk = Hkdf::<Sha512>::new(Some(b"dermagraph:v1:child"), &self.master_seed);
        let mut child_seed = [0u8; 32];
        hk.expand(&child_index.to_be_bytes(), &mut child_seed)
            .map_err(|_| TuringError::InvalidParameter {
                name: "child_derivation",
                value: "hkdf expand failed".to_string(),
            })?;

        let signing_key = derive_secp256k1_key(&child_seed)?;
        let public_key = compute_public_key(&signing_key);
        let address = compute_ethereum_address(&public_key);

        Ok(SovereignWallet {
            master_seed: child_seed,
            signing_key,
            public_key,
            address,
            chain_id: self.chain_id,
        })
    }

    pub fn zeroize(&mut self) {
        self.master_seed.fill(0);
        self.signing_key.fill(0);
    }
}

impl Drop for SovereignWallet {
    fn drop(&mut self) {
        self.zeroize();
    }
}

#[derive(Clone, Debug)]
pub struct Signature {
    pub r: [u8; 32],
    pub s: [u8; 32],
    pub v: u8,
}

impl Signature {
    pub fn to_bytes(&self) -> [u8; 65] {
        let mut bytes = [0u8; 65];
        bytes[0..32].copy_from_slice(&self.r);
        bytes[32..64].copy_from_slice(&self.s);
        bytes[64] = self.v;
        bytes
    }

    pub fn to_hex(&self) -> String {
        let bytes = self.to_bytes();
        format!("0x{}", hex::encode(bytes))
    }
}

const SECP256K1_N: [u8; 32] = [
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFE,
    0xBA, 0xAE, 0xDC, 0xE6, 0xAF, 0x48, 0xA0, 0x3B,
    0xBF, 0xD2, 0x5E, 0x8C, 0xD0, 0x36, 0x41, 0x41,
];

const SECP256K1_N_HALF: [u8; 32] = [
    0x7F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0x5D, 0x57, 0x6E, 0x73, 0x57, 0xA4, 0x50, 0x1D,
    0xDF, 0xE9, 0x2F, 0x46, 0x68, 0x1B, 0x20, 0xA0,
];

fn derive_secp256k1_key(seed: &[u8; 32]) -> Result<[u8; 32]> {
    let hk = Hkdf::<Sha512>::new(Some(domains::SIGNING), seed);

    for counter in 0u8..=255 {
        let mut key = [0u8; 32];
        let info = [counter];
        hk.expand(&info, &mut key)
            .map_err(|_| TuringError::InvalidParameter {
                name: "signing_key_derivation",
                value: "hkdf expand failed".to_string(),
            })?;

        if is_valid_secp256k1_scalar(&key) {
            return Ok(key);
        }
    }

    Err(TuringError::InvalidParameter {
        name: "signing_key",
        value: "could not derive valid key after 256 attempts".to_string(),
    })
}

fn is_valid_secp256k1_scalar(bytes: &[u8; 32]) -> bool {
    let is_zero = bytes.iter().all(|&b| b == 0);
    if is_zero {
        return false;
    }

    for i in 0..32 {
        if bytes[i] < SECP256K1_N[i] {
            return true;
        }
        if bytes[i] > SECP256K1_N[i] {
            return false;
        }
    }
    false
}

fn compute_public_key(private_key: &[u8; 32]) -> [u8; 33] {
    use k256::ecdsa::SigningKey;
    use k256::elliptic_curve::sec1::ToEncodedPoint;

    let signing_key = SigningKey::from_bytes(private_key.into())
        .expect("already validated private key");
    let verifying_key = signing_key.verifying_key();
    let point = verifying_key.to_encoded_point(true);

    let mut pubkey = [0u8; 33];
    pubkey.copy_from_slice(point.as_bytes());
    pubkey
}

fn decompress_public_key(compressed: &[u8; 33]) -> [u8; 65] {
    use k256::PublicKey;
    use k256::elliptic_curve::sec1::ToEncodedPoint;

    let pubkey = PublicKey::from_sec1_bytes(compressed)
        .expect("valid compressed public key");
    let point = pubkey.to_encoded_point(false);

    let mut uncompressed = [0u8; 65];
    uncompressed.copy_from_slice(point.as_bytes());
    uncompressed
}

fn compute_ethereum_address(compressed_pubkey: &[u8; 33]) -> [u8; 20] {
    use sha3::{Keccak256, Digest as Keccak256Digest};

    let uncompressed = decompress_public_key(compressed_pubkey);

    let mut hasher = Keccak256::new();
    hasher.update(&uncompressed[1..65]);
    let hash = hasher.finalize();

    let mut address = [0u8; 20];
    address.copy_from_slice(&hash[12..32]);
    address
}

fn eip55_checksum(address: &[u8; 20]) -> String {
    use sha3::{Keccak256, Digest as Keccak256Digest};

    let hex_addr = hex::encode(address);
    let hash = Keccak256::digest(hex_addr.as_bytes());

    let mut result = String::with_capacity(42);
    result.push_str("0x");

    for (i, c) in hex_addr.chars().enumerate() {
        let hash_nibble = if i % 2 == 0 {
            hash[i / 2] >> 4
        } else {
            hash[i / 2] & 0x0F
        };

        if hash_nibble >= 8 {
            result.push(c.to_ascii_uppercase());
        } else {
            result.push(c);
        }
    }

    result
}

fn ecdsa_sign_deterministic(
    private_key: &[u8; 32],
    message_hash: &[u8; 32],
    chain_id: u64,
) -> Result<Signature> {
    use k256::ecdsa::{SigningKey, signature::Signer};
    use k256::ecdsa::Signature as K256Signature;

    let signing_key = SigningKey::from_bytes(private_key.into())
        .map_err(|_| TuringError::InvalidParameter {
            name: "private_key",
            value: "invalid secp256k1 scalar".to_string(),
        })?;

    let (signature, recovery_id) = signing_key
        .sign_prehash_recoverable(message_hash)
        .map_err(|_| TuringError::InvalidParameter {
            name: "signing",
            value: "ecdsa signing failed".to_string(),
        })?;

    let r_bytes: [u8; 32] = signature.r().to_bytes().into();
    let s_bytes: [u8; 32] = signature.s().to_bytes().into();

    let (s_normalized, recovery_adjustment) = normalize_s(&s_bytes);

    let recovery_id_adjusted = (recovery_id.to_byte() ^ recovery_adjustment) & 1;
    let v = if chain_id > 0 {
        ((chain_id * 2 + 35) as u8).wrapping_add(recovery_id_adjusted)
    } else {
        27 + recovery_id_adjusted
    };

    Ok(Signature {
        r: r_bytes,
        s: s_normalized,
        v,
    })
}

fn normalize_s(s: &[u8; 32]) -> ([u8; 32], u8) {
    let mut s_is_high = false;
    for i in 0..32 {
        if s[i] > SECP256K1_N_HALF[i] {
            s_is_high = true;
            break;
        }
        if s[i] < SECP256K1_N_HALF[i] {
            break;
        }
    }

    if s_is_high {
        let mut s_normalized = [0u8; 32];
        let mut borrow = 0u16;
        for i in (0..32).rev() {
            let diff = (SECP256K1_N[i] as u16) - (s[i] as u16) - borrow;
            s_normalized[i] = diff as u8;
            borrow = if diff > 255 { 1 } else { 0 };
        }
        (s_normalized, 1)
    } else {
        (*s, 0)
    }
}

fn eip191_hash(message: &[u8]) -> [u8; 32] {
    use sha3::{Keccak256, Digest as Keccak256Digest};

    let prefix = format!("\x19Ethereum Signed Message:\n{}", message.len());

    let mut hasher = Keccak256::new();
    hasher.update(prefix.as_bytes());
    hasher.update(message);
    hasher.finalize().into()
}

fn eip712_hash(domain_separator: &[u8; 32], struct_hash: &[u8; 32]) -> [u8; 32] {
    use sha3::{Keccak256, Digest as Keccak256Digest};

    let mut hasher = Keccak256::new();
    hasher.update(b"\x19\x01");
    hasher.update(domain_separator);
    hasher.update(struct_hash);
    hasher.finalize().into()
}

#[derive(Debug)]
pub struct SecurityAnalysis {
    pub biometric_entropy: f64,
    pub passphrase_entropy: f64,
    pub total_entropy: f64,
    pub memory_cost: u64,
    pub time_cost: u32,
    pub attack_cost_usd: f64,
    pub assessment: SecurityLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityLevel {
    Broken,
    Weak,
    Moderate,
    Good,
    Strong,
    Paranoid,
}

impl SecurityAnalysis {
    pub fn analyze(
        biometric_entropy: f64,
        passphrase: Option<&str>,
        hardening: &HardeningParams,
    ) -> Self {
        let passphrase_entropy = passphrase
            .map(|p| estimate_passphrase_entropy(p))
            .unwrap_or(0.0);

        let total_entropy = biometric_entropy + passphrase_entropy;
        let attack_cost = hardening.attack_cost_estimate(total_entropy);

        let assessment = match total_entropy {
            e if e < 40.0 => SecurityLevel::Broken,
            e if e < 60.0 => SecurityLevel::Weak,
            e if e < 80.0 => SecurityLevel::Moderate,
            e if e < 100.0 => SecurityLevel::Good,
            e if e < 128.0 => SecurityLevel::Strong,
            _ => SecurityLevel::Paranoid,
        };

        Self {
            biometric_entropy,
            passphrase_entropy,
            total_entropy,
            memory_cost: hardening.memory_kib as u64 * 1024,
            time_cost: hardening.iterations,
            attack_cost_usd: attack_cost,
            assessment,
        }
    }

    pub fn print_report(&self) {
        println!("╔══════════════════════════════════════════════════════════════╗");
        println!("║               SECURITY ANALYSIS REPORT                        ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║ Biometric entropy:    {:>6.1} bits                            ║", self.biometric_entropy);
        println!("║ Passphrase entropy:   {:>6.1} bits                            ║", self.passphrase_entropy);
        println!("║ Total entropy:        {:>6.1} bits                            ║", self.total_entropy);
        println!("║ Memory hardening:     {:>6} MB                              ║", self.memory_cost / 1_048_576);
        println!("║ Time hardening:       {:>6} iterations                      ║", self.time_cost);
        println!("║ Attack cost:          ${:>12.0}                      ║", self.attack_cost_usd);
        println!("║ Assessment:           {:?}                                   ║", self.assessment);
        println!("╚══════════════════════════════════════════════════════════════╝");
    }
}

fn estimate_passphrase_entropy(passphrase: &str) -> f64 {
    let len = passphrase.len() as f64;
    let has_lower = passphrase.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = passphrase.chars().any(|c| c.is_ascii_uppercase());
    let has_digit = passphrase.chars().any(|c| c.is_ascii_digit());
    let has_special = passphrase.chars().any(|c| !c.is_alphanumeric());

    let charset_size: f64 = match (has_lower, has_upper, has_digit, has_special) {
        (true, true, true, true) => 95.0,
        (true, true, true, false) => 62.0,
        (true, false, true, false) => 36.0,
        (true, false, false, false) => 26.0,
        _ => 26.0,
    };

    len * charset_size.log2()
}

pub mod solana_domains {
    pub const ED25519_SIGNING: &[u8] = b"ed25519:signing:v1";
    pub const VAULT_PROGRAM: &[u8] = b"dermagraph:solana:vault:v1";
    pub const X402_PAYMENT: &[u8] = b"x402:payment:v1";
}

#[derive(Clone)]
pub struct SolanaSovereignWallet {
    master_seed: [u8; 32],
    secret_key: [u8; 32],
    public_key: [u8; 32],
}

impl SolanaSovereignWallet {
    pub fn derive(
        minutiae_x: &[f64],
        minutiae_y: &[f64],
        minutiae_theta: &[f64],
        quantized_spectrum: &[u64],
        laplacian: &GraphLaplacian,
        passphrase: Option<&str>,
        account_index: u32,
        hardening: &HardeningParams,
    ) -> Result<Self> {
        let sth_params = STHParams::standard_128bit();
        let identity = SpectralTuringHash::compute(
            minutiae_x,
            minutiae_y,
            minutiae_theta,
            quantized_spectrum,
            laplacian,
            &sth_params,
        )?;

        let identity_bytes = Fr::to_be_bytes(&identity);

        let master_input = if let Some(pass) = passphrase {
            let pass_salt = &identity_bytes[0..16];

            let pass_params = Params::new(
                262_144,
                4,
                4,
                Some(32),
            ).map_err(|e| TuringError::InvalidParameter {
                name: "argon2_params",
                value: e.to_string(),
            })?;

            let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, pass_params);
            let mut passphrase_key = [0u8; 32];
            argon2.hash_password_into(pass.as_bytes(), pass_salt, &mut passphrase_key)
                .map_err(|e| TuringError::InvalidParameter {
                    name: "passphrase_hash",
                    value: e.to_string(),
                })?;

            let mut combined = [0u8; 32];
            for i in 0..32 {
                combined[i] = identity_bytes[i] ^ passphrase_key[i];
            }
            combined
        } else {
            identity_bytes
        };

        let hardening_params = Params::new(
            hardening.memory_kib,
            hardening.iterations,
            hardening.parallelism,
            Some(hardening.output_len),
        ).map_err(|e| TuringError::InvalidParameter {
            name: "hardening_params",
            value: e.to_string(),
        })?;

        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, hardening_params);
        let mut hardened = [0u8; 32];
        argon2.hash_password_into(&master_input, domains::HARDENING, &mut hardened)
            .map_err(|e| TuringError::InvalidParameter {
                name: "hardening",
                value: e.to_string(),
            })?;

        let mut info = Vec::with_capacity(12);
        info.extend_from_slice(b"solana");
        info.extend_from_slice(&account_index.to_be_bytes());

        let hk = Hkdf::<Sha512>::new(Some(domains::MASTER), &hardened);
        let mut master_seed = [0u8; 32];
        hk.expand(&info, &mut master_seed)
            .map_err(|_| TuringError::InvalidParameter {
                name: "hkdf_expand",
                value: "output too long".to_string(),
            })?;

        let (secret_key, public_key) = derive_ed25519_keypair(&master_seed)?;

        Ok(Self {
            master_seed,
            secret_key,
            public_key,
        })
    }

    pub fn address(&self) -> String {
        bs58::encode(&self.public_key).into_string()
    }

    pub fn public_key(&self) -> &[u8; 32] {
        &self.public_key
    }

    pub fn pubkey_bytes(&self) -> [u8; 32] {
        self.public_key
    }

    pub fn sign(&self, message: &[u8]) -> Result<[u8; 64]> {
        ed25519_sign(&self.secret_key, &self.public_key, message)
    }

    pub fn sign_transaction(&self, transaction_message: &[u8]) -> Result<[u8; 64]> {
        self.sign(transaction_message)
    }

    pub fn sign_x402_payment(
        &self,
        serialized_tx_message: &[u8],
    ) -> Result<X402PaymentProof> {
        let signature = self.sign(serialized_tx_message)?;

        Ok(X402PaymentProof {
            signature,
            public_key: self.public_key,
            serialized_message: serialized_tx_message.to_vec(),
        })
    }

    pub fn derive_nullifier(&self, scope: &str) -> [u8; 32] {
        let hk = Hkdf::<Sha256>::new(Some(domains::NULLIFIER), &self.master_seed);
        let mut nullifier = [0u8; 32];
        hk.expand(scope.as_bytes(), &mut nullifier).expect("valid output length");
        nullifier
    }

    pub fn vault_pda_seeds(&self) -> Vec<Vec<u8>> {
        let commitment = self.derive_vault_commitment();
        vec![
            b"biometric_vault".to_vec(),
            commitment.to_vec(),
        ]
    }

    pub fn derive_vault_commitment(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(&self.master_seed);
        hasher.update(b"vault_commitment");
        hasher.finalize().into()
    }

    pub fn derive_child(&self, child_index: u32) -> Result<SolanaSovereignWallet> {
        let hk = Hkdf::<Sha512>::new(Some(b"dermagraph:v1:child:solana"), &self.master_seed);
        let mut child_seed = [0u8; 32];
        hk.expand(&child_index.to_be_bytes(), &mut child_seed)
            .map_err(|_| TuringError::InvalidParameter {
                name: "child_derivation",
                value: "hkdf expand failed".to_string(),
            })?;

        let (secret_key, public_key) = derive_ed25519_keypair(&child_seed)?;

        Ok(SolanaSovereignWallet {
            master_seed: child_seed,
            secret_key,
            public_key,
        })
    }

    pub fn zeroize(&mut self) {
        self.master_seed.fill(0);
        self.secret_key.fill(0);
    }
}

impl Drop for SolanaSovereignWallet {
    fn drop(&mut self) {
        self.zeroize();
    }
}

#[derive(Clone, Debug)]
pub struct X402PaymentProof {
    pub signature: [u8; 64],
    pub public_key: [u8; 32],
    pub serialized_message: Vec<u8>,
}

impl X402PaymentProof {
    pub fn to_x402_header(&self, network: &str) -> String {
        use base64::{Engine as _, engine::general_purpose::STANDARD};

        let mut signed_tx = Vec::with_capacity(64 + self.serialized_message.len());
        signed_tx.extend_from_slice(&self.signature);
        signed_tx.extend_from_slice(&self.serialized_message);

        let payload = format!(
            r#"{{"x402Version":1,"scheme":"exact","network":"{}","payload":{{"serializedTransaction":"{}"}}}}"#,
            network,
            STANDARD.encode(&signed_tx)
        );

        STANDARD.encode(payload.as_bytes())
    }

    pub fn verify(&self) -> bool {
        ed25519_verify(&self.public_key, &self.serialized_message, &self.signature)
    }
}

fn derive_ed25519_keypair(seed: &[u8; 32]) -> Result<([u8; 32], [u8; 32])> {
    use ed25519_dalek::{SigningKey, VerifyingKey};

    let hk = Hkdf::<Sha512>::new(Some(solana_domains::ED25519_SIGNING), seed);
    let mut ed_seed = [0u8; 32];
    hk.expand(&[], &mut ed_seed)
        .map_err(|_| TuringError::InvalidParameter {
            name: "ed25519_derivation",
            value: "hkdf expand failed".to_string(),
        })?;

    let signing_key = SigningKey::from_bytes(&ed_seed);
    let verifying_key: VerifyingKey = (&signing_key).into();

    Ok((ed_seed, verifying_key.to_bytes()))
}

fn ed25519_sign(
    secret_key: &[u8; 32],
    _public_key: &[u8; 32],
    message: &[u8],
) -> Result<[u8; 64]> {
    use ed25519_dalek::{SigningKey, Signer};

    let signing_key = SigningKey::from_bytes(secret_key);
    let signature = signing_key.sign(message);

    Ok(signature.to_bytes())
}

fn ed25519_verify(public_key: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool {
    use ed25519_dalek::{VerifyingKey, Verifier, Signature};

    let Ok(verifying_key) = VerifyingKey::from_bytes(public_key) else {
        return false;
    };

    let sig = Signature::from_bytes(signature);
    verifying_key.verify(message, &sig).is_ok()
}

pub struct DualChainWallet {
    pub ethereum: SovereignWallet,
    pub solana: SolanaSovereignWallet,
    nullifier_seed: [u8; 32],
}

impl DualChainWallet {
    pub fn derive(
        minutiae_x: &[f64],
        minutiae_y: &[f64],
        minutiae_theta: &[f64],
        quantized_spectrum: &[u64],
        laplacian: &GraphLaplacian,
        passphrase: Option<&str>,
        eth_chain_id: u64,
        account_index: u32,
        hardening: &HardeningParams,
    ) -> Result<Self> {
        let ethereum = SovereignWallet::derive(
            minutiae_x,
            minutiae_y,
            minutiae_theta,
            quantized_spectrum,
            laplacian,
            passphrase,
            eth_chain_id,
            account_index,
            hardening,
        )?;

        let solana = SolanaSovereignWallet::derive(
            minutiae_x,
            minutiae_y,
            minutiae_theta,
            quantized_spectrum,
            laplacian,
            passphrase,
            account_index,
            hardening,
        )?;

        let sth_params = STHParams::standard_128bit();
        let identity = SpectralTuringHash::compute(
            minutiae_x,
            minutiae_y,
            minutiae_theta,
            quantized_spectrum,
            laplacian,
            &sth_params,
        )?;
        let identity_bytes = Fr::to_be_bytes(&identity);

        let hk = Hkdf::<Sha256>::new(Some(b"dermagraph:nullifier:shared"), &identity_bytes);
        let mut nullifier_seed = [0u8; 32];
        hk.expand(&[], &mut nullifier_seed).expect("valid length");

        Ok(Self {
            ethereum,
            solana,
            nullifier_seed,
        })
    }

    pub fn eth_address(&self) -> String {
        self.ethereum.address_checksum()
    }

    pub fn sol_address(&self) -> String {
        self.solana.address()
    }

    pub fn derive_cross_chain_nullifier(&self, scope: &str) -> [u8; 32] {
        let hk = Hkdf::<Sha256>::new(Some(domains::NULLIFIER), &self.nullifier_seed);
        let mut nullifier = [0u8; 32];
        hk.expand(scope.as_bytes(), &mut nullifier).expect("valid output length");
        nullifier
    }

    pub fn zeroize(&mut self) {
        self.ethereum.zeroize();
        self.solana.zeroize();
        self.nullifier_seed.fill(0);
    }
}

impl Drop for DualChainWallet {
    fn drop(&mut self) {
        self.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eip55_checksum() {
        let addr_bytes: [u8; 20] = hex::decode("5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed")
            .unwrap()
            .try_into()
            .unwrap();
        let checksum = eip55_checksum(&addr_bytes);
        assert!(checksum.contains("5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed"));
    }

    #[test]
    fn test_security_analysis() {
        let analysis = SecurityAnalysis::analyze(
            47.0,
            Some("correcthorsebatterystaple"),
            &HardeningParams::standard(),
        );
        assert!(analysis.total_entropy > 80.0);
        assert!(analysis.attack_cost_usd > 1_000_000.0);
    }

    #[test]
    fn test_passphrase_entropy() {
        assert!(estimate_passphrase_entropy("password") < 40.0);
        assert!(estimate_passphrase_entropy("P@ssw0rd!") > 40.0);
        assert!(estimate_passphrase_entropy("correct horse battery staple") > 80.0);
    }
}
