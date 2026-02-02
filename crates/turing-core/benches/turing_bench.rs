
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use turing_core::{Fr, GraphLaplacian, MorphogenState, TuringHash, TuringParams};

fn bench_turing_hash(c: &mut Criterion) {
    let x = vec![100.0, 150.0, 200.0, 250.0, 180.0];
    let y = vec![100.0, 120.0, 180.0, 160.0, 220.0];
    let theta = vec![0.0, 0.5, 1.0, 1.5, 2.0];

    let initial = MorphogenState::from_biometric(&x, &y, &theta);
    let laplacian = GraphLaplacian::from_edges(5, &[
        (0, 1, Fr::from_u64(100)),
        (1, 2, Fr::from_u64(100)),
        (2, 3, Fr::from_u64(100)),
        (3, 4, Fr::from_u64(100)),
        (0, 4, Fr::from_u64(50)),
    ], true);
    let params = TuringParams::crypto();

    c.bench_function("turing_hash_5_nodes", |b| {
        b.iter(|| {
            TuringHash::compute(
                black_box(&initial),
                black_box(&laplacian),
                black_box(&params),
            )
        })
    });
}

fn bench_turing_hash_large(c: &mut Criterion) {
    let n = 20;
    let x: Vec<f64> = (0..n).map(|i| 100.0 + i as f64 * 10.0).collect();
    let y: Vec<f64> = (0..n).map(|i| 100.0 + (i as f64 * 0.5).sin() * 50.0).collect();
    let theta: Vec<f64> = (0..n).map(|i| i as f64 * 0.3).collect();

    let initial = MorphogenState::from_biometric(&x, &y, &theta);

    let mut edges = Vec::new();
    for i in 0..n {
        for j in (i+1)..n {
            if (j - i) <= 3 {
                edges.push((i, j, Fr::from_u64(100 / (j - i) as u64)));
            }
        }
    }

    let laplacian = GraphLaplacian::from_edges(n, &edges, true);
    let params = TuringParams::crypto();

    c.bench_function("turing_hash_20_nodes", |b| {
        b.iter(|| {
            TuringHash::compute(
                black_box(&initial),
                black_box(&laplacian),
                black_box(&params),
            )
        })
    });
}

criterion_group!(benches, bench_turing_hash, bench_turing_hash_large);
criterion_main!(benches);
