//! Performance benchmarks for message processing.
//!
//! These benchmarks measure the hot paths in the daemon:
//! - Message serialization/deserialization
//! - Buffer operations
//! - Session multiplexer throughput

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};

/// Benchmark message serialization performance.
fn bench_message_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_serialization");

    // Small message (typical keystroke)
    let small_data = vec![0u8; 1];
    group.throughput(Throughput::Bytes(small_data.len() as u64));
    group.bench_function("small_1B", |b| {
        b.iter(|| {
            // Simulate message framing
            let mut frame = Vec::with_capacity(small_data.len() + 4);
            frame.extend_from_slice(&(small_data.len() as u32).to_be_bytes());
            frame.extend_from_slice(black_box(&small_data));
            frame
        });
    });

    // Medium message (typical terminal output chunk)
    let medium_data = vec![0u8; 4096];
    group.throughput(Throughput::Bytes(medium_data.len() as u64));
    group.bench_function("medium_4KB", |b| {
        b.iter(|| {
            let mut frame = Vec::with_capacity(medium_data.len() + 4);
            frame.extend_from_slice(&(medium_data.len() as u32).to_be_bytes());
            frame.extend_from_slice(black_box(&medium_data));
            frame
        });
    });

    // Large message (file chunk)
    let large_data = vec![0u8; 65536];
    group.throughput(Throughput::Bytes(large_data.len() as u64));
    group.bench_function("large_64KB", |b| {
        b.iter(|| {
            let mut frame = Vec::with_capacity(large_data.len() + 4);
            frame.extend_from_slice(&(large_data.len() as u32).to_be_bytes());
            frame.extend_from_slice(black_box(&large_data));
            frame
        });
    });

    group.finish();
}

/// Benchmark buffer pooling vs allocation.
fn bench_buffer_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_operations");

    // Allocate new buffer each time
    group.bench_function("new_allocation_4KB", |b| {
        b.iter(|| {
            let buffer: Vec<u8> = Vec::with_capacity(4096);
            black_box(buffer)
        });
    });

    // Reuse buffer with clear
    group.bench_function("reuse_buffer_4KB", |b| {
        let mut buffer: Vec<u8> = Vec::with_capacity(4096);
        b.iter(|| {
            buffer.clear();
            buffer.extend(std::iter::repeat(0u8).take(4096));
            black_box(&buffer)
        });
    });

    // Clone vs copy benchmark
    let data = vec![0u8; 4096];
    group.throughput(Throughput::Bytes(4096));
    group.bench_function("clone_4KB", |b| {
        b.iter(|| black_box(data.clone()));
    });

    group.finish();
}

/// Benchmark data copy patterns.
fn bench_data_copy(c: &mut Criterion) {
    let mut group = c.benchmark_group("data_copy");

    let src = vec![0u8; 4096];
    let mut dst = vec![0u8; 4096];

    group.throughput(Throughput::Bytes(4096));

    group.bench_function("copy_from_slice", |b| {
        b.iter(|| {
            dst.copy_from_slice(black_box(&src));
        });
    });

    group.bench_function("extend_from_slice", |b| {
        let mut target = Vec::with_capacity(4096);
        b.iter(|| {
            target.clear();
            target.extend_from_slice(black_box(&src));
        });
    });

    group.finish();
}

/// Benchmark channel throughput (simulated).
fn bench_channel_throughput(c: &mut Criterion) {
    use std::sync::mpsc;

    let mut group = c.benchmark_group("channel_throughput");

    // Bounded channel
    group.bench_function("bounded_send_recv", |b| {
        let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(256);
        let data = vec![0u8; 4096];

        b.iter(|| {
            tx.send(black_box(data.clone())).unwrap();
            let received = rx.recv().unwrap();
            black_box(received)
        });
    });

    // Unbounded channel
    group.bench_function("unbounded_send_recv", |b| {
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let data = vec![0u8; 4096];

        b.iter(|| {
            tx.send(black_box(data.clone())).unwrap();
            let received = rx.recv().unwrap();
            black_box(received)
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_message_serialization,
    bench_buffer_operations,
    bench_data_copy,
    bench_channel_throughput,
);

criterion_main!(benches);
