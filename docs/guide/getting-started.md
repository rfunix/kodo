# Getting Started with Kōdo

This guide walks you through installing the Kōdo compiler and running your first program.

## Prerequisites

- **Rust toolchain** (1.75 or later) — install via [rustup](https://rustup.rs/)
- **C linker** (`cc`) — needed to link the final executable
  - **macOS**: Install Xcode Command Line Tools (`xcode-select --install`)
  - **Linux**: Install `build-essential` (`apt install build-essential`) or equivalent

## Installation

Clone the repository and build all crates:

```bash
git clone https://github.com/kodo-lang/kodo.git
cd kodo
cargo build --workspace
```

This builds the compiler (`kodoc`) and the runtime library (`libkodo_runtime.a`). Both are placed in `target/debug/`.

You can verify the build worked:

```bash
cargo run -p kodoc -- --version
```

## Your First Program

Create a file called `hello.ko`:

```
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
cargo run -p kodoc -- build hello.ko -o hello
./hello
```

You should see:

```
Successfully compiled `hello` → hello
Hello, World!
```

## A More Interesting Example

Let's write a program that computes Fibonacci numbers:

```
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
cargo run -p kodoc -- build fibonacci.ko -o fibonacci
./fibonacci
```

This prints `55` — the 10th Fibonacci number.

## Checking Without Compiling

You can type-check and verify contracts without generating a binary:

```bash
cargo run -p kodoc -- check hello.ko
```

This is useful for fast feedback during development.

## Next Steps

- [A Tour of Kōdo](tour.md) — a quick walkthrough of all language features
- [Language Basics](language-basics.md) — types, variables, and control flow
- [Data Types and Pattern Matching](data-types.md) — structs, enums, and `match`
- [Contracts](contracts.md) — runtime preconditions and postconditions
- [CLI Reference](cli-reference.md) — all available commands and flags
