use criterion::BenchmarkId;
use criterion::Criterion;
use criterion::Throughput;
use criterion::criterion_group;
use criterion::criterion_main;
use crawfish::CompilerContext;
use crawfish::bench_tokenize;

fn bench_tokenizer(c: &mut Criterion) {
    let fixtures = [
        ("small", include_str!("fixtures/small.crw")),
        ("medium", include_str!("fixtures/medium.crw")),
        ("large", include_str!("fixtures/large.crw")),
    ];

    let mut group = c.benchmark_group("find capacity to pre-allocate");

    for (fixture_name, source) in fixtures {
        let token_count = bench_tokenize(source, &mut CompilerContext::new(), 0) as u64;

        let caps: &[(&str, usize)] = &[
            ("none", 0),
            ("len/8", source.len() / 8),
            ("len/4", source.len() / 4),
            ("len/3", source.len() / 3),
            ("len/2", source.len() / 2),
            ("len", source.len()),
        ];

        for (cap_name, cap) in caps.iter().copied() {
            group.throughput(Throughput::Elements(token_count));
            group.bench_function(
                BenchmarkId::new(fixture_name, cap_name),
                |b| {
                    let mut ctx = CompilerContext::new();
                    b.iter(|| bench_tokenize(source, &mut ctx, cap));
                },
            );
        }
    }

    group.finish();
}

criterion_group!(benches, bench_tokenizer);
criterion_main!(benches);
