# Research Report: Leanstral, Mojo Linear Types, Cranelift Updates

**Date**: 2026-03-25
**Author**: Kōdo Architect (RESEARCH mode)
**Topics**: AI-assisted formal verification, linear types in Mojo, Cranelift exception handling

---

## 1. Leanstral — Mistral's Open-Source Proof Agent for Lean 4

**Source**: [Mistral AI announcement](https://www.mexc.com/news/941903), [AIToolly analysis](https://aitoolly.com/ai-news/article/2026-03-17-leanstral-mistral-ais-open-source-agent-for-trustworthy-coding-and-formal-proof-engineering)
**Date**: 2026-03-16

### Summary

Mistral AI released Leanstral, the first open-source AI agent built specifically for Lean 4 formal verification. Key specs:
- 120B parameter model, 6B active parameters (mixture of experts)
- Apache 2.0 license
- Optimized for real-world formal repositories, not isolated math problems
- Aims to formally prove implementations against strict specifications

### Applicability to Kōdo

**High relevance.** This validates Kōdo's core thesis: AI agents need formal verification tools. Leanstral proves there's market demand for:
- AI agents that can reason about contracts and proofs
- Languages with machine-verifiable specifications
- The "error → fix → recompile" loop that Kōdo optimizes for

**Kōdo advantage**: While Leanstral requires Lean 4 (a proof assistant), Kōdo embeds contracts directly in the language grammar (`requires`/`ensures`) with Z3 verification. An agent using Kōdo doesn't need a separate proof language — the contracts ARE the specifications.

**Opportunity**: Consider creating a `kodoc verify --ai-assist` mode that uses LLMs to suggest missing contracts, similar to how Leanstral suggests proofs. This would make Kōdo the first general-purpose language where AI agents can both write code AND verify its contracts in one tool.

### Recommendation

**MONITOR** — Leanstral validates our direction but targets a different niche (theorem proving vs general-purpose programming). Watch for:
- Whether Mistral releases a general-purpose version
- Whether other AI labs create formal verification agents
- Integration patterns we could adopt for `kodoc`

---

## 2. Mojo Introduces Linear Types and Typed Errors

**Source**: [Mojo Changelog](https://docs.modular.com/mojo/changelog/), [Modular Blog](https://www.modular.com/blog/the-path-to-mojo-1-0)
**Date**: 2026-03 (v26.2, pre-1.0)

### Summary

Mojo (now at v26.2, 1.0 expected H1 2026) has added several features that overlap with Kōdo:

1. **"Explicitly-destroyed types" (linear types)**: Encode invariants requiring explicit resource handling — similar to Kōdo's `own`/`ref`/`mut` linear ownership model
2. **Typed errors**: Functions specify what type they raise (no stack unwinding) — exactly what Kōdo's Milestone 2 (Custom Error Types) targets
3. **Conditional trait conformance**: Structs declare trait conformances when type parameters satisfy conditions — List, Dict, Set, Optional now use this
4. **Compile-time reflection**: Used for metaprogramming

### Applicability to Kōdo

**Competitive threat — medium.** Mojo targets AI/ML workloads (GPU, inference) while Kōdo targets agent-written general-purpose software. However:

- Mojo's linear types validate that the PL community considers ownership semantics important for modern languages
- Mojo's typed errors are identical to Kōdo's Milestone 2 scope — we should prioritize this
- Mojo does NOT have contracts (`requires`/`ensures`) — this remains Kōdo's unique differentiator
- Mojo does NOT have agent traceability (`@authored_by`, `@confidence`) — also unique to Kōdo

**Key difference**: Mojo is "Python with systems features" for AI workloads. Kōdo is "agent-first language with formal guarantees" for trustworthy software. Different target markets, but feature overlap is growing.

### Recommendation

**IMPLEMENT** (prioritize Milestone 2: Custom Error Types). Mojo shipping typed errors before Kōdo would weaken our narrative. The roadmap already has this at v0.10.0 — consider accelerating.

---

## 3. Cranelift: Exception Handling and New APIs

**Source**: [Wasmer 7.0 release](https://wasmer.io/posts/wasmer-7), [Bytecode Alliance](https://bytecodealliance.org/articles/)
**Date**: 2026-01-30 (Wasmer 7), 2025-12 (Wasmtime v28)

### Summary

Recent Cranelift developments:

1. **Exception handling APIs** (Wasmer 7.0): Full support for WebAssembly exceptions via new Cranelift exception-handling APIs
2. **Experimental Async API**: Available across singlepass, cranelift, and LLVM backends
3. **New optimization option** (Wasmtime v28): Compile-time optimization level control
4. **New first-class DSL type** (Wasmtime v28): Extended Cranelift's internal IR
5. **GC/stack map overhaul** (Wasmtime v27): Infrastructure for garbage collection support

### Applicability to Kōdo

**Medium relevance.** Kōdo uses Cranelift as its primary code generation backend.

- **Exception handling**: Could improve Kōdo's `--contracts=recoverable` mode. Currently, contract violations call `abort()` or use a recoverable wrapper. Cranelift's native exception handling could make recoverable contracts more efficient (no manual stack unwinding)
- **Async API**: Not directly applicable — Kōdo uses its own green thread runtime
- **Optimization options**: Could expose `kodoc build --opt-level` flag using Cranelift's new optimization controls

### Recommendation

**MONITOR** — Exception handling in Cranelift could improve recoverable contracts performance in a future version. Not urgent since current implementation works. Track Cranelift releases for when exception handling is stable outside Wasmer/Wasmtime.

---

## Market Context: AI Agents in 2026

### Key Trends

1. **Long-running autonomous workflows**: Agents now operate on codebases for minutes/hours, not just prompt-response. This validates Kōdo's compilation certificates and confidence tracking.
2. **Multi-agent teams**: Platforms use specialized agent teams. Kōdo's `@authored_by` traceability becomes critical for auditing multi-agent contributions.
3. **Go gaining traction for agents**: "Agents produce valid Go in one shot 95% of the time" — Kōdo should track this metric. Our LL(1) grammar and zero ambiguity should enable similar or better first-shot success rates.
4. **No competitor has contracts+ownership+agent-traceability combined**: Kōdo remains unique in this triple.

### Competitive Landscape

| Feature | Kōdo | Mojo | Rust | Go | Lean 4 |
|---------|------|------|------|-----|--------|
| Contracts (requires/ensures) | ✅ Z3 | ❌ | ❌ | ❌ | ✅ (different) |
| Linear ownership | ✅ own/ref/mut | ✅ (new) | ✅ borrow checker | ❌ | ❌ |
| Typed errors | 🔜 M2 | ✅ (new) | ✅ | ❌ | ✅ |
| Agent traceability | ✅ @authored_by | ❌ | ❌ | ❌ | ❌ |
| Machine-readable errors | ✅ --json-errors | ❌ | ✅ (partial) | ✅ | ❌ |
| AI-first design | ✅ | ❌ (AI workload, not agent) | ❌ | ❌ | ✅ (proof) |

---

## Action Items

1. **Accelerate Milestone 2** (Custom Error Types) — Mojo shipping typed errors creates urgency
2. **Track Leanstral adoption** — If successful, consider `kodoc verify --ai-assist` for contract suggestion
3. **Benchmark agent first-shot success rate** — Compare with Go's claimed 95%
4. **Monitor Cranelift exception handling** — Potential future improvement for recoverable contracts
