//! Benchmarks for the Kōdo lexer.
//!
//! Measures tokenization throughput (tokens/second and bytes/second).

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::hint::black_box;

/// A sample Kōdo source file for benchmarking.
const SAMPLE_SOURCE: &str = r#"module benchmark {
    meta {
        purpose: "A benchmark sample with multiple functions",
        version: "1.0.0",
        author: "bench"
    }

    fn add(a: Int, b: Int) -> Int {
        return a + b
    }

    fn multiply(a: Int, b: Int) -> Int {
        return a * b
    }

    fn factorial(n: Int) -> Int {
        if n <= 1 {
            return 1
        }
        return n * factorial(n - 1)
    }

    fn main() {
        let x: Int = add(1, 2)
        let y: Int = multiply(3, 4)
        let z: Int = factorial(10)
    }
}
"#;

fn bench_tokenize(c: &mut Criterion) {
    let mut group = c.benchmark_group("lexer");
    group.throughput(Throughput::Bytes(SAMPLE_SOURCE.len() as u64));

    group.bench_function("tokenize_sample", |b| {
        b.iter(|| kodo_lexer::tokenize(black_box(SAMPLE_SOURCE)));
    });

    // Benchmark with a larger input (10x repeated)
    let large_source = SAMPLE_SOURCE.repeat(10);
    group.throughput(Throughput::Bytes(large_source.len() as u64));
    group.bench_function("tokenize_large", |b| {
        b.iter(|| kodo_lexer::tokenize(black_box(&large_source)));
    });

    group.finish();
}

criterion_group!(benches, bench_tokenize);
criterion_main!(benches);
