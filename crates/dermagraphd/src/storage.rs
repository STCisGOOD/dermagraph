
use std::path::{Path, PathBuf};
use turing_core::Fr;
use anyhow::{Result, Context};
use tracing::info;

use crate::crypto::{
    EncryptionContext,
    PassphrasePolicy,
    encrypt_with_header,
    decrypt_with_header,
    encrypt_with_raw_key,
    decrypt_with_raw_key,
    extract_salt,
    KEY_SIZE,
    SALT_SIZE,
};

pub struct Storage {
    data_dir: PathBuf,
    identity_path: PathBuf,
    laplacian_path: PathBuf,
    biometric_path: PathBuf,
    salt_path: PathBuf,
    xlock_path: PathBuf,
    embedding_path: PathBuf,
    merkle_tree_path: PathBuf,
    commitment_path: PathBuf,
}

impl Storage {
    pub async fn open(data_dir: &Path) -> Result<Self> {
        tokio::fs::create_dir_all(data_dir).await?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o700);
            std::fs::set_permissions(data_dir, perms).ok();
        }

        let identity_path = data_dir.join("identity.enc");
        let laplacian_path = data_dir.join("laplacian.enc");
        let biometric_path = data_dir.join("biometric.enc");
        let salt_path = data_dir.join("salt.bin");
        let xlock_path = data_dir.join("xlock.bin");
        let embedding_path = data_dir.join("embedding.enc");
        let merkle_tree_path = data_dir.join("merkle_tree.bin");
        let commitment_path = data_dir.join("commitment.bin");

        info!("Encrypted storage opened at {:?}", data_dir);

        Ok(Self {
            data_dir: data_dir.to_path_buf(),
            identity_path,
            laplacian_path,
            biometric_path,
            salt_path,
            xlock_path,
            embedding_path,
            merkle_tree_path,
            commitment_path,
        })
    }

    pub async fn is_registered(&self) -> Result<bool> {
        Ok(self.identity_path.exists() || self.xlock_path.exists())
    }

    pub async fn get_salt(&self) -> Result<Option<[u8; SALT_SIZE]>> {
        if !self.salt_path.exists() {
            return Ok(None);
        }
        let data = tokio::fs::read(&self.salt_path).await?;
        if data.len() != SALT_SIZE {
            return Err(anyhow::anyhow!("Invalid salt file size"));
        }
        let mut salt = [0u8; SALT_SIZE];
        salt.copy_from_slice(&data);
        Ok(Some(salt))
    }

    async fn store_salt(&self, salt: &[u8; SALT_SIZE]) -> Result<()> {
        tokio::fs::write(&self.salt_path, salt).await?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&self.salt_path, perms).ok();
        }

        Ok(())
    }

    pub async fn store_identity(
        &self,
        ctx: &EncryptionContext,
        data: &IdentityData,
    ) -> Result<()> {
        let serialized = serde_json::to_vec(data)?;

        let encrypted = encrypt_with_header(ctx, &serialized)
            .context("Failed to encrypt identity")?;

        let temp_path = self.identity_path.with_extension("enc.tmp");
        tokio::fs::write(&temp_path, &encrypted).await?;
        tokio::fs::rename(&temp_path, &self.identity_path).await?;

        self.store_salt(ctx.salt()).await?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&self.identity_path, perms).ok();
        }

        info!("Identity stored (encrypted with biometric-bound key)");
        Ok(())
    }

    pub async fn load_identity(
        &self,
        biometric_key: &[u8; KEY_SIZE],
        policy: PassphrasePolicy,
    ) -> Result<IdentityData> {
        let encrypted = tokio::fs::read(&self.identity_path).await
            .context("Failed to read identity file")?;

        let decrypted = decrypt_with_header(biometric_key, policy, &encrypted)
            .context("Failed to decrypt identity (wrong passphrase or biometric?)")?;

        let identity: IdentityData = serde_json::from_slice(&decrypted)
            .context("Failed to parse identity data")?;

        Ok(identity)
    }

    pub async fn store_laplacian(
        &self,
        ctx: &EncryptionContext,
        laplacian: &LaplacianData,
    ) -> Result<()> {
        let serialized = serde_json::to_vec(laplacian)?;
        let encrypted = encrypt_with_header(ctx, &serialized)
            .context("Failed to encrypt laplacian")?;

        let temp_path = self.laplacian_path.with_extension("enc.tmp");
        tokio::fs::write(&temp_path, &encrypted).await?;
        tokio::fs::rename(&temp_path, &self.laplacian_path).await?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&self.laplacian_path, perms).ok();
        }

        Ok(())
    }

    pub async fn load_laplacian(
        &self,
        biometric_key: &[u8; KEY_SIZE],
        policy: PassphrasePolicy,
    ) -> Result<LaplacianData> {
        let encrypted = tokio::fs::read(&self.laplacian_path).await
            .context("Failed to read laplacian file")?;

        let decrypted = decrypt_with_header(biometric_key, policy, &encrypted)
            .context("Failed to decrypt laplacian")?;

        let laplacian: LaplacianData = serde_json::from_slice(&decrypted)?;
        Ok(laplacian)
    }

    pub async fn store_biometric(
        &self,
        ctx: &EncryptionContext,
        biometric: &BiometricFeatures,
    ) -> Result<()> {
        let serialized = serde_json::to_vec(biometric)?;
        let encrypted = encrypt_with_header(ctx, &serialized)
            .context("Failed to encrypt biometric features")?;

        let temp_path = self.biometric_path.with_extension("enc.tmp");
        tokio::fs::write(&temp_path, &encrypted).await?;
        tokio::fs::rename(&temp_path, &self.biometric_path).await?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&self.biometric_path, perms).ok();
        }

        info!("Biometric features stored (encrypted)");
        Ok(())
    }

    pub async fn load_biometric(
        &self,
        biometric_key: &[u8; KEY_SIZE],
        policy: PassphrasePolicy,
    ) -> Result<BiometricFeatures> {
        let encrypted = tokio::fs::read(&self.biometric_path).await
            .context("Failed to read biometric file")?;

        let decrypted = decrypt_with_header(biometric_key, policy, &encrypted)
            .context("Failed to decrypt biometric features")?;

        let biometric: BiometricFeatures = serde_json::from_slice(&decrypted)?;
        Ok(biometric)
    }

    pub async fn has_biometric(&self) -> bool {
        self.biometric_path.exists()
    }

    pub async fn store_xlock(&self, helper_data: &[u8], scope: &str) -> Result<()> {
        let mut data = Vec::new();

        let scope_bytes = scope.as_bytes();
        data.extend(&(scope_bytes.len() as u16).to_le_bytes());
        data.extend(scope_bytes);
        data.extend(helper_data);

        let temp_path = self.xlock_path.with_extension("bin.tmp");
        tokio::fs::write(&temp_path, &data).await?;
        tokio::fs::rename(&temp_path, &self.xlock_path).await?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&self.xlock_path, perms).ok();
        }

        info!("X-Lock helper data stored ({} bytes, scope={})", helper_data.len(), scope);
        Ok(())
    }

    pub async fn load_xlock(&self) -> Result<(Vec<u8>, String)> {
        let data = tokio::fs::read(&self.xlock_path).await
            .context("Failed to read X-Lock helper data")?;

        if data.len() < 2 {
            anyhow::bail!("X-Lock data too short");
        }

        let scope_len = u16::from_le_bytes([data[0], data[1]]) as usize;
        if data.len() < 2 + scope_len {
            anyhow::bail!("X-Lock data corrupted (scope truncated)");
        }

        let scope = String::from_utf8(data[2..2 + scope_len].to_vec())
            .context("Invalid UTF-8 in scope")?;
        let helper_data = data[2 + scope_len..].to_vec();

        Ok((helper_data, scope))
    }

    pub async fn has_xlock(&self) -> bool {
        self.xlock_path.exists()
    }

    pub async fn store_embedding(&self, embedding: &[f32], embedding_key: &[u8; KEY_SIZE]) -> Result<()> {
        let mut plaintext = Vec::with_capacity(embedding.len() * 4);
        for &val in embedding {
            plaintext.extend(&val.to_le_bytes());
        }

        let encrypted = encrypt_with_raw_key(embedding_key, &plaintext)
            .context("Failed to encrypt embedding")?;

        let temp_path = self.embedding_path.with_extension("enc.tmp");
        tokio::fs::write(&temp_path, &encrypted).await?;
        tokio::fs::rename(&temp_path, &self.embedding_path).await?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&self.embedding_path, perms).ok();
        }

        info!("Representative embedding stored (encrypted, {} dimensions)", embedding.len());
        Ok(())
    }

    pub async fn load_embedding(&self, embedding_key: &[u8; KEY_SIZE]) -> Result<Vec<f32>> {
        let encrypted = tokio::fs::read(&self.embedding_path).await
            .context("Failed to read embedding data")?;

        let plaintext = decrypt_with_raw_key(embedding_key, &encrypted)
            .context("Failed to decrypt embedding (wrong biometric or password?)")?;

        if plaintext.len() % 4 != 0 {
            anyhow::bail!("Decrypted embedding corrupted (not aligned to f32)");
        }

        let embedding: Vec<f32> = plaintext
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();

        info!("Representative embedding loaded (decrypted, {} dimensions)", embedding.len());
        Ok(embedding)
    }

    pub async fn has_embedding(&self) -> bool {
        self.embedding_path.exists()
    }

    pub async fn store_merkle_tree(&self, tree_bytes: &[u8]) -> Result<()> {
        let temp_path = self.merkle_tree_path.with_extension("bin.tmp");
        tokio::fs::write(&temp_path, tree_bytes).await?;
        tokio::fs::rename(&temp_path, &self.merkle_tree_path).await?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&self.merkle_tree_path, perms).ok();
        }

        info!("Merkle tree stored ({} bytes)", tree_bytes.len());
        Ok(())
    }

    pub async fn load_merkle_tree(&self) -> Result<Vec<u8>> {
        let data = tokio::fs::read(&self.merkle_tree_path).await
            .context("Failed to read merkle tree")?;
        info!("Merkle tree loaded ({} bytes)", data.len());
        Ok(data)
    }

    pub async fn has_merkle_tree(&self) -> bool {
        self.merkle_tree_path.exists()
    }

    pub async fn store_commitment_data(&self, commitment: &[u8; 32], blinding: &[u8; 32]) -> Result<()> {
        let mut data = Vec::with_capacity(64);
        data.extend_from_slice(commitment);
        data.extend_from_slice(blinding);

        let temp_path = self.commitment_path.with_extension("bin.tmp");
        tokio::fs::write(&temp_path, &data).await?;
        tokio::fs::rename(&temp_path, &self.commitment_path).await?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&self.commitment_path, perms).ok();
        }

        info!("Commitment data stored (commitment + blinding)");
        Ok(())
    }

    pub async fn load_commitment_data(&self) -> Result<([u8; 32], [u8; 32])> {
        let data = tokio::fs::read(&self.commitment_path).await
            .context("Failed to read commitment data")?;

        if data.len() != 64 {
            anyhow::bail!("Invalid commitment data size: expected 64, got {}", data.len());
        }

        let mut commitment = [0u8; 32];
        let mut blinding = [0u8; 32];
        commitment.copy_from_slice(&data[0..32]);
        blinding.copy_from_slice(&data[32..64]);

        info!("Commitment data loaded");
        Ok((commitment, blinding))
    }

    pub async fn has_commitment_data(&self) -> bool {
        self.commitment_path.exists()
    }

    pub async fn clear(&self) -> Result<()> {
        for path in [&self.identity_path, &self.laplacian_path, &self.biometric_path, &self.salt_path, &self.xlock_path, &self.embedding_path, &self.merkle_tree_path, &self.commitment_path] {
            if path.exists() {
                if let Ok(metadata) = tokio::fs::metadata(path).await {
                    let zeros = vec![0u8; metadata.len() as usize];
                    tokio::fs::write(path, &zeros).await.ok();
                }
                tokio::fs::remove_file(path).await?;
            }
        }
        info!("Storage cleared (secure wipe)");
        Ok(())
    }

    pub async fn is_legacy_format(&self) -> bool {
        if !self.identity_path.exists() {
            return false;
        }

        if let Ok(data) = tokio::fs::read(&self.identity_path).await {
            data.first() == Some(&b'{')
        } else {
            false
        }
    }

    pub async fn load_legacy_identity(&self) -> Result<IdentityData> {
        let data = tokio::fs::read(&self.identity_path).await?;
        let identity: IdentityData = serde_json::from_slice(&data)?;
        Ok(identity)
    }

    pub async fn load_legacy_laplacian(&self) -> Result<LaplacianData> {
        let legacy_path = self.data_dir.join("laplacian.bin");
        let data = tokio::fs::read(&legacy_path).await?;
        let laplacian: LaplacianData = serde_json::from_slice(&data)?;
        Ok(laplacian)
    }

    pub async fn load_legacy_biometric(&self) -> Result<BiometricFeatures> {
        let legacy_path = self.data_dir.join("biometric.json");
        let data = tokio::fs::read(&legacy_path).await?;
        let biometric: BiometricFeatures = serde_json::from_slice(&data)?;
        Ok(biometric)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IdentityData {
    pub master_secret: [u8; 32],
    pub commitment: [u8; 32],
    pub registered_at: u64,
    pub version: u32,
}

impl IdentityData {
    pub fn new(master_secret: Fr, commitment: Fr) -> Self {
        Self {
            master_secret: master_secret.to_be_bytes(),
            commitment: commitment.to_be_bytes(),
            registered_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            version: 2,
        }
    }

    pub fn master_secret_fr(&self) -> Fr {
        Fr::from_be_bytes_mod_order(&self.master_secret)
    }

    pub fn commitment_fr(&self) -> Fr {
        Fr::from_be_bytes_mod_order(&self.commitment)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LaplacianData {
    pub dim: usize,
    pub entries: Vec<(usize, usize, [u8; 32])>,
}

impl LaplacianData {
    pub fn from_laplacian(lap: &turing_core::GraphLaplacian) -> Self {
        let entries: Vec<_> = lap.matrix.iter()
            .map(|(r, c, v)| (r, c, v.to_be_bytes()))
            .collect();

        Self {
            dim: lap.dim(),
            entries,
        }
    }

    pub fn to_laplacian(&self) -> turing_core::GraphLaplacian {
        let edges: Vec<_> = self.entries.iter()
            .filter(|(r, c, _)| r != c)
            .map(|(r, c, v)| {
                let val = Fr::from_be_bytes_mod_order(v);
                (*r, *c, -val)
            })
            .collect();

        turing_core::GraphLaplacian::from_edges(self.dim, &edges, false)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BiometricFeatures {
    pub minutiae_x: Vec<f64>,
    pub minutiae_y: Vec<f64>,
    pub minutiae_theta: Vec<f64>,
    pub quantized_spectrum: Vec<u64>,
}

impl BiometricFeatures {
    pub fn from_biometric_data(data: &biometric_extract::BiometricData) -> Self {
        Self {
            minutiae_x: data.minutiae.x_coords(),
            minutiae_y: data.minutiae.y_coords(),
            minutiae_theta: data.minutiae.orientations(),
            quantized_spectrum: data.quantized.to_field_elements(),
        }
    }

    pub fn to_biometric_data(&self, laplacian: &turing_core::GraphLaplacian) -> biometric_extract::BiometricData {
        use biometric_extract::{MinutiaeSet, RidgeGraph, SpectralSignature, QuantizedSpectrum, QuantizationParams};

        let minutiae = MinutiaeSet::from_coords(
            &self.minutiae_x,
            &self.minutiae_y,
            &self.minutiae_theta,
        );

        let graph = RidgeGraph::from_minutiae(&minutiae);

        let spectrum = SpectralSignature::from_graph(&graph)
            .expect("Graph should have valid spectrum");

        let quantized = QuantizedSpectrum::from_values(self.quantized_spectrum.clone());

        biometric_extract::BiometricData {
            minutiae,
            graph,
            laplacian: laplacian.clone(),
            spectrum,
            quantized,
        }
    }
}
