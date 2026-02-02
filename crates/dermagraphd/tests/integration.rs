
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_full_auth_flow() {
    let daemon_handle = tokio::spawn(async {
        sleep(Duration::from_secs(10)).await;
    });

    sleep(Duration::from_millis(500)).await;

    daemon_handle.abort();
}

#[tokio::test]
async fn test_nullifier_properties() {
}

#[tokio::test]
async fn test_turing_hash_determinism() {
    use turing_core::*;

    let params = TuringParams::crypto();
    let x = vec![100.0, 150.0, 200.0];
    let y = vec![100.0, 120.0, 180.0];
    let theta = vec![0.0, 0.5, 1.0];

    let state = MorphogenState::from_biometric(&x, &y, &theta);
    let laplacian = GraphLaplacian::from_edges(3, &[
        (0, 1, Fr::from_u64(100)),
        (1, 2, Fr::from_u64(100)),
    ], true);

    let hash1 = TuringHash::compute(&state, &laplacian, &params).unwrap();
    let hash2 = TuringHash::compute(&state, &laplacian, &params).unwrap();

    assert_eq!(hash1, hash2);
}
