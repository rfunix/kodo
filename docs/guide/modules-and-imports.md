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

### Map\<K, V\>

A key-value hash map. Maps support both `Int` and `String` keys:

#### Int-keyed Maps

```rust
let scores: Map<Int, Int> = map_new()
map_insert(scores, 1, 100)
let val: Int = map_get(scores, 1)           // 100
let has: Bool = map_contains_key(scores, 1)  // true
let len: Int = map_length(scores)            // 1
```

#### String-keyed Maps

```rust
let config: Map<String, Int> = map_string_new()
map_string_insert(config, "port", 8080)
map_string_insert(config, "timeout", 30)
let port: Int = map_string_get(config, "port")            // 8080
let has: Bool = map_string_contains(config, "timeout")     // true
map_string_remove(config, "timeout")
let len: Int = map_string_len(config)                      // 1
```

#### Full Map API

| Function (Int keys) | Function (String keys) | Description |
|---------------------|----------------------|-------------|
| `map_new()` | `map_string_new()` | Create a new empty map |
| `map_insert(m, k, v)` | `map_string_insert(m, k, v)` | Insert or update a key-value pair |
| `map_get(m, k)` | `map_string_get(m, k)` | Get value by key |
| `map_contains_key(m, k)` | `map_string_contains(m, k)` | Check if key exists |
| `map_length(m)` | `map_string_len(m)` | Number of entries |
| `map_remove(m, k)` | `map_string_remove(m, k)` | Remove a key-value pair |

### String.split()

Splits a string by a separator, returning a `List<String>`:

```rust
let parts: List<String> = "a,b,c".split(",")
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
