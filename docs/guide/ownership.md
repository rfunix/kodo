# Ownership

Kōdo uses a linear ownership system inspired by Rust and based on substructural type systems ([ATAPL] Ch. 1). Every value has a single owner, and ownership can be transferred (moved) or temporarily shared (borrowed).

## Ownership Qualifiers

Kōdo has three ownership qualifiers for function parameters:

| Qualifier | Meaning | Caller retains access? |
|-----------|---------|----------------------|
| `own` | Ownership is transferred to the function | No --the value is moved |
| `ref` | The value is immutably borrowed (shared) | Yes --the caller keeps the value |
| `mut` | The value is mutably borrowed (exclusive) | Yes --but no other borrows allowed |

### Default behavior

By default, parameters use `own` (owned) semantics. When you pass a value to a function taking `own`, the value is **moved** --you cannot use it afterward.

## Copy Types

Primitive types (`Int`, `Bool`, `Float32`, `Float64`, `Byte`) are implicitly **Copy**. They are never moved --assigning or passing them always creates a copy:

```kodo
let x: Int = 42
let y: Int = x    // x is copied, not moved
let z: Int = x    // still fine --x was never moved
```

## Use After Move (E0240)

Once a non-Copy value is moved, attempting to use it is a compile-time error:

```kodo
fn consume(own s: String) {
    println(s)
}

fn main() {
    let greeting: String = "hello"
    consume(greeting)    // greeting is moved here
    println(greeting)    // ERROR E0240: variable 'greeting' was moved
}
```

**Fix:** Use `ref` to borrow instead of moving:

```kodo
fn borrow(ref s: String) {
    println(s)
}

fn main() {
    let greeting: String = "hello"
    borrow(greeting)     // greeting is borrowed, not moved
    println(greeting)    // OK --greeting is still available
}
```

## Borrow Rules

### Multiple `ref` borrows are OK

Multiple shared (immutable) borrows of the same value can coexist:

```kodo
fn read(ref s: String) { println(s) }

fn main() {
    let msg: String = "hello"
    read(msg)    // first ref borrow
    read(msg)    // second ref borrow -- OK
    println(msg) // msg is still alive
}
```

### `mut` borrows are exclusive (E0245, E0246, E0247)

A `mut` borrow grants exclusive mutable access. No other borrows (`ref` or `mut`) may coexist with it:

```kodo
fn two_args(mut a: String, ref b: String) -> Int { return 0 }

fn main() {
    let x: String = "hi"
    two_args(x, x)  // ERROR E0246: cannot borrow 'x' as ref while mutably borrowed
}
```

Two simultaneous `mut` borrows of the same variable are also forbidden (E0247).

### Assign through `ref` is forbidden (E0248)

A `ref` parameter cannot be reassigned --it is an immutable borrow:

```kodo
fn bad(ref x: Int) -> Int {
    x = 42       // ERROR E0248: cannot assign to 'x' because it is borrowed as ref
    return x
}
```

## Move While Borrowed (E0242)

A value cannot be moved while it is actively borrowed within the same expression:

```kodo
fn take(ref a: String, own b: String) -> Int { return 0 }

fn main() {
    let s: String = "hello"
    take(s, s)   // ERROR E0242: cannot move 's' while it is borrowed
}
```

## Borrow Escapes Scope (E0241)

A borrowed reference cannot outlive the scope of the value it references:

```kodo
fn escape(ref s: String) -> String {
    return s     // ERROR E0241: reference cannot escape scope
}
```

## Design Philosophy

Kōdo's ownership system catches the most common ownership bugs (use-after-move, dangling references, aliasing violations) while keeping the rules simple enough for AI agents to reason about deterministically. The borrow rules follow [ATAPL] Ch. 1 on linear and affine type systems.
