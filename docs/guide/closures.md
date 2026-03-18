# Closures

Kōdo supports closures (anonymous functions) that can capture variables from their enclosing scope. The compiler uses **lambda lifting** to transform closures into regular functions at compile time.

## Basic Closures

A closure is defined with `|params| { body }` syntax:

```rust
let double = |x: Int| -> Int { return x * 2 }
```

## Closures as Function Parameters

Closures are most useful when passed to higher-order functions:

```rust
fn apply(f: (Int) -> Int, x: Int) -> Int {
    return f(x)
}

fn main() {
    let result: Int = apply(|x: Int| -> Int { return x * 3 }, 10)
    print_int(result)  // 30
}
```

## Function Types

Function types use the `(ParamTypes) -> ReturnType` syntax:

```rust
fn apply_twice(f: (Int) -> Int, x: Int) -> Int {
    let once: Int = f(x)
    return f(once)
}

fn main() {
    let result: Int = apply_twice(|x: Int| -> Int { return x + 1 }, 5)
    print_int(result)  // 7
}
```

## Passing Named Functions

Named functions can also be passed where a function type is expected:

```rust
fn square(x: Int) -> Int {
    return x * x
}

fn apply(f: (Int) -> Int, x: Int) -> Int {
    return f(x)
}

fn main() {
    let result: Int = apply(square, 4)
    print_int(result)  // 16
}
```

## Capturing Variables

Closures can capture variables from their enclosing scope. The compiler performs **capture analysis** and transforms the closure into a top-level function with the captured variables passed as extra parameters (lambda lifting):

```rust
fn make_adder(n: Int) -> (Int) -> Int {
    return |x: Int| -> Int { return x + n }
}

fn main() {
    let add5 = make_adder(5)
    print_int(add5(10))  // 15
}
```

Internally, the closure `|x| { x + n }` is lifted to a function `__closure_0(x: Int, n: Int) -> Int` and the captured variable `n` is passed automatically at the call site.

## How Lambda Lifting Works

Lambda lifting is a compiler technique that eliminates closures by converting them to regular functions:

1. **Capture analysis**: The compiler identifies free variables in the closure body
2. **Lifting**: The closure becomes a top-level function with captured variables as extra parameters
3. **Call site transformation**: Where the closure is created, the captured values are passed as arguments

This means closures have zero runtime overhead compared to regular function calls — there is no heap-allocated closure object.

## Complete Example

```rust
module closures_demo {
    meta {
        purpose: "Demonstrate closures and higher-order functions",
        version: "0.1.0"
    }

    fn apply(f: (Int) -> Int, x: Int) -> Int {
        return f(x)
    }

    fn apply_twice(f: (Int) -> Int, x: Int) -> Int {
        let once: Int = f(x)
        return f(once)
    }

    fn main() {
        // Direct closure call
        let double: Int = apply(|x: Int| -> Int { return x * 2 }, 5)
        print_int(double)  // 10

        // Apply twice
        let result: Int = apply_twice(|x: Int| -> Int { return x + 3 }, 0)
        print_int(result)  // 6

        // Named function as argument
        print_int(apply(|x: Int| -> Int { return x * x }, 4))  // 16
    }
}
```

## Closure Ownership Analysis

As of v0.4.0, the compiler performs **ownership analysis on closure captures**. Closures that capture variables from their enclosing scope are subject to the same ownership rules as regular code. The compiler tracks what each closure captures and enforces move/borrow rules on those captures.

### Capture After Move (E0281)

A closure cannot capture a variable that has already been moved:

```rust
fn main() {
    let data: String = "hello"
    let f1 = |x: Int| -> Int { println(data); return x }
    // data was captured (moved) by f1
    let f2 = |x: Int| -> Int { println(data); return x }
    // ERROR E0281: variable `data` was moved and cannot be captured by this closure
}
```

**Fix:** Clone the value before the first capture, or restructure so only one closure uses it.

### Capture Moves Variable (E0282)

When a closure captures a non-Copy variable by move, that variable becomes unavailable in the enclosing scope:

```rust
fn main() {
    let data: String = "hello"
    let f = |x: Int| -> Int { println(data); return x }
    // data was moved into f's capture
    let result: String = data + " world"
    // ERROR E0282: closure captures `data` by move, making it unavailable after this point
}
```

**Fix:** Clone the value before creating the closure, or use `ref` borrowing if applicable.

### Double Capture (E0283)

Two closures in the same scope cannot both capture the same non-Copy variable:

```rust
fn main() {
    let data: String = "hello"
    let f1 = |x: Int| -> Int { println(data); return x }
    let f2 = |x: Int| -> Int { println(data); return x }
    // ERROR E0283: variable `data` cannot be captured by two closures
}
```

**Fix:** Clone the value so each closure gets its own copy.

### Copy Types Are Fine

Primitive types (`Int`, `Bool`, `Float64`, `Byte`) are implicitly Copy and can be captured by any number of closures without restriction:

```rust
fn main() {
    let n: Int = 42
    let f1 = |x: Int| -> Int { return x + n }
    let f2 = |x: Int| -> Int { return x * n }
    // OK — Int is Copy, so n is not moved
}
```

## Next Steps

- [Generics](generics.md) — generic types and generic functions
- [Modules and Imports](modules-and-imports.md) — multi-file programs and the standard library
- [Data Types and Pattern Matching](data-types.md) — structs, enums, and `match`
