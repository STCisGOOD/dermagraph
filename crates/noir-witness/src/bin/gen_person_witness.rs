
use std::path::PathBuf;
use turing_core::person_identity::PersonEmbedding;
use noir_witness::PersonWitnessGenerator;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    let scope = args.get(1)
        .map(|s| s.as_str())
        .unwrap_or("dao-vote-2024");

    let output_path = args.get(2)
        .map(|s| PathBuf::from(s))
        .unwrap_or_else(|| PathBuf::from("Prover.toml"));

    println!("Generating person_identity witness");
    println!("  Scope: {}", scope);
    println!("  Output: {:?}", output_path);

    let embedding_values: Vec<f32> = (0..128)
        .map(|i| (i as f32 / 128.0) - 0.5)
        .collect();
    let embedding = PersonEmbedding::new(embedding_values);

    println!("  Embedding: 128-dim test vector (deterministic)");

    let mut generator = PersonWitnessGenerator::new();
    let mut rng = rand::thread_rng();

    let witness = generator.generate(&embedding, scope, &mut rng)?;

    assert!(
        !witness.merkle_root.is_zero(),
        "BUG: Merkle root should not be zero!"
    );

    witness.write_prover_toml(&output_path)?;

    println!("\nGenerated valid Prover.toml:");
    println!("  - merkle_root: {} (NOT zero!)",
        noir_witness::FieldFormatter::from_tc_fr(&witness.merkle_root));
    println!("  - commitment: registered in Merkle tree");
    println!("  - nullifier: derived from embedding + scope");

    println!("\nThe proof should now verify successfully!");

    Ok(())
}
