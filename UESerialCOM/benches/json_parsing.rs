use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};

fn generate_sensor_json(num_points: usize) -> String {
    let mut json = String::from("{");
    for i in 1..=num_points {
        if i > 1 {
            json.push(',');
        }
        json.push_str(&format!(
            "\"Sx{}\":\"100.5\",\"Sy{}\":\"200.3\",\"Sc{}\":\"0.8\"",
            i, i, i
        ));
    }
    json.push('}');
    json
}

fn bench_json_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_parsing");

    for num_points in [10, 21, 50, 100].iter() {
        let json = generate_sensor_json(*num_points);

        group.bench_with_input(
            BenchmarkId::new("fast_parser", num_points),
            &json,
            |b, json| {
                b.iter(|| {
                    // 这里需要实际的解析器实现
                    black_box(json.len())
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("serde_parser", num_points),
            &json,
            |b, json| {
                b.iter(|| {
                    let _: serde_json::Value = serde_json::from_str(json).unwrap();
                    black_box(())
                });
            },
        );
    }

    group.finish();
}

fn bench_json_extraction(c: &mut Criterion) {
    let json = generate_sensor_json(21);

    c.bench_function("find_json_object", |b| {
        let buffer = format!("garbage{}", json).into_bytes();
        b.iter(|| {
            let buffer_str = std::str::from_utf8(&buffer).unwrap();
            let start = buffer_str.find('{');
            let end = buffer_str.rfind('}');
            black_box((start, end))
        });
    });
}

criterion_group!(benches, bench_json_parsing, bench_json_extraction);
criterion_main!(benches);
