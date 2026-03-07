use criterion::{criterion_group, criterion_main, Criterion};
use engram_core::graph::{Graph, Provenance};
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

fn bench_fulltext_search(c: &mut Criterion) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bench.brain");
    let prov = Provenance::user("bench");
    let mut g = Graph::create_with_capacity(&path, 10_000, 1).unwrap();

    let labels = [
        "postgresql-primary", "postgresql-replica", "nginx-proxy",
        "redis-cache", "elasticsearch-node", "kafka-broker",
        "rabbitmq-server", "mongodb-shard", "mysql-replica",
        "grafana-dashboard",
    ];

    for i in 0..1_000 {
        let label = format!("{}-{i}", labels[i % labels.len()]);
        g.store(&label, &prov).unwrap();
    }

    c.bench_function("fulltext_search_1k", |b| {
        b.iter(|| {
            let _results = g.search_text("postgresql", 10).unwrap();
        });
    });
}

fn bench_query_confidence(c: &mut Criterion) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("bench.brain");
    let prov = Provenance::user("bench");
    let mut g = Graph::create_with_capacity(&path, 10_000, 1).unwrap();

    for i in 0..1_000 {
        let conf = (i as f32) / 1000.0;
        g.store_with_confidence(&format!("node-{i}"), conf, &prov).unwrap();
    }

    c.bench_function("query_confidence_filter_1k", |b| {
        b.iter(|| {
            let _results = g.search("confidence>0.8", 10).unwrap();
        });
    });
}

criterion_group!(
    benches,
    bench_node_store,
    bench_node_read,
    bench_node_find_by_label,
    bench_fulltext_search,
    bench_query_confidence
);
criterion_main!(benches);
