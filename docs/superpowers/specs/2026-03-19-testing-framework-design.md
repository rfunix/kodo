# Testing Framework Enhancement — Design Spec

**Date**: 2026-03-19
**Status**: Approved
**Scope**: Refine existing test framework (A), auto-generate test stubs (B), property-based testing (C)

## Context

Kōdo v0.5.1 already has a functional test framework: `test "name" { ... }` blocks, polymorphic `assert_eq`/`assert_ne`, `kodoc test` with `--filter` and `--json`, and a runtime test harness. This spec enhances the framework with test grouping, lifecycle hooks, property-based testing, and contract-driven stub generation.

## Architecture

```
┌─────────────────────────────────────────────────┐
│  kodoc generate-tests  (stub generation)        │
│  Reads contracts → generates test blocks → .ko  │
└──────────────────────┬──────────────────────────┘
                       │ generates
┌──────────────────────▼──────────────────────────┐
│  Language: test blocks + annotations            │
│  test, describe, setup, teardown                │
│  @property, @skip, @todo, @timeout              │
│  forall statement                               │
└──────────────────────┬──────────────────────────┘
                       │ compiles
┌──────────────────────▼──────────────────────────┐
│  Runtime: kodo_runtime/test_ops.rs              │
│  Assertions, lifecycle, property engine,        │
│  basic shrinking, timeout, isolation            │
└─────────────────────────────────────────────────┘
```

**Changes by crate**:
- `kodo_lexer` — keywords `describe`, `setup`, `teardown`, `forall`
- `kodo_ast` — new nodes `DescribeDecl`, `SetupBlock`, `TeardownBlock`, `ForallStmt`
- `kodo_parser` — parsing of new constructs
- `kodo_types` — type checking new nodes + generator builtins
- `kodo_runtime` — property engine, basic shrinking, timeout, isolation
- `kodoc/commands/test.rs` — desugar describe/setup/teardown, orchestration
- `kodoc/commands/generate_tests.rs` — new command

## Part A: Framework Refinement

### A.1 — `describe` blocks (test grouping)

```kodo
describe "math operations" {
    setup {
        let base: Int = 100
    }

    teardown {
        cleanup()
    }

    test "addition" {
        assert_eq(base + 1, 101)
    }

    test "subtraction" {
        assert_eq(base - 50, 50)
    }
}
```

- `describe` can be nested (sub-groups)
- `setup` runs before **each** test within the describe
- `teardown` runs after **each** test
- Variables from `setup` are visible in the describe's tests
- Test names are hierarchical: `"math operations > addition"`

### A.2 — Annotations for test control

```kodo
@skip("not implemented yet")
test "future feature" {
    assert(false)
}

@todo("implement when async is ready")
test "async operation" {
    assert(false)
}

@timeout(5000)  // milliseconds
test "long computation" {
    // ...
}
```

- `@skip` — test not executed, reported as "skipped"
- `@todo` — like skip, reported as "todo" (distinct in JSON output)
- `@timeout` — aborts test if it exceeds the time limit

### A.3 — Better failure messages

```
test expression in assertion ... FAILED
  assertion failed: assert_eq
    left:  84
    right: 85
```

Values of both sides are printed. Expression source (e.g., `x * 2`) is a future enhancement.

### A.4 — Test isolation

Each test runs with `kodo_test_isolate_start()` / `kodo_test_isolate_end()` that:
- Reset heap allocations (lists, maps created in the test)
- Ensure side effects from one test don't affect the next

### A.5 — Test timeout

Implemented via a timer thread in the runtime:
- `kodo_test_set_timeout(ms)` — starts timer
- `kodo_test_clear_timeout()` — cancels timer
- If timeout fires, the test is marked as failed with "timeout" reason

## Part B: Auto-generation of Test Stubs

### B.1 — Command

```bash
# Generate file_test.ko (separate file, default)
kodoc generate-tests mymodule.ko

# Inline at end of same file
kodoc generate-tests mymodule.ko --inline

# Output to stdout
kodoc generate-tests mymodule.ko --stdout

# JSON report
kodoc generate-tests mymodule.ko --json
```

### B.2 — Generation strategy

| Contract | Generated stub |
|----------|---------------|
| `requires { x > 0 }` | Test with valid input + test with boundary input |
| `ensures { result >= 0 }` | `@property` test verifying postcondition |
| `requires` + `ensures` | Combination of both |
| No contracts | Basic stub with `// TODO` for each public function |

### B.3 — Example

Input:
```kodo
fn safe_divide(a: Int, b: Int) -> Int
    requires { b != 0 }
    ensures  { result >= 0 }
{
    return a / b
}
```

Generated output:
```kodo
test "safe_divide: basic call" {
    assert_eq(safe_divide(10, 2), 5)
}

test "safe_divide: precondition boundary" {
    // TODO: verify behavior near requires { b != 0 }
}

@property(iterations: 100)
test "safe_divide: postcondition result >= 0" {
    forall a: Int, b: Int {
        if b != 0 {
            assert(safe_divide(a, b) >= 0)
        }
    }
}
```

### B.4 — JSON output

```json
{
  "generated": [
    {
      "function": "safe_divide",
      "tests": 3,
      "from_contracts": true,
      "property_tests": 1
    }
  ],
  "total_tests": 3,
  "file": "mymodule_test.ko"
}
```

## Part C: Property-Based Testing

### C.1 — Syntax

```kodo
@property(iterations: 100)
test "abs is non-negative" {
    forall x: Int {
        assert(abs(x) >= 0)
    }
}

@property(iterations: 50, int_range: [-100, 100])
test "addition is commutative" {
    forall a: Int, b: Int {
        assert_eq(a + b, b + a)
    }
}

@property(iterations: 30)
test "list reverse is involutive" {
    forall items: List<Int> {
        assert_eq(list_reverse(list_reverse(items)), items)
    }
}
```

### C.2 — Supported generator types

| Tier | Types |
|------|-------|
| 1 (essential) | `Int`, `Bool`, `String` |
| 2 (important) | `Float64`, `List<Int>`, `List<String>`, `Option<Int>` |
| 3 (complete) | Custom generators, `Result<T,E>`, `Map<K,V>`, `Option<T>` for any T |

All tiers implemented in v1.

### C.3 — Annotation parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `iterations` | Int | 100 | Number of random inputs to generate |
| `int_range` | [Int, Int] | [-1000000, 1000000] | Range for Int generation |
| `float_range` | [Float64, Float64] | [-1e6, 1e6] | Range for Float64 generation |
| `max_string_len` | Int | 100 | Maximum string length |
| `max_list_len` | Int | 20 | Maximum list length |
| `seed` | Int | 0 (random) | Deterministic seed for reproducibility |

### C.4 — Shrinking (basic, v1)

When a property test fails, the runtime tries minimal values for each input type:

| Type | Shrink candidates |
|------|------------------|
| `Int` | `0`, `-1`, `1`, `value / 2` |
| `Bool` | `false` |
| `String` | `""`, first character |
| `Float64` | `0.0`, `1.0`, `-1.0` |
| `List<T>` | `[]`, first element only |
| `Map<K,V>` | `{}`, first entry only |
| `Option<T>` | `None`, `Some(shrunk_inner)` |
| `Result<T,E>` | Keep variant, shrink inner |

No compositional backtracking. Each variable is shrunk independently. The smallest failing combination is reported.

### C.5 — Desugar

```kodo
// Input
@property(iterations: 100)
test "commutative" {
    forall a: Int, b: Int {
        assert_eq(a + b, b + a)
    }
}

// Desugared
fn __test_0() {
    kodo_prop_start(100, 0)
    let __iter: Int = 0
    while __iter < 100 {
        let a: Int = kodo_prop_gen_int(-1000000, 1000000)
        let b: Int = kodo_prop_gen_int(-1000000, 1000000)
        assert_eq(a + b, b + a)
        __iter = __iter + 1
    }
}
```

On assertion failure, the runtime captures the current inputs, runs the shrink candidates, and reports the minimal failing input.

## Runtime: New Functions

```
// Timeout
kodo_test_set_timeout(ms: i64)
kodo_test_clear_timeout()

// Property testing — generators
kodo_prop_start(iterations: i64, seed: i64)
kodo_prop_gen_int(min: i64, max: i64) -> i64
kodo_prop_gen_bool() -> i64
kodo_prop_gen_string(max_len: i64) -> (ptr, len)
kodo_prop_gen_float(min: f64, max: f64) -> f64
kodo_prop_gen_list_int(max_len: i64) -> List
kodo_prop_gen_list_string(max_len: i64) -> List
kodo_prop_gen_option_int() -> Option<Int>
kodo_prop_gen_option_string() -> Option<String>
kodo_prop_gen_result_int_string() -> Result<Int, String>
kodo_prop_gen_map_int_int(max_len: i64) -> Map<Int, Int>
kodo_prop_gen_map_string_string(max_len: i64) -> Map<String, String>

// Shrinking (basic)
kodo_prop_shrink_int(value: i64) -> i64
kodo_prop_shrink_string(ptr: i64) -> (ptr, len)
kodo_prop_shrink_list(ptr: i64) -> ptr
kodo_prop_shrink_bool(value: i64) -> i64

// Isolation
kodo_test_isolate_start()
kodo_test_isolate_end()
```

## JSON Output (expanded)

```json
{"event": "test_result", "name": "math > addition", "group": "math", "status": "passed", "duration_ms": 2}
{"event": "test_result", "name": "skipped test", "status": "skipped", "reason": "not implemented yet"}
{"event": "test_result", "name": "property test", "status": "failed", "property": true, "iterations_run": 47, "failing_input": {"a": 847293, "b": -1}, "shrunk_input": {"a": 1, "b": -1}}
{"event": "summary", "total": 10, "passed": 7, "failed": 1, "skipped": 1, "todo": 1, "duration_ms": 250}
```

## Grammar Changes

```ebnf
describe_decl = annotation* "describe" STRING_LIT "{"
                  setup_block? teardown_block?
                  (test_decl | describe_decl)*
                "}" ;
setup_block   = "setup" block ;
teardown_block = "teardown" block ;
forall_stmt   = "forall" ident ":" type_expr ("," ident ":" type_expr)* block ;
```

## Non-Goals (v1)

- Compositional shrinking (backtracking across multiple variables)
- Code coverage measurement
- Parallel test execution
- Test fixtures / shared state across tests
- Mocking / stubbing framework
- Expression source in failure messages (only values)
