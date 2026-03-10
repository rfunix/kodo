# Best Practices from Open-Source Languages

Research conducted on Rust, Zig, Go, Swift, Kotlin, Vale, Austral, Roc, and Gleam.
Focus: actionable insights for the Kōdo compiler.

---

## 1. Type System & Ownership

### From Austral: Linear Types with a 600-Line Borrow Checker

Austral uses a **two-universe model** (Free/Linear) with lexical scoping. The borrow checker is ~600 lines — no lifetime inference, no NLL, no regions. Linear types must be consumed exactly once; forgetting to consume is a compile error.

**Recommendation:** Adopt the Free/Linear universe model for `own`/`ref`/`mut`. Start with lexical scoping (no lifetimes). This is dramatically simpler than Rust's approach and sufficient for Kōdo's goals.

### From Vale: Progressive Disclosure

Vale's region borrow checker is **opt-in**. Programs work without annotations; regions are added for performance. `pure` functions automatically eliminate ownership checks.

**Recommendation:** Make ownership progressive — `own` by default, `ref`/`mut` annotations only when needed. `pure` functions in Kōdo could skip ownership verification.

### From Swift/Kotlin: Flow Typing (Smart Casts)

After `if x != null`, Kotlin automatically narrows the type. Swift does similar with `if let`.

**Recommendation:** Implement flow typing in the type checker. After `match`/`if let` on `Option<T>`, narrow the type automatically. Connect this with contracts: `ensures { result implies x.is_some() }` enables smart casts.

### From Swift: Optional Sugar

Swift's `?.` (optional chaining), `??` (nil coalescing), and `T?` syntax make optionals ergonomic without sacrificing safety.

**Recommendation:** Add `T?` as alias for `Option<T>`, `?.` for chaining, `??` for defaults. **Do NOT add force unwrap (`!`/`!!`)** — it violates Kōdo's "correct by construction" principle.

---

## 2. Error Handling

### From Zig: Error Sets as Documentation

Zig's error unions make errors visible in signatures. Error sets can be merged with `||`. `errdefer` handles cleanup on error paths.

**Recommendation:** Keep `Result<T, E>` as primary mechanism. Add `?` operator for propagation. Consider `errdefer` equivalent for cleanup in error paths. Always require explicit error types (never untyped `throws`).

### From Go: Errors as Structured Values

Go errors are values, not exceptions. The `if err != nil` pattern forces handling at every point.

**Recommendation:** Extend error codes (E01xx, E02xx) to runtime errors. Every runtime error should have a code, message, and structured context — agent-friendly by design.

---

## 3. Compiler Architecture

### From Zig: InternPool for Types

Zig deduplicates all types and values in a canonical pool. Type comparison becomes integer comparison.

**Recommendation:** Adopt an InternPool in `kodo_types`. Each type gets a unique index. Comparison by index is O(1).

### From Zig: Lazy Analysis

Zig's Sema only analyzes functions reachable from the entry point.

**Recommendation:** Only analyze and codegen functions reachable from `main`. Reduces compile time and binary size.

### From Swift SIL: Layered MIR

Swift has Raw SIL → Canonical SIL → Optimized SIL, each with different invariants.

**Recommendation:** Consider splitting `kodo_mir` into Raw MIR (direct from AST) and Canonical MIR (after contract injection, ownership checks). Process functions bottom-up for better inlining decisions.

### From Go: Walk/Desugaring Phase

Go has an explicit Walk phase that desugars high-level constructs (switch → binary search, channel ops → runtime calls) before SSA generation.

**Recommendation:** Add an explicit desugaring pass before MIR → Cranelift. Transform contracts, match expressions, and future features (channels, closures) into simpler primitives.

### From Rust: Diagnostic Trait + Builder Pattern

Rustc uses `DiagCtxt` with deferred emission and composable labels/notes/suggestions. Suggestions have confidence levels (`MachineApplicable`, `MaybeIncorrect`).

**Recommendation:** Create a unified `Diagnostic` trait with `.with_label()`, `.with_note()`, `.with_suggestion()`. Add `Applicability` levels so AI agents can auto-apply `MachineApplicable` fixes.

### From Rust: `kodoc explain E0xxx`

Rustc maintains per-error Markdown explanations accessible via `rustc --explain E0200`.

**Recommendation:** Add `kodoc explain E0100` subcommand. Embed explanations as `const` strings.

---

## 4. Concurrency

### From Go: M:N Scheduler with Work-Stealing

Go's G-M-P model multiplexes goroutines over OS threads. 2 KB initial stacks. Channels are buffer + mutex + waiter queues.

**Recommendation:** For Kōdo's `parallel` blocks, use M:N scheduling with work-stealing. Channels as circular buffer + mutex + waiter queues.

### From Swift: Actor Isolation

Swift actors isolate mutable state. External access requires `await`. `Sendable` marks types safe for cross-task transfer.

**Recommendation:** Consider `actor` keyword for isolated state. Built-in `Sendable` trait. Compile-time enforcement of isolation boundaries.

### From Kotlin: CPS + State Machines for Coroutines

Kotlin transforms suspend functions into state machines. Each suspension point becomes a label. Local variables surviving suspension are saved as continuation fields.

**Recommendation:** Use CPS + state machine transformation in Cranelift (no LLVM coroutine support needed). Represent continuations as linear types (consumed exactly once).

### From Go: Structured Concurrency as Language Primitive

Go's `errgroup` and `context` prove that library-level structured concurrency is fragile — goroutines leak.

**Recommendation:** Make structured concurrency a language primitive (`parallel` blocks). Tasks cannot escape parent scope. Cancellation is automatic.

---

## 5. Testing

### From Rust/Go: UI Tests with Error Comments

Both rustc and Go's compiler use source files with `// ERROR` comments. The test harness compiles and verifies errors match.

**Recommendation:** Adopt `// ERROR E0101: "message"` pattern in `.ko` test files. Closes the loop with existing error codes.

### From Zig: Inline Test Blocks

Zig's `test "name" { ... }` blocks live in the same file as code. `zig test` discovers and runs them. Tests are omitted from production binaries.

**Recommendation:** Consider native `test` blocks in Kōdo. Co-located tests are easier for agents to discover. Extend `.ko.cert.json` with test results.

### From Gleam: Snapshot Testing with cargo-insta

Gleam uses cargo-insta for all compiler output testing.

**Recommendation:** Expand insta usage for error messages, MIR output, and codegen output. Normalize paths (`$DIR`) in snapshots for CI stability.

### From Go: Fuzzing

Go 1.18+ has built-in coverage-guided fuzzing.

**Recommendation:** Expand proptest coverage to type checker and codegen — generate random valid programs, compile, execute, verify no crashes.

---

## 6. Standard Library & Tooling

### From Austral: Capabilities as Linear Types

Capabilities are linear values representing irrevocable permission to access resources. Cannot be duplicated, must be explicitly surrendered.

**Recommendation:** Combine capabilities with Kōdo's `meta` block:
```
meta {
    purpose: "Process user data"
    capabilities: [filesystem.read("/data"), network.none]
}
```
The compiler verifies no undeclared capabilities are used. Certificates record exactly what a module accesses.

### From Go: Mandatory Formatter

`gofmt` eliminates style debates and enables automatic refactoring.

**Recommendation:** Implement `kodofmt` early. Canonical formatting enables tooling ecosystem.

### From Go: Unused Imports as Errors

Go rejects programs with unused imports — keeps dependency trees precise.

**Recommendation:** Enforce unused import detection in the type checker.

### From Roc: Effects via Platform

All Roc functions are pure. IO comes from a "platform" that provides capabilities.

**Recommendation:** The Kōdo runtime can function as a platform providing capabilities. Pure functions generate stronger certificates (determinism guaranteed).

---

## 7. Philosophy

### From Zig: No Hidden Control Flow

No operator overloading, no properties/getters, no implicit allocations. "If code doesn't appear to jump to call a function, it doesn't."

**Recommendation:** Maintain this principle. Contracts should never be silently removed in release builds. The agent must know contracts are active.

### From Go: Simplicity as Feature

25 keywords. No inheritance, no exceptions, no default arguments, no implicit conversions. "Simplicity is complicated."

**Recommendation:** Keep LL(1) grammar. No default arguments. No implicit conversions. Each feature must pass: "can an AI agent use this without ambiguity?"

### From Gleam: Omitting Features is a Feature

No type classes, no dependent types, no mutation.

**Recommendation:** Keep the language small enough for an agent to model completely.

---

## Priority Matrix

### Tier 1: High Impact, Achievable Now

| Action | Source | Crate |
|--------|--------|-------|
| Unified `Diagnostic` trait + builder | Rust | `kodo_ast` or new `kodo_diagnostics` |
| `kodoc explain E0xxx` | Rust | `kodoc` |
| UI tests with `// ERROR` comments | Rust/Go | `kodoc/tests` |
| Expand snapshot testing | Gleam | all crates |
| `set_srcloc()` in Cranelift codegen | Rust | `kodo_codegen` |

### Tier 2: High Impact, Medium Effort

| Action | Source | Crate |
|--------|--------|-------|
| Flow typing / smart casts | Swift/Kotlin | `kodo_types` |
| Optional sugar (`T?`, `?.`, `??`) | Swift/Kotlin | `kodo_parser`, `kodo_types` |
| `?` error propagation operator | Rust/Zig | `kodo_parser`, `kodo_types`, `kodo_mir` |
| InternPool for types | Zig | `kodo_types` |
| Desugaring pass before codegen | Go | `kodo_mir` |
| `kodofmt` formatter | Go | new crate |

### Tier 3: Strategic, High Effort

| Action | Source | Crate |
|--------|--------|-------|
| Free/Linear type universes | Austral | `kodo_types` |
| Lexical borrow checker | Austral | `kodo_types` or `kodo_mir` |
| CPS state machines for concurrency | Kotlin | `kodo_mir`, `kodo_codegen` |
| Actor isolation | Swift | `kodo_types`, `kodo_mir` |
| Capability system in meta blocks | Austral/Roc | `kodo_types`, `kodoc` |
| Contracts feeding smart casts | Kotlin/Swift | `kodo_types` |

---

## Sources

### Rust
- [Errors and lints - Rust Compiler Dev Guide](https://rustc-dev-guide.rust-lang.org/diagnostics.html)
- [Monomorphization - Rust Compiler Dev Guide](https://rustc-dev-guide.rust-lang.org/backend/monomorph.html)
- [Polonius revisited - baby steps](https://smallcultfollowing.com/babysteps/blog/2023/09/22/polonius-part-1/)
- [UI tests - Rust Compiler Dev Guide](https://rustc-dev-guide.rust-lang.org/tests/ui.html)

### Zig
- [Why Zig - No hidden control flow](https://ziglang.org/learn/why_zig_rust_d_cpp/)
- [What is Zig's Comptime?](https://kristoff.it/blog/what-is-zig-comptime/)
- [Mitchell Hashimoto - Sema: ZIR to AIR](https://mitchellh.com/zig/sema)

### Go
- [Go Scheduler - Ardan Labs](https://www.ardanlabs.com/blog/2018/08/scheduling-in-go-part2.html)
- [GC Shape Stenciling Proposal](https://go.googlesource.com/proposal/+/refs/heads/master/design/generics-implementation-gcshape.md)
- [Go at Google: Language Design](https://go.dev/talks/2012/splash.article)

### Swift
- [Swift Concurrency Manifesto - Chris Lattner](https://gist.github.com/lattner/31ed37682ef1576b16bca1432ea9f782)
- [SIL Documentation](https://apple-swift.readthedocs.io/en/latest/SIL.html)
- [Typed Throws (SE-0413)](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0413-typed-throws.md)

### Kotlin
- [Inside Kotlin Coroutines: State Machines](https://www.droidcon.com/2025/11/24/inside-kotlin-coroutines-state-machines-continuations-and-structured-concurrency/)
- [KEEP: Kotlin Contracts](https://github.com/Kotlin/KEEP/blob/master/proposals/kotlin-contracts.md)

### Vale/Austral/Roc/Gleam
- [Vale - Generational References](https://verdagon.dev/blog/generational-references)
- [Design of the Austral Compiler](https://borretti.me/article/design-austral-compiler)
- [How Capabilities Work in Austral](https://borretti.me/article/how-capabilities-work-austral)
- [Roc - Platforms and Apps](https://www.roc-lang.org/platforms)
