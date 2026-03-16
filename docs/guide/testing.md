# Testing

Kōdo has a built-in testing framework. Tests are first-class constructs in the language — you write them alongside your code using the `test` keyword, and run them with `kodoc test`.

## Writing Tests

A test block uses the `test` keyword followed by a string literal name and a body:

```rust
module math_utils {
    meta {
        purpose: "Math utilities with tests"
    }

    fn add(a: Int, b: Int) -> Int {
        return a + b
    }

    fn multiply(a: Int, b: Int) -> Int {
        return a * b
    }

    test "add returns correct sum" {
        assert_eq(add(2, 3), 5)
        assert_eq(add(-1, 1), 0)
        assert_eq(add(0, 0), 0)
    }

    test "multiply returns correct product" {
        assert_eq(multiply(3, 4), 12)
        assert_eq(multiply(0, 5), 0)
    }
}
```

Tests live inside the same module as the code they test. Each test block has a descriptive name (a string literal) and a body containing assertions.

## Assertions

Kōdo provides these built-in assertion functions:

| Assertion | Signature | Description |
|-----------|-----------|-------------|
| `assert` | `assert(Bool)` | Fails if the value is `false` |
| `assert_true` | `assert_true(Bool)` | Fails if the value is `false` (alias for `assert`) |
| `assert_false` | `assert_false(Bool)` | Fails if the value is `true` |
| `assert_eq` | `assert_eq(T, T)` | Fails if left and right are not equal |
| `assert_ne` | `assert_ne(T, T)` | Fails if left and right are equal |

`assert_eq` and `assert_ne` support these types: `Int`, `String`, `Bool`, and `Float64`. Both operands must be the same type.

### Examples

```rust
test "boolean assertions" {
    assert(true)
    assert_true(10 > 5)
    assert_false(3 == 4)
}

test "equality assertions" {
    assert_eq(42, 42)
    assert_eq("hello", "hello")
    assert_eq(true, true)
    assert_eq(3.14, 3.14)

    assert_ne(1, 2)
    assert_ne("foo", "bar")
}
```

## Running Tests

Use `kodoc test` to run tests in a file:

```bash
# Run all tests in a file
kodoc test examples/testing.ko

# Run only tests whose name matches a pattern
kodoc test examples/testing.ko --filter "add"

# Output results as JSON (for agent consumption)
kodoc test examples/testing.ko --json
```

### Exit Codes

| Code | Meaning |
|------|---------|
| `0` | All tests passed |
| `1` | One or more tests failed |

## Annotations on Tests

Tests support the same annotations as functions — `@confidence` and `@authored_by`:

```rust
@confidence(0.95)
@authored_by(agent: "claude")
test "critical invariant holds" {
    let result: Int = compute_critical_value()
    assert_eq(result, 42)
}
```

This integrates with Kōdo's agent traceability system. See [Agent Traceability](agent-traceability.md) for details.

## How Test Mode Works

When you run `kodoc test`, the compiler:

1. Parses the file normally, collecting all `test` blocks.
2. Desugars each `test "name" { body }` into a regular function (e.g., `__test_0_add_returns_correct_sum`).
3. Generates a synthetic `main` function that calls each test function in sequence, printing pass/fail results.
4. Compiles and runs the resulting program.

This means tests have full access to all functions and types defined in the module — no special imports needed.

## Complete Example

```rust
module string_utils {
    meta {
        purpose: "String utility functions"
    }

    fn repeat_string(s: String, n: Int) -> String {
        let result: String = ""
        let i: Int = 0
        while i < n {
            result = result + s
            i = i + 1
        }
        return result
    }

    fn is_empty(s: String) -> Bool {
        return string_length(s) == 0
    }

    test "repeat_string builds repeated output" {
        assert_eq(repeat_string("ab", 3), "ababab")
        assert_eq(repeat_string("x", 1), "x")
        assert_eq(repeat_string("hello", 0), "")
    }

    test "is_empty detects empty strings" {
        assert_true(is_empty(""))
        assert_false(is_empty("hello"))
    }

    test "repeat_string with single char" {
        let result: String = repeat_string("-", 5)
        assert_eq(result, "-----")
        assert_false(is_empty(result))
    }
}
```

Run it:

```bash
$ kodoc test testing.ko
Running 3 tests...
  PASS  repeat_string builds repeated output
  PASS  is_empty detects empty strings
  PASS  repeat_string with single char

3/3 tests passed.
```

## Known Limitations

- **Variable names across test blocks**: Closures in separate test blocks should use unique variable names to avoid conflicts during lambda lifting.
- **`assert_true` / `assert_false`**: Work with inline expressions and function return values. They accept any expression of type `Bool`.
- **Generic functions**: If the same generic function is instantiated with multiple different types in the same file, monomorphization may produce conflicts. Use each instantiation in a separate test block.
- **Single file only**: `kodoc test` currently runs tests within a single file. Cross-file test discovery is not yet supported.

## Future Plans

- **Test isolation**: Each test will run in its own isolated scope, preventing state leakage between tests.
- **Setup and teardown**: `before_each` and `after_each` blocks for common test setup.
- **Cross-file test discovery**: `kodoc test src/` to discover and run all tests in a directory.
- **Test coverage reporting**: Integration with confidence scores for coverage-aware trust policies.
- **Parallel test execution**: Run independent tests concurrently for faster feedback loops.
