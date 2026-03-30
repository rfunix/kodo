# Self-Hosted Mode (`--self-hosted`)

> **Status**: Experimental — Bootstrap Phase 4 (Kōdo roadmap v1.x)

The `--self-hosted` flag instructs `kodoc` to route the lexer and parser stages
through binaries written in Kōdo itself (the bootstrap implementation located in
`examples/`), rather than the native Rust implementation.

## Why Self-Hosting?

Self-hosting is a key milestone for any compiled language.  Kōdo's bootstrap path
proves that the language is expressive enough to implement real compiler work:

- `examples/self_hosted_lexer/main.ko` — ~550 lines, full Kōdo tokeniser
- `examples/self_hosted_parser/main.ko` — ~1,900 lines, full LL(1) recursive-descent parser

Running these under `--self-hosted` closes the first loop of the bootstrap cycle:
**Kōdo code compiled by `kodoc` → Kōdo binary that can lex/parse Kōdo source**.

## Prerequisites

The bootstrap binaries must be compiled from their Kōdo source before the flag
can be activated.  Run the following commands once from the repository root:

```sh
mkdir -p ./bootstrap
kodoc build examples/self_hosted_lexer/main.ko  -o ./bootstrap/self_hosted_lexer
kodoc build examples/self_hosted_parser/main.ko -o ./bootstrap/self_hosted_parser
```

`kodoc` searches for the binaries in these directories (in priority order):

1. `./bootstrap/`
2. `./bin/`
3. `.` (current working directory)

## Usage

Once the binaries are available, pass `--self-hosted` to any of these commands:

```sh
# Build with self-hosted lex/parse stage
kodoc build --self-hosted examples/hello.ko

# Tokenize using the Kōdo-written lexer
kodoc lex --self-hosted examples/hello.ko

# Parse (currently falls back to Rust AST display; see Limitations)
kodoc parse --self-hosted examples/hello.ko
```

### Error When Binaries Are Missing

If the bootstrap binaries have not been compiled, `kodoc` emits a structured
diagnostic and exits with code 1:

```
error[E0700]: self-hosted mode requires pre-compiled bootstrap binaries

  missing: self_hosted_lexer  (not found in: ./bootstrap, ./bin, .)
  missing: self_hosted_parser (not found in: ./bootstrap, ./bin, .)

  To build the bootstrap binaries, run:

    kodoc build examples/self_hosted_lexer/main.ko  -o ./bootstrap/self_hosted_lexer
    kodoc build examples/self_hosted_parser/main.ko -o ./bootstrap/self_hosted_parser

  Then re-run with --self-hosted.
```

## Current Limitations

| Area | Status |
|------|--------|
| Lexer binary execution | Supported — binary is invoked and output lines are counted |
| Token stream JSON handoff to Rust type-checker | Not yet — planned for Bootstrap Phase 5 |
| Parser binary execution + AST handoff | Not yet — `parse --self-hosted` falls back to Rust parser |
| Incremental / cached bootstrap | Not yet |
| Windows `.exe` extension | Handled automatically |

## Roadmap

Full self-hosting integration is tracked as Kōdo Bootstrap Phase 4–5:

- **Phase 4** (current): flag available, bootstrap binary detection, lexer liveness check
- **Phase 5**: standardised JSON token stream protocol between bootstrap lexer and Rust type-checker
- **Phase 6**: bootstrap parser JSON AST consumed by type-checker, MIR, codegen
- **Phase 7**: `kodoc` compiled by `kodoc` — full bootstrap complete

## Related Files

| File | Description |
|------|-------------|
| `examples/self_hosted_lexer/main.ko` | Self-hosted lexer source |
| `examples/self_hosted_parser/main.ko` | Self-hosted parser source |
| `crates/kodoc/src/self_hosted.rs` | Bootstrap detection and invocation logic |
