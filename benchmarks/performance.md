# Runtime & Compiler Performance Benchmarks

> Measured on Apple M3 Pro, macOS 15.5, Kōdo v1.3.1 (Cranelift backend).
> Each test run 3 times, best time reported. All times in seconds.

## Runtime Performance

### Recursive Fibonacci — fib(35)

Pure recursive computation, no I/O. Tests function call overhead and recursion.

| Language | Time (s) | Relative |
|----------|-------:|------:|
| **Rust** (release) | 0.04 | 1.0x |
| **Node.js** (V8 JIT) | 0.09 | 2.3x |
| **Go** | 0.09 | 2.3x |
| **Kōdo** (LLVM -O3) | 0.26 | 6.5x |
| **Kōdo** (Cranelift) | 0.30 | 7.5x |
| **Python** | 0.87 | 21.8x |

Kōdo is **3.3x faster than Python**. The LLVM backend with `-O3` is ~13% faster
than Cranelift on recursive workloads. The gap vs Rust/Go is expected — migrating
to the LLVM C API (inkwell) would enable full interprocedural optimization.

### Sum Loop — 10 million iterations

Tight loop with integer addition. Tests loop overhead and basic arithmetic.

| Language | Time (s) | Relative |
|----------|-------:|------:|
| **Rust** (release) | 0.00 | 1.0x |
| **Node.js** (V8 JIT) | 0.06 | — |
| **Go** | 0.08 | — |
| **Kōdo** (LLVM -O3) | 0.07 | — |
| **Go** | 0.08 | — |
| **Kōdo** (Cranelift) | 0.11 | — |
| **Python** | 0.48 | — |

Kōdo LLVM -O3 is **36% faster than Cranelift** on tight loops and **faster than Go**.

Kōdo is **4.8x faster than Python** on tight loops and competitive with Go.

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

1. **Kōdo is 3-5x faster than Python** on compute-bound tasks
2. **Competitive with Go** on tight loops (within 25%)
3. **Compiler check is sub-10ms** — instant feedback for agents
4. **Full build is ~100ms** — fast enough for tight compile-fix loops
5. **Not as fast as Rust/C** — Cranelift trades runtime speed for compile speed

## Inkwell Backend (Experimental)

The inkwell backend uses the LLVM C API (via inkwell crate) for programmatic
IR construction, enabling full LLVM optimization passes. Status:

- **Working**: Hello World, arithmetic, if/else, print_int
- **Not yet**: Recursion, closures, complex programs
- **Expected**: 2-3x speedup over textual LLVM once complete
- **Build**: `cargo build -p kodoc --features inkwell`
- **Use**: `kodoc build file.ko --backend=inkwell`

Once recursion is fixed, benchmark results will be updated.

## Methodology

- All benchmarks use the default Cranelift backend (`kodoc build`)
- Rust uses `rustc -O` (release optimizations)
- Go uses `go build` (default optimizations)
- Node.js uses V8's JIT compiler
- Python uses CPython 3.12
- Kōdo does NOT use the LLVM backend — `--release` (LLVM -O3) would improve results
- No warmup runs; cold-start measurement for compilation
- Runtime benchmarks exclude compilation time
