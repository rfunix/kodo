# Building CLI Tools

Kodo provides built-in functions for reading command-line arguments, standard input, and controlling process exit. Combined with File I/O and JSON, you can build complete CLI tools.

## Command-Line Arguments

### `args() -> List<String>`

Returns all command-line arguments as a `List<String>`, including the binary name as the first element.

```rust
module cli_args {
    meta {
        purpose: "demonstrates reading command-line arguments"
        version: "0.1.0"
    }

    fn main() -> Int {
        let arg_list: List<String> = args()
        println("arguments received")
        return 0
    }
}
```

Compile and run:

```bash
kodoc build cli_args.ko -o cli_args
./cli_args foo bar baz
```

## Standard Input

### `readln() -> String`

Reads a single line from standard input, stripping the trailing newline.

```rust
println("Enter your name:")
let name: String = readln()
println(f"Hello, {name}!")
```

## Process Exit

### `exit(code: Int)`

Exits the process immediately with the given exit code.

```rust
if error_occurred {
    println("fatal error")
    exit(1)
}
```

## File I/O

Kodo provides a complete set of file and directory operations:

| Builtin | Parameters | Returns | Description |
|---------|-----------|---------|-------------|
| `file_read` | `path: String` | `Result<String, String>` | Read file contents |
| `file_write` | `path: String, content: String` | `Result<String, String>` | Write file (overwrite) |
| `file_append` | `path: String, content: String` | `Result<String, String>` | Append to file |
| `file_exists` | `path: String` | `Bool` | Check if file exists |
| `file_delete` | `path: String` | `Int` (0=ok) | Delete a file |
| `dir_list` | `path: String` | `List<String>` | List directory entries |
| `dir_exists` | `path: String` | `Bool` | Check if directory exists |

## Extended Math

| Builtin | Parameters | Returns | Description |
|---------|-----------|---------|-------------|
| `sqrt` | `x: Float64` | `Float64` | Square root |
| `pow` | `base: Float64, exp: Float64` | `Float64` | Power |
| `sin` | `x: Float64` | `Float64` | Sine |
| `cos` | `x: Float64` | `Float64` | Cosine |
| `log` | `x: Float64` | `Float64` | Natural logarithm |
| `floor` | `x: Float64` | `Float64` | Floor |
| `ceil` | `x: Float64` | `Float64` | Ceiling |
| `round` | `x: Float64` | `Float64` | Round to nearest |
| `rand_int` | `min: Int, max: Int` | `Int` | Random integer in range |

## Next Steps

- [HTTP, JSON & Networking](http.md) — HTTP client and server, JSON parsing and building
- [Modules and Imports](modules-and-imports.md) — multi-file programs and the standard library
