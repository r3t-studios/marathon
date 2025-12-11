//! VectorClock performance benchmarks
//!
//! These benchmarks measure the performance improvements from the two-pass →
//! single-pass optimization in the happened_before() method.

use criterion::{
    black_box,
    criterion_group,
    criterion_main,
    BenchmarkId,
    Criterion,
};
use lib::networking::VectorClock;

/// Helper to create a vector clock with N nodes
fn create_clock_with_nodes(num_nodes: usize) -> VectorClock {
    let mut clock = VectorClock::new();

    for i in 0..num_nodes {
        let node_id = uuid::Uuid::from_u128(i as u128);
        clock.increment(node_id);
    }

    clock
}

/// Benchmark: happened_before comparison with small clocks (5 nodes)
///
/// This is the common case in typical distributed systems with a small number
/// of replicas.
fn bench_happened_before_small_clocks(c: &mut Criterion) {
    let mut group = c.benchmark_group("VectorClock::happened_before (5 nodes)");

    group.bench_function("happened_before (true)", |b| {
        let node = uuid::Uuid::from_u128(1);

        let mut clock1 = VectorClock::new();
        clock1.increment(node);

        let mut clock2 = VectorClock::new();
        clock2.increment(node);
        clock2.increment(node);

        b.iter(|| black_box(clock1.happened_before(&clock2)));
    });

    group.bench_function("happened_before (false)", |b| {
        let node1 = uuid::Uuid::from_u128(1);
        let node2 = uuid::Uuid::from_u128(2);

        let mut clock1 = VectorClock::new();
        clock1.increment(node1);

        let mut clock2 = VectorClock::new();
        clock2.increment(node2);

        b.iter(|| black_box(clock1.happened_before(&clock2)));
    });

    group.bench_function("happened_before (concurrent)", |b| {
        let node1 = uuid::Uuid::from_u128(1);
        let node2 = uuid::Uuid::from_u128(2);
        let node3 = uuid::Uuid::from_u128(3);

        let mut clock1 = create_clock_with_nodes(5);
        clock1.increment(node1);

        let mut clock2 = create_clock_with_nodes(5);
        clock2.increment(node2);
        clock2.increment(node3);

        b.iter(|| black_box(clock1.happened_before(&clock2)));
    });

    group.finish();
}

/// Benchmark: happened_before comparison with large clocks (100 nodes)
///
/// This tests the optimization's effectiveness with larger clock sizes.
/// The single-pass algorithm should show better improvements here.
fn bench_happened_before_large_clocks(c: &mut Criterion) {
    let mut group = c.benchmark_group("VectorClock::happened_before (100 nodes)");

    group.bench_function("happened_before (100 nodes, true)", |b| {
        let clock1 = create_clock_with_nodes(100);

        let mut clock2 = clock1.clone();
        let node = uuid::Uuid::from_u128(50);
        clock2.increment(node);

        b.iter(|| black_box(clock1.happened_before(&clock2)));
    });

    group.bench_function("happened_before (100 nodes, concurrent)", |b| {
        let mut clock1 = create_clock_with_nodes(100);
        let node1 = uuid::Uuid::from_u128(200);
        clock1.increment(node1);

        let mut clock2 = create_clock_with_nodes(100);
        let node2 = uuid::Uuid::from_u128(201);
        clock2.increment(node2);

        b.iter(|| black_box(clock1.happened_before(&clock2)));
    });

    group.finish();
}

/// Benchmark: happened_before with disjoint node sets
///
/// This is the scenario where early exit optimization provides the most benefit.
fn bench_happened_before_disjoint(c: &mut Criterion) {
    let mut group = c.benchmark_group("VectorClock::happened_before (disjoint)");

    for num_nodes in [5, 20, 50, 100] {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_nodes),
            &num_nodes,
            |b, &num_nodes| {
                // Create two clocks with completely different nodes
                let mut clock1 = VectorClock::new();
                for i in 0..num_nodes {
                    clock1.increment(uuid::Uuid::from_u128(i as u128));
                }

                let mut clock2 = VectorClock::new();
                for i in 0..num_nodes {
                    clock2.increment(uuid::Uuid::from_u128((i + 1000) as u128));
                }

                b.iter(|| black_box(clock1.happened_before(&clock2)));
            },
        );
    }

    group.finish();
}

/// Benchmark: merge operation
fn bench_merge(c: &mut Criterion) {
    let mut group = c.benchmark_group("VectorClock::merge");

    for num_nodes in [5, 20, 50, 100] {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_nodes),
            &num_nodes,
            |b, &num_nodes| {
                b.iter_batched(
                    || {
                        // Setup: Create two clocks with overlapping nodes
                        let clock1 = create_clock_with_nodes(num_nodes);
                        let clock2 = create_clock_with_nodes(num_nodes / 2);
                        (clock1, clock2)
                    },
                    |(mut clock1, clock2)| {
                        clock1.merge(&clock2);
                        black_box(clock1)
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark: increment operation
fn bench_increment(c: &mut Criterion) {
    let mut group = c.benchmark_group("VectorClock::increment");

    group.bench_function("increment (new node)", |b| {
        b.iter_batched(
            || {
                let clock = VectorClock::new();
                let node_id = uuid::Uuid::new_v4();
                (clock, node_id)
            },
            |(mut clock, node_id)| {
                black_box(clock.increment(node_id));
                clock
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function("increment (existing node)", |b| {
        b.iter_batched(
            || {
                let node_id = uuid::Uuid::new_v4();
                let mut clock = VectorClock::new();
                clock.increment(node_id);
                (clock, node_id)
            },
            |(mut clock, node_id)| {
                black_box(clock.increment(node_id));
                clock
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.finish();
}

/// Benchmark: is_concurrent_with operation
fn bench_is_concurrent(c: &mut Criterion) {
    let mut group = c.benchmark_group("VectorClock::is_concurrent_with");

    group.bench_function("concurrent (5 nodes)", |b| {
        let node1 = uuid::Uuid::from_u128(1);
        let node2 = uuid::Uuid::from_u128(2);

        let mut clock1 = VectorClock::new();
        clock1.increment(node1);

        let mut clock2 = VectorClock::new();
        clock2.increment(node2);

        b.iter(|| black_box(clock1.is_concurrent_with(&clock2)));
    });

    group.bench_function("not concurrent (ordered)", |b| {
        let node = uuid::Uuid::from_u128(1);

        let mut clock1 = VectorClock::new();
        clock1.increment(node);

        let mut clock2 = VectorClock::new();
        clock2.increment(node);
        clock2.increment(node);

        b.iter(|| black_box(clock1.is_concurrent_with(&clock2)));
    });

    group.finish();
}

/// Benchmark: Realistic workload simulation
///
/// Simulates a typical distributed system workflow with increments, merges,
/// and comparisons.
fn bench_realistic_workload(c: &mut Criterion) {
    let mut group = c.benchmark_group("VectorClock::realistic_workload");

    group.bench_function("3 nodes, 100 operations", |b| {
        let node1 = uuid::Uuid::from_u128(1);
        let node2 = uuid::Uuid::from_u128(2);
        let node3 = uuid::Uuid::from_u128(3);

        b.iter(|| {
            let mut clock1 = VectorClock::new();
            let mut clock2 = VectorClock::new();
            let mut clock3 = VectorClock::new();

            // Simulate 100 operations across 3 nodes
            for i in 0..100 {
                match i % 7 {
                    0 => {
                        clock1.increment(node1);
                    }
                    1 => {
                        clock2.increment(node2);
                    }
                    2 => {
                        clock3.increment(node3);
                    }
                    3 => {
                        clock1.merge(&clock2);
                    }
                    4 => {
                        clock2.merge(&clock3);
                    }
                    5 => {
                        let _ = clock1.happened_before(&clock2);
                    }
                    _ => {
                        let _ = clock2.is_concurrent_with(&clock3);
                    }
                }
            }

            black_box((clock1, clock2, clock3))
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_happened_before_small_clocks,
    bench_happened_before_large_clocks,
    bench_happened_before_disjoint,
    bench_merge,
    bench_increment,
    bench_is_concurrent,
    bench_realistic_workload,
);
criterion_main!(benches);
