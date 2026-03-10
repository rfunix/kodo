# Ownership

Kōdo uses a linear ownership system inspired by Rust and based on substructural type systems ([ATAPL] Ch. 1). Every value has a single owner, and ownership can be transferred (moved) or temporarily shared (borrowed).

## Ownership Qualifiers

Kōdo has two ownership qualifiers for function parameters:

| Qualifier | Meaning | Caller retains access? |
|-----------|---------|----------------------|
| `own` | Ownership is transferred to the function | No --the value is moved |
| `ref` | The value is borrowed | Yes --the caller keeps the value |

### Default behavior

By default, parameters use `own` (owned) semantics. When you pass a value to a function taking `own`, the value is **moved** --you cannot use it afterward.

## Use After Move (E0240)

Once a value is moved, attempting to use it is a compile-time error:

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

## Move While Borrowed (E0242)

A value cannot be moved while it is actively borrowed:

```kodo
fn consume(own s: String) { }

fn main() {
    let s: String = "hello"
    let r: ref String = s    // s is now borrowed
    consume(s)               // ERROR E0242: cannot move 's' while it is borrowed
}
```

## Borrow Escapes Scope (E0241)

A borrowed reference cannot outlive the scope of the value it references:

```kodo
fn escape() -> ref String {
    let s: String = "local"
    return s                 // ERROR E0241: reference cannot escape scope
}
```

## Design Philosophy

Kōdo's ownership system is deliberately simpler than Rust's borrow checker. The goal is to catch the most common ownership bugs (use-after-move, dangling references) while keeping the rules simple enough for AI agents to reason about deterministically.

Future versions may add:
- Copy semantics for primitive types (Int, Bool, etc.)
- Lifetime annotations for complex borrow patterns
- Mutable borrowing with exclusivity rules
