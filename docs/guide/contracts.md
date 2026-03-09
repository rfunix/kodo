# Contracts

Contracts are a core feature of Kōdo. They let you express preconditions that are checked at runtime, ensuring your functions are only called with valid inputs.

## What Are Contracts?

A contract is a boolean expression attached to a function that must be true for the function to execute correctly. Kōdo uses contracts to make correctness guarantees explicit — instead of documenting assumptions in comments, you encode them in the language.

## `requires` — Preconditions

A `requires` block specifies conditions that must hold when a function is called:

```
fn safe_divide(a: Int, b: Int) -> Int
    requires { b != 0 }
{
    return a / b
}
```

This tells both humans and AI agents: "never call `safe_divide` with `b` equal to zero." The compiler injects a runtime check before the function body executes.

### Multiple Conditions

You can use logical operators to combine conditions:

```
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

```
Contract violation: requires clause failed
```

The program terminates with a non-zero exit code. This is intentional — a contract violation means the program has a bug, and continuing execution could produce incorrect results.

## Practical Example

Here is a complete program that demonstrates contracts passing and failing:

```
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

## `ensures` — Postconditions (Planned)

Kōdo's syntax supports `ensures` blocks for postconditions:

```
fn abs(x: Int) -> Int
    ensures { result >= 0 }
{
    if x < 0 {
        return -x
    }
    return x
}
```

The `ensures` clause is currently **parsed and type-checked** but does **not** inject runtime checks yet. It serves as documentation of intent until the runtime checks are implemented.

## When to Use Contracts

Contracts are most useful for:

- **Preventing invalid inputs**: division by zero, out-of-range values, invalid state
- **Documenting assumptions**: making implicit requirements explicit
- **Catching bugs early**: failing fast at the point of misuse rather than producing wrong results downstream

## Next Steps

- [CLI Reference](cli-reference.md) — all available commands and flags
- [Language Basics](language-basics.md) — types, variables, and control flow
