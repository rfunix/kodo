//! Benchmarks for the Kōdo type checker.
//!
//! Measures type checking throughput for modules of varying complexity.

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

/// A simple module for type checking benchmarks.
const SIMPLE_MODULE: &str = r#"module benchmark {
    meta {
        purpose: "Simple type checking benchmark"
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

/// A module with structs, enums, and pattern matching.
const COMPLEX_MODULE: &str = r#"module benchmark {
    meta {
        purpose: "Complex type checking benchmark"
    }

    struct Point {
        x: Int,
        y: Int
    }

    enum Direction {
        North,
        South,
        East,
        West
    }

    fn make_point(x: Int, y: Int) -> Point {
        return Point { x: x, y: y }
    }

    fn distance(p: Point) -> Int {
        return p.x + p.y
    }

    fn direction_value(d: Direction) -> Int {
        match d {
            Direction::North => { return 0 }
            Direction::South => { return 1 }
            Direction::East => { return 2 }
            Direction::West => { return 3 }
        }
    }

    fn factorial(n: Int) -> Int {
        if n <= 1 {
            return 1
        }
        return n * factorial(n - 1)
    }

    fn apply(f: (Int) -> Int, x: Int) -> Int {
        return f(x)
    }

    fn main() -> Int {
        let p: Point = make_point(3, 4)
        let d: Int = distance(p)
        let f: Int = factorial(10)
        let doubled: Int = apply(|x: Int| -> Int { return x * 2 }, d)
        return d + f + doubled
    }
}
"#;

fn bench_type_check(c: &mut Criterion) {
    let mut group = c.benchmark_group("type_checker");

    // Simple module
    group.bench_function("check_simple", |b| {
        let module = kodo_parser::parse(SIMPLE_MODULE).expect("parse should succeed");
        b.iter(|| {
            let mut checker = kodo_types::TypeChecker::new();
            let _ = checker.check_module(black_box(&module));
        });
    });

    // Complex module with structs, enums, closures
    group.bench_function("check_complex", |b| {
        let module = kodo_parser::parse(COMPLEX_MODULE).expect("parse should succeed");
        b.iter(|| {
            let mut checker = kodo_types::TypeChecker::new();
            let _ = checker.check_module(black_box(&module));
        });
    });

    group.finish();
}

criterion_group!(benches, bench_type_check);
criterion_main!(benches);
