# Research Report: SARIF Diagnostics, Gleam LSP, Carbon Delay, Roc Platforms

**Date**: 2026-03-26
**Author**: Kōdo Architect (RESEARCH mode)
**Topics**: SARIF structured diagnostics, Gleam v1.15, Carbon 0.1 delay, Roc 0.1.0 progress

---

## 1. SARIF Becoming the Standard for Compiler Diagnostics

**Sources**: [GCC 15 improvements](https://developers.redhat.com/articles/2025/04/10/6-usability-improvements-gcc-15), [GCC 16 HTML diagnostics](https://www.webpronews.com/gcc-16-enhances-diagnostics-with-clearer-c-errors-and-html-output/), [C++ SARIF proposal P3358R0](https://www.open-std.org/jtc1/sc22/wg21/docs/papers/2024/p3358r0.html)

### Summary

SARIF (Static Analysis Results Interchange Format) is rapidly becoming the industry standard for machine-readable compiler diagnostics:

- **GCC 15**: Added `-fdiagnostics-add-output=sarif` for SARIF output alongside text
- **GCC 16**: Shifting diagnostics entirely to SARIF, adding experimental HTML output for interactive error reports
- **Clang**: Active RFCs for `-fdiagnostics-format=sarif`
- **C++ WG21**: Formal proposal (P3358R0) to standardize SARIF for C++ diagnostics
- **MSVC**: Already supports SARIF natively

### Applicability to Kōdo

**HIGH relevance.** Kōdo already has `--json-errors` with a custom JSON schema. But SARIF is becoming the universal format that all tooling (IDEs, CI/CD, GitHub Code Scanning, VS Code) understands natively.

**Current state**: Kōdo's `--json-errors` outputs a custom format. Agents that work with multiple languages would benefit from a standard format.

**Opportunity**: Add `--diagnostics-format=sarif` to `kodoc check`/`kodoc build`. This would make Kōdo errors immediately consumable by:
- GitHub Code Scanning (uploads SARIF natively)
- VS Code Problems panel
- Any SARIF-compatible tool
- Multi-language AI agents that already parse SARIF from GCC/Clang

### Recommendation

**IMPLEMENT** — Add SARIF output as an alternative diagnostic format. Low effort (SARIF is JSON with a specific schema), high impact for tooling integration. Keep `--json-errors` for backward compatibility.

---

## 2. Gleam v1.15 — Smarter LSP Autocompletions

**Source**: [Gleam news](https://gleam.run/news/), [Gleam roadmap](https://gleam.run/roadmap/)

### Summary

Gleam v1.15 improvements:
- LSP autocompletions now produce correct qualified annotations (no more invalid code from accepting completions)
- LSP suggests adding missing type parameters to generic types
- Function inlining for performance being worked on
- First Gleam conference (Gleam Gathering 2026) in Bristol
- 2nd most admired language in recent surveys

### Applicability to Kōdo

**MEDIUM relevance.** Two LSP features worth cherry-picking:

1. **Smart qualified completions**: When completing a name from another module, insert the qualified path. Kōdo's LSP should do the same.
2. **Missing type parameter suggestions**: When a generic type is used without parameters, suggest adding them. Could enhance Kōdo's type error fix patches.

### Recommendation

**MONITOR** — Good LSP ideas to incorporate in future Kōdo LSP improvements. Not urgent since Kōdo's LSP already has hover, goto-definition, completions, and code actions.

---

## 3. Carbon 0.1 Delayed — Memory Safety Added to Roadmap

**Source**: [Carbon roadmap](https://github.com/carbon-language/carbon-lang/blob/trunk/docs/project/roadmap.md), [Wikipedia](https://en.wikipedia.org/wiki/Carbon_(programming_language))

### Summary

Google's Carbon language has **pushed back its 0.1 milestone by at least a year** because they added memory safety to the roadmap:

- **Original**: 0.1 expected mid-2025
- **Current**: 0.1 "late 2026 at the earliest", 1.0 "after 2028"
- **Reason**: Adding Rust-style compile-time memory safety guarantees using the type system
- **Approach**: Temporal memory safety following Rust's direction (type system enforcement, no GC/RC overhead)

### Applicability to Kōdo

**Validates Kōdo's design.** Even Google, with massive engineering resources, considers ownership/safety so critical that they delayed their flagship language by a year+ to add it. Kōdo had ownership (own/ref/mut) from the start.

**Competitive intelligence**: Carbon is not a threat to Kōdo (different niche — C++ migration vs agent-first), but the delay confirms the market demands memory safety as table stakes for any new compiled language.

### Recommendation

**IGNORE** as a competitive threat. **CITE** in marketing: "Even Google's Carbon added memory safety after seeing how critical it is — Kōdo had it from day one."

---

## 4. Roc 0.1.0 Coming in 2026 — Platform/Host Separation

**Source**: [Roc website](https://www.roc-lang.org/), [Roc platforms](https://www.roc-lang.org/platforms)

### Summary

Roc is approaching its first numbered release (0.1.0) after a full compiler rewrite:

- Platform/host separation: the host (Rust, Zig, or C++) determines memory allocation, I/O, and process lifecycle
- Application code is purely functional — no direct system access
- This creates a natural sandboxing boundary
- 30,000+ commits but intentionally no numbered release until stable

### Applicability to Kōdo

**MEDIUM-HIGH relevance for v2.0+.** Roc's platform/host model is directly applicable to agent sandboxing:

- An AI agent writing Kōdo code should not have arbitrary system access
- A "platform" could define what I/O primitives are available (file, network, database)
- Different platforms could enforce different security policies
- This aligns with Kōdo's `@confidence` system — low-confidence code could run on a restricted platform

### Recommendation

**MONITOR** for v2.0 design. Create a design document for "Kōdo Platforms" that defines how agent-generated code can be sandboxed using a platform abstraction. Not for current milestone but essential for production agent deployments.

---

## Action Items

1. **IMPLEMENT**: `kodoc check --diagnostics-format=sarif` — SARIF output for GitHub/IDE integration
2. **MONITOR**: Gleam LSP patterns for future Kōdo LSP improvements
3. **CITE**: Carbon's memory safety delay in Kōdo positioning materials
4. **DESIGN** (v2.0): Platform/host model for agent sandboxing, inspired by Roc
