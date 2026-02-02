
use argon2::{Argon2, Algorithm, Version, Params};
use chacha20poly1305::{
    XChaCha20Poly1305,
    aead::{Aead, KeyInit, OsRng},
    XNonce,
};
use hkdf::Hkdf;
use sha2::Sha256;
use rand::RngCore;
use thiserror::Error;
use zeroize::Zeroize;

#[derive(Debug, Clone)]
pub enum PassphrasePolicy {
    Required(String),

    BiometricOnly,
}

impl PassphrasePolicy {
    fn as_passphrase(&self) -> &str {
        match self {
            PassphrasePolicy::Required(p) => p.as_str(),
            PassphrasePolicy::BiometricOnly => "",
        }
    }

    pub fn is_two_factor(&self) -> bool {
        matches!(self, PassphrasePolicy::Required(_))
    }
}

pub const SALT_SIZE: usize = 32;

pub const NONCE_SIZE: usize = 24;

pub const KEY_SIZE: usize = 32;

const ARGON2_M_COST: u32 = 19 * 1024;
const ARGON2_T_COST: u32 = 2;
const ARGON2_P_COST: u32 = 1;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("Argon2 key derivation failed: {0}")]
    Argon2Error(String),

    #[error("HKDF expansion failed")]
    HkdfError,

    #[error("Encryption failed: {0}")]
    EncryptionError(String),

    #[error("Decryption failed: authentication tag mismatch")]
    DecryptionError,

    #[error("Invalid ciphertext format: {0}")]
    InvalidFormat(String),

    #[error("Missing biometric key")]
    MissingBiometricKey,

    #[error("Missing passphrase")]
    MissingPassphrase,

    #[error("Empty passphrase provided with Required policy - use BiometricOnly if intentional")]
    EmptyPassphraseViolation,
}

#[derive(Clone)]
pub struct EncryptionContext {
    combined_key: [u8; KEY_SIZE],
    salt: [u8; SALT_SIZE],
}

impl Drop for EncryptionContext {
    fn drop(&mut self) {
        self.combined_key.zeroize();
        self.salt.zeroize();
    }
}

impl Zeroize for EncryptionContext {
    fn zeroize(&mut self) {
        self.combined_key.zeroize();
        self.salt.zeroize();
    }
}

impl EncryptionContext {
    pub fn new_for_registration(
        biometric_key: &[u8; KEY_SIZE],
        policy: PassphrasePolicy,
    ) -> Result<Self, CryptoError> {
        Self::validate_policy(&policy)?;

        let mut salt = [0u8; SALT_SIZE];
        OsRng.fill_bytes(&mut salt);

        Self::derive_context(biometric_key, &policy, &salt)
    }

    pub fn restore(
        biometric_key: &[u8; KEY_SIZE],
        policy: PassphrasePolicy,
        salt: &[u8; SALT_SIZE],
    ) -> Result<Self, CryptoError> {
        Self::validate_policy(&policy)?;
        Self::derive_context(biometric_key, &policy, salt)
    }

    fn validate_policy(policy: &PassphrasePolicy) -> Result<(), CryptoError> {
        if let PassphrasePolicy::Required(ref p) = policy {
            if p.is_empty() {
                return Err(CryptoError::EmptyPassphraseViolation);
            }
        }
        Ok(())
    }

    fn derive_context(
        biometric_key: &[u8; KEY_SIZE],
        policy: &PassphrasePolicy,
        salt: &[u8; SALT_SIZE],
    ) -> Result<Self, CryptoError> {
        let mut passphrase_key = derive_passphrase_key(
            policy.as_passphrase(),
            salt,
        )?;

        let result = combine_keys(biometric_key, &passphrase_key);

        passphrase_key.zeroize();

        let combined_key = result?;

        Ok(Self {
            combined_key,
            salt: *salt,
        })
    }

    pub fn salt(&self) -> &[u8; SALT_SIZE] {
        &self.salt
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = XNonce::from_slice(&nonce_bytes);

        let cipher = XChaCha20Poly1305::new_from_slice(&self.combined_key)
            .map_err(|e| CryptoError::EncryptionError(e.to_string()))?;

        let ciphertext = cipher.encrypt(nonce, plaintext)
            .map_err(|e| CryptoError::EncryptionError(e.to_string()))?;

        let mut output = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
        output.extend_from_slice(&nonce_bytes);
        output.extend_from_slice(&ciphertext);

        Ok(output)
    }

    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if ciphertext.len() < NONCE_SIZE + 16 {
            return Err(CryptoError::InvalidFormat(
                format!("Ciphertext too short: {} bytes", ciphertext.len())
            ));
        }

        let nonce = XNonce::from_slice(&ciphertext[..NONCE_SIZE]);
        let encrypted_data = &ciphertext[NONCE_SIZE..];

        let cipher = XChaCha20Poly1305::new_from_slice(&self.combined_key)
            .map_err(|e| CryptoError::EncryptionError(e.to_string()))?;

        cipher.decrypt(nonce, encrypted_data)
            .map_err(|_| CryptoError::DecryptionError)
    }
}

fn derive_passphrase_key(
    passphrase: &str,
    salt: &[u8; SALT_SIZE],
) -> Result<[u8; KEY_SIZE], CryptoError> {
    let params = Params::new(
        ARGON2_M_COST,
        ARGON2_T_COST,
        ARGON2_P_COST,
        Some(KEY_SIZE),
    ).map_err(|e| CryptoError::Argon2Error(e.to_string()))?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut output = [0u8; KEY_SIZE];
    argon2.hash_password_into(passphrase.as_bytes(), salt, &mut output)
        .map_err(|e| CryptoError::Argon2Error(e.to_string()))?;

    Ok(output)
}

fn combine_keys(
    biometric_key: &[u8; KEY_SIZE],
    passphrase_key: &[u8; KEY_SIZE],
) -> Result<[u8; KEY_SIZE], CryptoError> {
    let mut ikm = Vec::with_capacity(KEY_SIZE * 2);
    ikm.extend_from_slice(biometric_key);
    ikm.extend_from_slice(passphrase_key);

    let info = b"dermagraphic-identity-v1-combined-key";

    let hkdf = Hkdf::<Sha256>::new(None, &ikm);

    let mut output = [0u8; KEY_SIZE];
    let result = hkdf.expand(info, &mut output)
        .map_err(|_| CryptoError::HkdfError);

    ikm.zeroize();

    result?;
    Ok(output)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EncryptedFileHeader {
    pub version: u32,
    pub salt: [u8; SALT_SIZE],
    pub algorithm: String,
}

impl EncryptedFileHeader {
    pub fn new(salt: [u8; SALT_SIZE]) -> Self {
        Self {
            version: 1,
            salt,
            algorithm: "argon2id+hkdf+xchacha20poly1305".to_string(),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let json = serde_json::to_vec(self).expect("Header serialization cannot fail");
        let len = (json.len() as u32).to_le_bytes();

        let mut output = Vec::with_capacity(4 + json.len());
        output.extend_from_slice(&len);
        output.extend_from_slice(&json);
        output
    }

    pub fn from_bytes(data: &[u8]) -> Result<(Self, usize), CryptoError> {
        if data.len() < 4 {
            return Err(CryptoError::InvalidFormat("Header too short".into()));
        }

        let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;

        if data.len() < 4 + len {
            return Err(CryptoError::InvalidFormat("Truncated header".into()));
        }

        let header: Self = serde_json::from_slice(&data[4..4 + len])
            .map_err(|e| CryptoError::InvalidFormat(e.to_string()))?;

        Ok((header, 4 + len))
    }
}

pub fn encrypt_with_header(
    ctx: &EncryptionContext,
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let header = EncryptedFileHeader::new(*ctx.salt());
    let header_bytes = header.to_bytes();
    let ciphertext = ctx.encrypt(plaintext)?;

    let mut output = Vec::with_capacity(header_bytes.len() + ciphertext.len());
    output.extend_from_slice(&header_bytes);
    output.extend_from_slice(&ciphertext);

    Ok(output)
}

pub fn decrypt_with_header(
    biometric_key: &[u8; KEY_SIZE],
    policy: PassphrasePolicy,
    data: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let (header, header_len) = EncryptedFileHeader::from_bytes(data)?;

    if header.version != 1 {
        return Err(CryptoError::InvalidFormat(
            format!("Unsupported version: {}", header.version)
        ));
    }

    let ctx = EncryptionContext::restore(biometric_key, policy, &header.salt)?;

    ctx.decrypt(&data[header_len..])
}

pub fn extract_salt(data: &[u8]) -> Result<[u8; SALT_SIZE], CryptoError> {
    let (header, _) = EncryptedFileHeader::from_bytes(data)?;
    Ok(header.salt)
}

pub fn encrypt_with_raw_key(
    key: &[u8; KEY_SIZE],
    plaintext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = XNonce::from_slice(&nonce_bytes);

    let cipher = XChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| CryptoError::EncryptionError(e.to_string()))?;

    let ciphertext = cipher.encrypt(nonce, plaintext)
        .map_err(|e| CryptoError::EncryptionError(e.to_string()))?;

    let mut output = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);

    Ok(output)
}

pub fn decrypt_with_raw_key(
    key: &[u8; KEY_SIZE],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    if ciphertext.len() < NONCE_SIZE + 16 {
        return Err(CryptoError::InvalidFormat(
            format!("Ciphertext too short: {} bytes (need at least {} for nonce + tag)",
                    ciphertext.len(), NONCE_SIZE + 16)
        ));
    }

    let nonce = XNonce::from_slice(&ciphertext[..NONCE_SIZE]);
    let encrypted_data = &ciphertext[NONCE_SIZE..];

    let cipher = XChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| CryptoError::EncryptionError(e.to_string()))?;

    cipher.decrypt(nonce, encrypted_data)
        .map_err(|_| CryptoError::DecryptionError)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let biometric_key = [0x42u8; KEY_SIZE];
        let policy = PassphrasePolicy::Required("test-passphrase-123".to_string());
        let plaintext = b"Hello, biometric encryption!";

        let ctx = EncryptionContext::new_for_registration(&biometric_key, policy.clone())
            .expect("Context creation should succeed");
        let ciphertext = encrypt_with_header(&ctx, plaintext)
            .expect("Encryption should succeed");

        let decrypted = decrypt_with_header(&biometric_key, policy, &ciphertext)
            .expect("Decryption should succeed");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_wrong_passphrase_fails() {
        let biometric_key = [0x42u8; KEY_SIZE];
        let policy = PassphrasePolicy::Required("correct-passphrase".to_string());
        let plaintext = b"Secret data";

        let ctx = EncryptionContext::new_for_registration(&biometric_key, policy)
            .expect("Context creation should succeed");
        let ciphertext = encrypt_with_header(&ctx, plaintext)
            .expect("Encryption should succeed");

        let wrong_policy = PassphrasePolicy::Required("wrong-passphrase".to_string());
        let result = decrypt_with_header(&biometric_key, wrong_policy, &ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_biometric_fails() {
        let biometric_key = [0x42u8; KEY_SIZE];
        let wrong_biometric_key = [0x43u8; KEY_SIZE];
        let policy = PassphrasePolicy::Required("test-passphrase".to_string());
        let plaintext = b"Secret data";

        let ctx = EncryptionContext::new_for_registration(&biometric_key, policy.clone())
            .expect("Context creation should succeed");
        let ciphertext = encrypt_with_header(&ctx, plaintext)
            .expect("Encryption should succeed");

        let result = decrypt_with_header(&wrong_biometric_key, policy, &ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_biometric_only_works_with_explicit_opt_in() {
        let biometric_key = [0x42u8; KEY_SIZE];
        let plaintext = b"Biometric-only encryption";

        let ctx = EncryptionContext::new_for_registration(
            &biometric_key,
            PassphrasePolicy::BiometricOnly,
        ).expect("Context creation should succeed");
        let ciphertext = encrypt_with_header(&ctx, plaintext)
            .expect("Encryption should succeed");

        let decrypted = decrypt_with_header(
            &biometric_key,
            PassphrasePolicy::BiometricOnly,
            &ciphertext,
        ).expect("Decryption should succeed");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_empty_passphrase_in_required_mode_fails() {
        let biometric_key = [0x42u8; KEY_SIZE];

        let result = EncryptionContext::new_for_registration(
            &biometric_key,
            PassphrasePolicy::Required("".to_string()),
        );

        assert!(matches!(result, Err(CryptoError::EmptyPassphraseViolation)));
    }

    #[test]
    fn test_policy_type_mismatch_fails_decryption() {
        let biometric_key = [0x42u8; KEY_SIZE];
        let plaintext = b"Two-factor encrypted data";

        let ctx = EncryptionContext::new_for_registration(
            &biometric_key,
            PassphrasePolicy::Required("secret".to_string()),
        ).expect("Context creation should succeed");
        let ciphertext = encrypt_with_header(&ctx, plaintext)
            .expect("Encryption should succeed");

        let result = decrypt_with_header(
            &biometric_key,
            PassphrasePolicy::BiometricOnly,
            &ciphertext,
        );

        assert!(result.is_err());
    }
}
