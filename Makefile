.PHONY: all check fmt clippy test doc build clean run-lex run-parse run-check run-build bench ci

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
