# Agent Traceability

Kōdo is the first programming language with built-in support for tracking AI agent authorship and enforcing trust policies. This enables organizations to maintain accountability over AI-generated code.

## Annotations

### `@authored_by`

Declares who wrote a function — human or AI agent:

```rust
@authored_by(agent: "claude")
fn ai_generated() -> Int {
    return 42
}
```

### `@confidence`

Declares how confident the author is in the correctness of the code, on a scale from 0.0 to 1.0:

```rust
@confidence(0.95)
fn well_tested() -> Int {
    return 42
}
```

### `@reviewed_by`

Declares that a human has reviewed the code:

```rust
@reviewed_by(human: "alice")
fn human_approved() -> Int {
    return 42
}
```

### `@security_sensitive`

Marks a function as security-sensitive, requiring formal contracts:

```rust
@security_sensitive
fn validate_input(data: String) -> Bool
    requires { data != "" }
{
    return true
}
```

## Trust Policies

### Low Confidence Review (E0260)

Functions with `@confidence(X)` where X < 0.8 **must** have `@reviewed_by(human: "...")`:

```rust
// ERROR: @confidence(0.5) < 0.8 without review
@confidence(0.5)
fn risky() -> Int { return 42 }

// OK: low confidence but reviewed
@confidence(0.5)
@reviewed_by(human: "alice")
fn reviewed_risky() -> Int { return 42 }
```

### Security-Sensitive Contracts (E0262)

Functions marked `@security_sensitive` **must** have at least one `requires` or `ensures` clause.

## Confidence Propagation

Kōdo computes **transitive confidence** for each function. A function's computed confidence is the minimum of:
- Its own declared `@confidence` (defaults to 1.0 if not specified)
- The computed confidence of every function it calls

This means confidence propagates through the call chain — a function is only as trustworthy as its least trustworthy dependency.

### Module Confidence Threshold (E0261)

Set `min_confidence` in the `meta` block to enforce a minimum confidence level:

```rust
module secure_app {
    meta {
        purpose: "A security-critical application"
        min_confidence: "0.9"
    }

    @confidence(0.5)
    @reviewed_by(human: "alice")
    fn weak_link() -> Int { return 1 }

    fn main() -> Int {
        return weak_link()  // ERROR E0261: module confidence 0.50 < threshold 0.90
    }
}
```

### Confidence Report

Use `kodoc confidence-report` to inspect confidence across a module:

```bash
kodoc confidence-report my_module.ko

# Output:
# Confidence Report for module `my_module`
# ============================================================
# Overall confidence: 0.50
#
# Function                       Declared   Computed
# ------------------------------------------------------------
# weak_link                          0.50       0.50
# main                               1.00       0.50
```

For JSON output (suitable for AI agent consumption):

```bash
kodoc confidence-report my_module.ko --json
```
