# Research — 2026-03-28: Austral Capabilities & SWE-bench Agent Accuracy

## Topics
1. Austral — linear types with capability-based security
2. SWE-bench 2026 — AI agent coding benchmarks

---

## 1. Austral — Linear Types + Capabilities

### Findings

Austral is a systems language with two key innovations relevant to Kōdo:

- **Linear types**: Resources (memory, files, db handles) managed provably safely without runtime overhead. Prevents double-free, use-after-free, double-fetch.
- **Capability-based security**: IO access is controlled via linear capabilities. Third-party dependencies are constrained in what resources they can access — mitigates supply chain attacks.
- **Design philosophy**: "A programmer should be able to say exactly what code does." No ambiguity, no implicit behavior.

**How capabilities work**: A `Root` capability is passed to `main()`. Functions that need IO must receive the appropriate capability as a parameter. Libraries that don't receive capabilities physically cannot perform IO.

### Relevance to Kōdo: HIGH

Kōdo has linear ownership (`own`/`ref`/`mut`) but lacks capability-based security. Combined with Roc's platform model (previous research), Austral's capabilities suggest a powerful agent sandboxing model:

1. **Agent capability scoping**: An agent's code receives only the capabilities it needs. A config-writing agent gets `FileCapability` but not `HttpCapability`. Enforced at compile time.
2. **Supply chain defense**: Third-party Kōdo packages can't access IO unless explicitly granted capabilities. Critical for agent trust.
3. **Contract + capability synergy**: `requires { has_capability(FileWrite) }` could express IO preconditions in the contract system.

### Recommendation: RESEARCH DEEPLY + DESIGN
- Design capability system for Kōdo v2.0+ (integrate with existing ownership model)
- Create design document: "Capability-Based Agent Sandboxing"
- This + contracts + ownership would make Kōdo unique: the only language with all three

---

## 2. SWE-bench 2026 — Agent Coding Accuracy

### Findings

**SWE-bench Verified (February 2026):**
| Model | Score |
|-------|-------|
| Gemini 3.1 Pro Preview | 78.80% |
| Claude Opus 4.6 (Thinking) | 78.20% |
| GPT 5.4 | 78.20% |
| GPT 5.3 Codex | 78.00% |

**SWE-bench Pro (realistic):**
- Best: GPT-5 at 23.3%, Claude Opus 4.1 at 23.1%
- **54% relative overestimation** on Verified vs real-world scenarios
- Realistic-mutation success rates up to 36.5% lower

**Key insight**: There's a massive gap between benchmark performance (78%) and real-world agent coding accuracy (23%). This gap is the opportunity Kōdo addresses.

### Relevance to Kōdo: CRITICAL

This data directly validates Kōdo's thesis:

1. **The 78% → 23% gap**: Agents struggle with real-world code because existing languages don't provide enough structure for agents to succeed. Kōdo's contracts, structured errors, and fix patches exist to close this gap.

2. **Benchmark opportunity**: Kōdo should create a "Kōdo-bench" — a benchmark measuring agent success rate on Kōdo tasks. If agents achieve >50% on realistic Kōdo tasks (vs 23% on Python), that proves the language's value proposition.

3. **Competitive positioning**: "Agents score 23% on real Python tasks, but X% on Kōdo tasks" would be the most powerful marketing claim possible.

### Recommendation: IMPLEMENT
- Create `kodo-bench`: a set of 50-100 realistic coding tasks in Kōdo
- Measure agent success rate with and without contracts/fix patches
- Compare against SWE-bench Pro Python baseline (23%)
- Create issue for this initiative

---

## Summary of Actions

| Finding | Priority | Action |
|---------|----------|--------|
| Capability-based agent sandboxing | HIGH | Design document for v2.0+ |
| Kōdo-bench agent benchmark | CRITICAL | Create benchmark suite + measure |
| SWE-bench gap as marketing proof | HIGH | "Agents do X% better on Kōdo" |

---

## Sources

- [Austral Programming Language](https://austral-lang.org/)
- [Austral Capability-Based Security](https://austral-lang.org/tutorial/capability-based-security)
- [Austral Linear Types](https://austral-lang.org/tutorial/linear-types)
- [What Austral Proves](https://animaomnium.github.io/what-austral-proves/)
- [SWE-bench Leaderboards](https://www.swebench.com/)
- [SWE-bench February 2026 Update](https://simonwillison.net/2026/Feb/19/swe-bench/)
- [SWE-bench Verified — Epoch AI](https://epoch.ai/benchmarks/swe-bench-verified)
- [SWE-Bench Pro — Scale Labs](https://labs.scale.com/leaderboard/swe_bench_pro_public)
