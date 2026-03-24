# Research — 2026-03-25: Error Message UX and Functional Language Updates

## Topics
1. Compiler error message innovation (Rust, Elm)
2. Gleam v1.15 — LSP and developer experience
3. Roc 0.1.0 — platform/host architecture

---

## 1. Compiler Error Message Innovation

### Findings

- **Elm remains the gold standard** for error messages. Clear, conversational, explains *why* not just *what*.
- **Rust learned from Elm** and now leads in compiled languages. Recent additions include `#[diagnostic]` attributes for library authors to customize error messages.
- **Developer productivity research** shows error message quality directly impacts onboarding speed and team productivity. Rust and Elm consistently rank highest.

### Relevance to Kōdo: HIGH

Kōdo already has structured JSON errors with fix patches (98% coverage) — this puts it **ahead** of both Rust and Elm in machine-consumable diagnostics. However, for *human* readability:

1. **Opportunity**: Add Elm-style conversational error explanations alongside structured JSON. The `kodoc explain E0200` command exists but could be expanded.
2. **Opportunity**: Allow library authors to customize error messages for their types (like Rust's `#[diagnostic]`). Kōdo could add `@error_hint` annotations to contracts.
3. **Kōdo's unique advantage**: Fix patches with byte offsets are something neither Rust nor Elm offer. This is the killer feature for agents.

### Recommendation: ENHANCE
- Keep structured JSON errors as primary (agent-first)
- Add Elm-style human-readable explanations to more error codes
- Consider `@error_hint` annotation for custom contract error messages

---

## 2. Gleam v1.15 — LSP Excellence

### Findings

- Gleam is now the **2nd most admired language** in Stack Overflow 2025 survey.
- LSP improvements in v1.15:
  - `textDocument/foldingRange` for imports and multi-line definitions
  - Smart "fill labels" code action using variables from scope matching by name and type
  - Pipeline extraction into variables
  - Documentation on hover for custom types and variants
- Build tool refuses to publish packages with default/missing README.

### Relevance to Kōdo: MEDIUM

Kōdo's LSP already has hover, completions, goto-definition, diagnostics, code actions (fix patches), and symbol search. Gleam's innovations suggest:

1. **Smart code actions**: Gleam's "fill labels using scope variables" is clever — Kōdo could do similar for function call arguments, auto-filling from scope.
2. **Folding ranges**: Low effort, nice UX improvement for IDE users.
3. **Pipeline extraction**: Not applicable (Kōdo doesn't have pipes), but the principle of "extract expression to variable" refactoring is valuable.
4. **Package quality gates**: Kōdo's package manager could refuse to publish without README/meta block.

### Recommendation: MONITOR + CHERRY-PICK
- Add folding ranges to Kōdo LSP (low effort, high UX value)
- Consider smart argument fill from scope
- Add publish quality gates to package manager

---

## 3. Roc 0.1.0 — Platform/Host Architecture

### Findings

- Roc targeting **0.1.0 release in 2026** after 30,000+ commits (no numbered releases yet).
- **Platform architecture**: Pure application code (Roc) separated from host (Rust/Zig/C++) that handles IO, memory, and system access.
- Host migration from Rust to Zig underway.
- **Rewritten compiler** accompanying the 0.1.0 release.
- Available platforms: `basic-cli`, `basic-webserver`, `roc-pg` (PostgreSQL with type checking against schema).

### Relevance to Kōdo: HIGH (conceptual)

Roc's platform/host separation is highly relevant to Kōdo's agent-first design:

1. **Agent sandboxing**: Kōdo could adopt a platform model where agent code is pure (contracts, ownership, no direct IO) and the "host" controls what IO primitives are available. This enables:
   - Permission-scoped agents (an agent writing a config file can't make HTTP requests)
   - Auditable IO surface (all side effects go through the host)
   - Different hosts for different deployment targets (local, cloud, sandbox)

2. **Type-checked database access**: Roc's `roc-pg` checks SQL types against the schema at compile time. Kōdo could offer similar guarantees for agents accessing databases.

3. **Compiler rewrite precedent**: Roc is rewriting their compiler for 0.1.0. Kōdo is heading toward self-hosting (v2.0 bootstrap) — different approach but same goal.

### Recommendation: RESEARCH DEEPLY
- The platform/host model deserves a dedicated design document for Kōdo v2.0+
- Agent permission scoping via platforms could be a major differentiator
- Create issue for "Platform architecture for agent sandboxing" discussion

---

## Summary of Actions

| Finding | Priority | Action |
|---------|----------|--------|
| Elm-style error explanations | Medium | Expand `kodoc explain` with conversational descriptions |
| `@error_hint` for contracts | Medium | Design annotation for custom contract error messages |
| LSP folding ranges | Low | Add to kodo_lsp (quick win) |
| Smart argument fill | Low | Monitor Gleam's implementation |
| Platform/host sandboxing | High | Design document for Kōdo v2.0+ |
| Package publish quality gates | Low | Add to kodoc when package manager matures |

---

## Sources

- [Comparing Compiler Errors across Languages](https://www.amazingcto.com/developer-productivity-compiler-errors/)
- [Rust Diagnostic Attributes](https://dev.to/ibrahimbagalwa/customizing-rust-error-messages-with-diagnostic-attributes-3761)
- [Rust Compiler Diagnostics Guide](https://rustc-dev-guide.rust-lang.org/diagnostics.html)
- [Gleam News](https://gleam.run/news/)
- [Gleam Roadmap](https://gleam.run/roadmap/)
- [Gleam: Rising Star of Functional Programming 2026](https://pulse-scope.ovidgame.com/2026-01-14-17-54/gleam-the-rising-star-of-functional-programming-in-2026)
- [Roc Programming Language](https://www.roc-lang.org/)
- [Roc Platforms and Apps](https://www.roc-lang.org/platforms)
- [Understanding Roc: Separate from the Runtime](https://www.techtarget.com/searchapparchitecture/tip/Understanding-Roc-Functional-and-separate-from-the-runtime)
