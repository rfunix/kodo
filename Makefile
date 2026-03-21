.PHONY: all check fmt clippy test doc build clean run-lex run-parse run-check run-build bench ci install uninstall coverage coverage-report validate-docs ui-test ui-bless

# Default: run all checks
all: fmt clippy test doc

# === Quality Checks ===

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --workspace -- -D warnings

test:
	cargo test --workspace

doc:
	cargo doc --workspace --no-deps

doc-open:
	cargo doc --workspace --no-deps --open

# Validate documentation examples against the compiler
validate-docs:
	./scripts/validate-doc-examples.sh

# Run UI tests (kotest harness)
ui-test:
	cargo run -p kotest -- tests/ui/

# Auto-update UI test baselines
ui-bless:
	cargo run -p kotest -- tests/ui/ --bless

# Test LLVM backend against all examples
llvm-test:
	./scripts/test-llvm-backend.sh

# All checks (CI-equivalent)
ci: fmt-check clippy test ui-test doc

# Full validation: CI + doc examples
ci-full: ci validate-docs

# === Build ===

build:
	cargo build --workspace

release:
	cargo build --workspace --release

check:
	cargo check --workspace

clean:
	cargo clean

# === Install ===

# Install to ~/.local/bin (standard user bin) AND /usr/local/bin if writable
install: release
	@mkdir -p $(HOME)/.local/bin
	@cp target/release/kodoc $(HOME)/.local/bin/kodoc
	@chmod +x $(HOME)/.local/bin/kodoc
	@# Also update /usr/local/bin if it exists and has an old kodoc
	@if [ -f /usr/local/bin/kodoc ]; then \
		cp target/release/kodoc /usr/local/bin/kodoc 2>/dev/null && \
		echo "  Updated /usr/local/bin/kodoc" || true; \
	fi
	@echo ""
	@echo "  kodoc installed to $(HOME)/.local/bin/kodoc"
	@echo ""
	@echo "  Verify: kodoc --version"
	@echo ""

uninstall:
	@rm -f $(HOME)/.local/bin/kodoc
	@rm -f /usr/local/bin/kodoc 2>/dev/null || true
	@echo "kodoc removed"

# === Run Compiler ===

# Usage: make run-lex FILE=examples/hello.ko
FILE ?= examples/hello.ko

run-lex:
	cargo run -p kodoc -- lex $(FILE)

run-parse:
	cargo run -p kodoc -- parse $(FILE)

run-check:
	cargo run -p kodoc -- check $(FILE)

run-build:
	cargo run -p kodoc -- build $(FILE)

run-repl:
	cargo run -p kodoc -- repl

# Check with JSON error output (agent-friendly)
run-check-json:
	cargo run -p kodoc -- check $(FILE) --json-errors

# Generate confidence report
run-confidence:
	cargo run -p kodoc -- confidence-report $(FILE)

# Describe module
run-describe:
	cargo run -p kodoc -- describe $(FILE)

# Explain an error code (e.g., make explain CODE=E0200)
CODE ?= E0200
explain:
	cargo run -p kodoc -- explain $(CODE)

# Auto-fix errors
run-fix:
	cargo run -p kodoc -- fix $(FILE)

# === MCP Server ===

mcp:
	cargo run -p kodo-mcp

# === Examples ===

# Build and run an example (e.g., make example FILE=examples/hello.ko)
example: run-build
	@BINARY=$$(basename $(FILE) .ko) && \
	DIR=$$(dirname $(FILE)) && \
	echo "Running $$DIR/$$BINARY..." && \
	$$DIR/$$BINARY

# Check all examples compile
check-examples:
	@echo "Checking all examples..."
	@PASS=0; FAIL=0; \
	for f in examples/*.ko; do \
		if cargo run -p kodoc -- check "$$f" >/dev/null 2>&1; then \
			PASS=$$((PASS + 1)); \
		else \
			echo "  FAIL: $$f"; \
			FAIL=$$((FAIL + 1)); \
		fi; \
	done; \
	echo "  $$PASS passed, $$FAIL failed"

# === Benchmarks ===

bench:
	cargo bench --workspace

bench-lexer:
	cargo bench -p kodo_lexer

bench-parser:
	cargo bench -p kodo_parser

bench-types:
	cargo bench -p kodo_types

bench-codegen:
	cargo bench -p kodo_codegen

# === Fuzzing ===

fuzz-lexer:
	cargo +nightly fuzz run fuzz_lexer -- -max_total_time=60

fuzz-parser:
	cargo +nightly fuzz run fuzz_parser -- -max_total_time=60

fuzz:
	$(MAKE) fuzz-lexer
	$(MAKE) fuzz-parser

# === Utilities ===

loc:
	@echo "Rust source:"
	@find crates -name '*.rs' | xargs wc -l | tail -1
	@echo "Kōdo examples:"
	@find examples -name '*.ko' | wc -l | xargs -I{} echo "  {} files"
	@echo "UI tests:"
	@find tests/ui -name '*.ko' | wc -l | xargs -I{} echo "  {} files"

tree:
	@tree crates -I target --dirsfirst

deps:
	cargo tree --workspace --depth 1

# Show what kodoc is installed and where
which:
	@echo "Installed kodoc binaries:"
	@type -a kodoc 2>/dev/null || echo "  not found in PATH"
	@echo ""
	@kodoc --version 2>/dev/null || echo "  kodoc not available"

# Count tests across the workspace
test-count:
	@cargo test --workspace -- --list 2>/dev/null | grep ": test$$" | wc -l | xargs -I{} echo "{} tests"

# === Coverage ===

coverage:
	cargo llvm-cov --workspace --summary-only

coverage-report:
	cargo llvm-cov --workspace --html
	@echo "Coverage report generated at target/llvm-cov/html/index.html"

# === Help ===

help:
	@echo "Kōdo Compiler — Makefile targets"
	@echo ""
	@echo "  Quality:"
	@echo "    make            Run fmt + clippy + test + doc"
	@echo "    make ci         CI-equivalent checks (fmt-check + clippy + test + ui-test + doc)"
	@echo "    make ci-full    CI + validate-docs"
	@echo "    make test       Run all workspace tests"
	@echo "    make ui-test    Run UI tests (kotest)"
	@echo "    make clippy     Run clippy with -D warnings"
	@echo "    make fmt        Format all code"
	@echo ""
	@echo "  Build:"
	@echo "    make build      Debug build"
	@echo "    make release    Release build"
	@echo "    make install    Build release + install to ~/.local/bin"
	@echo "    make clean      Remove build artifacts"
	@echo ""
	@echo "  Run (FILE=examples/hello.ko):"
	@echo "    make run-check          Type-check a file"
	@echo "    make run-check-json     Type-check with JSON errors"
	@echo "    make run-build          Compile to binary"
	@echo "    make run-fix            Auto-fix errors"
	@echo "    make run-confidence     Confidence report"
	@echo "    make run-describe       Describe module"
	@echo "    make run-repl           Interactive REPL"
	@echo "    make example            Build and run an example"
	@echo "    make explain CODE=E0200 Explain an error code"
	@echo ""
	@echo "  Examples:"
	@echo "    make check-examples  Check all examples compile"
	@echo "    make validate-docs   Validate doc examples"
	@echo ""
	@echo "  Bench & Fuzz:"
	@echo "    make bench       Run all benchmarks"
	@echo "    make fuzz        Fuzz lexer + parser (60s each)"
	@echo ""
	@echo "  Utilities:"
	@echo "    make loc         Count lines of code"
	@echo "    make which       Show installed kodoc"
	@echo "    make test-count  Count total tests"
	@echo "    make deps        Show dependency tree"
	@echo "    make help        This message"
