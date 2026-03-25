# Research — 2026-03-26: Zig Self-Hosting & AI-Driven Contract Verification

## Topics
1. Zig 0.15.1 — self-hosted compiler progress
2. DafnyPro — LLM-assisted automated contract verification

---

## 1. Zig 0.15.1 — Self-Hosted Compiler

### Findings

- **x86_64 backend now default** for Debug builds on Linux and macOS. Passes 1987 behavior tests (vs 1980 for LLVM backend — self-hosted is now *more correct* than LLVM).
- **Compilation pipeline parallelized**: Machine code generation runs in parallel with everything else for self-hosted backends.
- **aarch64 next**: Expected to be accelerated by their new Legalize pass.
- Jumped from #149 to #61 on TIOBE index.

### Relevance to Kōdo: MEDIUM

Kōdo has completed self-hosted lexer (v0.17.0) and parser (v1.0.0) in Kōdo. The next milestone is bootstrap compiler (v2.0.0). Zig's experience shows:

1. **Self-hosted can exceed LLVM quality**: Zig's x86_64 backend passes more tests than LLVM. This validates Kōdo's Cranelift-first strategy — a simpler backend can be higher quality.
2. **Parallelized pipeline**: Kōdo's codegen could benefit from parallelizing compilation stages.
3. **Architecture-specific backends**: Zig prioritized x86_64 first, aarch64 second. Kōdo should do the same for bootstrap.

### Recommendation: MONITOR
- Track Zig's bootstrap experience for lessons applicable to Kōdo v2.0
- Consider parallelizing Cranelift codegen pipeline

---

## 2. DafnyPro — LLM-Assisted Contract Verification

### Findings

This is the most significant finding for Kōdo's roadmap:

- **DafnyPro**: Using Claude Sonnet 3.5, achieves **86% correct proofs** on DafnyBench — 16pp improvement over previous state of the art.
- **dafny-annotator**: Tool using LLMs + search strategies to automatically add logical annotations to Dafny methods. Goal: specify *what* you want, system figures out the proof.
- **miniF2F-Dafny**: First translation of miniF2F math benchmark to Dafny. Dafny's automation verifies **40.6% with empty proofs** (no manual steps needed).
- **Automated contract generation**: Systems now generate Dafny code WITH contracts, then verify automatically. Applied to railway protection systems.
- **Key stat**: Automated contract inference now routinely attains **>80% accuracy** on large benchmarks.

### Relevance to Kōdo: CRITICAL

This is the strongest signal yet that AI-driven contract verification is production-viable. Direct implications for Kōdo:

1. **`kodoc annotate` command**: Like dafny-annotator, Kōdo could offer a command that auto-generates `requires`/`ensures` clauses for functions. Workflow: write function → `kodoc annotate` → review suggestions → accept/reject.

2. **Contract inference via Z3 + LLM**: Combine Kōdo's existing Z3 integration with an LLM to:
   - LLM suggests candidate contracts based on function body
   - Z3 verifies the candidates are consistent
   - Present verified contracts to the agent/human

3. **MCP tool for contract suggestion**: Extend `kodo_mcp` with a `kodo.suggest_contracts` tool that agents can call during the error→fix→recompile loop.

4. **Competitive moat**: If Kōdo offers AI-assisted contract generation that Rust cannot (Rust has no contracts infrastructure), this becomes the killer feature for agent adoption.

### Recommendation: IMPLEMENT (HIGH PRIORITY)
- Design `kodoc annotate` command as proof-of-concept
- Start with simple patterns: null checks → `requires { x != 0 }`, array bounds → `requires { index < length }`
- Integrate Z3 validation of AI-suggested contracts
- Create GitHub issue for this feature

---

## Summary of Actions

| Finding | Priority | Action |
|---------|----------|--------|
| AI-driven contract inference (DafnyPro) | CRITICAL | Design `kodoc annotate` — create issue |
| MCP `kodo.suggest_contracts` tool | HIGH | Extend MCP server |
| Zig parallelized pipeline | LOW | Monitor for v2.0 |

---

## Sources

- [Zig 0.15.1 Release Notes](https://ziglang.org/download/0.15.1/release-notes.html)
- [Zig Devlog 2025](https://ziglang.org/devlog/2025/)
- [DafnyPro: LLM-Assisted Verification — POPL 2026](https://popl26.sigplan.org/details/dafny-2026-papers/12/DafnyPro-LLM-Assisted-Automated-Verification-for-Dafny-Programs)
- [miniF2F-Dafny — POPL 2026](https://popl26.sigplan.org/details/dafny-2026-papers/16/MiniF2F-Dafny-LLM-Guided-Mathematical-Theorem-Proving-via-Auto-Active-Verification)
- [dafny-annotator: AI-Assisted Verification](https://dafny.org/blog/2025/06/21/dafny-annotator/)
- [Dafny 2026 Workshop — POPL 2026](https://popl26.sigplan.org/home/dafny-2026)
