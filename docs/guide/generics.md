# Generics

Generics let you write types and functions that work with any type, without sacrificing type safety. Kōdo compiles generics via **monomorphization** — each concrete usage generates a specialized version at compile time, with zero runtime overhead.

## Generic Types

### Generic Enums

Add type parameters in angle brackets after the type name:

```
enum Option<T> {
    Some(T),
    None
}
```

This defines an `Option` that can hold a value of any type `T`. When you use it, you specify the concrete type:

```
let x: Option<Int> = Option::Some(42)
let y: Option<Int> = Option::None
```

The compiler generates a concrete `Option<Int>` type behind the scenes — there is no boxing or dynamic dispatch.

### Generic Structs

Structs can also be generic:

```
struct Pair<T> {
    first: T,
    second: T
}
```

Use it with a concrete type:

```
let p: Pair<Int> = Pair { first: 1, second: 2 }
print_int(p.first)
print_int(p.second)
```

### Multiple Type Parameters

Types can have more than one type parameter:

```
enum Result<T, E> {
    Ok(T),
    Err(E)
}
```

Each parameter is independent — `T` and `E` can be different types:

```
let success: Result<Int, Int> = Result::Ok(42)
let failure: Result<Int, Int> = Result::Err(1)
```

## Pattern Matching with Generics

`match` works with generic types just like with concrete types:

```
fn unwrap_or(opt: Option<Int>, default: Int) -> Int {
    match opt {
        Option::Some(v) => {
            return v
        }
        Option::None => {
            return default
        }
    }
}
```

## Generic Functions

Functions can also be parameterized with type variables:

```
fn identity<T>(x: T) -> T {
    return x
}
```

Call it with any type — the compiler infers the type argument from the actual argument:

```
let a: Int = identity(42)
let b: Int = identity(99)
```

The compiler generates a specialized `identity` for `Int` at compile time.

## Trait Bounds

You can constrain generic type parameters to require specific trait implementations. This is called **bounded quantification** (System F<:) and ensures that only types satisfying the required interface can be used.

### Single Bound

```
fn display<T: Printable>(item: T) -> String {
    return item.display()
}
```

The `T: Printable` means "any type `T` that implements the `Printable` trait". If you try to call `display` with a type that does not implement `Printable`, the compiler will reject it with error E0232.

### Multiple Bounds

Use `+` to require multiple traits:

```
fn process<T: Printable + Comparable>(item: T) -> Int {
    return item.compare()
}
```

Here `T` must implement both `Printable` and `Comparable`.

### Bounds on Structs and Enums

Structs and enums can also have trait bounds on their type parameters:

```
enum SortedOption<T: Orderable> {
    Some(T),
    None
}
```

Any type used as the argument to `SortedOption` must implement `Orderable`.

### Mixed Bounded and Unbounded Parameters

You can mix bounded and unbounded parameters:

```
struct Pair<T: Ord, U> {
    first: T,
    second: U,
}
```

Here `T` must implement `Ord`, but `U` can be any type.

## How Monomorphization Works

When you write `Option<Int>`, the compiler doesn't create a single generic implementation that works for all types. Instead, it creates a **separate, concrete** type called `Option__Int` with `Int` substituted everywhere `T` appeared.

If you also use `Option<Bool>`, the compiler creates a second type `Option__Bool`. Each one is as efficient as if you'd written it by hand.

This is the same strategy used by Rust and C++. The tradeoff: compile time grows with the number of distinct instantiations, but runtime performance is optimal.

## Complete Example

```
module generics_demo {
    meta {
        purpose: "Demonstrate generic types and functions"
        version: "0.1.0"
    }

    enum Option<T> {
        Some(T),
        None
    }

    fn identity<T>(x: T) -> T {
        return x
    }

    fn print_option(opt: Option<Int>) {
        match opt {
            Option::Some(v) => {
                print_int(v)
            }
            Option::None => {
                println("none")
            }
        }
    }

    fn main() {
        let a: Option<Int> = Option::Some(42)
        let b: Option<Int> = Option::None
        print_option(a)
        print_option(b)

        let x: Int = identity(99)
        print_int(x)
    }
}
```

Output:
```
42
none
99
```

## Next Steps

- [Error Handling](error-handling.md) — use the standard library's `Option<T>` and `Result<T, E>`
- [Data Types and Pattern Matching](data-types.md) — structs, enums, and `match`
- [Modules and Imports](modules-and-imports.md) — multi-file programs
