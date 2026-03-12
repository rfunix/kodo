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

## Dynamic Dispatch with `dyn Trait`

When the concrete type is not known at compile time, you can use `dyn Trait` to dispatch method calls through a vtable at runtime.

### Syntax

Use `dyn Trait` as a parameter type to accept any value that implements the trait:

```rust
fn print_it(item: dyn Printable) {
    println(item.to_display())
}
```

You can pass any type that implements `Printable`:

```rust
fn main() {
    let p: Point = Point { x: 1, y: 2 }
    print_it(p)
}
```

### How It Works

A `dyn Trait` value is represented internally as a fat pointer with two fields:

- **data_ptr** — pointer to the concrete value
- **vtable_ptr** — pointer to a table of function pointers for the trait's methods

The compiler generates a vtable for each `(Type, Trait)` pair used with `dyn`. When a method is called, the runtime looks up the function pointer in the vtable and performs an indirect call.

### Performance Implications

Dynamic dispatch introduces a small overhead compared to static dispatch:

- Each method call goes through an indirect function pointer (one extra pointer dereference)
- The compiler cannot inline dynamically dispatched calls
- Each `dyn Trait` value is two pointers wide instead of one

### When to Use

| Use static dispatch when... | Use `dyn Trait` when... |
|-----------------------------|------------------------|
| The concrete type is known at compile time | You need to store heterogeneous types in a collection |
| Performance is critical | You need to abstract over many types without generics |
| You want the compiler to inline calls | The set of types is determined at runtime |

## Example

See [`examples/traits.ko`](../../examples/traits.ko) for a complete working example.
