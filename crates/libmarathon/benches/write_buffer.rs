//! WriteBuffer performance benchmarks
//!
//! These benchmarks measure the performance improvements from the O(n²) → O(1)
//! optimization that replaced Vec::retain() with HashMap-based indexing.

use criterion::{
    BenchmarkId,
    Criterion,
    black_box,
    criterion_group,
    criterion_main,
};
use libmarathon::persistence::{
    PersistenceOp,
    WriteBuffer,
};

/// Benchmark: Add many updates to the same component (worst case for old
/// implementation)
///
/// This scenario heavily stresses the deduplication logic. In the old O(n)
/// implementation, each add() would scan the entire buffer. With HashMap
/// indexing, this is now O(1).
fn bench_repeated_updates_same_component(c: &mut Criterion) {
    let mut group = c.benchmark_group("WriteBuffer::add (same component)");

    for num_updates in [10, 100, 500, 1000] {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_updates),
            &num_updates,
            |b, &num_updates| {
                b.iter(|| {
                    let mut buffer = WriteBuffer::new(2000);
                    let entity_id = uuid::Uuid::new_v4();

                    // Apply many updates to the same (entity, component) pair
                    for i in 0..num_updates {
                        let op = PersistenceOp::UpsertComponent {
                            entity_id,
                            component_type: "Transform".to_string(),
                            data: vec![i as u8; 100], // 100 bytes per update
                        };
                        buffer.add(op).ok(); // Ignore errors for benchmarking
                    }

                    black_box(buffer);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Add updates to different components (best case, no deduplication)
///
/// This tests the performance when there's no deduplication happening.
/// Both old and new implementations should perform similarly here.
fn bench_different_components(c: &mut Criterion) {
    let mut group = c.benchmark_group("WriteBuffer::add (different components)");

    for num_components in [10, 100, 500, 1000] {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_components),
            &num_components,
            |b, &num_components| {
                b.iter(|| {
                    let mut buffer = WriteBuffer::new(2000);
                    let entity_id = uuid::Uuid::new_v4();

                    // Add different components (no deduplication)
                    for i in 0..num_components {
                        let op = PersistenceOp::UpsertComponent {
                            entity_id,
                            component_type: format!("Component{}", i),
                            data: vec![i as u8; 100],
                        };
                        buffer.add(op).ok();
                    }

                    black_box(buffer);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Mixed workload (realistic scenario)
///
/// This simulates a realistic workload with a mix of repeated updates to
/// popular components and unique component updates.
fn bench_mixed_workload(c: &mut Criterion) {
    let mut group = c.benchmark_group("WriteBuffer::add (mixed workload)");

    for num_ops in [100, 500, 1000] {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_ops),
            &num_ops,
            |b, &num_ops| {
                b.iter(|| {
                    let mut buffer = WriteBuffer::new(2000);
                    let entity_id = uuid::Uuid::new_v4();

                    // 70% updates to Transform, 20% to Material, 10% to unique components
                    for i in 0..num_ops {
                        let (component_type, data_size) = match i % 10 {
                            | 0..=6 => ("Transform".to_string(), 64), // 70%
                            | 7..=8 => ("Material".to_string(), 128), // 20%
                            | _ => (format!("Component{}", i), 32),   // 10%
                        };

                        let op = PersistenceOp::UpsertComponent {
                            entity_id,
                            component_type,
                            data: vec![i as u8; data_size],
                        };
                        buffer.add(op).ok();
                    }

                    black_box(buffer);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Entity updates (different code path)
///
/// Tests the performance of entity-level deduplication.
fn bench_entity_updates(c: &mut Criterion) {
    let mut group = c.benchmark_group("WriteBuffer::add (entity updates)");

    for num_updates in [10, 100, 500] {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_updates),
            &num_updates,
            |b, &num_updates| {
                b.iter(|| {
                    let mut buffer = WriteBuffer::new(2000);
                    let entity_id = uuid::Uuid::new_v4();

                    // Repeatedly update the same entity
                    for i in 0..num_updates {
                        let op = PersistenceOp::UpsertEntity {
                            id: entity_id,
                            data: lib::persistence::EntityData {
                                id: entity_id,
                                created_at: chrono::Utc::now(),
                                updated_at: chrono::Utc::now(),
                                entity_type: format!("Type{}", i),
                            },
                        };
                        buffer.add(op).ok();
                    }

                    black_box(buffer);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: take_operations() performance
///
/// Measures the cost of clearing the buffer and returning operations.
fn bench_take_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("WriteBuffer::take_operations");

    for buffer_size in [10, 100, 500, 1000] {
        group.bench_with_input(
            BenchmarkId::from_parameter(buffer_size),
            &buffer_size,
            |b, &buffer_size| {
                b.iter_batched(
                    || {
                        // Setup: Create a buffer with many operations
                        let mut buffer = WriteBuffer::new(2000);
                        let entity_id = uuid::Uuid::new_v4();

                        for i in 0..buffer_size {
                            let op = PersistenceOp::UpsertComponent {
                                entity_id,
                                component_type: format!("Component{}", i),
                                data: vec![i as u8; 64],
                            };
                            buffer.add(op).ok();
                        }

                        buffer
                    },
                    |mut buffer| {
                        // Benchmark: Take all operations
                        black_box(buffer.take_operations())
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_repeated_updates_same_component,
    bench_different_components,
    bench_mixed_workload,
    bench_entity_updates,
    bench_take_operations,
);
criterion_main!(benches);
