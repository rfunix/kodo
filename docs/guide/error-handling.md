# Error Handling

Kōdo has no null values and no exceptions. Instead, it uses two types from the standard library to represent the possibility of absence or failure: `Option<T>` and `Result<T, E>`.

Both types are available in every Kōdo program without an import — they are part of the **prelude**.

## The Problem with Null

Many languages use `null` to represent "no value." This is a frequent source of bugs because any value could secretly be null, and nothing in the type system warns you. Tony Hoare, who invented null references, called it his "billion-dollar mistake."

Kōdo takes a different approach: if a function might not return a value, its return type says so explicitly.

## `Option<T>` — A Value That Might Not Exist

`Option<T>` represents an optional value. It has two variants:

```rust
enum Option<T> {
    Some(T),   // a value is present
    None       // no value
}
```

### Returning Optional Values

Use `Option` when a function might not have a meaningful result:

```rust
fn find_positive(a: Int, b: Int) -> Option<Int> {
    if a > 0 {
        return Option::Some(a)
    }
    if b > 0 {
        return Option::Some(b)
    }
    return Option::None
}
```

### Handling Optional Values

Use `match` to handle both cases — the compiler ensures you never forget the `None` case:

```rust
fn main() {
    let result: Option<Int> = find_positive(-1, 42)
    match result {
        Option::Some(v) => {
            print_int(v)
        }
        Option::None => {
            println("no positive number found")
        }
    }
}
```

This is safer than checking for null: the type system **forces** you to handle the absent case. You cannot accidentally use an `Option<Int>` as if it were an `Int`.

## `Result<T, E>` — Success or Failure

`Result<T, E>` represents an operation that can succeed with a value of type `T` or fail with an error of type `E`:

```rust
enum Result<T, E> {
    Ok(T),    // success
    Err(E)    // failure
}
```

### Returning Results

Use `Result` when a function can fail in a way the caller should handle:

```rust
fn safe_divide(a: Int, b: Int) -> Result<Int, Int> {
    if b == 0 {
        return Result::Err(0)
    }
    return Result::Ok(a / b)
}
```

### Handling Results

Again, `match` forces you to handle both the success and error cases:

```rust
fn main() {
    let result: Result<Int, Int> = safe_divide(100, 5)
    match result {
        Result::Ok(v) => {
            print_int(v)
        }
        Result::Err(e) => {
            println("division failed")
        }
    }
}
```

## Unwrap Methods

When you are confident a `Result` or `Option` holds the expected variant, you can extract the value directly:

```rust
let ok_val: Result<Int, String> = Result::Ok(42)
let x: Int = ok_val.unwrap()          // 42

let err_val: Result<Int, String> = Result::Err("oops")
let e: String = err_val.unwrap_err()  // "oops"

let some_val: Option<Int> = Option::Some(99)
let y: Int = some_val.unwrap()        // 99
```

Use `unwrap_or` to provide a default when the value might be an error:

```rust
let fallback: Result<Int, String> = Result::Err("fail")
let d: Int = fallback.unwrap_or(0)    // 0
```

**Warning:** `unwrap()` aborts the program if called on `Err`/`None`. Prefer `match` or `unwrap_or` in production code. Use `unwrap()` in tests and prototypes.

## When to Use What

| Situation | Use |
|-----------|-----|
| A value might be absent (lookup, search, find) | `Option<T>` |
| An operation can fail with error information | `Result<T, E>` |
| An input must never be invalid | `requires { ... }` contract |
| A return value must satisfy a guarantee | `ensures { ... }` contract |

### `Option<T>` vs Contracts

`Option<T>` and contracts serve different purposes:

- **`Option<T>`** says: "this function might not have an answer, and that's normal."
- **`requires`** says: "calling this function with bad inputs is a bug."

For example, a lookup in a list might legitimately return nothing (`Option::None`). But dividing by zero is always a programming error — use a `requires { b != 0 }` contract instead.

## Complete Example

```rust
module error_handling {
    meta {
        purpose: "Demonstrate Option and Result"
        version: "0.1.0"
    }

    fn safe_divide(a: Int, b: Int) -> Result<Int, Int> {
        if b == 0 {
            return Result::Err(0)
        }
        return Result::Ok(a / b)
    }

    fn first_positive(a: Int, b: Int, c: Int) -> Option<Int> {
        if a > 0 {
            return Option::Some(a)
        }
        if b > 0 {
            return Option::Some(b)
        }
        if c > 0 {
            return Option::Some(c)
        }
        return Option::None
    }

    fn main() {
        let div: Result<Int, Int> = safe_divide(100, 5)
        match div {
            Result::Ok(v) => { print_int(v) }
            Result::Err(e) => { println("error") }
        }

        let found: Option<Int> = first_positive(-1, -2, 42)
        match found {
            Option::Some(v) => { print_int(v) }
            Option::None => { println("none found") }
        }
    }
}
```

Output:
```rust
20
42
```

## Next Steps

- [Contracts](contracts.md) — `requires` and `ensures` for runtime verification
- [Generics](generics.md) — how `Option<T>` and `Result<T, E>` are built with generics
- [Data Types and Pattern Matching](data-types.md) — structs, enums, and `match`
