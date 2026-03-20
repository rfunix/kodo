---
name: Bug Report
about: Report a bug in the Kōdo compiler, runtime, or tooling
title: "[BUG] "
labels: ["bug", "needs-triage"]
assignees: []
---

## Description

A clear and concise description of the bug.

## Reproduction

**Minimal `.ko` source file that reproduces the issue:**

```ko
module repro {
    meta { version: "0.1.0" }

    fn main() -> Int {
        // paste minimal reproducer here
        return 0
    }
}
```

**Command used:**

```bash
kodoc check repro.ko
# or: kodoc build repro.ko
# or: kodoc run repro.ko
```

**Full compiler output (copy-paste, do not screenshot):**

```
paste output here
```

## Expected behavior

What you expected to happen.

## Actual behavior

What actually happened. Include the full error message and any stack trace.

## Environment

| Field | Value |
|-------|-------|
| `kodoc --version` | |
| OS / architecture | |
| Rust toolchain (`rustc --version`) | |
| Z3 version (if using `--contracts=static`) | |
| Feature flag / contracts mode | `runtime` / `static` / `none` |

## Additional context

Any other context about the problem (related issues, workarounds you tried, etc.).

---

> **Security vulnerability?** Do NOT open a public issue — see [SECURITY.md](../../SECURITY.md) instead.
