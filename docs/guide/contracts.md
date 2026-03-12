# Contracts

Contracts are a core feature of Kōdo. They let you express preconditions that are checked at runtime, ensuring your functions are only called with valid inputs.

## What Are Contracts?

A contract is a boolean expression attached to a function that must be true for the function to execute correctly. Kōdo uses contracts to make correctness guarantees explicit — instead of documenting assumptions in comments, you encode them in the language.

## `requires` — Preconditions

A `requires` block specifies conditions that must hold when a function is called:

```rust
fn safe_divide(a: Int, b: Int) -> Int
    requires { b != 0 }
{
    return a / b
}
```

This tells both humans and AI agents: "never call `safe_divide` with `b` equal to zero." The compiler injects a runtime check before the function body executes.

### Multiple Conditions

You can use logical operators to combine conditions:

```rust
fn clamp(value: Int, min: Int, max: Int) -> Int
    requires { min <= max }
{
    if value < min {
        return min
    }
    if value > max {
        return max
    }
    return value
}
```

## What Happens When a Contract Fails

When a `requires` condition evaluates to `false` at runtime, the program **aborts immediately** with an error message:

```rust
Contract violation: requires clause failed
```

The program terminates with a non-zero exit code. This is intentional — a contract violation means the program has a bug, and continuing execution could produce incorrect results.

## Practical Example

Here is a complete program that demonstrates contracts passing and failing:

```rust
module contracts_demo {
    meta {
        purpose: "Demonstrate contract behavior",
        version: "0.1.0",
        author: "Kōdo Team"
    }

    fn safe_divide(a: Int, b: Int) -> Int
        requires { b != 0 }
    {
        return a / b
    }

    fn main() {
        // This works — b is 2, which is not zero
        let result: Int = safe_divide(10, 2)
        print_int(result)

        // If you uncomment the next line, the program will abort:
        // let bad: Int = safe_divide(10, 0)
    }
}
```

Compile and run:

```bash
cargo run -p kodoc -- build contracts_demo.ko -o contracts_demo
./contracts_demo
```

Output: `5`

To see a contract failure, change the call to `safe_divide(10, 0)` and recompile. The program will abort before the division happens.

## `ensures` — Postconditions

An `ensures` block specifies conditions that must hold when a function returns:

```rust
fn abs(x: Int) -> Int
    ensures { result >= 0 }
{
    if x < 0 {
        return -x
    }
    return x
}
```

Inside an `ensures` expression, the special name `result` refers to the function's return value. The compiler injects a runtime check **before every `return` statement** (and before the implicit return at the end of the function body).

### How It Works

1. The function body executes normally.
2. Before returning, the `ensures` expression is evaluated with `result` bound to the return value.
3. If the expression evaluates to `false`, the program aborts with:

```rust
Contract violation: ensures clause failed in function_name
```

### Combining `requires` and `ensures`

You can use both contracts on the same function:

```rust
fn safe_divide(a: Int, b: Int) -> Int
    requires { b != 0 }
    ensures { result * b <= a }
{
    return a / b
}
```

`requires` checks run at function entry; `ensures` checks run at function exit. Together, they form a complete contract: callers must satisfy preconditions, and the function guarantees postconditions.

## When to Use Contracts

Contracts are most useful for:

- **Preventing invalid inputs**: division by zero, out-of-range values, invalid state
- **Documenting assumptions**: making implicit requirements explicit
- **Catching bugs early**: failing fast at the point of misuse rather than producing wrong results downstream

## Static Verification with Z3

Kōdo can verify contracts at **compile time** using the Z3 SMT solver. When enabled, the compiler translates contract expressions into Z3 formulas and attempts to prove them automatically.

```bash
kodoc build my_program.ko --contracts static
```

### Contract Modes

| Mode | Behavior |
|------|----------|
| `runtime` (default) | Contracts checked at runtime — program aborts on violation |
| `static` | Z3 attempts to prove contracts at compile time |
| `both` | Z3 verification + runtime fallback for unproven contracts |
| `none` | No contract checking (not recommended) |

### How Static Verification Works

1. The compiler translates `requires`/`ensures` expressions into Z3 formulas
2. Z3 attempts to prove the formula within a timeout
3. If Z3 **proves** the contract, no runtime check is generated (optimization)
4. If Z3 **refutes** the contract, error E0302 is emitted with a counter-example
5. If Z3 **times out**, the contract falls back to a runtime check

### Supported Expressions

Static verification supports:
- Integer arithmetic (`+`, `-`, `*`, `/`, `%`)
- Comparisons (`==`, `!=`, `<`, `>`, `<=`, `>=`)
- Boolean logic (`&&`, `||`, `!`)

### Example

```rust
fn safe_divide(a: Int, b: Int) -> Int
    requires { b != 0 }
{
    return a / b
}
```

With `--contracts static`, Z3 verifies that callers satisfy `b != 0`. The compilation certificate records which contracts were statically verified.

## Next Steps

- [Error Handling](error-handling.md) — using `Option<T>` and `Result<T, E>` for safe error handling
- [Modules and Imports](modules-and-imports.md) — multi-file programs and the standard library
- [CLI Reference](cli-reference.md) — all available commands and flags
