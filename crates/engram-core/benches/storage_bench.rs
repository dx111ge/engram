use criterion::{criterion_group, criterion_main, Criterion};
use engram_core::BrainFile;
use tempfile::TempDir;

fn bench_node_store(c: &mut Criterion) {
    c.bench_function("store_node", |b| {
        b.iter_custom(|iters| {
            let dir = TempDir::new().unwrap();
            let path = dir.path().join("bench.brain");
            let mut brain =
                BrainFile::create_with_capacity(&path, iters + 1, 1).unwrap();

            let start = std::time::Instant::now();
            for _ in 0..iters {
                brain.store_node("bench-node").unwrap();
            }
            start.elapsed()
        });
    });
}

fn bench_node_read(c: &mut Criterion) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bench.brain");
    let mut brain = BrainFile::create_with_capacity(&path, 100_000, 1).unwrap();

    for i in 0..10_000 {
        brain.store_node(&format!("node-{i}")).unwrap();
    }

    c.bench_function("read_node_by_slot", |b| {
        let mut slot = 0u64;
        b.iter(|| {
            let _node = brain.read_node(slot).unwrap();
            slot = (slot + 1) % 10_000;
        });
    });
}

fn bench_node_find_by_label(c: &mut Criterion) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bench.brain");
    let mut brain = BrainFile::create_with_capacity(&path, 10_000, 1).unwrap();

    for i in 0..1_000 {
        brain.store_node(&format!("node-{i}")).unwrap();
    }

    c.bench_function("find_node_by_label_1k", |b| {
        b.iter(|| {
            let _result = brain.find_node_by_label("node-500").unwrap();
        });
    });
}

criterion_group!(benches, bench_node_store, bench_node_read, bench_node_find_by_label);
criterion_main!(benches);
