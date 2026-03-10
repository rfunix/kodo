# Modules and Imports

Kōdo programs can span multiple files. Each file contains a single module, and modules can import functions and types from other modules.

## Module Structure

Every `.ko` file contains exactly one module:

```
module my_module {
    meta {
        purpose: "What this module does"
        version: "0.1.0"
    }

    // functions, types, etc.
}
```

The module name should match the filename (e.g., `math.ko` contains `module math`).

## Importing Modules

Use `import` to bring another module's definitions into scope:

```
import math
```

This makes all functions and types defined in `math.ko` available in the current module.

### Import Resolution

When the compiler sees `import math`, it looks for `math.ko` in the same directory as the importing file. The imported module is compiled first, and its exported definitions are made available.

### Example: Two-File Program

**`math.ko`** — a utility module:

```
module math {
    meta {
        purpose: "Math utilities"
        version: "0.1.0"
    }

    fn add(a: Int, b: Int) -> Int {
        return a + b
    }

    fn multiply(a: Int, b: Int) -> Int {
        return a * b
    }
}
```

**`main.ko`** — uses the math module:

```
module main {
    meta {
        purpose: "Main program"
        version: "0.1.0"
    }

    import math

    fn main() {
        let sum: Int = add(3, 4)
        let product: Int = multiply(5, 6)
        print_int(sum)
        print_int(product)
    }
}
```

Compile and run:

```bash
cargo run -p kodoc -- build main.ko -o main
./main
```

Output:
```
7
30
```

The compiler resolves the import, compiles `math.ko`, and links everything into a single binary.

## The Standard Library Prelude

Kōdo's standard library provides two foundational types that are available in every program without an explicit import:

- **`Option<T>`** — represents an optional value (`Some(T)` or `None`)
- **`Result<T, E>`** — represents success or failure (`Ok(T)` or `Err(E)`)

These types are automatically injected before your code is type-checked. You can use them immediately:

```
module my_program {
    meta {
        purpose: "Using stdlib types"
        version: "0.1.0"
    }

    fn maybe_double(x: Int) -> Option<Int> {
        if x > 0 {
            return Option::Some(x * 2)
        }
        return Option::None
    }

    fn main() {
        let result: Option<Int> = maybe_double(21)
        match result {
            Option::Some(v) => { print_int(v) }
            Option::None => { println("nothing") }
        }
    }
}
```

Output: `42`

## Compilation Certificates

When you compile a Kōdo program, the compiler emits a **compilation certificate** alongside the binary. For `hello.ko`, the compiler creates `hello.ko.cert.json`:

```json
{
  "module_name": "hello",
  "purpose": "My first Kōdo program",
  "version": "0.1.0",
  "contracts": {
    "requires_count": 0,
    "ensures_count": 0,
    "mode": "runtime"
  },
  "functions": ["main"],
  "source_hash": "sha256:..."
}
```

This certificate is a machine-readable record of what was compiled. AI agents can use certificates to verify:

- What the module claims to do (from `meta`)
- How many contracts are in place
- Whether the source has changed since the last compilation

## Next Steps

- [Error Handling](error-handling.md) — using `Option<T>` and `Result<T, E>`
- [CLI Reference](cli-reference.md) — all `kodoc` commands and flags
- [Language Basics](language-basics.md) — types, variables, and control flow
