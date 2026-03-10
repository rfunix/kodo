# Traits

Kōdo supports traits for defining shared behavior across types. Traits enable static dispatch — the compiler resolves which implementation to call at compile time, not runtime.

## Defining a Trait

A trait declares a set of functions that implementors must provide:

```rust
trait Printable {
    fn to_display(self) -> String
}
```

## Implementing a Trait

Use `impl` blocks to provide trait implementations for a type:

```rust
struct Point {
    x: Int,
    y: Int
}

impl Printable for Point {
    fn to_display(self) -> String {
        return "Point"
    }
}
```

## Using Traits

Trait methods are called on values of types that implement the trait:

```rust
fn main() {
    let p: Point = Point { x: 10, y: 20 }
    println(p.to_display())
}
```

## Static Dispatch

Kōdo uses static dispatch exclusively — the compiler knows exactly which function to call at every call site. This means:

- Zero runtime overhead for trait calls
- No vtable allocation or indirection
- The compiler can inline trait method calls

## Example

See [`examples/traits.ko`](../../examples/traits.ko) for a complete working example.
