//! Benchmarks for the Kōdo parser.
//!
//! Measures parsing throughput for small and large source files.

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::hint::black_box;

/// A small Kōdo source file (~20 lines) for benchmarking.
const SMALL_SOURCE: &str = r#"module benchmark {
    meta {
        purpose: "A benchmark sample"
        version: "1.0.0"
    }

    fn add(a: Int, b: Int) -> Int {
        return a + b
    }

    fn main() -> Int {
        let x: Int = add(1, 2)
        return x
    }
}
"#;

/// A medium Kōdo source file with structs, enums, closures, and control flow.
const MEDIUM_SOURCE: &str = r#"module benchmark {
    meta {
        purpose: "A medium benchmark with varied constructs"
        version: "1.0.0"
    }

    struct Point {
        x: Int,
        y: Int
    }

    enum Shape {
        Circle(Int),
        Rectangle(Int, Int)
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

    fn sum_range(n: Int) -> Int {
        let total: Int = 0
        for i in 0..n {
            total = total + i
        }
        return total
    }

    fn classify(x: Int) -> String {
        if x > 0 {
            return "positive"
        } else {
            if x < 0 {
                return "negative"
            } else {
                return "zero"
            }
        }
    }

    fn apply(f: (Int) -> Int, x: Int) -> Int {
        return f(x)
    }

    fn main() -> Int {
        let x: Int = add(1, 2)
        let y: Int = multiply(3, 4)
        let z: Int = factorial(10)
        let total: Int = sum_range(100)
        let label: String = classify(x)
        let doubled: Int = apply(|n: Int| -> Int { return n * 2 }, x)
        return x + y + z + total + doubled
    }
}
"#;

fn bench_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser");

    // Small file benchmark
    group.throughput(Throughput::Bytes(SMALL_SOURCE.len() as u64));
    group.bench_function("parse_small", |b| {
        b.iter(|| kodo_parser::parse(black_box(SMALL_SOURCE)));
    });

    // Medium file benchmark
    group.throughput(Throughput::Bytes(MEDIUM_SOURCE.len() as u64));
    group.bench_function("parse_medium", |b| {
        b.iter(|| kodo_parser::parse(black_box(MEDIUM_SOURCE)));
    });

    // Large file benchmark (medium source repeated 5x with unique module names)
    let large_source: String = (0..5)
        .map(|i| MEDIUM_SOURCE.replace("module benchmark", &format!("module benchmark_{i}")))
        .collect::<Vec<_>>()
        .join("\n");
    group.throughput(Throughput::Bytes(large_source.len() as u64));
    group.bench_function("parse_large", |b| {
        // Parse each module individually (parser expects single module)
        let modules: Vec<&str> = large_source
            .split("\nmodule ")
            .enumerate()
            .map(|(i, s)| if i == 0 { s } else { s })
            .collect();
        b.iter(|| {
            for src in &modules {
                let full = if src.starts_with("module ") {
                    (*src).to_string()
                } else {
                    format!("module {src}")
                };
                let _ = kodo_parser::parse(black_box(&full));
            }
        });
    });

    group.finish();
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
