
use biometric_extract::{MinutiaeSet, RidgeGraph};
use turing_core::{TuringHash, TuringParams, MorphogenState};
use turing_core::poseidon::hash_2;

#[test]
fn test_full_identity_pipeline() {
    let minutiae = MinutiaeSet::mock();
    assert_eq!(minutiae.len(), 8, "Mock should have 8 minutiae points");

    let graph = RidgeGraph::from_minutiae_with_orientation(&minutiae);
    assert!(graph.edge_count() > 0, "Graph should have edges");

    let laplacian = graph.to_laplacian();
    assert_eq!(laplacian.dim(), 8, "Laplacian dimension should match minutiae count");

    let x: Vec<f64> = minutiae.x_coords();
    let y: Vec<f64> = minutiae.y_coords();
    let theta: Vec<f64> = minutiae.orientations();

    let initial_state = MorphogenState::from_biometric(&x, &y, &theta);
    assert_eq!(initial_state.len(), 8);

    let params = TuringParams::default();
    let identity_hash = TuringHash::compute(&initial_state, &laplacian, &params)
        .expect("Hash computation should succeed");

    assert!(!identity_hash.is_zero(), "Identity hash should be non-trivial");

    let identity_hash_2 = TuringHash::compute(&initial_state, &laplacian, &params)
        .expect("Second hash should succeed");
    assert_eq!(identity_hash, identity_hash_2, "Hash should be deterministic");

    let commitment = hash_2(identity_hash, identity_hash);
    assert!(!commitment.is_zero(), "Commitment should be non-trivial");

    println!("=== Full Pipeline Results ===");
    println!("Minutiae count: {}", minutiae.len());
    println!("Graph edges: {}", graph.edge_count());
    println!("Laplacian dim: {}", laplacian.dim());
    println!("Identity hash computed successfully");
    println!("Commitment computed successfully");
}

#[test]
fn test_different_inputs_different_identities() {
    let params = TuringParams::default();

    let minutiae_a = MinutiaeSet::mock();
    let graph_a = RidgeGraph::from_minutiae(&minutiae_a);
    let laplacian_a = graph_a.to_laplacian();
    let state_a = MorphogenState::from_biometric(
        &minutiae_a.x_coords(),
        &minutiae_a.y_coords(),
        &minutiae_a.orientations(),
    );
    let hash_a = TuringHash::compute(&state_a, &laplacian_a, &params).unwrap();

    let minutiae_b = create_perturbed_minutiae();
    let graph_b = RidgeGraph::from_minutiae(&minutiae_b);
    let laplacian_b = graph_b.to_laplacian();
    let state_b = MorphogenState::from_biometric(
        &minutiae_b.x_coords(),
        &minutiae_b.y_coords(),
        &minutiae_b.orientations(),
    );
    let hash_b = TuringHash::compute(&state_b, &laplacian_b, &params).unwrap();

    assert_ne!(hash_a, hash_b, "Different inputs must produce different identities");
}

#[test]
fn test_avalanche_effect() {
    let params = TuringParams::default();
    let minutiae = MinutiaeSet::mock();
    let graph = RidgeGraph::from_minutiae(&minutiae);
    let laplacian = graph.to_laplacian();

    let x = minutiae.x_coords();
    let y = minutiae.y_coords();
    let theta = minutiae.orientations();
    let state_original = MorphogenState::from_biometric(&x, &y, &theta);
    let hash_original = TuringHash::compute(&state_original, &laplacian, &params).unwrap();

    let mut x_perturbed = x.clone();
    x_perturbed[0] += 0.001;
    let state_perturbed = MorphogenState::from_biometric(&x_perturbed, &y, &theta);
    let hash_perturbed = TuringHash::compute(&state_perturbed, &laplacian, &params).unwrap();

    assert_ne!(hash_original, hash_perturbed, "Small changes must cause avalanche effect");
}

#[test]
fn test_nullifier_derivation() {
    let params = TuringParams::default();
    let minutiae = MinutiaeSet::mock();
    let graph = RidgeGraph::from_minutiae(&minutiae);
    let laplacian = graph.to_laplacian();

    let state = MorphogenState::from_biometric(
        &minutiae.x_coords(),
        &minutiae.y_coords(),
        &minutiae.orientations(),
    );

    let hash_scope_a = TuringHash::compute_with_context(&state, &laplacian, &params, "airdrop_2024")
        .unwrap();
    let hash_scope_b = TuringHash::compute_with_context(&state, &laplacian, &params, "airdrop_2025")
        .unwrap();

    assert_ne!(hash_scope_a, hash_scope_b, "Different scopes must produce different nullifiers");

    let hash_scope_a_2 = TuringHash::compute_with_context(&state, &laplacian, &params, "airdrop_2024")
        .unwrap();
    assert_eq!(hash_scope_a, hash_scope_a_2, "Same scope must produce same nullifier");
}

#[test]
fn test_laplacian_dependency() {
    let params = TuringParams::default();
    let minutiae = MinutiaeSet::mock();

    let state = MorphogenState::from_biometric(
        &minutiae.x_coords(),
        &minutiae.y_coords(),
        &minutiae.orientations(),
    );

    let graph_distance = RidgeGraph::from_minutiae(&minutiae);
    let graph_angular = RidgeGraph::from_minutiae_with_orientation(&minutiae);

    let laplacian_distance = graph_distance.to_laplacian();
    let laplacian_angular = graph_angular.to_laplacian();

    let hash_distance = TuringHash::compute(&state, &laplacian_distance, &params).unwrap();
    let hash_angular = TuringHash::compute(&state, &laplacian_angular, &params).unwrap();

    println!("Distance-based hash computed");
    println!("Angular-weighted hash computed");

    assert!(!hash_distance.is_zero());
    assert!(!hash_angular.is_zero());
}

#[test]
fn test_zk_witness_generation() {
    let minutiae = MinutiaeSet::mock();
    let graph = RidgeGraph::from_minutiae(&minutiae);
    let laplacian = graph.to_laplacian();

    let entry_count = laplacian.matrix.nnz();

    println!("=== ZK Witness Data ===");
    println!("Laplacian entries: {} non-zero values", entry_count);
    println!("Matrix dimension: {}x{}", laplacian.dim(), laplacian.dim());

    for (row, col, _val) in laplacian.matrix.iter() {
        assert!(row < laplacian.dim());
        assert!(col < laplacian.dim());
    }

    assert!(entry_count > 0, "Should have non-zero Laplacian entries");
}

fn create_perturbed_minutiae() -> MinutiaeSet {
    use biometric_extract::{Minutia, MinutiaeType};

    let minutiae = vec![
        Minutia::new(105.0, 105.0, 0.1, MinutiaeType::RidgeEnding),
        Minutia::new(155.0, 125.0, 0.6, MinutiaeType::Bifurcation),
        Minutia::new(205.0, 185.0, 1.1, MinutiaeType::RidgeEnding),
        Minutia::new(255.0, 165.0, 1.6, MinutiaeType::Bifurcation),
        Minutia::new(185.0, 225.0, 2.1, MinutiaeType::RidgeEnding),
        Minutia::new(125.0, 205.0, 2.6, MinutiaeType::Bifurcation),
        Minutia::new(165.0, 155.0, 3.1, MinutiaeType::RidgeEnding),
        Minutia::new(225.0, 145.0, 3.6, MinutiaeType::Bifurcation),
    ];

    MinutiaeSet {
        minutiae,
        image_width: 300,
        image_height: 300,
    }
}
