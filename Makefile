.PHONY: all check fmt clippy test doc build clean run-lex run-parse run-check run-build bench ci install uninstall

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

# All checks (CI-equivalent)
ci: fmt-check clippy test doc

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
	cargo bench -p kodo_lexer

# === Utilities ===

loc:
	@find crates -name '*.rs' | xargs wc -l | tail -1

tree:
	@tree crates -I target --dirsfirst

deps:
	cargo tree --workspace --depth 1
