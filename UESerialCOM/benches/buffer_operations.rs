use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};

fn bench_buffer_push(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_push");

    for size in [64, 256, 1024, 4096].iter() {
        let data = vec![0u8; *size];

        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &data,
            |b, data| {
                b.iter(|| {
                    let mut buffer = Vec::with_capacity(8192);
                    buffer.extend_from_slice(data);
                    black_box(buffer)
                });
            },
        );
    }

    group.finish();
}

fn bench_buffer_drain(c: &mut Criterion) {
    c.bench_function("buffer_drain", |b| {
        b.iter(|| {
            let mut buffer = vec![0u8; 4096];
            buffer.drain(0..1024);
            black_box(buffer)
        });
    });
}

fn bench_buffer_overflow_handling(c: &mut Criterion) {
    c.bench_function("buffer_overflow", |b| {
        b.iter(|| {
            let mut buffer = vec![0u8; 1024 * 1024];
            let new_data = vec![0u8; 1024];

            // 模拟溢出处理
            if buffer.len() + new_data.len() > 1024 * 1024 {
                let discard = buffer.len() + new_data.len() - 512 * 1024;
                buffer.drain(0..discard);
            }
            buffer.extend_from_slice(&new_data);

            black_box(buffer)
        });
    });
}

criterion_group!(
    benches,
    bench_buffer_push,
    bench_buffer_drain,
    bench_buffer_overflow_handling
);
criterion_main!(benches);