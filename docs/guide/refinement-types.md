# Refinement Types

Refinement types add constraints to existing types, creating subtypes that are validated at runtime and can be verified statically by Z3. They are one of Kōdo's most powerful features for correctness.

## What Are Refinement Types?

A refinement type narrows an existing type with a boolean constraint. Any value of the refined type must satisfy the constraint:

```rust
// A port number must be between 1 and 65534
type Port = Int requires { self > 0 && self < 65535 }

// A percentage must be between 0 and 100
type Percentage = Int requires { self >= 0 && self <= 100 }

// Age must be non-negative
type Age = Int requires { self >= 0 }
```

The `self` keyword refers to the value being constrained. The `requires` block is checked at runtime when a value is assigned to the refined type.

## Using Refined Types

Refined types work like their base types — they can be used as parameters, return types, and local variables:

```rust
fn serve(port: Port) -> Int {
    return port
}

fn main() {
    let p: Port = 8080       // OK — 8080 satisfies self > 0 && self < 65535
    let result: Int = serve(p)
    print_int(result)
}
```

## Type Aliases Without Constraints

You can also create simple aliases without a constraint:

```rust
type Name = String
type UserId = Int
```

These are transparent aliases — `Name` and `String` are interchangeable. Adding a `requires` block turns them into proper refinement types.

## SMT Verification

When a function receives a refined type parameter, Z3 uses the refinement constraint as an **assumption** to prove the function's own contracts. This is where refinement types become powerful:

```rust
type Port = Int requires { self > 0 && self < 65535 }

// Z3 can prove this contract automatically because Port guarantees port > 0
fn serve(port: Port) -> Int
    requires { port > 0 }
{
    return port
}
```

With `--contracts=static`, the compiler sends the refinement constraint to Z3 as an axiom. Z3 then proves that `port > 0` is always true when `port` is a `Port` — no runtime check needed.

### Combining Refined Parameters

Multiple refined parameters compound their guarantees:

```rust
type Percentage = Int requires { self >= 0 && self <= 100 }

fn combine_pct(pct: Percentage, pct2: Percentage) -> Int {
    // Z3 knows: pct >= 0 && pct <= 100 && pct2 >= 0 && pct2 <= 100
    // So it can prove: pct + pct2 >= 0
    let total: Int = pct + pct2
    return total
}
```

## Practical Patterns

### Domain Modeling

Use refinement types to encode domain rules that would otherwise be buried in validation code:

```rust
type Email = String requires { self.length() > 0 }
type PositiveInt = Int requires { self > 0 }
type Latitude = Float64 requires { self >= -90.0 && self <= 90.0 }
type Longitude = Float64 requires { self >= -180.0 && self <= 180.0 }
```

### API Boundaries

Refine types at API boundaries to catch invalid input early:

```rust
type HttpStatus = Int requires { self >= 100 && self <= 599 }
type PageSize = Int requires { self > 0 && self <= 100 }

fn paginate(page: PositiveInt, size: PageSize) -> List<String> {
    // Both parameters are guaranteed valid by their types
    let offset: Int = (page - 1) * size
    return list_new()
}
```

## Relationship with Contracts

Refinement types and function contracts (`requires`/`ensures`) complement each other:

| Feature | Purpose | Scope |
|---------|---------|-------|
| Refinement types | Constrain a type's valid values | Every use of the type |
| `requires` | Precondition on a specific function call | Single function |
| `ensures` | Postcondition on a function's return value | Single function |

Refinement types are reusable — define `Port` once, use it everywhere. Contracts are local — they express function-specific guarantees.

## Compilation Modes

| Mode | Refinement behavior |
|------|-------------------|
| `--contracts=runtime` (default) | Constraint checked at runtime on assignment |
| `--contracts=static` | Z3 attempts to prove constraint statically; falls back to runtime |
| `--contracts=none` | Constraints are skipped (use for trusted builds) |
| `--contracts=recoverable` | Constraint violation returns an error instead of aborting |

## Next Steps

- [Contracts](contracts.md) — function-level `requires` and `ensures`
- [Ownership](ownership.md) — linear ownership with `own`, `ref`, `mut ref`
- [Agent Traceability](agent-traceability.md) — `@authored_by`, `@confidence` annotations
