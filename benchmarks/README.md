# Kōdo Language Benchmark

Comparison of identical Task Management APIs implemented in **Kōdo**, **Python**, **TypeScript**, **Rust**, and **Go**.

## What We Measure

| Metric | Description |
|--------|-------------|
| **Token Count** | GPT-4 tokens to represent the complete project (via tiktoken) |
| **Lines of Code** | Total, code-only, comments |
| **Compile-Time Safety** | Bug classes caught before runtime (7 categories) |
| **Error Machine-Readability** | How easily agents parse compiler errors (0-5 score) |
| **Agent-Unique Features** | Features designed for AI agent workflows (9 categories) |

## The Project

A Task Management REST API with:
- CRUD operations (create, read, update, delete tasks)
- Priority validation (1-5, enforced)
- Status workflow: `pending` → `in_progress` → `done`
- JSON serialization/deserialization
- File-based persistence
- Health check and statistics endpoints
- Comprehensive test suite

## Running the Benchmark

```bash
# Install tiktoken for accurate token counting
pip install tiktoken

# Run measurements
python benchmarks/measure.py
```

Results are written to `results.md` and `results.json`.

## Project Structure

```
benchmarks/
├── README.md           # This file
├── measure.py          # Measurement script
├── results.md          # Generated comparison report
├── results.json        # Raw measurement data
├── python/             # Python (FastAPI + Pydantic)
├── typescript/         # TypeScript (Express + Zod)
├── rust/               # Rust (Axum + serde)
└── go/                 # Go (net/http + encoding/json)
```

The Kōdo implementation lives at `examples/task_manager/task_manager.ko`.

## Why Kōdo?

Kōdo is purpose-built for AI agents. Every other language in this benchmark was designed for humans. The difference shows:

- **Contracts** (`requires`/`ensures`) catch bugs at compile time that tests can only find at runtime
- **Agent traceability** (`@confidence`, `@authored_by`) is built into the grammar, not comments
- **Structured errors** with JSON output and auto-fix patches enable closed-loop repair
- **Self-describing modules** (`meta` blocks) give agents instant context
- **Refinement types** (`type Priority = Int requires { self >= 1 && self <= 5 }`) eliminate invalid states

See [results.md](results.md) for the full comparison.
