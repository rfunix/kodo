# Kōdo Language for VS Code

Full language support for [Kōdo](https://kodo-lang.dev) — the programming language designed for AI agents to write correct software.

## Features

- **Syntax Highlighting** — keywords, types, contracts, annotations, strings, numbers
- **LSP Integration** — real-time diagnostics, hover, go-to-definition, completions
- **Contract-Aware** — `requires`/`ensures` highlighted and shown in hover
- **Agent Annotations** — `@authored_by`, `@confidence`, `@reviewed_by` highlighted
- **Code Actions** — quick fixes from the compiler's FixPatch system

## Requirements

Install the Kōdo compiler (`kodoc`) and ensure it's on your PATH:

```bash
# From source
git clone https://github.com/rfunix/kodo.git
cd kodo && cargo install --path crates/kodoc

# Or download from releases
# https://github.com/rfunix/kodo/releases
```

## Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `kodo.serverPath` | `kodoc` | Path to the kodoc binary |
| `kodo.trace.server` | `off` | Trace LSP communication |

## Language Features

Kōdo is a compiled language with:
- Zero-ambiguity LL(1) grammar
- Z3-verified contracts (`requires`/`ensures`)
- Linear ownership (`own`/`ref`/`mut`)
- Agent traceability annotations
- Self-describing modules with mandatory `meta` blocks
- Built-in testing with property-based testing

## Links

- [Kōdo Website](https://kodo-lang.dev)
- [Playground](https://kodo-lang.dev/playground)
- [Documentation](https://kodo-lang.dev/docs/getting-started/)
- [GitHub](https://github.com/rfunix/kodo)
