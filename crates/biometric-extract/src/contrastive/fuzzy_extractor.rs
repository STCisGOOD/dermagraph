
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use sha2::{Sha256, Sha512, Digest};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct XLockConfig {
    pub feature_bits: usize,

    pub entropy_bits: usize,

    pub lockers_per_bit: usize,

    pub indices_per_locker: usize,

    pub hard_majority_threshold: f64,

    pub domain_separator: String,

    pub use_hard_majority: bool,

    pub min_avg_confidence: f64,
}

impl Default for XLockConfig {
    fn default() -> Self {
        Self {
            feature_bits: 512,
            entropy_bits: 48,
            lockers_per_bit: 15,
            indices_per_locker: 5,
            hard_majority_threshold: 0.1,
            domain_separator: "dermagraph-xlock-v1".to_string(),
            use_hard_majority: true,
            min_avg_confidence: 0.3,
        }
    }
}

impl XLockConfig {
    pub fn high_security() -> Self {
        Self {
            entropy_bits: 64,
            lockers_per_bit: 21,
            indices_per_locker: 4,
            hard_majority_threshold: 0.15,
            min_avg_confidence: 0.35,
            ..Default::default()
        }
    }

    pub fn high_usability() -> Self {
        Self {
            entropy_bits: 32,
            lockers_per_bit: 25,
            indices_per_locker: 3,
            hard_majority_threshold: 0.05,
            min_avg_confidence: 0.15,
            ..Default::default()
        }
    }

    pub fn validate(&self) -> Result<(), XLockError> {
        if self.indices_per_locker * self.lockers_per_bit >= self.feature_bits {
            return Err(XLockError::InvalidConfig(
                "indices_per_locker * lockers_per_bit must be < feature_bits".into()
            ));
        }

        if self.entropy_bits > 128 {
            return Err(XLockError::InvalidConfig(
                "entropy_bits should not exceed 128 for practical security".into()
            ));
        }

        if self.lockers_per_bit < 3 {
            return Err(XLockError::InvalidConfig(
                "lockers_per_bit must be at least 3 for majority voting".into()
            ));
        }

        if self.lockers_per_bit % 2 == 0 {
            return Err(XLockError::InvalidConfig(
                "lockers_per_bit should be odd for unambiguous majority".into()
            ));
        }

        Ok(())
    }

    pub fn helper_data_size(&self) -> usize {
        let index_bits = (self.feature_bits as f64).log2().ceil() as usize;
        let total_index_bits = self.entropy_bits * self.lockers_per_bit * self.indices_per_locker * index_bits;

        let vault_bits = self.entropy_bits * self.lockers_per_bit;

        (total_index_bits + vault_bits + 7) / 8
    }
}

#[derive(Debug, Clone)]
pub enum XLockError {
    InvalidConfig(String),
    InvalidInputSize { expected: usize, got: usize },
    InvalidHelperData(String),
    ReproductionFailed { confidence: f64 },
    UncertainVote { bit_index: usize, margin: f64 },
    NonEnrollableFinger { finger: String },
}

impl std::fmt::Display for XLockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            XLockError::InvalidConfig(msg) => write!(f, "Invalid config: {}", msg),
            XLockError::InvalidInputSize { expected, got } => {
                write!(f, "Invalid input size: expected {} bits, got {}", expected, got)
            }
            XLockError::InvalidHelperData(msg) => write!(f, "Invalid helper data: {}", msg),
            XLockError::ReproductionFailed { confidence } => {
                write!(f, "Reproduction failed (confidence: {:.2}%)", confidence * 100.0)
            }
            XLockError::UncertainVote { bit_index, margin } => {
                write!(f, "Uncertain vote at bit {}: margin {:.2}%", bit_index, margin * 100.0)
            }
            XLockError::NonEnrollableFinger { finger } => {
                write!(f, "Cannot enroll with {} finger - only thumb, index, and middle allowed for enrollment", finger)
            }
        }
    }
}

impl std::error::Error for XLockError {}

#[derive(Debug, Clone)]
pub struct HelperData {
    pub indices: Vec<Vec<Vec<u16>>>,

    pub vault: Vec<Vec<bool>>,

    pub config: XLockConfig,

    pub version: u8,
}

impl HelperData {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.config.helper_data_size() + 64);

        bytes.push(self.version);

        bytes.extend(&(self.config.feature_bits as u16).to_le_bytes());
        bytes.extend(&(self.config.entropy_bits as u16).to_le_bytes());
        bytes.extend(&(self.config.lockers_per_bit as u16).to_le_bytes());
        bytes.extend(&(self.config.indices_per_locker as u16).to_le_bytes());

        for i in 0..self.config.entropy_bits {
            for j in 0..self.config.lockers_per_bit {
                for idx in &self.indices[i][j] {
                    bytes.extend(&idx.to_le_bytes());
                }
            }
        }

        let mut vault_bits = Vec::new();
        for i in 0..self.config.entropy_bits {
            for j in 0..self.config.lockers_per_bit {
                vault_bits.push(self.vault[i][j]);
            }
        }

        for chunk in vault_bits.chunks(8) {
            let mut byte = 0u8;
            for (bit_idx, &bit) in chunk.iter().enumerate() {
                if bit {
                    byte |= 1 << bit_idx;
                }
            }
            bytes.push(byte);
        }

        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, XLockError> {
        if bytes.len() < 9 {
            return Err(XLockError::InvalidHelperData("Too short".into()));
        }

        let version = bytes[0];
        if version != 1 {
            return Err(XLockError::InvalidHelperData(
                format!("Unsupported version: {}", version)
            ));
        }

        let feature_bits = u16::from_le_bytes([bytes[1], bytes[2]]) as usize;
        let entropy_bits = u16::from_le_bytes([bytes[3], bytes[4]]) as usize;
        let lockers_per_bit = u16::from_le_bytes([bytes[5], bytes[6]]) as usize;
        let indices_per_locker = u16::from_le_bytes([bytes[7], bytes[8]]) as usize;

        let config = XLockConfig {
            feature_bits,
            entropy_bits,
            lockers_per_bit,
            indices_per_locker,
            ..Default::default()
        };

        let mut offset = 9;

        let mut indices = vec![vec![vec![0u16; indices_per_locker]; lockers_per_bit]; entropy_bits];
        for i in 0..entropy_bits {
            for j in 0..lockers_per_bit {
                for k in 0..indices_per_locker {
                    if offset + 2 > bytes.len() {
                        return Err(XLockError::InvalidHelperData("Truncated indices".into()));
                    }
                    indices[i][j][k] = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
                    offset += 2;
                }
            }
        }

        let total_vault_bits = entropy_bits * lockers_per_bit;
        let vault_bytes_needed = (total_vault_bits + 7) / 8;

        if offset + vault_bytes_needed > bytes.len() {
            return Err(XLockError::InvalidHelperData("Truncated vault".into()));
        }

        let mut vault = vec![vec![false; lockers_per_bit]; entropy_bits];
        let mut bit_idx = 0;
        for i in 0..entropy_bits {
            for j in 0..lockers_per_bit {
                let byte_idx = offset + bit_idx / 8;
                let bit_pos = bit_idx % 8;
                vault[i][j] = (bytes[byte_idx] >> bit_pos) & 1 == 1;
                bit_idx += 1;
            }
        }

        Ok(Self {
            indices,
            vault,
            config,
            version,
        })
    }
}

pub struct XLockExtractor {
    config: XLockConfig,
}

impl XLockExtractor {
    pub fn new(config: XLockConfig) -> Result<Self, XLockError> {
        config.validate()?;
        Ok(Self { config })
    }

    pub fn default_config() -> Result<Self, XLockError> {
        Self::new(XLockConfig::default())
    }

    pub fn gen(
        &self,
        biometric: &[bool],
        additional_entropy: Option<&[u8]>,
    ) -> Result<(HelperData, [u8; 32]), XLockError> {
        self.gen_internal(biometric, additional_entropy, None)
    }

    pub fn gen_with_entropy(
        &self,
        biometric: &[bool],
        additional_entropy: Option<&[u8]>,
        existing_beta: &[bool],
    ) -> Result<(HelperData, [u8; 32]), XLockError> {
        if existing_beta.len() != self.config.entropy_bits {
            return Err(XLockError::InvalidConfig(format!(
                "existing_beta length {} != entropy_bits {}",
                existing_beta.len(),
                self.config.entropy_bits
            )));
        }
        self.gen_internal(biometric, additional_entropy, Some(existing_beta))
    }

    fn gen_internal(
        &self,
        biometric: &[bool],
        additional_entropy: Option<&[u8]>,
        existing_beta: Option<&[bool]>,
    ) -> Result<(HelperData, [u8; 32]), XLockError> {
        if biometric.len() != self.config.feature_bits {
            return Err(XLockError::InvalidInputSize {
                expected: self.config.feature_bits,
                got: biometric.len(),
            });
        }

        let mut rng = ChaCha20Rng::from_entropy();
        let beta: Vec<bool> = match existing_beta {
            Some(b) => b.to_vec(),
            None => {
                let mut fresh_beta = vec![false; self.config.entropy_bits];
                for bit in &mut fresh_beta {
                    *bit = rng.gen();
                }
                fresh_beta
            }
        };

        let mut indices = vec![
            vec![
                vec![0u16; self.config.indices_per_locker];
                self.config.lockers_per_bit
            ];
            self.config.entropy_bits
        ];

        for i in 0..self.config.entropy_bits {
            for j in 0..self.config.lockers_per_bit {
                let mut used = HashSet::new();
                for k in 0..self.config.indices_per_locker {
                    loop {
                        let idx = rng.gen_range(0..self.config.feature_bits) as u16;
                        if used.insert(idx) {
                            indices[i][j][k] = idx;
                            break;
                        }
                    }
                }
            }
        }

        let mut vault = vec![vec![false; self.config.lockers_per_bit]; self.config.entropy_bits];

        for i in 0..self.config.entropy_bits {
            for j in 0..self.config.lockers_per_bit {
                let mut locker = false;
                for &idx in &indices[i][j] {
                    locker ^= biometric[idx as usize];
                }

                vault[i][j] = locker ^ beta[i];
            }
        }

        let secret_key = self.derive_key(&beta, additional_entropy);

        let helper_data = HelperData {
            indices,
            vault,
            config: self.config.clone(),
            version: 1,
        };

        Ok((helper_data, secret_key))
    }

    pub fn extract_beta_from_rep(
        &self,
        biometric: &[bool],
        helper_data: &HelperData,
    ) -> Result<Vec<bool>, XLockError> {
        if biometric.len() != helper_data.config.feature_bits {
            return Err(XLockError::InvalidInputSize {
                expected: helper_data.config.feature_bits,
                got: biometric.len(),
            });
        }

        let mut beta_hat = vec![false; helper_data.config.entropy_bits];

        for i in 0..helper_data.config.entropy_bits {
            let mut votes_for_one = 0;
            let mut votes_for_zero = 0;

            for j in 0..helper_data.config.lockers_per_bit {
                let mut locker_hat = false;
                for &idx in &helper_data.indices[i][j] {
                    locker_hat ^= biometric[idx as usize];
                }
                let beta_candidate = locker_hat ^ helper_data.vault[i][j];
                if beta_candidate {
                    votes_for_one += 1;
                } else {
                    votes_for_zero += 1;
                }
            }

            let total_votes = votes_for_one + votes_for_zero;
            let margin = (votes_for_one as f64 - votes_for_zero as f64).abs() / total_votes as f64;

            if self.config.use_hard_majority && margin < self.config.hard_majority_threshold * 2.0 {
                return Err(XLockError::UncertainVote {
                    bit_index: i,
                    margin,
                });
            }

            beta_hat[i] = votes_for_one > votes_for_zero;
        }

        Ok(beta_hat)
    }

    pub fn rep(
        &self,
        biometric: &[bool],
        helper_data: &HelperData,
        additional_entropy: Option<&[u8]>,
    ) -> Result<[u8; 32], XLockError> {
        if biometric.len() != helper_data.config.feature_bits {
            return Err(XLockError::InvalidInputSize {
                expected: helper_data.config.feature_bits,
                got: biometric.len(),
            });
        }

        let mut beta_hat = vec![false; helper_data.config.entropy_bits];
        let mut total_confidence = 0.0;
        let mut low_margin_bits = 0;
        let mut min_margin: f64 = 1.0;

        for i in 0..helper_data.config.entropy_bits {
            let mut votes_for_one = 0;
            let mut votes_for_zero = 0;

            for j in 0..helper_data.config.lockers_per_bit {
                let mut locker_hat = false;
                for &idx in &helper_data.indices[i][j] {
                    locker_hat ^= biometric[idx as usize];
                }

                let beta_candidate = locker_hat ^ helper_data.vault[i][j];

                if beta_candidate {
                    votes_for_one += 1;
                } else {
                    votes_for_zero += 1;
                }
            }

            let total_votes = votes_for_one + votes_for_zero;
            let margin = (votes_for_one as f64 - votes_for_zero as f64).abs() / total_votes as f64;

            if margin < min_margin {
                min_margin = margin;
            }
            if margin < 0.2 {
                low_margin_bits += 1;
            }

            if self.config.use_hard_majority && margin < self.config.hard_majority_threshold * 2.0 {
                return Err(XLockError::UncertainVote {
                    bit_index: i,
                    margin,
                });
            }

            beta_hat[i] = votes_for_one > votes_for_zero;
            total_confidence += margin;
        }

        let avg_confidence = total_confidence / helper_data.config.entropy_bits as f64;

        println!("[X-Lock] Voting stats: avg={:.1}% min={:.1}% low_margin={}/{} entropy_bits={}",
            avg_confidence * 100.0, min_margin * 100.0, low_margin_bits, helper_data.config.entropy_bits, helper_data.config.entropy_bits);

        if avg_confidence < self.config.min_avg_confidence {
            return Err(XLockError::ReproductionFailed {
                confidence: avg_confidence,
            });
        }

        let secret_key = self.derive_key(&beta_hat, additional_entropy);

        Ok(secret_key)
    }

    fn derive_key(&self, beta: &[bool], additional_entropy: Option<&[u8]>) -> [u8; 32] {
        self.derive_key_internal(beta, additional_entropy, b"nullifier")
    }

    fn derive_embedding_key(&self, beta: &[bool], password: Option<&[u8]>) -> [u8; 32] {
        self.derive_key_internal(beta, password, b"embedding-encryption-v1")
    }

    fn derive_key_internal(&self, beta: &[bool], additional_entropy: Option<&[u8]>, context: &[u8]) -> [u8; 32] {
        let mut hasher = Sha512::new();

        hasher.update(self.config.domain_separator.as_bytes());
        hasher.update(&[0x00]);

        hasher.update(context);
        hasher.update(&[0x00]);

        let mut beta_bytes = vec![0u8; (beta.len() + 7) / 8];
        for (i, &bit) in beta.iter().enumerate() {
            if bit {
                beta_bytes[i / 8] |= 1 << (i % 8);
            }
        }
        hasher.update(&beta_bytes);

        if let Some(extra) = additional_entropy {
            hasher.update(&[0x01]);
            hasher.update(extra);
        }

        let hash = hasher.finalize();
        let mut key = [0u8; 32];
        key.copy_from_slice(&hash[..32]);
        key
    }

    pub fn config(&self) -> &XLockConfig {
        &self.config
    }
}

pub fn quantize_embedding_binary(embedding: &[f32], threshold: f32) -> Vec<bool> {
    embedding.iter().map(|&v| v >= threshold).collect()
}

pub fn quantize_embedding_random_projection(
    embedding: &[f32],
    target_bits: usize,
    seed: u64,
) -> Vec<bool> {
    let mut rng = ChaCha20Rng::seed_from_u64(seed);
    let d = embedding.len();

    let mut result = Vec::with_capacity(target_bits);

    for _ in 0..target_bits {
        let mut projection = 0.0f64;
        for &v in embedding {
            let u1: f64 = rng.gen();
            let u2: f64 = rng.gen();
            let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
            projection += (v as f64) * z;
        }

        result.push(projection >= 0.0);
    }

    result
}

pub fn expand_embedding_to_bits(embedding: &[f32]) -> Vec<bool> {
    let mut bits = Vec::with_capacity(512);

    for &v in embedding {
        let normalized = ((v + 1.0) / 2.0).clamp(0.0, 0.9999);
        let quantile = (normalized * 16.0) as u8;

        bits.push(quantile & 0b0001 != 0);
        bits.push(quantile & 0b0010 != 0);
        bits.push(quantile & 0b0100 != 0);
        bits.push(quantile & 0b1000 != 0);
    }

    bits
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FingerType {
    Thumb,
    Index,
    Middle,
    Ring,
    Pinky,
}

impl FingerType {
    pub fn can_enroll(&self) -> bool {
        matches!(self, FingerType::Thumb | FingerType::Index | FingerType::Middle)
    }

    pub fn enrollable_fingers() -> &'static [FingerType] {
        &[FingerType::Thumb, FingerType::Index, FingerType::Middle]
    }
}

impl std::fmt::Display for FingerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FingerType::Thumb => write!(f, "thumb"),
            FingerType::Index => write!(f, "index"),
            FingerType::Middle => write!(f, "middle"),
            FingerType::Ring => write!(f, "ring"),
            FingerType::Pinky => write!(f, "pinky"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MultiFingerHelperData {
    pub finger_helpers: Vec<(FingerType, HelperData)>,

    pub nullifier: [u8; 32],

    pub version: u8,
}

impl MultiFingerHelperData {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        bytes.push(3u8);

        bytes.push(self.finger_helpers.len() as u8);

        bytes.extend_from_slice(&self.nullifier);

        for (finger_type, helper) in &self.finger_helpers {
            bytes.push(*finger_type as u8);

            let helper_bytes = helper.to_bytes();
            bytes.extend(&(helper_bytes.len() as u32).to_le_bytes());
            bytes.extend(helper_bytes);
        }

        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, XLockError> {
        if bytes.len() < 34 {
            return Err(XLockError::InvalidHelperData("Too short for MultiFingerHelperData".into()));
        }

        let version = bytes[0];
        if version != 3 {
            return Err(XLockError::InvalidHelperData(format!(
                "Unsupported MultiFingerHelperData version: {} (expected 3, security update)",
                version
            )));
        }

        let num_fingers = bytes[1] as usize;
        if num_fingers == 0 || num_fingers > 10 {
            return Err(XLockError::InvalidHelperData(format!(
                "Invalid number of fingers: {}",
                num_fingers
            )));
        }

        let mut nullifier = [0u8; 32];
        nullifier.copy_from_slice(&bytes[2..34]);

        let mut offset = 34;
        let mut finger_helpers = Vec::with_capacity(num_fingers);

        for _ in 0..num_fingers {
            if offset + 5 > bytes.len() {
                return Err(XLockError::InvalidHelperData("Truncated finger data".into()));
            }

            let finger_type = match bytes[offset] {
                0 => FingerType::Thumb,
                1 => FingerType::Index,
                2 => FingerType::Middle,
                3 => FingerType::Ring,
                4 => FingerType::Pinky,
                _ => return Err(XLockError::InvalidHelperData("Invalid finger type".into())),
            };
            offset += 1;

            let helper_len = u32::from_le_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]) as usize;
            offset += 4;

            if offset + helper_len > bytes.len() {
                return Err(XLockError::InvalidHelperData("Truncated helper data".into()));
            }

            let helper = HelperData::from_bytes(&bytes[offset..offset + helper_len])?;
            offset += helper_len;

            finger_helpers.push((finger_type, helper));
        }

        Ok(Self {
            finger_helpers,
            nullifier,
            version,
        })
    }

    pub fn num_fingers(&self) -> usize {
        self.finger_helpers.len()
    }
}

pub struct FuzzyNullifier {
    extractor: XLockExtractor,
    projection_seed: u64,
}

impl FuzzyNullifier {
    pub fn new(config: XLockConfig) -> Result<Self, XLockError> {
        Ok(Self {
            extractor: XLockExtractor::new(config)?,
            projection_seed: 0xDE12A_612AF_2025,
        })
    }

    pub fn default_settings() -> Result<Self, XLockError> {
        Self::new(XLockConfig::default())
    }

    pub fn enroll(
        &self,
        embedding: &[f32],
        scope: &str,
        password: Option<&str>,
    ) -> Result<(HelperData, [u8; 32]), XLockError> {
        let bits = expand_embedding_to_bits(embedding);

        let additional = password.map(|p| {
            let mut hasher = Sha256::new();
            hasher.update(scope.as_bytes());
            hasher.update(&[0x00]);
            hasher.update(p.as_bytes());
            hasher.finalize().to_vec()
        });

        self.extractor.gen(&bits, additional.as_deref())
    }

    pub fn verify(
        &self,
        embedding: &[f32],
        helper_data: &HelperData,
        scope: &str,
        password: Option<&str>,
    ) -> Result<[u8; 32], XLockError> {
        let bits = expand_embedding_to_bits(embedding);

        let additional = password.map(|p| {
            let mut hasher = Sha256::new();
            hasher.update(scope.as_bytes());
            hasher.update(&[0x00]);
            hasher.update(p.as_bytes());
            hasher.finalize().to_vec()
        });

        self.extractor.rep(&bits, helper_data, additional.as_deref())
    }

    pub fn derive_scoped_nullifier(
        key: &[u8; 32],
        scope: &str,
    ) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"dermagraph-scoped-nullifier-v1");
        hasher.update(key);
        hasher.update(scope.as_bytes());

        let hash = hasher.finalize();
        let mut nullifier = [0u8; 32];
        nullifier.copy_from_slice(&hash);
        nullifier
    }

    pub fn extractor(&self) -> &XLockExtractor {
        &self.extractor
    }

    pub fn config(&self) -> &XLockConfig {
        self.extractor.config()
    }

    pub fn enroll_multiple(
        &self,
        embeddings: &[(FingerType, &[f32])],
        scope: &str,
        password: Option<&str>,
    ) -> Result<(MultiFingerHelperData, [u8; 32]), XLockError> {
        if embeddings.is_empty() {
            return Err(XLockError::InvalidConfig("No embeddings provided".into()));
        }

        for (finger_type, _) in embeddings {
            if !finger_type.can_enroll() {
                return Err(XLockError::NonEnrollableFinger {
                    finger: finger_type.to_string(),
                });
            }
        }

        let additional = password.map(|p| {
            let mut hasher = Sha256::new();
            hasher.update(scope.as_bytes());
            hasher.update(&[0x00]);
            hasher.update(p.as_bytes());
            hasher.finalize().to_vec()
        });

        let mut finger_helpers = Vec::with_capacity(embeddings.len());
        let mut shared_beta: Option<Vec<bool>> = None;
        let mut nullifier = [0u8; 32];

        for (finger_type, embedding) in embeddings {
            let bits = expand_embedding_to_bits(embedding);

            match &shared_beta {
                None => {
                    let (helper, key) = self.extractor.gen(&bits, additional.as_deref())?;

                    shared_beta = Some(self.extractor.extract_beta_from_rep(&bits, &helper)?);
                    nullifier = key;

                    finger_helpers.push((*finger_type, helper));
                }
                Some(beta) => {
                    let (helper, key) = self.extractor.gen_with_entropy(
                        &bits,
                        additional.as_deref(),
                        beta,
                    )?;

                    if key != nullifier {
                        return Err(XLockError::InvalidConfig(
                            "Beta reuse produced different key - this should not happen".into()
                        ));
                    }

                    finger_helpers.push((*finger_type, helper));
                }
            }
        }

        let embedding_key = match &shared_beta {
            Some(beta) => {
                let password_bytes = password.map(|p| p.as_bytes().to_vec());
                self.extractor.derive_embedding_key(beta, password_bytes.as_deref())
            }
            None => {
                return Err(XLockError::InvalidConfig("No beta generated".into()));
            }
        };

        let helper_data = MultiFingerHelperData {
            finger_helpers,
            nullifier,
            version: 3,
        };

        Ok((helper_data, embedding_key))
    }

    pub fn verify_against_multiple(
        &self,
        embedding: &[f32],
        multi_helper: &MultiFingerHelperData,
        scope: &str,
        password: Option<&str>,
    ) -> Result<([u8; 32], [u8; 32], FingerType), XLockError> {
        let bits = expand_embedding_to_bits(embedding);

        let additional = password.map(|p| {
            let mut hasher = Sha256::new();
            hasher.update(scope.as_bytes());
            hasher.update(&[0x00]);
            hasher.update(p.as_bytes());
            hasher.finalize().to_vec()
        });

        let mut last_error = None;
        let mut best_confidence: f64 = 0.0;

        for (finger_type, helper) in &multi_helper.finger_helpers {
            #[cfg(feature = "tracing")]
            tracing::debug!("Trying {} finger helper...", finger_type);

            match self.extractor.rep(&bits, helper, additional.as_deref()) {
                Ok(recovered_nullifier) => {
                    println!("[X-Lock] {} finger: recovered=0x{}... expected=0x{}... match={}",
                        finger_type,
                        recovered_nullifier.iter().take(4).map(|b| format!("{:02x}", b)).collect::<String>(),
                        multi_helper.nullifier.iter().take(4).map(|b| format!("{:02x}", b)).collect::<String>(),
                        recovered_nullifier == multi_helper.nullifier);

                    if recovered_nullifier == multi_helper.nullifier {
                        let recovered_beta = self.extractor.extract_beta_from_rep(&bits, helper)?;
                        let password_bytes = password.map(|p| p.as_bytes().to_vec());
                        let embedding_key = self.extractor.derive_embedding_key(
                            &recovered_beta,
                            password_bytes.as_deref()
                        );

                        return Ok((recovered_nullifier, embedding_key, *finger_type));
                    }
                    last_error = Some(XLockError::ReproductionFailed {
                        confidence: 0.99,
                    });
                }
                Err(e) => {
                    if let XLockError::ReproductionFailed { confidence } = &e {
                        if *confidence > best_confidence {
                            best_confidence = *confidence;
                        }
                    }
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or(XLockError::ReproductionFailed { confidence: 0.0 }))
    }

    pub fn enroll_three_fingers(
        &self,
        thumb: &[f32],
        index: &[f32],
        middle: &[f32],
        scope: &str,
        password: Option<&str>,
    ) -> Result<(MultiFingerHelperData, [u8; 32]), XLockError> {
        self.enroll_multiple(
            &[
                (FingerType::Thumb, thumb),
                (FingerType::Index, index),
                (FingerType::Middle, middle),
            ],
            scope,
            password,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_validation() {
        let config = XLockConfig::default();
        assert!(config.validate().is_ok());

        let bad_config = XLockConfig {
            lockers_per_bit: 2,
            ..Default::default()
        };
        assert!(bad_config.validate().is_err());
    }

    #[test]
    fn test_gen_rep_identical() {
        let extractor = XLockExtractor::default_config().unwrap();

        let mut rng = ChaCha20Rng::seed_from_u64(42);
        let biometric: Vec<bool> = (0..512).map(|_| rng.gen()).collect();

        let (helper_data, key1) = extractor.gen(&biometric, None).unwrap();
        let key2 = extractor.rep(&biometric, &helper_data, None).unwrap();

        assert_eq!(key1, key2, "Identical biometric should produce identical key");
    }

    #[test]
    fn test_gen_rep_with_noise() {
        let config = XLockConfig {
            use_hard_majority: false,
            ..Default::default()
        };
        let extractor = XLockExtractor::new(config).unwrap();

        let mut rng = ChaCha20Rng::seed_from_u64(42);
        let biometric: Vec<bool> = (0..512).map(|_| rng.gen()).collect();

        let (helper_data, key1) = extractor.gen(&biometric, None).unwrap();

        let mut noisy_biometric = biometric.clone();
        for i in 0..10 {
            noisy_biometric[i * 50] = !noisy_biometric[i * 50];
        }

        let key2 = extractor.rep(&noisy_biometric, &helper_data, None).unwrap();

        assert_eq!(key1, key2, "Small noise should still produce same key");
    }

    #[test]
    fn test_different_biometric_different_key() {
        let extractor = XLockExtractor::default_config().unwrap();

        let mut rng = ChaCha20Rng::seed_from_u64(42);
        let biometric1: Vec<bool> = (0..512).map(|_| rng.gen()).collect();

        let mut rng2 = ChaCha20Rng::seed_from_u64(999);
        let biometric2: Vec<bool> = (0..512).map(|_| rng2.gen()).collect();

        let (helper_data, _key1) = extractor.gen(&biometric1, None).unwrap();

        let result = extractor.rep(&biometric2, &helper_data, None);
        assert!(result.is_err() || result.unwrap() != _key1);
    }

    #[test]
    fn test_helper_data_serialization() {
        let extractor = XLockExtractor::default_config().unwrap();

        let mut rng = ChaCha20Rng::seed_from_u64(42);
        let biometric: Vec<bool> = (0..512).map(|_| rng.gen()).collect();

        let (helper_data, key1) = extractor.gen(&biometric, None).unwrap();

        let bytes = helper_data.to_bytes();
        let restored = HelperData::from_bytes(&bytes).unwrap();

        let key2 = extractor.rep(&biometric, &restored, None).unwrap();
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_embedding_expansion() {
        let embedding = vec![0.0f32; 128];
        let bits = expand_embedding_to_bits(&embedding);
        assert_eq!(bits.len(), 512);

        assert!(!bits[0]);
        assert!(!bits[1]);
        assert!(!bits[2]);
        assert!(bits[3]);
    }

    #[test]
    fn test_fuzzy_nullifier_flow() {
        let nullifier_gen = FuzzyNullifier::default_settings().unwrap();

        let embedding: Vec<f32> = (0..128).map(|i| ((i as f32) / 128.0) * 2.0 - 1.0).collect();

        let (helper_data, nullifier1) = nullifier_gen
            .enroll(&embedding, "test-scope", Some("password123"))
            .unwrap();

        let nullifier2 = nullifier_gen
            .verify(&embedding, &helper_data, "test-scope", Some("password123"))
            .unwrap();

        assert_eq!(nullifier1, nullifier2);
    }

    #[test]
    fn test_scoped_nullifier() {
        let key = [0u8; 32];
        let null1 = FuzzyNullifier::derive_scoped_nullifier(&key, "scope-a");
        let null2 = FuzzyNullifier::derive_scoped_nullifier(&key, "scope-b");

        assert_ne!(null1, null2, "Different scopes should produce different nullifiers");
    }

    #[test]
    fn test_gen_with_entropy_produces_same_key() {
        let extractor = XLockExtractor::default_config().unwrap();

        let mut rng = ChaCha20Rng::seed_from_u64(42);
        let biometric1: Vec<bool> = (0..512).map(|_| rng.gen()).collect();
        let biometric2: Vec<bool> = (0..512).map(|_| rng.gen()).collect();

        let (helper1, key1) = extractor.gen(&biometric1, None).unwrap();

        let beta = extractor.extract_beta_from_rep(&biometric1, &helper1).unwrap();

        let (helper2, key2) = extractor.gen_with_entropy(&biometric2, None, &beta).unwrap();

        assert_eq!(key1, key2, "Same β should produce same key");

        assert_ne!(helper1.indices, helper2.indices, "Different biometrics should have different indices");
    }

    #[test]
    fn test_multi_finger_enrollment() {
        let fuzzy = FuzzyNullifier::default_settings().unwrap();

        let thumb: Vec<f32> = (0..128).map(|i| ((i as f32) / 128.0) * 2.0 - 1.0).collect();
        let index: Vec<f32> = (0..128).map(|i| ((i as f32 + 10.0) / 128.0) * 2.0 - 1.0).collect();
        let middle: Vec<f32> = (0..128).map(|i| ((i as f32 + 20.0) / 128.0) * 2.0 - 1.0).collect();

        let (multi_helper, embedding_key) = fuzzy
            .enroll_three_fingers(&thumb, &index, &middle, "test-scope", Some("password"))
            .unwrap();

        assert_eq!(multi_helper.num_fingers(), 3);
        assert_eq!(multi_helper.finger_helpers[0].0, FingerType::Thumb);
        assert_eq!(multi_helper.finger_helpers[1].0, FingerType::Index);
        assert_eq!(multi_helper.finger_helpers[2].0, FingerType::Middle);

        assert_ne!(embedding_key, [0u8; 32], "embedding_key should be derived from β");

        println!("Multi-finger enrollment nullifier: {:02x?}", &multi_helper.nullifier[..8]);
    }

    #[test]
    fn test_multi_finger_verification_with_enrolled_finger() {
        let config = XLockConfig {
            use_hard_majority: false,
            ..Default::default()
        };
        let fuzzy = FuzzyNullifier::new(config).unwrap();

        let thumb: Vec<f32> = (0..128).map(|i| ((i as f32) / 128.0) * 2.0 - 1.0).collect();
        let index: Vec<f32> = (0..128).map(|i| (((i + 40) as f32) / 128.0) * 2.0 - 1.0).collect();
        let middle: Vec<f32> = (0..128).map(|i| (((i + 80) as f32) / 128.0) * 2.0 - 1.0).collect();

        let (multi_helper, enrollment_key) = fuzzy
            .enroll_three_fingers(&thumb, &index, &middle, "test-scope", Some("pwd"))
            .unwrap();

        let (nullifier, verify_key, matched) = fuzzy
            .verify_against_multiple(&thumb, &multi_helper, "test-scope", Some("pwd"))
            .unwrap();

        assert_eq!(nullifier, multi_helper.nullifier, "Nullifier should match");
        assert_eq!(matched, FingerType::Thumb, "Should match thumb HelperData");
        assert_eq!(verify_key, enrollment_key, "Re-derived embedding_key should match enrollment key");

        let (nullifier, verify_key, matched) = fuzzy
            .verify_against_multiple(&index, &multi_helper, "test-scope", Some("pwd"))
            .unwrap();

        assert_eq!(nullifier, multi_helper.nullifier, "Nullifier should match");
        assert_eq!(matched, FingerType::Index, "Should match index HelperData");
        assert_eq!(verify_key, enrollment_key, "Re-derived embedding_key should match enrollment key");

        let (nullifier, verify_key, matched) = fuzzy
            .verify_against_multiple(&middle, &multi_helper, "test-scope", Some("pwd"))
            .unwrap();

        assert_eq!(nullifier, multi_helper.nullifier, "Nullifier should match");
        assert_eq!(matched, FingerType::Middle, "Should match middle HelperData");
        assert_eq!(verify_key, enrollment_key, "Re-derived embedding_key should match enrollment key");
    }

    #[test]
    fn test_multi_finger_verification_with_noisy_input() {
        let config = XLockConfig {
            use_hard_majority: false,
            ..Default::default()
        };
        let fuzzy = FuzzyNullifier::new(config).unwrap();

        let base: Vec<f32> = (0..128).map(|i| ((i as f32) / 128.0) * 2.0 - 1.0).collect();

        let thumb: Vec<f32> = base.iter().map(|&x| x).collect();
        let index: Vec<f32> = base.iter().map(|&x| x + 0.02).collect();
        let middle: Vec<f32> = base.iter().map(|&x| x + 0.04).collect();

        let (multi_helper, _embedding_key) = fuzzy
            .enroll_three_fingers(&thumb, &index, &middle, "scope", None)
            .unwrap();

        let noisy_thumb: Vec<f32> = thumb.iter().map(|&x| x + 0.001).collect();
        let result = fuzzy.verify_against_multiple(&noisy_thumb, &multi_helper, "scope", None);

        assert!(result.is_ok(), "Noisy thumb should match enrolled thumb");
    }

    #[test]
    fn test_multi_finger_helper_data_serialization() {
        let fuzzy = FuzzyNullifier::default_settings().unwrap();

        let thumb: Vec<f32> = (0..128).map(|i| ((i as f32) / 128.0) * 2.0 - 1.0).collect();
        let index: Vec<f32> = (0..128).map(|i| ((i as f32 + 5.0) / 128.0) * 2.0 - 1.0).collect();
        let middle: Vec<f32> = (0..128).map(|i| ((i as f32 + 10.0) / 128.0) * 2.0 - 1.0).collect();

        let (multi_helper, _embedding_key) = fuzzy
            .enroll_three_fingers(&thumb, &index, &middle, "scope", Some("pwd"))
            .unwrap();

        let bytes = multi_helper.to_bytes();
        println!("MultiFingerHelperData size: {} bytes (embedding_key NOT included)", bytes.len());

        let restored = MultiFingerHelperData::from_bytes(&bytes).unwrap();

        assert_eq!(restored.num_fingers(), 3);
        assert_eq!(restored.nullifier, multi_helper.nullifier);
        assert_eq!(restored.finger_helpers[0].0, FingerType::Thumb);
        assert_eq!(restored.finger_helpers[1].0, FingerType::Index);
        assert_eq!(restored.finger_helpers[2].0, FingerType::Middle);
    }

    #[test]
    fn test_multi_finger_different_person_fails() {
        let config = XLockConfig {
            use_hard_majority: false,
            ..Default::default()
        };
        let fuzzy = FuzzyNullifier::new(config).unwrap();

        let thumb_a: Vec<f32> = (0..128).map(|i| ((i as f32) / 128.0) * 2.0 - 1.0).collect();
        let index_a: Vec<f32> = (0..128).map(|i| ((i as f32 + 1.0) / 128.0) * 2.0 - 1.0).collect();
        let middle_a: Vec<f32> = (0..128).map(|i| ((i as f32 + 2.0) / 128.0) * 2.0 - 1.0).collect();

        let finger_b: Vec<f32> = (0..128).map(|i| (-(i as f32) / 128.0) * 2.0 + 1.0).collect();

        let (multi_helper, _embedding_key) = fuzzy
            .enroll_three_fingers(&thumb_a, &index_a, &middle_a, "scope", None)
            .unwrap();

        let result = fuzzy.verify_against_multiple(&finger_b, &multi_helper, "scope", None);

        match result {
            Err(_) => {
                println!("✓ Different person correctly rejected");
            }
            Ok((nullifier, _embedding_key, _finger)) => {
                assert_ne!(
                    nullifier, multi_helper.nullifier,
                    "Different person should NOT produce matching nullifier"
                );
            }
        }
    }

    #[test]
    fn test_ring_pinky_enrollment_rejected() {
        let fuzzy = FuzzyNullifier::default_settings().unwrap();

        let embedding: Vec<f32> = (0..128).map(|i| ((i as f32) / 128.0) * 2.0 - 1.0).collect();

        let result = fuzzy.enroll_multiple(
            &[(FingerType::Ring, &embedding)],
            "test-scope",
            None,
        );
        assert!(result.is_err(), "Ring finger enrollment should be rejected");
        match result.unwrap_err() {
            XLockError::NonEnrollableFinger { finger } => {
                assert_eq!(finger, "ring");
                println!("✓ Ring finger correctly rejected for enrollment");
            }
            other => panic!("Expected NonEnrollableFinger error, got: {:?}", other),
        }

        let result = fuzzy.enroll_multiple(
            &[(FingerType::Pinky, &embedding)],
            "test-scope",
            None,
        );
        assert!(result.is_err(), "Pinky finger enrollment should be rejected");
        match result.unwrap_err() {
            XLockError::NonEnrollableFinger { finger } => {
                assert_eq!(finger, "pinky");
                println!("✓ Pinky finger correctly rejected for enrollment");
            }
            other => panic!("Expected NonEnrollableFinger error, got: {:?}", other),
        }

        let thumb: Vec<f32> = (0..128).map(|i| ((i as f32) / 128.0) * 2.0 - 1.0).collect();
        let result = fuzzy.enroll_multiple(
            &[
                (FingerType::Thumb, &thumb),
                (FingerType::Ring, &embedding),
            ],
            "test-scope",
            None,
        );
        assert!(result.is_err(), "Mixed enrollment with ring should be rejected");
        println!("✓ Mixed enrollment (thumb + ring) correctly rejected");
    }

    #[test]
    fn test_ring_pinky_can_verify() {
        let config = XLockConfig {
            use_hard_majority: false,
            ..Default::default()
        };
        let fuzzy = FuzzyNullifier::new(config).unwrap();

        let base: Vec<f32> = (0..128).map(|i| ((i as f32) / 128.0) * 2.0 - 1.0).collect();

        let thumb: Vec<f32> = base.clone();
        let index: Vec<f32> = base.iter().map(|&x| x + 0.01).collect();
        let middle: Vec<f32> = base.iter().map(|&x| x + 0.02).collect();

        let (multi_helper, _enrollment_key) = fuzzy
            .enroll_three_fingers(&thumb, &index, &middle, "test-scope", None)
            .unwrap();

        let ring: Vec<f32> = base.iter().map(|&x| x + 0.005).collect();

        let result = fuzzy.verify_against_multiple(&ring, &multi_helper, "test-scope", None);

        assert!(result.is_ok(), "Ring finger should be able to VERIFY (not enroll)");
        let (nullifier, _embedding_key, _matched) = result.unwrap();
        assert_eq!(nullifier, multi_helper.nullifier, "Should produce same nullifier");
        println!("✓ Ring finger correctly allowed for verification");
    }

    #[test]
    fn test_finger_type_can_enroll() {
        assert!(FingerType::Thumb.can_enroll(), "Thumb should be enrollable");
        assert!(FingerType::Index.can_enroll(), "Index should be enrollable");
        assert!(FingerType::Middle.can_enroll(), "Middle should be enrollable");
        assert!(!FingerType::Ring.can_enroll(), "Ring should NOT be enrollable");
        assert!(!FingerType::Pinky.can_enroll(), "Pinky should NOT be enrollable");

        let enrollable = FingerType::enrollable_fingers();
        assert_eq!(enrollable.len(), 3);
        assert!(enrollable.contains(&FingerType::Thumb));
        assert!(enrollable.contains(&FingerType::Index));
        assert!(enrollable.contains(&FingerType::Middle));
    }
}
