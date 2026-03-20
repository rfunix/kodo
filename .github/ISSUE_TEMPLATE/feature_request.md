---
name: Feature Request
about: Propose a new language feature, compiler improvement, or tooling enhancement
title: "[FEATURE] "
labels: ["enhancement", "needs-triage"]
assignees: []
---

## Summary

One-sentence description of the feature you are proposing.

## Motivation

Why is this feature needed? Which use case does it enable? For language features,
describe the problem from an **agent-first** perspective: how does this help AI
agents write or verify Kōdo programs?

## Detailed Design

Describe the proposed feature in detail.

**If proposing a new syntax**, show an example `.ko` program:

```ko
module example {
    meta { version: "0.1.0" }

    // proposed new syntax here
}
```

**If proposing a compiler/tooling change**, describe:
- Which compiler phase is affected (lexer / parser / types / contracts / MIR / codegen)
- How it interacts with existing features (ownership, contracts, generics, etc.)
- Any new error codes that would be introduced (see `docs/error_index.md` for ranges)

## Alternatives Considered

What other approaches did you consider? Why is this the best one?

## Academic Reference (optional)

If this feature is grounded in PL theory, cite the relevant paper or textbook
chapter (see `docs/REFERENCES.md` for the project bibliography).

## Checklist

- [ ] I have read `docs/DESIGN.md` and confirmed this does not conflict with the language spec.
- [ ] I have read `CLAUDE.md` and confirmed this aligns with the project principles.
- [ ] I am willing to implement this (or help with it).
