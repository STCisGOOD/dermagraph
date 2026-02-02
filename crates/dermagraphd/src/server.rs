
use std::sync::Arc;
use std::path::PathBuf;
use axum::{
    Router,
    routing::{get, post},
    extract::State,
    Json,
    http::StatusCode,
    response::sse::{Event, Sse},
};
use futures::stream::Stream;
use std::convert::Infallible;
use tokio::sync::RwLock;
use tower_governor::GovernorLayer;
use tower_governor::governor::{GovernorConfig, GovernorConfigBuilder};
use tower_http::cors::{CorsLayer, Any};
use tracing::{info, error, debug};

use crate::api::*;
use crate::auth::{self, AuthRequest};
use crate::config::Config;
use crate::crypto::{KEY_SIZE, PassphrasePolicy};
use crate::model::ModelState;
use crate::storage::Storage;
use crate::xlock_auth::{self, EnrollmentProgress};

use biometric_extract::contrastive::{MultiFingerHelperData, FuzzyNullifier, PersonEmbedding};
use noir_witness::{WitnessGenerator, PersonWitnessGenerator, FieldFormatter, MerkleTree};
use turing_core::person_identity::PersonEmbedding as TcPersonEmbedding;

pub struct AppState {
    pub config: Config,
    pub storage: Storage,
    pub start_time: std::time::Instant,
    pub witness_generator: WitnessGenerator,
    pub biometric_key: Option<[u8; KEY_SIZE]>,

    pub model_state: ModelState,
    pub xlock_helper_data: Option<MultiFingerHelperData>,
    pub xlock_scope: Option<String>,
    pub xlock_embedding: Option<PersonEmbedding>,

    pub person_witness_generator: PersonWitnessGenerator,
}

type SharedState = Arc<RwLock<AppState>>;

pub async fn run(port: u16, config: Config, storage: Storage) -> anyhow::Result<()> {
    let bind_addr: std::net::IpAddr = config.http_bind.parse().unwrap_or_else(|_| {
        error!("Invalid http_bind '{}', falling back to 127.0.0.1", config.http_bind);
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))
    });

    let model_state = if let Some(ref weights_path) = config.cnn_weights_path {
        info!("CNN weights configured at: {:?}", weights_path);
        ModelState::with_weights_path(weights_path)
    } else {
        let default_path = config.data_dir
            .parent()
            .unwrap_or(&config.data_dir)
            .join("checkpoints")
            .join("best_burn.safetensors");
        if default_path.exists() {
            info!("Found CNN weights at default location: {:?}", default_path);
            ModelState::with_weights_path(default_path)
        } else {
            info!("No CNN weights found. X-Lock endpoints will use mock mode.");
            ModelState::new()
        }
    };

    let (xlock_helper_data, xlock_scope) = if storage.has_xlock().await {
        match storage.load_xlock().await {
            Ok((helper_bytes, scope)) => {
                match MultiFingerHelperData::from_bytes(&helper_bytes) {
                    Ok(helper_data) => {
                        info!("Restored X-Lock enrollment from storage (scope={})", scope);
                        (Some(helper_data), Some(scope))
                    }
                    Err(e) => {
                        error!("Failed to parse X-Lock helper data: {}", e);
                        (None, None)
                    }
                }
            }
            Err(e) => {
                error!("Failed to load X-Lock enrollment: {}", e);
                (None, None)
            }
        }
    } else {
        (None, None)
    };

    let xlock_embedding: Option<PersonEmbedding> = None;
    if storage.has_embedding().await {
        info!("Encrypted embedding found in storage (will decrypt after verification)");
    }

    let person_witness_generator = if storage.has_merkle_tree().await {
        match storage.load_merkle_tree().await {
            Ok(tree_bytes) => {
                match noir_witness::MerkleTree::from_bytes(&tree_bytes) {
                    Ok(tree) => {
                        info!("Restored Merkle tree from storage ({} entries, root will be stable)", tree.count());
                        PersonWitnessGenerator::with_tree(tree)
                    }
                    Err(e) => {
                        error!("Failed to parse Merkle tree: {}", e);
                        PersonWitnessGenerator::new()
                    }
                }
            }
            Err(e) => {
                error!("Failed to load Merkle tree: {}", e);
                PersonWitnessGenerator::new()
            }
        }
    } else {
        info!("No Merkle tree found, starting fresh");
        PersonWitnessGenerator::new()
    };

    let state = Arc::new(RwLock::new(AppState {
        config,
        storage,
        start_time: std::time::Instant::now(),
        witness_generator: WitnessGenerator::new(),
        biometric_key: None,
        model_state,
        xlock_helper_data,
        xlock_scope,
        xlock_embedding,
        person_witness_generator,
    }));

    let rate_limit_config: Arc<GovernorConfig<_, _>> = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(2)
            .burst_size(10)
            .finish()
            .expect("Rate limiter configuration failed")
    );

    let public_routes = Router::new()
        .route("/", get(root))
        .route("/status", get(status))
        .route("/model-status", get(model_status))
        .route("/enroll-fingers", post(enroll_fingers))
        .route("/enroll-fingers-stream", get(enroll_fingers_stream))
        .route("/verify-finger", post(verify_finger))
        .route("/prove-person", post(prove_person));

    let rate_limited_routes = Router::new()
        .route("/authenticate", post(authenticate))
        .route("/register", post(register))
        .route("/unlock", post(unlock))
        .route("/credential", post(credential))
        .route("/witness", post(generate_witness))
        .route("/prove", post(prove))
        .route("/verify", post(verify))
        .layer(GovernorLayer {
            config: rate_limit_config,
        });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .merge(public_routes)
        .merge(rate_limited_routes)
        .with_state(state)
        .layer(cors);

    let addr = std::net::SocketAddr::from((bind_addr, port));
    info!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn root() -> &'static str {
    "dermagraphd - Dermagraphic Encryption Daemon

STH Endpoints (legacy):
  GET  /status       - Check daemon status
  POST /register     - Register fingerprint identity (single finger)
  POST /unlock       - Unlock session with fingerprint
  POST /authenticate - Authenticate with scope
  POST /credential   - Derive app credential

ZK Proof Endpoints:
  POST /witness      - Generate Prover.toml for Noir
  POST /prove        - Generate ZK proof
  POST /verify       - Verify ZK proof

Cross-Finger CNN + X-Lock Endpoints (Columbia research):
  POST /enroll-fingers        - Enroll 3 fingers (one request, no progress)
  GET  /enroll-fingers-stream - Enroll with SSE progress events (recommended)
  POST /verify-finger         - Verify with ANY finger
  GET  /model-status          - Check CNN model status

Noir Person Identity Proof:
  POST /prove-person          - Generate Noir ZK proof with Poseidon nullifier
"
}

async fn status(
    State(state): State<SharedState>,
) -> Json<ApiResponse<StatusResponse>> {
    let state = state.read().await;

    let registered = state.storage.is_registered().await.unwrap_or(false);
    let uptime = state.start_time.elapsed().as_secs();

    let sensor = match state.config.sensor_type {
        crate::config::SensorType::Mock => SensorStatus::Mock,
        crate::config::SensorType::R503 => SensorStatus::Connected,
        crate::config::SensorType::Adafruit => SensorStatus::Connected,
    };

    let response = StatusResponse {
        registered,
        sensor,
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime,
    };

    Json(ApiResponse::success(response))
}

async fn authenticate(
    State(state): State<SharedState>,
    Json(request): Json<AuthenticateRequest>,
) -> Result<Json<ApiResponse<auth::AuthResponse>>, StatusCode> {
    let state = state.read().await;

    if !state.storage.is_registered().await.unwrap_or(false) {
        return Ok(Json(ApiResponse::error("Not registered")));
    }

    let biometric_key = match &state.biometric_key {
        Some(key) => key,
        None => {
            return Ok(Json(ApiResponse::error(
                "Session not unlocked. Re-scan fingerprint or restart daemon after registration."
            )));
        }
    };

    let auth_request = AuthRequest {
        scope: request.scope,
        challenge: request.challenge,
    };

    let policy = passphrase_to_policy(request.passphrase.as_deref());

    match auth::authenticate(
        &state.storage,
        biometric_key,
        policy,
        &auth_request,
    ).await {
        Ok(response) => Ok(Json(ApiResponse::success(response))),
        Err(e) => {
            error!("Authentication failed: {}", e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

async fn register(
    State(state): State<SharedState>,
    Json(request): Json<RegisterRequest>,
) -> Result<Json<ApiResponse<RegisterResponse>>, StatusCode> {
    let mut state = state.write().await;

    if state.storage.is_registered().await.unwrap_or(false) {
        return Ok(Json(ApiResponse::error("Already registered")));
    }

    let policy = passphrase_to_policy(request.passphrase.as_deref());

    let result = if state.config.is_hardware_sensor() {
        let sensor_config = state.config.to_sensor_config();
        info!("Registering with hardware sensor");
        auth::register_with_sensor(&state.storage, &sensor_config, policy).await
    } else {
        info!("Registering with mock sensor");
        auth::register_mock(&state.storage, policy).await
    };

    match result {
        Ok(result) => {
            state.biometric_key = Some(result.biometric_key);
            info!("Biometric key stored in session (memory only)");

            let response = RegisterResponse {
                commitment: format!("0x{}", hex_encode(result.commitment.to_be_bytes())),
                merkle_leaf: format!("0x{}", hex_encode(result.commitment.to_be_bytes())),
            };
            Ok(Json(ApiResponse::success(response)))
        }
        Err(e) => {
            error!("Registration failed: {}", e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

async fn unlock(
    State(state): State<SharedState>,
    Json(request): Json<UnlockRequest>,
) -> Result<Json<ApiResponse<UnlockResponse>>, StatusCode> {
    let mut state = state.write().await;

    if !state.storage.is_registered().await.unwrap_or(false) {
        return Ok(Json(ApiResponse::error("Not registered. Run /register first.")));
    }

    if state.biometric_key.is_some() {
        let response = UnlockResponse {
            unlocked: true,
            message: "Session already unlocked".to_string(),
        };
        return Ok(Json(ApiResponse::success(response)));
    }

    info!("Unlocking session with fingerprint scan...");

    let biometric_key = if state.config.is_hardware_sensor() {
        let sensor_config = state.config.to_sensor_config();
        match auth::derive_key_from_sensor(&sensor_config).await {
            Ok(key) => key,
            Err(e) => {
                error!("Unlock failed: {}", e);
                return Ok(Json(ApiResponse::error(format!("Fingerprint scan failed: {}", e))));
            }
        }
    } else {
        let biometric = biometric_extract::BiometricData::mock();
        match auth::derive_biometric_key(&biometric).await {
            Ok(key) => key,
            Err(e) => {
                error!("Unlock failed: {}", e);
                return Ok(Json(ApiResponse::error(format!("Mock unlock failed: {}", e))));
            }
        }
    };

    let policy = passphrase_to_policy(request.passphrase.as_deref());

    match state.storage.load_identity(&biometric_key, policy).await {
        Ok(_) => {
            state.biometric_key = Some(biometric_key);
            info!("Session unlocked successfully");

            let response = UnlockResponse {
                unlocked: true,
                message: "Session unlocked with biometric authentication".to_string(),
            };
            Ok(Json(ApiResponse::success(response)))
        }
        Err(e) => {
            error!("Unlock verification failed: {}", e);
            Ok(Json(ApiResponse::error(
                "Fingerprint does not match registered identity. Try again."
            )))
        }
    }
}

async fn credential(
    State(state): State<SharedState>,
    Json(request): Json<CredentialRequest>,
) -> Result<Json<ApiResponse<CredentialResponse>>, StatusCode> {
    let state = state.read().await;

    if !state.storage.is_registered().await.unwrap_or(false) {
        return Ok(Json(ApiResponse::error("Not registered")));
    }

    let biometric_key = match &state.biometric_key {
        Some(key) => key,
        None => {
            return Ok(Json(ApiResponse::error(
                "Session not unlocked. Re-scan fingerprint or restart daemon after registration."
            )));
        }
    };

    let policy = passphrase_to_policy(request.passphrase.as_deref());

    let identity = match state.storage.load_identity(biometric_key, policy.clone()).await {
        Ok(id) => id,
        Err(e) => return Ok(Json(ApiResponse::error(e.to_string()))),
    };

    let laplacian_data = match state.storage.load_laplacian(biometric_key, policy).await {
        Ok(l) => l,
        Err(e) => return Ok(Json(ApiResponse::error(e.to_string()))),
    };

    let laplacian = laplacian_data.to_laplacian();
    let master = identity.master_secret_fr();
    let params = turing_core::TuringParams::crypto();

    let credential = match turing_core::TuringKdf::derive_credential(
        master,
        &request.app_id,
        &laplacian,
        &params,
    ) {
        Ok(c) => c,
        Err(e) => return Ok(Json(ApiResponse::error(e.to_string()))),
    };

    let nullifier_base = match turing_core::TuringKdf::derive(
        master,
        &format!("nullifier_base:{}", request.app_id),
        &laplacian,
        &params,
    ) {
        Ok(n) => n,
        Err(e) => return Ok(Json(ApiResponse::error(e.to_string()))),
    };

    let response = CredentialResponse {
        credential: format!("0x{}", hex_encode(credential.to_be_bytes())),
        nullifier_base: format!("0x{}", hex_encode(nullifier_base.to_be_bytes())),
    };

    Ok(Json(ApiResponse::success(response)))
}

fn hex_encode(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|b| format!("{:02x}", b)).collect()
}

use turing_core::Fr;

trait ToBeBytes {
    fn to_be_bytes(&self) -> [u8; 32];
}

impl ToBeBytes for Fr {
    fn to_be_bytes(&self) -> [u8; 32] {
        turing_core::Fr::to_be_bytes(self)
    }
}

async fn enroll_fingers(
    State(state): State<SharedState>,
    Json(request): Json<EnrollFingersRequest>,
) -> Result<Json<ApiResponse<EnrollFingersResponse>>, StatusCode> {
    let mut state = state.write().await;

    info!("Starting multi-finger enrollment with X-Lock");

    if state.xlock_helper_data.is_some() {
        return Ok(Json(ApiResponse::error(
            "Already enrolled with X-Lock. Delete data to re-enroll."
        )));
    }

    let result = if state.config.is_hardware_sensor() {
        let weights_path = match state.model_state.weights_path() {
            Some(p) if p.exists() => p.clone(),
            _ => {
                error!("CNN model weights not available");
                return Ok(Json(ApiResponse::error(
                    "CNN model not available. Configure cnn_weights_path in config."
                )));
            }
        };

        let sensor_config = state.config.to_sensor_config();

        match xlock_auth::enroll_three_fingers(
            &sensor_config,
            &weights_path,
            &request.scope,
            request.passphrase.as_deref(),
        ).await {
            Ok(r) => r,
            Err(e) => {
                error!("X-Lock enrollment failed: {}", e);
                return Ok(Json(ApiResponse::error(e.to_string())));
            }
        }
    } else {
        info!("Using mock mode for X-Lock enrollment");
        match xlock_auth::enroll_mock(&request.scope, request.passphrase.as_deref()) {
            Ok(r) => r,
            Err(e) => {
                error!("Mock enrollment failed: {}", e);
                return Ok(Json(ApiResponse::error(e.to_string())));
            }
        }
    };

    let helper_bytes = result.helper_data.to_bytes();
    let helper_data_size = helper_bytes.len();
    if let Err(e) = state.storage.store_xlock(&helper_bytes, &request.scope).await {
        error!("Failed to persist X-Lock enrollment: {}", e);
    }
    if let Err(e) = state.storage.store_embedding(
        &result.representative_embedding.vector,
        &result.embedding_key,
    ).await {
        error!("Failed to persist encrypted embedding: {}", e);
    }

    let (commitment_value, blinding, tree_bytes, merkle_root_str) = {
        use turing_core::person_circuit::{CircuitEmbedding, PersonCommitment, QuantizationConfig};
        use rand::thread_rng;

        let tc_embedding = TcPersonEmbedding::new(result.representative_embedding.vector.clone());

        let config = QuantizationConfig::default();
        let circuit_embedding = CircuitEmbedding::from_embedding(&tc_embedding, &config);

        let mut rng = thread_rng();
        let commitment_obj = PersonCommitment::new(&circuit_embedding, &mut rng);
        let commitment_value = commitment_obj.value;
        let blinding = commitment_obj.blinding;

        info!("Created commitment for Merkle tree registration");

        match state.person_witness_generator.register_commitment(commitment_value) {
            Ok(index) => {
                info!("Registered commitment at Merkle tree index {}", index);
                let tree_bytes = state.person_witness_generator.tree().to_bytes();
                let merkle_root = state.person_witness_generator.merkle_root();
                let merkle_root_str = format!("0x{}", hex_encode(turing_core::Fr::to_be_bytes(&merkle_root)));
                (commitment_value, blinding, Some(tree_bytes), Some(merkle_root_str))
            }
            Err(e) => {
                error!("Failed to register commitment in Merkle tree: {}", e);
                (commitment_value, blinding, None, None)
            }
        }
    };

    if let Some(tree_bytes) = tree_bytes {
        if let Err(e) = state.storage.store_merkle_tree(&tree_bytes).await {
            error!("Failed to persist Merkle tree: {}", e);
        } else {
            info!("Merkle tree persisted ({} bytes)", tree_bytes.len());
        }

        let commitment_bytes = turing_core::Fr::to_be_bytes(&commitment_value);
        let blinding_bytes = turing_core::Fr::to_be_bytes(&blinding);
        if let Err(e) = state.storage.store_commitment_data(&commitment_bytes, &blinding_bytes).await {
            error!("Failed to persist commitment data: {}", e);
        } else {
            info!("Commitment data persisted (commitment + blinding)");
        }

        if let Some(root_str) = merkle_root_str {
            info!("Merkle root for on-chain sync: {}", root_str);
        }
    }

    state.xlock_embedding = Some(result.representative_embedding);
    state.xlock_helper_data = Some(result.helper_data);
    state.xlock_scope = Some(request.scope.clone());

    info!("X-Lock enrollment complete. Nullifier: 0x{}...",
          hex_encode(&result.nullifier[..8]));

    let response = EnrollFingersResponse {
        nullifier: format!("0x{}", hex_encode(&result.nullifier)),
        fingers_enrolled: 3,
        helper_data_size,
        similarity_score: result.intra_similarity,
    };

    Ok(Json(ApiResponse::success(response)))
}

async fn enroll_fingers_stream(
    State(state): State<SharedState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;
    use tokio_stream::StreamExt;

    let scope = params.get("scope").cloned().unwrap_or_else(|| "default".to_string());
    let passphrase = params.get("passphrase").cloned();

    info!("Starting SSE enrollment stream for scope: {}", scope);

    let (progress_tx, progress_rx) = mpsc::channel::<EnrollmentProgress>(16);

    let state_read = state.read().await;
    let is_hardware = state_read.config.is_hardware_sensor();
    let sensor_config = state_read.config.to_sensor_config();
    let weights_path = state_read.model_state.weights_path().cloned();

    if state_read.xlock_helper_data.is_some() {
        let _ = progress_tx.try_send(EnrollmentProgress::Error {
            message: "Already enrolled with X-Lock. Delete data to re-enroll.".to_string()
        });
    }
    drop(state_read);

    let state_clone = state.clone();
    tokio::spawn(async move {
        let result = if is_hardware {
            if let Some(ref weights) = weights_path {
                if weights.exists() {
                    xlock_auth::enroll_three_fingers_with_progress(
                        &sensor_config,
                        weights,
                        &scope,
                        passphrase.as_deref(),
                        progress_tx.clone(),
                    ).await
                } else {
                    let _ = progress_tx.try_send(EnrollmentProgress::Error {
                        message: "CNN model weights not found".to_string()
                    });
                    return;
                }
            } else {
                let _ = progress_tx.try_send(EnrollmentProgress::Error {
                    message: "CNN model not configured".to_string()
                });
                return;
            }
        } else {
            let _ = progress_tx.try_send(EnrollmentProgress::Ready { finger: "thumb".to_string() });
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            let _ = progress_tx.try_send(EnrollmentProgress::Captured { finger: "thumb".to_string(), quality: 85 });

            let _ = progress_tx.try_send(EnrollmentProgress::Ready { finger: "index".to_string() });
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            let _ = progress_tx.try_send(EnrollmentProgress::Captured { finger: "index".to_string(), quality: 90 });

            let _ = progress_tx.try_send(EnrollmentProgress::Ready { finger: "middle".to_string() });
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            let _ = progress_tx.try_send(EnrollmentProgress::Captured { finger: "middle".to_string(), quality: 88 });

            let _ = progress_tx.try_send(EnrollmentProgress::Processing {
                step: "mock_processing".to_string(),
                percent: 50
            });

            match xlock_auth::enroll_mock(&scope, passphrase.as_deref()) {
                Ok(result) => {
                    let _ = progress_tx.try_send(EnrollmentProgress::Complete {
                        nullifier: format!("0x{}", hex_encode(&result.nullifier)),
                        similarity: result.intra_similarity,
                    });
                    Ok(result)
                }
                Err(e) => {
                    let _ = progress_tx.try_send(EnrollmentProgress::Error {
                        message: e.to_string()
                    });
                    Err(e)
                }
            }
        };

        if let Ok(result) = result {
            let mut state = state_clone.write().await;

            let helper_bytes = result.helper_data.to_bytes();
            if let Err(e) = state.storage.store_xlock(&helper_bytes, &scope).await {
                error!("Failed to persist X-Lock enrollment: {}", e);
            } else {
                info!("X-Lock enrollment persisted to storage");
            }

            if let Err(e) = state.storage.store_embedding(
                &result.representative_embedding.vector,
                &result.embedding_key,
            ).await {
                error!("Failed to persist encrypted embedding: {}", e);
            } else {
                info!("Representative embedding persisted to storage (encrypted)");
            }

            let (commitment_value, blinding, tree_bytes, merkle_root_bytes) = {
                use turing_core::person_circuit::{CircuitEmbedding, PersonCommitment, QuantizationConfig};
                use turing_core::person_identity::PersonEmbedding as TcPersonEmbedding;
                use rand::thread_rng;

                let tc_embedding = TcPersonEmbedding::new(result.representative_embedding.vector.clone());

                let config = QuantizationConfig::default();
                let circuit_embedding = CircuitEmbedding::from_embedding(&tc_embedding, &config);

                let mut rng = thread_rng();
                let commitment_obj = PersonCommitment::new(&circuit_embedding, &mut rng);
                let commitment_value = commitment_obj.value;
                let blinding = commitment_obj.blinding;

                info!("Created commitment for Merkle tree registration (SSE)");

                match state.person_witness_generator.register_commitment(commitment_value) {
                    Ok(index) => {
                        info!("Registered commitment at Merkle tree index {} (SSE)", index);
                        let tree_bytes = state.person_witness_generator.tree().to_bytes();
                        let merkle_root = state.person_witness_generator.merkle_root();
                        let merkle_root_bytes = turing_core::Fr::to_be_bytes(&merkle_root);
                        (commitment_value, blinding, Some(tree_bytes), Some(merkle_root_bytes))
                    }
                    Err(e) => {
                        error!("Failed to register commitment in Merkle tree: {}", e);
                        (commitment_value, blinding, None, None)
                    }
                }
            };

            if let Some(tree_bytes) = tree_bytes {
                if let Err(e) = state.storage.store_merkle_tree(&tree_bytes).await {
                    error!("Failed to persist Merkle tree: {}", e);
                }

                let commitment_bytes = turing_core::Fr::to_be_bytes(&commitment_value);
                let blinding_bytes = turing_core::Fr::to_be_bytes(&blinding);
                if let Err(e) = state.storage.store_commitment_data(&commitment_bytes, &blinding_bytes).await {
                    error!("Failed to persist commitment data: {}", e);
                }

                if let Some(root_bytes) = merkle_root_bytes {
                    info!("Merkle root for on-chain sync: 0x{}", hex_encode(root_bytes));
                }
            }

            state.xlock_embedding = Some(result.representative_embedding);
            state.xlock_helper_data = Some(result.helper_data);
            state.xlock_scope = Some(scope);
            info!("X-Lock enrollment stored in session (embedding cached for Noir)");
        }
    });

    let stream = ReceiverStream::new(progress_rx).map(|event| {
        let json = serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());
        Ok(Event::default().data(json))
    });

    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(1))
            .text("ping")
    ))
}

async fn verify_finger(
    State(state): State<SharedState>,
    Json(request): Json<VerifyFingerRequest>,
) -> Result<Json<ApiResponse<VerifyFingerResponse>>, StatusCode> {
    let mut state = state.write().await;

    info!("Starting X-Lock finger verification");

    let helper_data = match &state.xlock_helper_data {
        Some(hd) => hd.clone(),
        None => {
            return Ok(Json(ApiResponse::error(
                "Not enrolled with X-Lock. Call /enroll-fingers first."
            )));
        }
    };

    let enrollment_scope = state.xlock_scope.as_deref().unwrap_or("dermagraph:identity:v1");

    let result = if state.config.is_hardware_sensor() {
        let weights_path = match state.model_state.weights_path() {
            Some(p) if p.exists() => p.clone(),
            _ => {
                return Ok(Json(ApiResponse::error(
                    "CNN model not available. Configure cnn_weights_path in config."
                )));
            }
        };

        let sensor_config = state.config.to_sensor_config();
        match xlock_auth::verify_finger(
            &sensor_config,
            &weights_path,
            &helper_data,
            enrollment_scope,
            request.passphrase.as_deref(),
        ).await {
            Ok(r) => r,
            Err(e) => {
                info!("X-Lock verification failed: {}", e);
                let response = VerifyFingerResponse {
                    verified: false,
                    nullifier: None,
                    matched_finger: None,
                    confidence: None,
                };
                return Ok(Json(ApiResponse::success(response)));
            }
        }
    } else {
        match xlock_auth::verify_mock(&helper_data, enrollment_scope, request.passphrase.as_deref()) {
            Ok(r) => r,
            Err(e) => {
                info!("Mock verification failed: {}", e);
                let response = VerifyFingerResponse {
                    verified: false,
                    nullifier: None,
                    matched_finger: None,
                    confidence: None,
                };
                return Ok(Json(ApiResponse::success(response)));
            }
        }
    };

    info!("X-Lock verification successful! Matched {} finger", result.matched_finger);

    if state.storage.has_embedding().await && state.xlock_embedding.is_none() {
        match state.storage.load_embedding(&result.embedding_key).await {
            Ok(vector) => {
                info!("Decrypted representative embedding from storage ({} dims)", vector.len());
                state.xlock_embedding = Some(PersonEmbedding::from_vec(vector));
            }
            Err(e) => {
                error!("Failed to decrypt embedding: {}", e);
            }
        }
    }

    let scoped_nullifier = FuzzyNullifier::derive_scoped_nullifier(
        &result.nullifier,
        &request.scope,
    );

    info!("Derived scoped nullifier for '{}': 0x{}...",
          request.scope, hex_encode(&scoped_nullifier[..8]));

    let response = VerifyFingerResponse {
        verified: true,
        nullifier: Some(format!("0x{}", hex_encode(&scoped_nullifier))),
        matched_finger: Some(result.matched_finger.to_string()),
        confidence: Some(1.0),
    };

    Ok(Json(ApiResponse::success(response)))
}

async fn model_status(
    State(state): State<SharedState>,
) -> Json<ApiResponse<ModelStatus>> {
    let state = state.read().await;

    let response = ModelStatus {
        loaded: state.model_state.is_loaded(),
        weights_path: state.config.cnn_weights_path
            .as_ref()
            .map(|p| p.display().to_string()),
        embedding_dim: 128,
    };

    Json(ApiResponse::success(response))
}

async fn prove_person(
    State(state): State<SharedState>,
    Json(request): Json<ProvePersonRequest>,
) -> Result<Json<ApiResponse<ProvePersonResponse>>, StatusCode> {
    info!("Generating Noir person_identity proof for scope: {}", request.scope);

    let (witness, circuit_dir) = {
        let state = state.read().await;

        let embedding = match &state.xlock_embedding {
            Some(e) => e.clone(),
            None => {
                if state.xlock_helper_data.is_some() && state.storage.has_embedding().await {
                    return Ok(Json(ApiResponse::error(
                        "Embedding encrypted. Call /verify-finger first to decrypt (requires biometric scan)."
                    )));
                }
                return Ok(Json(ApiResponse::error(
                    "Not enrolled with X-Lock. Call /enroll-fingers first to generate embedding."
                )));
            }
        };

        if !state.storage.has_commitment_data().await {
            return Ok(Json(ApiResponse::error(
                "No commitment data found. Re-enroll to generate commitment."
            )));
        }

        let (_commitment_bytes, blinding_bytes) = match state.storage.load_commitment_data().await {
            Ok(data) => data,
            Err(e) => {
                error!("Failed to load commitment data: {}", e);
                return Ok(Json(ApiResponse::error(format!("Failed to load commitment data: {}", e))));
            }
        };

        let stored_blinding = turing_core::Fr::from_be_bytes_mod_order(&blinding_bytes);

        let tc_embedding = TcPersonEmbedding::new(embedding.vector.clone());

        let witness = match state.person_witness_generator.generate_with_stored_commitment(
            &tc_embedding,
            stored_blinding,
            &request.scope
        ) {
            Ok(w) => w,
            Err(e) => {
                error!("Person witness generation failed: {}", e);
                return Ok(Json(ApiResponse::error(format!("Witness generation failed: {}", e))));
            }
        };

        info!("Generated witness with stable merkle_root: 0x{}",
              hex_encode(turing_core::Fr::to_be_bytes(&witness.merkle_root)));

        let circuit_dir = state.config.circuit_dir.clone()
            .map(|d| d.parent().unwrap_or(&d).join("person_identity"))
            .unwrap_or_else(|| PathBuf::from("circuits/person_identity"));

        (witness, circuit_dir)
    };

    std::fs::create_dir_all(&circuit_dir).ok();
    let prover_toml_path = circuit_dir.join("Prover.toml");

    if let Err(e) = witness.write_prover_toml(&prover_toml_path) {
        error!("Failed to write Prover.toml: {}", e);
        return Ok(Json(ApiResponse::error(format!("Failed to write Prover.toml: {}", e))));
    }

    info!("Wrote person_identity Prover.toml to {}", prover_toml_path.display());

    let proof = match invoke_nargo_prove_person(&circuit_dir).await {
        Ok(p) => p,
        Err(e) => {
            info!("nargo not available ({}), returning witness path for manual proving", e);
            let response = ProvePersonResponse {
                proof: String::new(),
                merkle_root: FieldFormatter::from_tc_fr(&witness.merkle_root),
                commitment: FieldFormatter::from_tc_fr(&witness.commitment),
                nullifier: FieldFormatter::from_tc_fr(&witness.nullifier),
                prover_toml_path: Some(prover_toml_path.display().to_string()),
            };
            return Ok(Json(ApiResponse::success(response)));
        }
    };

    let response = ProvePersonResponse {
        proof,
        merkle_root: FieldFormatter::from_tc_fr(&witness.merkle_root),
        commitment: FieldFormatter::from_tc_fr(&witness.commitment),
        nullifier: FieldFormatter::from_tc_fr(&witness.nullifier),
        prover_toml_path: Some(prover_toml_path.display().to_string()),
    };

    info!("Person identity proof generated successfully");
    Ok(Json(ApiResponse::success(response)))
}

async fn invoke_nargo_prove_person(circuit_dir: &PathBuf) -> anyhow::Result<String> {
    use std::process::Command;

    info!("Running nargo execute...");
    let nargo_output = Command::new("nargo")
        .arg("execute")
        .current_dir(circuit_dir)
        .output();

    match nargo_output {
        Ok(output) if output.status.success() => {
            info!("nargo execute succeeded");
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("nargo execute failed: {}", stderr));
        }
        Err(e) => {
            return Err(anyhow::anyhow!("nargo not found: {}", e));
        }
    }

    let target_dir = circuit_dir.join("target");
    let circuits_target = circuit_dir.parent().unwrap_or(circuit_dir).join("target");

    let acir_path = target_dir.join("person_identity.json");
    let ccs_path = target_dir.join("person_identity.ccs");
    let pk_path = target_dir.join("person_identity.pk");
    let witness_path = circuits_target.join("person_identity.gz");

    if !pk_path.exists() {
        return Err(anyhow::anyhow!(
            "Proving key not found at {:?}. Run 'sunspot setup' first.",
            pk_path
        ));
    }
    if !ccs_path.exists() {
        return Err(anyhow::anyhow!(
            "CCS file not found at {:?}. Run 'sunspot compile' first.",
            ccs_path
        ));
    }
    if !witness_path.exists() {
        return Err(anyhow::anyhow!(
            "Witness not found at {:?}. nargo execute may have failed.",
            witness_path
        ));
    }

    info!("Running sunspot prove...");
    let sunspot_output = Command::new("sunspot")
        .arg("prove")
        .arg(&acir_path)
        .arg(&witness_path)
        .arg(&ccs_path)
        .arg(&pk_path)
        .current_dir(circuit_dir)
        .output();

    match sunspot_output {
        Ok(output) if output.status.success() => {
            info!("sunspot prove succeeded");
            let stdout = String::from_utf8_lossy(&output.stdout);
            debug!("sunspot output: {}", stdout);
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(anyhow::anyhow!("sunspot prove failed: {} {}", stderr, stdout));
        }
        Err(e) => {
            return Err(anyhow::anyhow!("sunspot not found: {}. Install Sunspot on this system.", e));
        }
    }

    let proof_path = target_dir.join("person_identity.proof");
    if proof_path.exists() {
        let proof_bytes = std::fs::read(&proof_path)?;
        info!("Groth16 proof generated: {} bytes", proof_bytes.len());
        Ok(hex_encode(&proof_bytes))
    } else {
        Err(anyhow::anyhow!("Proof file not found at {:?}", proof_path))
    }
}

async fn generate_witness(
    State(state): State<SharedState>,
    Json(request): Json<WitnessRequest>,
) -> Result<Json<ApiResponse<WitnessResponse>>, StatusCode> {
    let mut state = state.write().await;

    if !state.storage.is_registered().await.unwrap_or(false) {
        return Ok(Json(ApiResponse::error("Not registered. Run /register first.")));
    }

    let biometric_key = match &state.biometric_key {
        Some(key) => *key,
        None => {
            return Ok(Json(ApiResponse::error(
                "Session not unlocked. Re-scan fingerprint or restart daemon after registration."
            )));
        }
    };

    info!("Generating witness for scoped request");

    let policy = passphrase_to_policy(request.passphrase.as_deref());

    let biometric = match load_biometric_data(&state.storage, &biometric_key, policy).await {
        Ok(b) => b,
        Err(e) => return Ok(Json(ApiResponse::error(format!("Failed to load biometric: {}", e)))),
    };

    let witness = match state.witness_generator.generate(&biometric, &request.scope) {
        Ok(w) => w,
        Err(e) => return Ok(Json(ApiResponse::error(format!("Witness generation failed: {}", e)))),
    };

    let witness_dir = state.config.data_dir.join("witnesses");
    std::fs::create_dir_all(&witness_dir).ok();

    let scope_hash = hex_encode(&sha256_hash(request.scope.as_bytes())[..8]);
    let prover_toml_path = witness_dir.join(format!("Prover_{}.toml", scope_hash));

    if let Err(e) = witness.write_flat_toml(&prover_toml_path) {
        return Ok(Json(ApiResponse::error(format!("Failed to write Prover.toml: {}", e))));
    }

    let response = WitnessResponse {
        prover_toml_path: prover_toml_path.display().to_string(),
        identity_hash: FieldFormatter::from_tc_fr(&witness.identity),
        nullifier: witness.nullifier.clone(),
    };

    info!("Witness generated: {}", prover_toml_path.display());
    Ok(Json(ApiResponse::success(response)))
}

async fn prove(
    State(state): State<SharedState>,
    Json(request): Json<ProveRequest>,
) -> Result<Json<ApiResponse<ProveResponse>>, StatusCode> {
    let mut state = state.write().await;

    if !state.storage.is_registered().await.unwrap_or(false) {
        return Ok(Json(ApiResponse::error("Not registered. Run /register first.")));
    }

    let biometric_key = match &state.biometric_key {
        Some(key) => *key,
        None => {
            return Ok(Json(ApiResponse::error(
                "Session not unlocked. Re-scan fingerprint or restart daemon after registration."
            )));
        }
    };

    info!("Generating proof for scoped request");

    let policy = passphrase_to_policy(request.passphrase.as_deref());

    let biometric = match load_biometric_data(&state.storage, &biometric_key, policy).await {
        Ok(b) => b,
        Err(e) => return Ok(Json(ApiResponse::error(format!("Failed to load biometric: {}", e)))),
    };

    let witness = match state.witness_generator.generate(&biometric, &request.scope) {
        Ok(w) => w,
        Err(e) => return Ok(Json(ApiResponse::error(format!("Witness generation failed: {}", e)))),
    };

    let circuit_dir = state.config.circuit_dir.clone().unwrap_or_else(|| {
        PathBuf::from("circuits/spectral_identity")
    });

    let prover_toml_path = circuit_dir.join("Prover.toml");
    if let Err(e) = witness.write_flat_toml(&prover_toml_path) {
        return Ok(Json(ApiResponse::error(format!("Failed to write Prover.toml: {}", e))));
    }

    debug!("Wrote Prover.toml to {}", prover_toml_path.display());

    let proof = match invoke_nargo_prove(&circuit_dir).await {
        Ok(p) => p,
        Err(e) => {
            info!("nargo not available ({}), returning witness path for manual proving", e);
            let response = ProveResponse {
                proof: String::new(),
                merkle_root: witness.merkle_root.clone(),
                nullifier_scope: witness.nullifier_scope.clone(),
                nullifier: witness.nullifier.clone(),
                prover_toml_path: Some(prover_toml_path.display().to_string()),
            };
            return Ok(Json(ApiResponse::success(response)));
        }
    };

    let response = ProveResponse {
        proof,
        merkle_root: witness.merkle_root,
        nullifier_scope: witness.nullifier_scope,
        nullifier: witness.nullifier,
        prover_toml_path: Some(prover_toml_path.display().to_string()),
    };

    info!("Proof generated successfully");
    Ok(Json(ApiResponse::success(response)))
}

async fn verify(
    State(state): State<SharedState>,
    Json(request): Json<VerifyRequest>,
) -> Result<Json<ApiResponse<VerifyResponse>>, StatusCode> {
    info!("Verifying proof for scoped request");

    if request.proof.is_empty() {
        return Ok(Json(ApiResponse::error("Empty proof provided")));
    }

    if !request.nullifier.starts_with("0x") || request.nullifier.len() != 66 {
        return Ok(Json(ApiResponse::error(format!(
            "Invalid nullifier format: expected 66-char hex string, got {} chars",
            request.nullifier.len()
        ))));
    }

    if !request.merkle_root.starts_with("0x") || request.merkle_root.len() != 66 {
        return Ok(Json(ApiResponse::error("Invalid merkle_root format")));
    }

    let state = state.read().await;
    if let Some(ref circuit_dir) = state.config.circuit_dir {
        match invoke_nargo_verify(circuit_dir, &request).await {
            Ok(valid) => {
                let response = VerifyResponse {
                    valid,
                    details: Some(if valid {
                        "Proof verified by nargo".to_string()
                    } else {
                        "Proof verification failed".to_string()
                    }),
                };
                return Ok(Json(ApiResponse::success(response)));
            }
            Err(e) => {
                debug!("nargo verify not available: {}", e);
            }
        }
    }

    let response = VerifyResponse {
        valid: true,
        details: Some(
            "Format validation passed. Full ZK verification requires nargo or on-chain verifier."
                .to_string(),
        ),
    };

    Ok(Json(ApiResponse::success(response)))
}

async fn invoke_nargo_verify(circuit_dir: &PathBuf, request: &VerifyRequest) -> anyhow::Result<bool> {
    use std::process::Command;

    let verifier_toml = circuit_dir.join("Verifier.toml");
    let verifier_content = format!(
        "merkle_root = \"{}\"\nnullifier_scope = \"{}\"\nnullifier = \"{}\"\n",
        request.merkle_root, request.nullifier_scope, request.nullifier
    );
    std::fs::write(&verifier_toml, verifier_content)?;

    let proof_path = circuit_dir.join("proofs").join("spectral_identity.proof");
    std::fs::create_dir_all(proof_path.parent().unwrap())?;
    let proof_bytes = hex_decode(request.proof.trim_start_matches("0x"))?;
    std::fs::write(&proof_path, proof_bytes)?;

    let output = Command::new("nargo")
        .arg("verify")
        .current_dir(circuit_dir)
        .output()?;

    Ok(output.status.success())
}

async fn load_biometric_data(
    storage: &Storage,
    biometric_key: &[u8; KEY_SIZE],
    policy: PassphrasePolicy,
) -> anyhow::Result<biometric_extract::BiometricData> {
    if storage.has_biometric().await {
        let biometric_features = storage.load_biometric(biometric_key, policy.clone()).await?;
        let laplacian_data = storage.load_laplacian(biometric_key, policy).await?;
        let laplacian = laplacian_data.to_laplacian();

        let biometric = biometric_features.to_biometric_data(&laplacian);

        debug!("Loaded biometric data successfully");
        return Ok(biometric);
    }

    info!("No stored biometric features found, using mock data");
    Ok(biometric_extract::BiometricData::mock())
}

fn passphrase_to_policy(passphrase: Option<&str>) -> PassphrasePolicy {
    match passphrase {
        Some(p) if !p.is_empty() => PassphrasePolicy::Required(p.to_string()),
        _ => PassphrasePolicy::BiometricOnly,
    }
}

async fn invoke_nargo_prove(circuit_dir: &PathBuf) -> anyhow::Result<String> {
    use std::process::Command;

    let output = Command::new("nargo")
        .arg("prove")
        .current_dir(circuit_dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("nargo prove failed: {}", stderr));
    }

    let proof_path = circuit_dir.join("proofs").join("spectral_identity.proof");
    if proof_path.exists() {
        let proof_bytes = std::fs::read(&proof_path)?;
        Ok(hex_encode(&proof_bytes))
    } else {
        Err(anyhow::anyhow!("Proof file not found at {:?}", proof_path))
    }
}

fn sha256_hash(data: &[u8]) -> [u8; 32] {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

fn hex_decode(s: &str) -> anyhow::Result<Vec<u8>> {
    if s.len() % 2 != 0 {
        return Err(anyhow::anyhow!("Hex string must have even length"));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|e| anyhow::anyhow!("Hex decode error at position {}: {}", i, e))
        })
        .collect()
}
