# Runtime & Compiler Performance Benchmarks

> Measured on Apple M3 Pro, macOS 15.5, Kōdo v1.7.0.
> Each test run 3 times, best time reported. All times in seconds.

## Runtime Performance

### Recursive Fibonacci — fib(35)

Pure recursive computation, no I/O. Tests function call overhead and recursion.

| Language | Time (s) | Relative |
|----------|-------:|------:|
| **Rust** (release) | 0.04 | 1.0x |
| **Node.js** (V8 JIT) | 0.09 | 2.3x |
| **Go** | 0.09 | 2.3x |
| **Kōdo** (Inkwell LLVM) | 0.25 | 6.3x |
| **Kōdo** (Cranelift) | 0.25 | 6.3x |
| **Python** | 0.87 | 21.8x |

Kōdo is **3.5x faster than Python**. On recursive workloads both backends
perform similarly — the Inkwell backend's advantage shows more clearly on
tight loops where LLVM optimization passes can eliminate overhead.

### Sum Loop — 10 million iterations

Tight loop with integer addition. Tests loop overhead and basic arithmetic.

| Language | Time (s) | Relative |
|----------|-------:|------:|
| **Rust** (release) | 0.00 | 1.0x |
| **Node.js** (V8 JIT) | 0.06 | — |
| **Kōdo** (Inkwell LLVM) | 0.07 | — |
| **Go** | 0.08 | — |
| **Kōdo** (Cranelift) | 0.08 | — |
| **Python** | 0.48 | — |

Kōdo Inkwell is **12% faster than Cranelift** on tight loops and **faster than Go**.

Kōdo is **6x faster than Python** on tight loops and competitive with Go.

## Backend Comparison — Cranelift vs Inkwell (LLVM)

Kōdo supports two code generation backends:

- **Cranelift** — default, fast compilation, good runtime performance
- **Inkwell (LLVM C API)** — uses the full LLVM optimization pipeline with
  alloca elimination, function inlining hints, and native CPU targeting

| Benchmark | Cranelift | Inkwell LLVM | Difference |
|-----------|----------|---------|-------------|
| fib(35) | 0.25s | **0.25s** | ~0% |
| sum 10M | 0.08s | **0.07s** | **12% faster** |

Inkwell matches or beats Cranelift on all benchmarks. The advantage is most
visible on tight loops where LLVM's optimization passes can vectorize and
eliminate redundant operations.

All 25 standard examples compile and run correctly on both backends.

**Build**: `cargo build -p kodoc --features llvm`
**Use**: `kodoc build file.ko --backend=inkwell`

## Compiler Performance

### Compilation Speed

How fast the Kōdo compiler processes source code.

| Operation | File | Lines | Time |
|-----------|------|------:|-----:|
| `kodoc check` | self_hosted_parser.ko | 1,897 | **7ms** |
| `kodoc check` | fibonacci.ko | 30 | **4ms** |
| `kodoc build` | fibonacci.ko | 30 | **105ms** |
| `kodoc build` | hello.ko | 12 | **105ms** |
| `kodoc build` | contracts_demo.ko | 45 | **105ms** |

### Comparison: Compilation Speed

| Compiler | File | Time | Notes |
|----------|------|-----:|-------|
| **kodoc check** | 1,897 lines | 7ms | Type checking only |
| **kodoc build** | small file | 105ms | Full pipeline + linking |
| **rustc** | small file | 68ms | Single file, no deps |
| **rustc -O** | small file | 68ms | With optimizations |
| **go build** | small file | 194ms | Single file |

Kōdo's type checker is extremely fast (7ms for ~2K lines). The `build` time is
dominated by the linker (`clang`), not the compiler itself. For the agent
feedback loop (`check` → fix → recheck), Kōdo delivers sub-10ms latency.

### Throughput

| Metric | Value |
|--------|------:|
| Check throughput | ~270K lines/sec |
| Build throughput | ~300 lines/sec (linker-bound) |
| WASM playground check | < 50ms (browser) |

## Key Takeaways

1. **Kōdo is 3-6x faster than Python** on compute-bound tasks
2. **Competitive with Go** on tight loops (Inkwell is faster)
3. **Compiler check is sub-10ms** — instant feedback for agents
4. **Full build is ~100ms** — fast enough for tight compile-fix loops
5. **Inkwell LLVM backend** matches or beats Cranelift on all workloads
6. **Not as fast as Rust/C** — but within an order of magnitude

## Methodology

- Cranelift is the default backend (`kodoc build`)
- Inkwell results use `kodoc build --backend=inkwell` with `--features llvm`
- Rust uses `rustc -O` (release optimizations)
- Go uses `go build` (default optimizations)
- Node.js uses V8's JIT compiler
- Python uses CPython 3.12
- No warmup runs; cold-start measurement for compilation
- Runtime benchmarks exclude compilation time
