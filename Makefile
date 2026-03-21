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

PREFIX ?= $(HOME)/.kodo

install: release
	@mkdir -p $(PREFIX)/bin
	@cp target/release/kodoc $(PREFIX)/bin/kodoc
	@chmod +x $(PREFIX)/bin/kodoc
	@echo ""
	@echo "  kodoc installed to $(PREFIX)/bin/kodoc"
	@echo ""
	@echo "  Add to your PATH (add this to ~/.zshrc or ~/.bashrc):"
	@echo ""
	@echo "    export PATH=\"$(PREFIX)/bin:\$$PATH\""
	@echo ""
	@echo "  Then restart your shell or run: source ~/.zshrc"
	@echo ""
	@echo "  Verify: kodoc --version"
	@echo ""

uninstall:
	@rm -f $(PREFIX)/bin/kodoc
	@echo "kodoc removed from $(PREFIX)/bin/"

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
	@find crates -name '*.rs' | xargs wc -l | tail -1

tree:
	@tree crates -I target --dirsfirst

deps:
	cargo tree --workspace --depth 1

# === Coverage ===

coverage:
	cargo llvm-cov --workspace --summary-only

coverage-report:
	cargo llvm-cov --workspace --html
	@echo "Coverage report generated at target/llvm-cov/html/index.html"
