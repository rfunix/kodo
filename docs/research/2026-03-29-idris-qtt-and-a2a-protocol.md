# Research — 2026-03-29: Idris 2 Quantitative Types & A2A Agent Protocol

## Topics
1. Idris 2 — Quantitative Type Theory (QTT)
2. A2A Protocol — agent-to-agent communication standard

---

## 1. Idris 2 — Quantitative Type Theory

### Findings

Idris 2's core is based on Quantitative Type Theory (QTT), which assigns each variable a *quantity*:
- **0**: Erased at runtime (compile-time only, like phantom types)
- **1**: Used exactly once (linear, like Kōdo's `own`)
- **Unrestricted**: Normal usage (like Kōdo's default)

This is strictly more expressive than Kōdo's `own`/`ref`/`mut` system because:
1. It unifies erasure and linearity in one framework
2. It enables type-safe session types for concurrent protocols
3. Dependent types allow expressing arbitrarily precise contracts in the type system itself

### Relevance to Kōdo: LOW (theoretical interest)

Idris 2's approach is academically superior but practically complex. Kōdo's simpler `own`/`ref`/`mut` with separate contracts is more agent-friendly. However:

1. **Erasure annotations**: The idea of marking variables as "compile-time only" (quantity 0) could be useful for Kōdo's contract system — `requires`/`ensures` expressions could be erased at runtime when `--contracts=none`.
2. **Session types**: Long-term, Kōdo's channel types could benefit from session typing for protocol verification.

### Recommendation: MONITOR (academic)
- No immediate action needed
- Keep as reference for Kōdo v3.0+ type system evolution

---

## 2. A2A Protocol — Agent-to-Agent Communication

### Findings

**A2A (Agent2Agent)** is Google's open protocol for agent interoperability, now under the Linux Foundation:

- **Agent Cards**: Each agent publishes `/.well-known/agent-card.json` describing capabilities and endpoint
- **50+ partners**: Atlassian, Salesforce, SAP, PayPal, Langchain, MongoDB, etc.
- **Complements MCP**: MCP = agent-to-resource, A2A = agent-to-agent delegation
- **Protocol stack**: A2A + MCP together form the complete agent communication stack

**Protocol ecosystem map 2026:**
| Protocol | Purpose | Status |
|----------|---------|--------|
| MCP (Anthropic) | Agent-to-resource (tools, data) | Linux Foundation, widely adopted |
| A2A (Google) | Agent-to-agent delegation | Linux Foundation, growing |
| ACP (Cisco) | Agent-to-agent (alternative) | Emerging |
| UCP (Fetch.ai) | Agent-to-agent (alternative) | Niche |

### Relevance to Kōdo: HIGH

Kōdo already has an MCP server (`kodo_mcp`). Adding A2A support would complete the agent communication stack:

1. **Agent Card for Kōdo**: Kōdo programs could publish `agent-card.json` describing their capabilities, contracts, and confidence scores. This maps perfectly to Kōdo's `meta` blocks and `@confidence` annotations.

2. **Multi-agent code generation**: Agent A writes a module, publishes it via A2A → Agent B discovers it, reads contracts, and integrates. Kōdo's contracts serve as the API specification.

3. **`kodoc a2a` command**: Generate agent card from module metadata. `kodoc a2a serve` to start an A2A endpoint.

4. **Competitive positioning**: "Kōdo: the only language with native MCP + A2A support" — both industry standards.

### Recommendation: IMPLEMENT (MEDIUM-HIGH)
- Design `agent-card.json` generation from Kōdo module `meta` blocks
- Create issue for A2A support
- This ties directly into the "language for AI agents" thesis

---

## Summary of Actions

| Finding | Priority | Action |
|---------|----------|--------|
| A2A protocol support | HIGH | Design agent-card generation from meta blocks |
| Idris 2 QTT erasure | LOW | Monitor for v3.0+ type system |

---

## Sources

- [Idris 2: QTT in Practice — arXiv](https://arxiv.org/abs/2104.00480)
- [Idris 2 Multiplicities](https://idris2.readthedocs.io/en/latest/tutorial/multiplicities.html)
- [A2A Protocol](https://a2a-protocol.org/latest/)
- [Google A2A Announcement](https://developers.googleblog.com/en/a2a-a-new-era-of-agent-interoperability/)
- [AI Agent Protocol Ecosystem Map 2026](https://www.digitalapplied.com/blog/ai-agent-protocol-ecosystem-map-2026-mcp-a2a-acp-ucp)
- [A2A Upgrade — Google Cloud Blog](https://cloud.google.com/blog/products/ai-machine-learning/agent2agent-protocol-is-getting-an-upgrade)
- [A2A — IBM](https://www.ibm.com/think/topics/agent2agent-protocol)
