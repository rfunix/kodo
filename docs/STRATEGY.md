# Kōdo: Evolution and Differentiation Strategy

This document defines the architectural guidelines for Kōdo to remain the definitive language for AI Agents, distinguishing itself from traditional languages like Go, Rust, or Zig.

## 1. Structured Concurrency (Closed-Scope Model) — PLANNED
Unlike Go (leaky goroutines) or Rust (complex async/await), Kōdo implements **Structured Concurrency**.
- **Guideline:** No thread or task can outlive the scope that created it.
- **Implementation:** `parallel { ... }` blocks where the compiler guarantees the joining of all executions at the end of the block.
- **AI Advantage:** Eliminates resource leaks and deadlocks that agents often cause by forgetting to close channels or manage lifetimes.
- **Current state:** `spawn` with captured variables and `actor` with state/message passing are DONE. Full structured concurrency (`parallel {}` blocks, channels) is planned for v2.

## 2. The Intent System (The "Brain" of Kōdo) — DONE
Kōdo's biggest innovation. While other languages ask "How," Kōdo focuses on "What."
- **Guideline:** The `intent` block is not just a macro; it is a contractual promise.
- **Mechanism:** The `Resolver` maps high-level intents to canonical, verified implementations (e.g., `intent serve_http` -> verified server).
- **Verification:** Every intent resolution must be validated against the `requires`/`ensures` contracts of the functions it utilizes.
- **Current state:** 3 resolvers implemented (`console_app`, `math_module`, `serve_http`). Intent blocks are expanded to concrete code at the AST level with full type checking.

## 3. Refinement Types (Contracts as Types) — PLANNED
Kōdo must evolve so that contracts are not just runtime asserts but part of the type system.
- **Example:** `type Port = Int requires { self > 0 && self < 65535 }`.
- **Differentiation:** This allows the AI to "know" the constraints of a piece of data just by looking at its type, without needing to parse the function's internal logic.
- **Current state:** Contracts are verified both statically (Z3 SMT, DONE) and at runtime (DONE). Refinement types as a type-system feature are planned.

## 4. Transparency and Auditability (Trust Chains) — DONE
Kōdo binaries are not "black boxes."
- **Guideline:** Maintain and expand the `--describe` functionality.
- **Evolution:** The binary must contain the provenance graph (which agents wrote which functions), allowing security systems to block the execution of code written by AIs with low confidence scores.
- **Current state:** `--describe`, `kodoc describe`, compilation certificates with SHA-256, `@authored_by`/`@confidence`/`@reviewed_by` with compiler enforcement — all implemented.

---

## Development Prompt (Copy and use with Claude/Gemini)

> "Act as a Senior Compiler Engineer specializing in Rust and Language Theory. We are working on Kōdo, an agent-first language. Based on `docs/PRODUCT.md` and `docs/STRATEGY.md`, help me implement the next phase of the project: [SPECIFY HERE: e.g., The Intent Resolver or Z3 SMT Integration]. Focus on maintaining the LL(1) grammar and structured JSON error output so that I can continue using you to maintain this codebase autonomously."
