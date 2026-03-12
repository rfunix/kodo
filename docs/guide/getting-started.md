# Getting Started with Kōdo

This guide walks you through installing the Kōdo compiler and running your first program.

## Installation

### Option A: Download Pre-Built Binary

The quickest way to get started. Download from the [Releases page](https://github.com/rfunix/kodo/releases):

```bash
# macOS (Apple Silicon)
curl -L https://github.com/rfunix/kodo/releases/latest/download/kodoc-macos-aarch64 -o kodoc

# macOS (Intel)
curl -L https://github.com/rfunix/kodo/releases/latest/download/kodoc-macos-x86_64 -o kodoc

# Linux (x86_64)
curl -L https://github.com/rfunix/kodo/releases/latest/download/kodoc-linux-x86_64 -o kodoc

chmod +x kodoc
sudo mv kodoc /usr/local/bin/
```

**Requirement:** A C linker (`cc`) is needed to link the final executable:
- **macOS**: Install Xcode Command Line Tools (`xcode-select --install`)
- **Linux**: Install `build-essential` (`sudo apt install build-essential`)

Verify:

```bash
kodoc --version
```

### Option B: Build from Source

**Prerequisites:**
- **Rust toolchain** (1.91 or later) — install via [rustup](https://rustup.rs/)
- **C linker** (`cc`) — see above

```bash
git clone https://github.com/rfunix/kodo.git
cd kodo
make install
```

This builds in release mode and installs `kodoc` to `~/.kodo/bin/`. Add to your PATH:

```bash
echo 'export PATH="$HOME/.kodo/bin:$PATH"' >> ~/.zshrc  # or ~/.bashrc
source ~/.zshrc
```

Verify:

```bash
kodoc --version
```

> **Tip:** You can also use `make install PREFIX=/usr/local` to install system-wide.

### Optional: Z3 for Static Verification

To enable compile-time contract verification via Z3:

- **macOS**: `brew install z3`
- **Ubuntu/Debian**: `sudo apt-get install libz3-dev`

Then rebuild with `cargo build -p kodoc --release --features smt`. See the [Contracts guide](contracts.md#enabling-z3-for-static-verification) for details.

## Your First Program

Create a file called `hello.ko`:

```rust
module hello {
    meta {
        purpose: "My first Kōdo program",
        version: "0.1.0",
        author: "Your Name"
    }

    fn main() {
        println("Hello, World!")
    }
}
```

Every Kōdo program has:
1. A **module** declaration with a name
2. A **meta** block describing the module's purpose, version, and author
3. A **`main` function** as the entry point

## Compile and Run

```bash
kodoc build hello.ko -o hello
./hello
```

You should see:

```rust
Successfully compiled `hello` → hello
Hello, World!
```

## A More Interesting Example

Let's write a program that computes Fibonacci numbers:

```rust
module fibonacci {
    meta {
        purpose: "Compute Fibonacci numbers",
        version: "0.1.0",
        author: "Your Name"
    }

    fn fib(n: Int) -> Int {
        if n <= 1 {
            return n
        }
        return fib(n - 1) + fib(n - 2)
    }

    fn main() {
        let result: Int = fib(10)
        print_int(result)
    }
}
```

Compile and run:

```bash
kodoc build fibonacci.ko -o fibonacci
./fibonacci
```

This prints `55` — the 10th Fibonacci number.

## Checking Without Compiling

You can type-check and verify contracts without generating a binary:

```bash
kodoc check hello.ko
```

This is useful for fast feedback during development.

## Next Steps

- [A Tour of Kōdo](tour.md) — a quick walkthrough of all language features
- [Language Basics](language-basics.md) — types, variables, and control flow
- [Data Types and Pattern Matching](data-types.md) — structs, enums, and `match`
- [Contracts](contracts.md) — runtime preconditions and postconditions
- [CLI Reference](cli-reference.md) — all available commands and flags
