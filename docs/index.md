# Kōdo Documentation

Welcome to the Kōdo documentation. Kōdo is a compiled programming language designed for AI agents to write, reason about, and maintain software — while remaining fully transparent and auditable by humans.

## Learn Kōdo

### Getting Started

- [Installing Kōdo](guide/getting-started.md) — prerequisites, build from source, and your first program
- [A Tour of Kōdo](guide/tour.md) — a quick walkthrough of the language's key features
- [Your First Kōdo Program](guide/getting-started.md#your-first-program) — hello world, compile, and run

### Language

- [Language Basics](guide/language-basics.md) — modules, functions, types, variables, and control flow
- [Data Types and Pattern Matching](guide/data-types.md) — structs, enums, and `match` expressions
- [Generics](guide/generics.md) — generic types and generic functions
- [Error Handling](guide/error-handling.md) — using `Option<T>` and `Result<T, E>` for safe error handling
- [Contracts](guide/contracts.md) — `requires` and `ensures` for runtime verification
- [Modules and Imports](guide/modules-and-imports.md) — multi-file programs and the standard library

### Tools

- [CLI Reference](guide/cli-reference.md) — all `kodoc` commands, flags, and environment variables

## Reference

- [Language Specification](DESIGN.md) — the full language design document
- [Formal Grammar](grammar.ebnf) — LL(1) grammar in EBNF
- [Error Index](error_index.md) — catalog of all compiler error codes
- [Academic References](REFERENCES.md) — foundational theory behind each compiler phase

## Examples

The [`examples/`](../examples/) directory contains compilable programs demonstrating every feature:

| Example | Feature |
|---------|---------|
| [`hello.ko`](../examples/hello.ko) | Minimal program |
| [`fibonacci.ko`](../examples/fibonacci.ko) | Recursion |
| [`while_loop.ko`](../examples/while_loop.ko) | Loops and mutable variables |
| [`contracts_demo.ko`](../examples/contracts_demo.ko) | Runtime contracts |
| [`structs.ko`](../examples/structs.ko) | Struct types |
| [`struct_params.ko`](../examples/struct_params.ko) | Structs as function parameters and return values |
| [`enums.ko`](../examples/enums.ko) | Enum types and pattern matching |
| [`enum_params.ko`](../examples/enum_params.ko) | Enums as function parameters |
| [`generics.ko`](../examples/generics.ko) | Generic enum types |
| [`generic_fn.ko`](../examples/generic_fn.ko) | Generic functions |
| [`option_demo.ko`](../examples/option_demo.ko) | Standard library `Option<T>` |
| [`result_demo.ko`](../examples/result_demo.ko) | Standard library `Result<T, E>` |
| [`multi_file/`](../examples/multi_file/) | Multi-file compilation with imports |
