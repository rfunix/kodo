# Research Report: OpenHands/Open SWE, Rust Contracts Status, Vale

**Date**: 2026-03-27
**Author**: Kōdo Architect (RESEARCH mode)
**Topics**: AI coding agent frameworks, Rust contracts stabilization, Vale generational references

---

## 1. OpenHands & Open SWE — AI Coding Agents Exploding

**Sources**: [OpenHands](https://openhands.dev/), [OpenHands vs SWE-Agent comparison](https://localaimaster.com/blog/openhands-vs-swe-agent), [Open SWE by LangChain](https://byteiota.com/open-swe-tutorial-build-ai-coding-agents-2026/)

### Summary

The autonomous coding agent ecosystem is rapidly maturing:

- **OpenHands** (formerly OpenDevin): 69,580 GitHub stars, open-source AI software engineer that writes code, runs commands, browses web. Near top on SWE-Bench for open-source models.
- **SWE-Agent** (Princeton/Stanford): 18,817 stars, clean research architecture.
- **Open SWE** (LangChain, released 2026-03-17): First open-source framework for building **asynchronous** coding agents. Cloud-based, tasks run while you work on other things.

Key insight: These agents work by receiving errors, fixing code, and recompiling in a loop — **exactly the workflow Kōdo optimizes for**.

### Applicability to Kōdo

**CRITICAL opportunity.** These frameworks are language-agnostic — they work with whatever language they're pointed at. Kōdo's structured JSON errors + fix patches + SARIF output make it the **ideal target language** for these agents.

**Integration path**:
1. Create a Kōdo plugin/tool for OpenHands (most popular framework)
2. The plugin provides: `kodoc check --json-errors` → structured error parsing → `kodoc fix --dry-run` → auto-apply patches
3. Benchmark: run OpenHands on kodo-bench tasks, compare success rate vs Python

### Recommendation

**IMPLEMENT** — Create OpenHands integration. This is the fastest path to proving Kōdo's value proposition: "Agents achieve X% on Kōdo vs Y% on Python, using the same agent framework."

---

## 2. Rust Contracts — Still Unstable, No 2026 Stabilization

**Sources**: [core::contracts docs](https://doc.rust-lang.org/core/contracts/index.html), [Rust contracts RFC draft](https://hackmd.io/@nG8Ewk1OTDS-qIUxGrXyVw/BJ7N-uRLs), [Rust 2026 project goals](https://rust-lang.github.io/rust-project-goals/)

### Summary

- `core::contracts` exists on nightly with `#[requires]` and `#[ensures]` attribute macros
- Still **unstable** — no stabilization RFC filed
- **Not listed** in Rust's 2026 project goals (which focus on const generics, trait solver, scalable vectors)
- RFC draft from 2022 still gathering community feedback
- External tools (Kani, Creusot, Prusti) handle verification but are not part of the language

### Applicability to Kōdo

**Kōdo's advantage holds.** Rust contracts are:
- Runtime-only (no Z3/SMT static verification)
- Attribute macros (not grammar-level like Kōdo's `requires { }`)
- No stabilization timeline — likely years away
- No `@confidence`, no `@authored_by`, no `kodoc annotate`

Even when Rust stabilizes contracts, Kōdo will have:
- Static Z3 verification vs Rust's runtime-only
- Contract inference (`kodoc annotate`)
- Agent traceability (unique to Kōdo)
- SARIF output for IDE/CI integration

### Recommendation

**CITE** in marketing: "Rust has experimental contracts on nightly with no stabilization date. Kōdo has Z3-verified contracts in stable since v0.1.0."

---

## 3. Vale — Generational References (Stalled)

**Sources**: [Vale website](https://vale.dev/), [Generational references blog](https://verdagon.dev/blog/generational-references), [Vale roadmap](https://vale.dev/roadmap)

### Summary

Vale introduces "generational references" — each object has a generation counter, incremented on free. Pointers carry a remembered generation; dereferencing asserts match. This gives memory safety without borrow checking or GC.

- v0.1 (2021): Foundation + generational references
- v0.2 (May 2022): FFI, Higher RAII, Modules
- **No releases since 2022** — project appears stalled
- Planned: Region Borrow Checker, Hybrid-Generational Memory

### Applicability to Kōdo

**LOW relevance.** Vale's approach is interesting technically but:
- Project stalled (no activity in 2+ years)
- Generational references add runtime overhead (generation check on every deref)
- Kōdo's own/ref/mut model is simpler and zero-overhead
- No contracts, no agent features, no ecosystem

### Recommendation

**IGNORE** — Vale is not a competitive threat. The generational references technique is academically interesting but practically stalled.

---

## Action Items

1. **IMPLEMENT**: OpenHands integration for Kōdo — create tool/plugin that uses `kodoc check --json-errors` + `kodoc fix` for automated error→fix loop
2. **CITE**: Rust contracts status in Kōdo positioning ("stable since v0.1.0 vs unstable on nightly")
3. **BENCHMARK**: Run OpenHands on kodo-bench with Kōdo vs Python comparison
