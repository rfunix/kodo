# Kōdo Vericoding Benchmark

**AlgoVeri-equivalent** benchmark for formally verified algorithm synthesis in Kōdo.

Inspired by the POPL 2026 paper *"A benchmark for vericoding: formally verified program synthesis"* (ArXiv 2509.22908) and the AlgoVeri benchmark suite (77 algorithms in Dafny/Verus/Lean).

## What is Vericoding?

Vericoding = **verified coding**: generating programs that are *formally correct* by construction, not just functionally correct. Every function carries machine-checkable contracts (`requires`/`ensures`) verified by the Z3 SMT solver.

The key thesis: **Kōdo is the first language designed for AI agents to write verified code as naturally as unverified code**.

## Benchmark vs. AlgoVeri

| Metric | AlgoVeri (POPL 2026) | Kōdo Vericoding |
|--------|----------------------|-----------------|
| Tasks  | 77 algorithms        | 20 tasks (initial) |
| Languages | Dafny, Verus, Lean | Kōdo |
| Dafny success rate | 82% | — |
| Verus/Rust success rate | 44% | — |
| Kōdo success rate | — | **100% (20/20)** |
| Verification backend | Dafny/Lean provers | Z3 SMT |

## Task Categories

| Category | Count | Description |
|----------|-------|-------------|
| `arithmetic` | 5 | abs, clamp, max, bounded_add, safe_div |
| `mathematical` | 5 | factorial, fibonacci, gcd, power, isqrt |
| `search` | 5 | sum_list, min_list, max_list, linear_search, count_positive |
| `data-structures` | 3 | running_max, safe_get, binary_search |
| `sorting` | 2 | bubble_sort, merge_sorted |

## Contract Patterns

Each task demonstrates one or more contract patterns:

- **Precondition (requires)**: `requires { b != 0 }` — prevents division by zero statically
- **Postcondition (ensures)**: `ensures { result >= 0 }` — proves output properties
- **Range invariant**: `requires { lo <= hi }` + `ensures { result >= lo && result <= hi }`
- **Dominance**: `ensures { result >= a && result >= b }` — max is ≥ both inputs
- **Identity bound**: `ensures { result == a + b }` — arithmetic identity
- **Monotone invariant**: `ensures { result >= current }` — running max never decreases
- **Length preservation**: `ensures { list_length(result) == list_length(lst) }`

## Running

```sh
# Build the release compiler first
cargo build -p kodoc --release

# Run all 20 tasks
./bench/vericoding/run.sh

# Verbose output (show failures)
./bench/vericoding/run.sh --verbose

# JSON output (for CI / agent consumption)
./bench/vericoding/run.sh --json
```

## Current Results

```
Total:   20
Passed:  20
Failed:  0
Success: 100%
```

## Adding Tasks

1. Create `tasks/vNNN-task-name.json` with the task spec
2. Create `solutions/vNNN.ko` with the verified implementation
3. Run `./bench/vericoding/run.sh --verbose` to verify

## References

- POPL 2026: "A benchmark for vericoding: formally verified program synthesis"
- AlgoVeri: https://arxiv.org/html/2602.09464
- Kōdo Contracts Guide: `/docs/guide/contracts.md`
- Z3 SMT Solver: https://github.com/Z3Prover/z3
