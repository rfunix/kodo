# Market Research — 2026-03-22

## Context

Market research on compiled languages, AI-assisted programming trends,
and innovations in type systems/contracts. Goal: identify opportunities and threats
for Kōdo's positioning as a language for AI agents.

---

## 1. Compiled Languages Landscape (2025-2026)

### Rust
- Climbed from #19 to #14 on TIOBE. Established as the standard choice for safe infrastructure.
- **Rust 2026 goals**: ~60 proposals in progress. Highlights:
  - **Next-gen trait solver**: refactoring that unblocks implied bounds, negative impls, and fixes unsoundnesses.
  - **Polonius**: new borrow checker enabling "lending iterators" and more expressive borrowing patterns. Target: stabilization in 2026.
  - **In-place initialization**: creating structs bound to memory locations, unblocking `dyn Trait` with `async fn` and `-> impl Trait`.
- **No native contracts**: Rust still has no accepted RFC for contracts (`requires`/`ensures`). This preserves Kōdo's differentiator.

### Mojo
- **First language built entirely on MLIR**. Enables compilation for GPUs, TPUs, ASICs.
- Compiler will be open-source in 2026. Production-ready version expected Q1 2026.
- Portable GPU support (NVIDIA + AMD) since June 2025.
- **Positioning**: Python-like with C performance. Focus on AI/ML, not autonomous agents.
- **Relevance to Kōdo**: Mojo validates that new languages can gain traction if they solve a clear problem. Its target (AI/ML hardware) is orthogonal to Kōdo's (AI agents writing code).

### Zig
- Jumped from #149 to #61 on TIOBE. Direct competitor to C.
- Radical simplicity: low-level control with optional safety features (bounds checking, null checks, error unions).
- **Relevance to Kōdo**: Zig shows that simplicity sells. Kōdo should maintain clean syntax and LL(1).

### Carbon
- Google's experimental language to succeed C++. Focus on interoperability with existing C++.
- Still in experimental stage. No production timeline.
- **Relevance to Kōdo**: Low. Carbon competes in the legacy C++ space.

### Gleam
- Runs on the Erlang VM. Focus on concurrency and fault-tolerance.
- **Relevance to Kōdo**: Gleam's concurrency model (actor-based via BEAM) is mature. Kōdo can study it to evolve structured concurrency in the future.

### Vale
- Innovative approach: **generational references** + **region borrowing**.
- Each object has a "current generation" integer incremented on free. Pointers carry a "remembered generation". On dereference, assert the numbers match.
- **Region borrowing**: the compiler knows that during a scope, a data region won't be modified, eliminating generation check overhead.
- Prototype showed **zero observable overhead** when using linear style + regions.
- **Relevance to Kōdo**: HIGH. Vale's model for memory safety without a complex borrow checker is an interesting alternative to Kōdo's own/ref/mut model. Generational references allow patterns that borrow checking prohibits (observers, back-references, graphs). Monitor evolution.

### Roc
- Pure functional language focused on performance and small binaries.
- **Relevance to Kōdo**: Low directly, but Roc's focus on "platform hosts" (separating IO from pure code) is an interesting concept for agent sandboxing.

---

## 2. AI-Assisted Programming Trends

### Anthropic "2026 Agentic Coding Trends" Report

Eight identified trends:

1. **Shifting engineer role**: from writing code to orchestrating agents. Focus on architecture, design, and strategic decisions.
2. **Single-agent to multi-agent**: organizations deploy specialized agents working in parallel with separate context windows.
3. **Human-AI collaboration**: 60% of work integrates AI, with active supervision in 80-100% of delegated tasks.
4. **Multi-agent coordination**: parallel reasoning in separate context windows is standard practice.
5. **Scaling agentic coding beyond engineering**: domain experts from other departments can use it.
6. **AI-automated review**: automated review systems are essential for managing agent output.
7. **Cross-functional adoption**: multiplicative value adoption.
8. **Quality assurance at scale**: maintaining quality with accelerated throughput.

### Language-Specialized Models
- In 2026, narrow-focused models trained exclusively on security and memory rules of specific ecosystems (Rust, Swift) emerged.
- **Opportunity for Kōdo**: a fine-tuned model for Kōdo that understands contracts, linear ownership, and intent blocks would be an enormous competitive differentiator.

### MCP as Standard
- MCP (Model Context Protocol) joined the Linux Foundation and became the standard for tool/data access in agentic systems.
- **Kōdo already has an MCP server** — this is a differentiator. Maintain and expand.

### Market Stack
- GPT-5.2 leads in logic, Claude 4.5 in engineering quality, Gemini 3 in large context.
- Devstral (Mistral) focuses on code-agent model.
- DeepSeek-V3.2 best open-source for reasoning and agentic workloads.

### No Competing Language for AI Agents
- **Critical finding**: No other language is designed specifically for AI agents. The space is dominated by **tools** (Cursor, Claude Code, Copilot, Devin) that work with existing languages.
- **Kōdo occupies an empty niche**. This is simultaneously an opportunity (first-mover) and a risk (the market may not see the need for a new language).

---

## 3. Type System and Contract Innovations

### Contracts: State of the Art
- **Racket**: native contract implementation with emphasis on "blame assignment" — when a contract is violated, the system identifies which part of the code is at fault with precise explanation.
- **UC Berkeley (2025)**: "Constraint-behavior contracts" for physical components using implicit equations. Focus on verification automation.
- **Type system + contracts integration**: trend of treating contracts as part of the type system, not as external annotations.

### Opportunities for Kōdo
1. **Enhanced blame assignment**: implement Racket-style blame tracking in Kōdo's contracts. When a `requires` fails, automatically identify which caller violated the precondition and generate a fix patch.
2. **Contracts as types**: explore the possibility of refinement types (`x: Int where x > 0`) that unify constraints with the type system.
3. **Contract inference**: automatically infer contracts from function bodies (e.g., if the function does `x / y`, infer `requires { y != 0 }`).

---

## 4. Impact Assessment for Kōdo

### Threats
| Threat | Severity | Mitigation |
|--------|----------|------------|
| Rust adopts native contracts | High | Kōdo already has contracts + Z3; maintain DX leadership |
| Mojo captures "new language" mindshare | Medium | Different positioning (AI agents vs AI/ML hardware) |
| Tools like Cursor/Devin make language irrelevant | Medium | Kōdo offers guarantees that tools on existing languages cannot |
| Narrow-focused models for Rust | Low | Create fine-tuned model for Kōdo |

### Opportunities
| Opportunity | Priority | Action |
|-------------|----------|--------|
| Empty niche of "language for AI agents" | Critical | Focused marketing and developer relations |
| Multi-agent coordination (Anthropic report) | High | Expand MCP server to support multi-agent workflows |
| Blame assignment in contracts | High | Implement Racket-style blame tracking |
| Automatic contract inference | Medium | Research feasibility with Z3 |
| Fine-tuned model for Kōdo | Medium | Collect .ko code dataset for fine-tuning |
| Region borrowing (Vale) | Low | Monitor; consider for v2.0 |

---

## Sources

- [Semaphore - Top 8 Emerging Programming Languages 2025](https://semaphore.io/blog/programming-languages-2025)
- [CodeCrafters - 7 New Programming Languages](https://codecrafters.io/blog/new-programming-languages)
- [Rust in 2026 - Medium](https://medium.com/@blogs-world/rust-in-2026-what-actually-changed-whats-trending-and-what-to-build-next-d70e38a4ad97)
- [Rust Project Goals 2026](https://rust-lang.github.io/rust-project-goals/)
- [Mojo Roadmap - Modular](https://docs.modular.com/mojo/roadmap/)
- [Mojo MLIR-Based HPC - arXiv](https://arxiv.org/html/2509.21039v1)
- [Vale - Generational References](https://verdagon.dev/blog/generational-references)
- [Vale - First Regions Prototype](https://verdagon.dev/blog/first-regions-prototype)
- [Anthropic 2026 Agentic Coding Trends Report](https://resources.anthropic.com/2026-agentic-coding-trends-report)
- [Anthropic Report Summary - tessl.io](https://tessl.io/blog/8-trends-shaping-software-engineering-in-2026-according-to-anthropics-agentic-coding-report/)
- [MIT Technology Review - AI Coding](https://www.technologyreview.com/2025/12/15/1128352/rise-of-ai-coding-developers-2026/)
- [Addy Osmani - LLM Coding Workflow 2026](https://addyosmani.com/blog/ai-coding-workflow/)
- [JetBrains - Best AI Models for Coding](https://blog.jetbrains.com/ai/2026/02/the-best-ai-models-for-coding-accuracy-integration-and-developer-fit/)
- [UC Berkeley - Contract-Based Design Automation](https://www2.eecs.berkeley.edu/Pubs/TechRpts/2025/EECS-2025-84.pdf)
