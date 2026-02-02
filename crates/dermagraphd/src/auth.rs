
use turing_core::{Fr, TuringHash, TuringKdf, TuringParams, MorphogenState, GraphLaplacian};
use turing_core::{SpectralTuringHash, STHParams};
use crate::crypto::{EncryptionContext, PassphrasePolicy, KEY_SIZE};
use crate::storage::{Storage, IdentityData, LaplacianData, BiometricFeatures};
use anyhow::{Result, Context};
use tracing::{info, warn};
use hardware::{Sensor, SensorConfig};

#[derive(Debug, Clone)]
pub struct RegistrationResult {
    pub commitment: Fr,
    pub biometric_key: [u8; KEY_SIZE],
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AuthRequest {
    pub scope: String,
    pub challenge: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AuthResponse {
    pub proof: String,
    pub nullifier: String,
    pub public_inputs: PublicInputs,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PublicInputs {
    pub scope_hash: String,
    pub nullifier: String,
    pub timestamp: u64,
}

pub async fn authenticate(
    storage: &Storage,
    biometric_key: &[u8; KEY_SIZE],
    policy: PassphrasePolicy,
    request: &AuthRequest,
) -> Result<AuthResponse> {
    info!("Authenticating for scoped request");

    let identity = storage.load_identity(biometric_key, policy.clone()).await?;
    let laplacian_data = storage.load_laplacian(biometric_key, policy).await?;
    let laplacian = laplacian_data.to_laplacian();

    let master_secret = identity.master_secret_fr();

    let params = TuringParams::crypto();
    let nullifier = TuringKdf::derive_nullifier(
        master_secret,
        &request.scope,
        &laplacian,
        &params,
    )?;

    let scope_hash = hash_scope(&request.scope);

    let proof = String::new();

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    Ok(AuthResponse {
        proof,
        nullifier: format!("0x{}", hex::encode(nullifier.to_be_bytes())),
        public_inputs: PublicInputs {
            scope_hash: format!("0x{}", hex::encode(scope_hash.to_be_bytes())),
            nullifier: format!("0x{}", hex::encode(nullifier.to_be_bytes())),
            timestamp,
        },
    })
}

pub async fn register_mock(
    storage: &Storage,
    policy: PassphrasePolicy,
) -> Result<RegistrationResult> {
    info!("Registering with mock biometric data");

    let biometric = biometric_extract::BiometricData::mock();

    register_with_biometric_data(storage, &biometric, policy).await
}

pub async fn register_with_sensor(
    storage: &Storage,
    sensor_config: &SensorConfig,
    policy: PassphrasePolicy,
) -> Result<RegistrationResult> {
    info!("Connecting to fingerprint sensor");

    let mut sensor = Sensor::connect(sensor_config).await
        .context("Failed to connect to fingerprint sensor")?;

    let info = sensor.info()
        .context("Failed to get sensor info")?;
    info!("Sensor connected successfully");

    info!("Place your finger on the sensor...");
    let image = sensor.capture().await
        .context("Failed to capture fingerprint")?;
    info!("Fingerprint captured successfully");

    if image.quality < 50 {
        warn!("Image quality is low. Consider rescanning for better results.");
    }

    let biometric = biometric_extract::BiometricData::from_raw(
        image.data,
        image.width,
        image.height,
    ).await.context("Failed to extract biometric features from fingerprint")?;

    info!("Biometric features extracted successfully");

    register_with_biometric_data(storage, &biometric, policy).await
}

pub async fn derive_key_from_sensor(
    sensor_config: &SensorConfig,
) -> Result<[u8; KEY_SIZE]> {
    info!("Scanning fingerprint for authentication...");

    let mut sensor = Sensor::connect(sensor_config).await
        .context("Failed to connect to sensor")?;

    let image = sensor.capture().await
        .context("Failed to capture fingerprint")?;

    let biometric = biometric_extract::BiometricData::from_raw(
        image.data,
        image.width,
        image.height,
    ).await.context("Failed to extract biometric features")?;

    derive_biometric_key(&biometric).await
}

pub async fn register_with_biometric(
    storage: &Storage,
    x: &[f64],
    y: &[f64],
    theta: &[f64],
    policy: PassphrasePolicy,
) -> Result<RegistrationResult> {
    let minutiae = biometric_extract::MinutiaeSet::from_coords(x, y, theta);
    let graph = biometric_extract::RidgeGraph::from_minutiae(&minutiae);
    let laplacian = graph.to_laplacian();
    let spectrum = biometric_extract::SpectralSignature::from_graph(&graph)?;
    let quantized = biometric_extract::QuantizedSpectrum::from_spectrum(
        &spectrum,
        &biometric_extract::QuantizationParams::default(),
    );

    let biometric = biometric_extract::BiometricData {
        minutiae,
        graph,
        laplacian,
        spectrum,
        quantized,
    };

    register_with_biometric_data(storage, &biometric, policy).await
}

pub async fn register_with_biometric_data(
    storage: &Storage,
    biometric: &biometric_extract::BiometricData,
    policy: PassphrasePolicy,
) -> Result<RegistrationResult> {
    let witness = biometric.to_sth_witness();
    info!("Registering identity using Spectral Turing Hash");

    let params = STHParams::standard_128bit();
    let master_secret = SpectralTuringHash::compute(
        &witness.minutiae_x,
        &witness.minutiae_y,
        &witness.minutiae_theta,
        &witness.quantized_spectrum,
        &biometric.laplacian,
        &params,
    )?;

    let biometric_key: [u8; KEY_SIZE] = master_secret.to_be_bytes();

    let commitment = master_secret;

    let ctx = EncryptionContext::new_for_registration(&biometric_key, policy)
        .context("Failed to create encryption context")?;

    let identity_data = IdentityData::new(master_secret, commitment);
    storage.store_identity(&ctx, &identity_data).await?;

    let laplacian_data = LaplacianData::from_laplacian(&biometric.laplacian);
    storage.store_laplacian(&ctx, &laplacian_data).await?;

    let biometric_features = BiometricFeatures::from_biometric_data(biometric);
    storage.store_biometric(&ctx, &biometric_features).await?;

    info!("Identity registered successfully (encrypted with biometric-bound key)");

    Ok(RegistrationResult {
        commitment,
        biometric_key,
    })
}

pub async fn derive_biometric_key(
    biometric: &biometric_extract::BiometricData,
) -> Result<[u8; KEY_SIZE]> {
    let witness = biometric.to_sth_witness();

    let params = STHParams::standard_128bit();
    let master_secret = SpectralTuringHash::compute(
        &witness.minutiae_x,
        &witness.minutiae_y,
        &witness.minutiae_theta,
        &witness.quantized_spectrum,
        &biometric.laplacian,
        &params,
    )?;

    Ok(master_secret.to_be_bytes())
}

fn build_ridge_graph(x: &[f64], y: &[f64], n: usize) -> Vec<(usize, usize, Fr)> {
    let mut edges = Vec::new();
    let max_distance = 100.0;

    for i in 0..n {
        for j in (i + 1)..n {
            let dx = x[i] - x[j];
            let dy = y[i] - y[j];
            let dist = (dx * dx + dy * dy).sqrt();

            if dist < max_distance {
                let weight = Fr::from_u64(((max_distance - dist) * 10.0) as u64 + 1);
                edges.push((i, j, weight));
            }
        }
    }

    edges
}

fn hash_scope(scope: &str) -> Fr {
    let elements: Vec<Fr> = scope.bytes()
        .map(|b| Fr::from_u64(b as u64))
        .collect();
    Fr::hash_many(&elements)
}

mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().map(|b| format!("{:02x}", b)).collect()
    }
}
