
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct ApiRequest<T> {
    pub id: Option<String>,
    #[serde(flatten)]
    pub payload: T,
}

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            id: None,
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            id: None,
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }

    pub fn with_id(mut self, id: Option<String>) -> Self {
        self.id = id;
        self
    }
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub registered: bool,
    pub sensor: SensorStatus,
    pub version: String,
    pub uptime: u64,
}

#[derive(Debug, Serialize)]
pub enum SensorStatus {
    Connected,
    Disconnected,
    Mock,
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub passphrase: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub commitment: String,
    pub merkle_leaf: String,
}

#[derive(Debug, Deserialize)]
pub struct UnlockRequest {
    pub passphrase: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UnlockResponse {
    pub unlocked: bool,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct AuthenticateRequest {
    pub scope: String,
    pub challenge: Option<String>,
    pub passphrase: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CredentialRequest {
    pub app_id: String,
    pub passphrase: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CredentialResponse {
    pub credential: String,
    pub nullifier_base: String,
}

#[derive(Debug, Deserialize)]
pub struct ProveRequest {
    pub scope: String,
    pub merkle_root: String,
    pub passphrase: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProveResponse {
    pub proof: String,
    pub merkle_root: String,
    pub nullifier_scope: String,
    pub nullifier: String,
    pub prover_toml_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    pub proof: String,
    pub merkle_root: String,
    pub nullifier_scope: String,
    pub nullifier: String,
}

#[derive(Debug, Serialize)]
pub struct VerifyResponse {
    pub valid: bool,
    pub details: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WitnessRequest {
    pub scope: String,
    pub passphrase: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WitnessResponse {
    pub prover_toml_path: String,
    pub identity_hash: String,
    pub nullifier: String,
}

#[derive(Debug, Deserialize)]
pub struct EnrollFingersRequest {
    pub scope: String,
    pub passphrase: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EnrollFingersResponse {
    pub nullifier: String,
    pub fingers_enrolled: usize,
    pub helper_data_size: usize,
    pub similarity_score: f32,
}

#[derive(Debug, Deserialize)]
pub struct VerifyFingerRequest {
    pub scope: String,
    pub passphrase: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VerifyFingerResponse {
    pub verified: bool,
    pub nullifier: Option<String>,
    pub matched_finger: Option<String>,
    pub confidence: Option<f32>,
}

#[derive(Debug, Serialize)]
pub struct ModelStatus {
    pub loaded: bool,
    pub weights_path: Option<String>,
    pub embedding_dim: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnrollmentProgress {
    pub step: usize,
    pub total: usize,
    pub next_finger: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct ProvePersonRequest {
    pub scope: String,
}

#[derive(Debug, Serialize)]
pub struct ProvePersonResponse {
    pub proof: String,
    pub merkle_root: String,
    pub commitment: String,
    pub nullifier: String,
    pub prover_toml_path: Option<String>,
}
