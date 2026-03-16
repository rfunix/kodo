# Modules and Imports

Kōdo programs can span multiple files. Each file contains a single module, and modules can import functions and types from other modules.

## Module Structure

Every `.ko` file contains exactly one module:

```rust
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

```rust
import math
```

This makes all functions and types defined in `math.ko` available in the current module.

### Import Resolution

When the compiler sees `import math`, it looks for `math.ko` in the same directory as the importing file. The imported module is compiled first, and its exported definitions are made available.

### Example: Two-File Program

**`math.ko`** — a utility module:

```rust
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

```rust
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
```rust
7
30
```

The compiler resolves the import, compiles `math.ko`, and links everything into a single binary.

## The Standard Library Prelude

Kōdo's standard library provides two foundational types that are available in every program without an explicit import:

- **`Option<T>`** — represents an optional value (`Some(T)` or `None`)
- **`Result<T, E>`** — represents success or failure (`Ok(T)` or `Err(E)`)

These types are automatically injected before your code is type-checked. You can use them immediately:

```rust
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

## Built-in Collection Types

Kōdo provides built-in collection types available in every program:

### List\<T\>

A dynamic array of elements, accessed via free functions:

```rust
let nums: List<Int> = list_new()
list_push(nums, 10)
list_push(nums, 20)
list_push(nums, 30)
let len: Int = list_length(nums)         // 3
let first: Int = list_get(nums, 0)       // 10
let has: Bool = list_contains(nums, 10)  // true
```

#### Full List API

| Function | Description |
|----------|-------------|
| `list_new()` | Create a new empty list |
| `list_push(list, value)` | Append a value to the end |
| `list_get(list, index)` | Get value at index |
| `list_length(list)` | Number of elements |
| `list_contains(list, value)` | Check if value exists |
| `list_pop(list)` | Remove and return the last element |
| `list_remove(list, index)` | Remove element at index |
| `list_set(list, index, value)` | Set value at index |
| `list_slice(list, start, end)` | Get a sub-list from start to end (exclusive) |
| `list_sort(list)` | Sort the list in ascending order (in place) |
| `list_join(list, separator)` | Join list elements into a `String` with separator |

### Map\<K, V\>

A generic key-value hash map. Keys and values can be `Int` or `String` in any combination. The type is determined by the annotation on the `let` binding:

```rust
// Map<Int, Int>
let scores: Map<Int, Int> = map_new()
map_insert(scores, 1, 100)
let val: Int = map_get(scores, 1)           // 100

// Map<String, Int>
let config: Map<String, Int> = map_new()
map_insert(config, "port", 8080)
let port: Int = map_get(config, "port")     // 8080

// Map<Int, String>
let names: Map<Int, String> = map_new()
map_insert(names, 1, "one")
let first: String = map_get(names, 1)       // "one"

// Map<String, String>
let headers: Map<String, String> = map_new()
map_insert(headers, "Content-Type", "application/json")
let ct: String = map_get(headers, "Content-Type")
```

#### Map API

All functions work with any `Map<K, V>` where K, V are `Int` or `String`:

| Function | Description |
|----------|-------------|
| `map_new()` | Create a new empty map (type from annotation) |
| `map_insert(m, k, v)` | Insert or update a key-value pair |
| `map_get(m, k)` | Get value by key |
| `map_contains_key(m, k)` | Check if key exists |
| `map_length(m)` | Number of entries |
| `m.remove(k)` | Remove a key-value pair |
| `m.is_empty()` | Check if map is empty |

### String Methods

#### `split(separator)`

Splits a string by a separator, returning a `List<String>`:

```rust
let parts: List<String> = "a,b,c".split(",")
```

#### `lines()`

Splits a string by newline characters, returning a `List<String>`:

```rust
let text: String = "line1\nline2\nline3"
let all_lines: List<String> = text.lines()
let count: Int = list_length(all_lines)  // 3
```

#### `parse_int()`

Parses a string as an integer, returning an `Option<Int>`:

```rust
let valid: Option<Int> = "42".parse_int()     // Option::Some(42)
let bad: Option<Int> = "hello".parse_int()     // Option::None
```

## Qualified Imports with `::`

You can use `::` as a path separator for imports, particularly useful for standard library modules:

```rust
import std::option
import std::result
```

The dot separator (`.`) is also supported for backward compatibility:

```rust
import math.utils
```

Both forms resolve to the same module.

### Selective Imports with `from...import`

To bring specific names from a module into scope, use the `from...import` syntax:

```rust
from std::option import Some, None
from math::utils import add, multiply
```

This imports only the named items, keeping the local scope clean.

## Qualified Calls

When importing a module, you can use qualified calls with dot notation or `::`:

```rust
import math
let result: Int = math.add(1, 2)
let result2: Int = math::add(1, 2)
```

This is equivalent to calling `add(1, 2)` directly — the module prefix makes the origin explicit.

## Compilation Certificates

When you compile a Kōdo program, the compiler emits a **compilation certificate** alongside the binary. For `hello.ko`, the compiler creates `hello.ko.cert.json`:

```json
{
  "module": "hello",
  "purpose": "My first Kōdo program",
  "version": "0.1.0",
  "contracts": {
    "requires_count": 1,
    "ensures_count": 1,
    "mode": "static",
    "static_verified": 1,
    "runtime_checks_needed": 1,
    "failures": 0
  },
  "functions": ["main", "validate"],
  "confidence": [
    {
      "name": "main",
      "declared": 0.95,
      "effective": 0.90,
      "callees": ["validate"]
    },
    {
      "name": "validate",
      "declared": 0.90,
      "effective": 0.90,
      "callees": []
    }
  ],
  "source_hash": "sha256:...",
  "binary_hash": "sha256:...",
  "certificate_hash": "sha256:..."
}
```

This certificate is a machine-readable record of what was compiled. AI agents can use certificates to verify:

- What the module claims to do (from `meta`)
- How many contracts are in place, and which were statically verified vs runtime-checked
- Per-function confidence scores (declared and effective after transitive propagation)
- Whether the source has changed since the last compilation

Use `kodoc audit <file> --json` for a consolidated report combining confidence, contracts, and annotations with a deployability verdict.

## Next Steps

- [Error Handling](error-handling.md) — using `Option<T>` and `Result<T, E>`
- [CLI Reference](cli-reference.md) — all `kodoc` commands and flags
- [Language Basics](language-basics.md) — types, variables, and control flow
