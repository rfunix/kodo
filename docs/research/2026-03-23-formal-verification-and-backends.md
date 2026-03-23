# Research — 2026-03-23: Formal Verification & Compiler Backends

## Topics
1. Lean 4 and AI-driven theorem proving
2. Cranelift production-ready initiative
3. LLVM 20/21 release highlights

---

## 1. Lean 4 + AI-Driven Theorem Proving

### Findings

The intersection of AI and formal verification has accelerated dramatically:

- **AlphaProof** (Google DeepMind): Proved mathematical statements in Lean 4 at IMO silver medal level — first AI to achieve medal-worthy performance on formal math competition problems.
- **DeepSeek-Prover-V2**: Open-source LLM for automated theorem proving in Lean 4, using recursive proving pipeline powered by DeepSeek-V3.
- **Goedel-Prover-V2** (Princeton): State-of-the-art in automated theorem proving, achieving remarkable results with models 80x smaller than competitors.
- **Lean4Lean**: First complete typechecker for Lean 4 written in Lean itself, running 20-50% slower than the reference implementation.

### Relevance to Kōdo: HIGH

Kōdo uses Z3 for static contract verification. The trend of AI-assisted proof generation suggests:

1. **Contract auto-verification**: An AI model could automatically generate Z3 proofs for contracts that the solver alone can't verify, bridging the gap between runtime and static verification.
2. **Contract suggestion**: AI could suggest `requires`/`ensures` clauses based on function body analysis, similar to how Goedel-Prover suggests lemmas.
3. **Self-verifying compiler**: Long-term, Kōdo could verify its own type system soundness using Lean-like techniques (cf. Lean4Lean).

### Recommendation: MONITOR + RESEARCH
- Short-term: Investigate using LLMs to suggest contracts for Kōdo functions
- Medium-term: Research AI-assisted Z3 proof generation for complex contracts
- Long-term: Consider formal soundness proofs for Kōdo's type system

---

## 2. Cranelift Production-Ready Initiative

### Findings

- Cranelift is being pushed toward **production-ready status** for Rust development (cargo test, cargo run).
- On large projects (Zed, Tauri, hickory-dns): **~20% reduction in codegen time**, translating to **~5% total compilation speedup** for clean builds.
- Supported architectures: x86-64, AArch64, RISC-V, IBM z/Architecture.
- Wasmtime v28.0 (Dec 2025): New compile-time optimization option, new first-class DSL type.

### Relevance to Kōdo: MEDIUM

Kōdo uses Cranelift 0.129.1 as default backend. Implications:

1. **Performance gains**: As Cranelift matures, Kōdo's default compilation speed improves automatically.
2. **Production confidence**: Once Cranelift is production-ready for Rust, the same confidence applies to Kōdo's codegen.
3. **New optimization options**: v28.0's compile-time optimization option could be exposed via `kodoc build --opt-level`.

### Recommendation: MONITOR
- Track Cranelift releases for codegen improvements
- Consider upgrading cranelift-* dependencies when stable releases align
- Test new optimization options when available

---

## 3. LLVM 20.1 and 21.1 Releases

### Findings

**LLVM 20.1** (March 2025):
- Expanded C++26 and C23 support
- Significant GPU backend improvements (AMD, NVIDIA)
- Enhanced RISC-V and AArch64 codegen
- Sharper static analysis tools

**LLVM 21.1** (August 2025):
- New AMD GFX1250 target (RDNA4)
- NVIDIA GB10 Superchip support
- New pointer arithmetic optimizations on null pointers
- Compiler diagnostic enhancements
- Various RISC-V backend improvements

### Relevance to Kōdo: MEDIUM

Kōdo uses inkwell 0.8 with LLVM 21.1 for the release backend:

1. **Already on latest**: Kōdo is using LLVM 21.1 — no upgrade needed.
2. **Pointer optimizations**: New null pointer arithmetic optimizations could benefit Kōdo's ownership model (own/ref/mut null checks).
3. **Diagnostic enhancements**: LLVM's improved diagnostics could surface better error messages through Kōdo's codegen layer.

### Recommendation: MONITOR
- Kōdo is already on LLVM 21.1, no action needed
- Watch for LLVM 22 release (expected early 2026) for potential improvements

---

## Summary of Actions

| Finding | Priority | Action |
|---------|----------|--------|
| AI-driven contract verification | High | Research feasibility of AI-suggested contracts |
| Cranelift production-ready | Low | Auto-benefits, track releases |
| LLVM 21.1 features | Low | Already on latest, monitor LLVM 22 |

---

## Sources

- [Lean4: How the theorem prover works - VentureBeat](https://venturebeat.com/ai/lean4-how-the-theorem-prover-works-and-why-its-the-new-competitive-edge-in-ai)
- [Lean4Lean: Verifying a Typechecker - arXiv](https://arxiv.org/abs/2403.14064)
- [Major Breakthroughs in Lean 4-Based Auto-Formalized Mathematics](https://www.cs.virginia.edu/~rmw7my/Courses/AgenticAISpring2026/Major%20Breakthroughs%20in%20Lean%204-Based%20Auto-Formalized%20Mathematics.html)
- [Production-ready Cranelift - Rust Project Goals](https://rust-lang.github.io/rust-project-goals/2025h2/production-ready-cranelift.html)
- [LLVM 21.1 Released - Phoronix](https://www.phoronix.com/news/LLVM-21.1-Released)
- [LLVM 20.1.0 Release Notes](https://releases.llvm.org/20.1.0/docs/ReleaseNotes.html)
