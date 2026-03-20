# Pattern Matching

Kōdo provides exhaustive pattern matching on enum types using `match` expressions. The compiler verifies that all variants are handled, preventing bugs from unmatched cases.

## Basic Match

```rust
enum Direction {
    North,
    South,
    East,
    West
}

fn describe(d: Direction) -> String {
    match d {
        Direction::North => { return "Going north" }
        Direction::South => { return "Going south" }
        Direction::East => { return "Going east" }
        Direction::West => { return "Going west" }
    }
}
```

## Match with Payload

Enum variants can carry data, which you can destructure in match arms:

```rust
enum Shape {
    Circle(Int),
    Rectangle(Int, Int)
}

fn area(s: Shape) -> Int {
    match s {
        Shape::Circle(r) => { return r * r * 3 }
        Shape::Rectangle(w, h) => { return w * h }
    }
}
```

## Exhaustiveness

The compiler requires all variants to be handled. Missing a variant produces a compile-time error, ensuring no unmatched cases at runtime.

## Option and Result

Pattern matching is the primary way to work with `Option<T>` and `Result<T, E>`:

```rust
fn safe_head(items: List<Int>) -> Int {
    let result: Option<Int> = list_get(items, 0)
    match result {
        Option::Some(v) => { return v }
        Option::None => { return 0 }
    }
}
```

### Short Variant Patterns

For `Option` and `Result`, you can omit the enum prefix in match arms. Both forms are equivalent:

```rust
// Long form (explicit enum prefix)
match result {
    Result::Ok(v) => { return v }
    Result::Err(e) => { return 0 }
}

// Short form (no prefix)
match result {
    Ok(v) => { return v }
    Err(e) => { return 0 }
}
```

The same applies to `Option`:

```rust
match maybe_val {
    Some(v) => { return v }
    None => { return 0 }
}
```

The compiler infers the enum type from the matched expression, so the prefix is optional.

## Example

See [`examples/enums.ko`](../../examples/enums.ko) for a complete working example with enum types and pattern matching, and [`examples/result_patterns.ko`](../../examples/result_patterns.ko) for Result/Option patterns and unwrap methods.
