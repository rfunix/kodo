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

## `ensures` — Postconditions

An `ensures` block specifies conditions that must hold when a function returns:

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

Inside an `ensures` expression, the special name `result` refers to the function's return value. The compiler injects a runtime check **before every `return` statement** (and before the implicit return at the end of the function body).

### How It Works

1. The function body executes normally.
2. Before returning, the `ensures` expression is evaluated with `result` bound to the return value.
3. If the expression evaluates to `false`, the program aborts with:

```
Contract violation: ensures clause failed in function_name
```

### Combining `requires` and `ensures`

You can use both contracts on the same function:

```
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

## Next Steps

- [CLI Reference](cli-reference.md) — all available commands and flags
- [Language Basics](language-basics.md) — types, variables, and control flow
