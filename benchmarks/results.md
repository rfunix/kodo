# Benchmark Results: Task Management API

## Overview

Five identical Task Management APIs implemented in Kōdo, Python, TypeScript, Rust, and Go.
All implementations have equivalent functionality: CRUD operations, priority validation (1-5),
status workflow (pending → in_progress → done), JSON API, persistence, and tests.

## Token Count (GPT-4 Tokenizer)

Lower is better — fewer tokens means cheaper and faster for AI agents to read and write.

| Metric | Kōdo | Python | TypeScript | Rust | Go |
|--------|-----:|-----:|-----:|-----:|-----:|
| **Tokens** | 5,053 | 2,230 | 4,467 | 4,819 | 4,655 |
| Total Lines | 635 | 342 | 586 | 652 | 768 |
| Code Lines | 503 | 220 | 495 | 552 | 639 |
| Comments | 59 | 9 | 3 | 10 | 12 |

## Token Analysis

Kōdo's token count includes **built-in safety guarantees** that other languages lack entirely:
contracts, agent traceability, refinement types, and compilation certificates.
Comparing raw tokens without considering what those tokens *buy you* misses the point.

**Kōdo**: 5,053 tokens → 7/7 compile-time bug classes caught

- **Python**: 2,230 tokens (2,823 fewer) — but only 0/7 bug classes caught
- **TypeScript**: 4,467 tokens (586 fewer) — but only 2/7 bug classes caught
- **Rust**: 4,819 tokens (234 fewer) — but only 4/7 bug classes caught
- **Go**: 4,655 tokens (398 fewer) — but only 2/7 bug classes caught

**Cost per safety class:**

- Kōdo: 721 tokens per bug class caught
- Python: ∞ (zero bug classes caught at compile time)
- TypeScript: 2,233 tokens per bug class caught
- Rust: 1,204 tokens per bug class caught
- Go: 2,327 tokens per bug class caught

## Compile-Time Safety

How many classes of bugs are caught **before** the code runs?

| Bug Class | Kōdo | Python | TypeScript | Rust | Go |
|-----------|:----:|:----:|:----:|:----:|:----:|
| Null/None dereference | ✅ | ❌ | ✅ | ✅ | ❌ |
| Type mismatch | ✅ | ❌ | ✅ | ✅ | ✅ |
| Contract violation | ✅ | ❌ | ❌ | ❌ | ❌ |
| Invalid status transition | ✅ | ❌ | ❌ | ❌ | ❌ |
| Value out of range | ✅ | ❌ | ❌ | ❌ | ❌ |
| Missing error handling | ✅ | ❌ | ❌ | ✅ | ✅ |
| Use after move | ✅ | ❌ | ❌ | ✅ | ❌ |
| **Total** | **7/7** | **0/7** | **2/7** | **4/7** | **2/7** |

## Machine-Readability of Errors

How easily can an AI agent parse and act on compiler/linter errors?

| Criterion | Kōdo | Python | TypeScript | Rust | Go |
|-----------|:----:|:----:|:----:|:----:|:----:|
| JSON parseable (+2) | +2 | — | — | — | — |
| Exact source spans (+1) | +1 | +1 | +1 | +1 | +1 |
| Suggests fix (+1) | +1 | — | — | +1 | — |
| Unique error code (+1) | +1 | — | +1 | +1 | — |
| **Total** | **5/5** | **1/5** | **2/5** | **3/5** | **1/5** |

## Agent-Unique Features

Features specifically designed for AI agent workflows — not available in general-purpose languages.

| Feature | Kōdo | Python | TypeScript | Rust | Go |
|---------|:----:|:----:|:----:|:----:|:----:|
| Self-describing modules (meta) | ✅ | ❌ | ❌ | ❌ | ❌ |
| Agent traceability annotations | ✅ | ❌ | ❌ | ❌ | ❌ |
| Formal contract verification (Z3) | ✅ | ❌ | ❌ | ❌ | ❌ |
| Refinement types | ✅ | ❌ | ❌ | ❌ | ❌ |
| Intent-driven code generation | ✅ | ❌ | ❌ | ❌ | ❌ |
| Compilation certificates | ✅ | ❌ | ❌ | ❌ | ❌ |
| Machine-applicable fix patches | ✅ | ❌ | ❌ | ❌ | ❌ |
| Confidence propagation | ✅ | ❌ | ❌ | ❌ | ❌ |
| MCP server (native agent support) | ✅ | ❌ | ❌ | ❌ | ❌ |
| **Total** | **9/9** | **0/9** | **0/9** | **0/9** | **0/9** |

## Summary

| Dimension | Winner | Why |
|-----------|--------|-----|
| **Safety per Token** | Kōdo | Best ratio of compile-time guarantees per token |
| **Compile-Time Safety** | Kōdo | 7/7 bug classes caught at compile time |
| **Error Machine-Readability** | Kōdo | 5/5 — JSON errors with auto-fix patches |
| **Agent Features** | Kōdo | 9/9 — purpose-built for AI agents |
| **Raw Token Count** | Python | Most concise syntax — but 0/7 compile-time safety |

### Why Kōdo Wins for AI Agents

The question isn't "which language uses the fewest tokens?" — it's **"which language lets agents
produce correct code with the least total effort?"** Total effort includes writing, debugging,
fixing, and verifying.

1. **Contracts catch bugs at compile time** that other languages only find at runtime (or never) — every `requires`/`ensures` clause eliminates entire categories of runtime failures
2. **Structured JSON errors** with machine-applicable fix patches enable autonomous error→fix loops — agents fix their own mistakes without human intervention
3. **Agent traceability** (`@confidence`, `@authored_by`) is built into the grammar — not comments that get lost or ignored
4. **Self-describing modules** (`meta` blocks) give agents instant context without reading code
5. **Refinement types** (`type Priority = Int requires { self >= 1 && self <= 5 }`) eliminate invalid states at the type level
6. **Intent blocks** reduce boilerplate — agents declare WHAT, the compiler generates HOW
7. **Compilation certificates** provide verifiable proof of correctness for deployment pipelines

---

*Generated by `benchmarks/measure.py` — Kōdo Language Benchmark Suite*
