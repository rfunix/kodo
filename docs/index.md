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
- [Closures](guide/closures.md) — closures, lambda lifting, and higher-order functions
- [Traits](guide/traits.md) — trait definitions and static dispatch
- [Inherent Methods](guide/methods.md) — struct methods without traits
- [String Interpolation](guide/string-interpolation.md) — f-strings with `{expression}` embedding
- [Pattern Matching](guide/pattern-matching.md) — exhaustive match on enums
- [Contracts](guide/contracts.md) — `requires` and `ensures` for runtime verification
- [Modules and Imports](guide/modules-and-imports.md) — multi-file programs and the standard library
- [Ownership](guide/ownership.md) — linear ownership with `own`, `ref`, and `mut`
- [Agent Traceability](guide/agent-traceability.md) — annotations, trust policies, and confidence propagation
- [HTTP & JSON](guide/http.md) — HTTP client and JSON parsing
- [Actors](guide/actors.md) — actor model with state and message passing
- [Concurrency & Spawn](guide/concurrency.md) — spawn with captured variables

### Tools

- [CLI Reference](guide/cli-reference.md) — all `kodoc` commands, flags, and environment variables, including `confidence-report` and `fix`

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
| [`intent_demo.ko`](../examples/intent_demo.ko) | Intent-driven programming |
| [`intent_math.ko`](../examples/intent_math.ko) | Math module intent resolver |
| [`intent_composed.ko`](../examples/intent_composed.ko) | Composing multiple intents |
| [`agent_traceability.ko`](../examples/agent_traceability.ko) | Agent annotations and trust policies |
| [`closures.ko`](../examples/closures.ko) | Closures and direct closure calls |
| [`closures_functional.ko`](../examples/closures_functional.ko) | Higher-order functions and indirect calls |
| [`float_math.ko`](../examples/float_math.ko) | Float64 arithmetic operations |
| [`string_concat_operator.ko`](../examples/string_concat_operator.ko) | String concatenation with `+` operator |
| [`intent_http.ko`](../examples/intent_http.ko) | HTTP intent resolver |
| [`stdlib_demo.ko`](../examples/stdlib_demo.ko) | Standard library math functions |
| [`async_real.ko`](../examples/async_real.ko) | Cooperative `spawn` with deferred execution |
| [`ownership.ko`](../examples/ownership.ko) | Linear ownership with `own` and `ref` |
| [`borrow_rules.ko`](../examples/borrow_rules.ko) | Borrow rules: multiple `ref` borrows |
| [`move_semantics.ko`](../examples/move_semantics.ko) | Move semantics, Copy vs non-Copy types |
| [`list_demo.ko`](../examples/list_demo.ko) | `List<T>` built-in collection |
| [`map_demo.ko`](../examples/map_demo.ko) | `Map<K,V>` built-in collection |
| [`string_demo.ko`](../examples/string_demo.ko) | String methods including `split` |
| [`file_io_demo.ko`](../examples/file_io_demo.ko) | File I/O operations |
| [`contracts_smt_demo.ko`](../examples/contracts_smt_demo.ko) | SMT-verified contracts |
| [`smt_verified.ko`](../examples/smt_verified.ko) | SMT contract verification |
| [`http_client.ko`](../examples/http_client.ko) | HTTP GET and JSON parsing |
| [`async_tasks.ko`](../examples/async_tasks.ko) | Spawn with captured variables |
| [`actors.ko`](../examples/actors.ko) | Actor state and message passing |
| [`actor_demo.ko`](../examples/actor_demo.ko) | Actor demonstration |
| [`type_inference.ko`](../examples/type_inference.ko) | Local type inference for `let` bindings |
| [`intent_database.ko`](../examples/intent_database.ko) | Database intent resolver |
| [`intent_json_api.ko`](../examples/intent_json_api.ko) | JSON API intent resolver |
| [`intent_cache.ko`](../examples/intent_cache.ko) | Cache intent resolver |
| [`intent_queue.ko`](../examples/intent_queue.ko) | Queue intent resolver |
| [`parallel_demo.ko`](../examples/parallel_demo.ko) | Structured concurrency with `parallel {}` |
| [`channel_string.ko`](../examples/channel_string.ko) | Generic typed channels |
| [`refinement_smt.ko`](../examples/refinement_smt.ko) | SMT-verified refinement types |
| [`struct_predicates.ko`](../examples/struct_predicates.ko) | Struct field predicates in contracts |
| [`generic_bounds.ko`](../examples/generic_bounds.ko) | Generic trait bounds (`<T: Ord>`) |
| [`sorted_list.ko`](../examples/sorted_list.ko) | Bounded generics with sorted collections |
| [`methods.ko`](../examples/methods.ko) | Inherent impl blocks |
| [`string_interpolation.ko`](../examples/string_interpolation.ko) | F-string interpolation |
| [`memory_management.ko`](../examples/memory_management.ko) | Reference counting |
| [`tuples.ko`](../examples/tuples.ko) | Tuple types, indexing, and destructuring |
| [`for_in.ko`](../examples/for_in.ko) | For-in loops over collections |
