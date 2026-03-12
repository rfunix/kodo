# HTTP & JSON

Kodo provides built-in functions for making HTTP requests and parsing JSON responses. These builtins enable AI agents to interact with web services and process structured data without external dependencies.

## HTTP Requests

### `http_get(url)`

Performs an HTTP GET request and returns the response body as a String. The return value is an Int status code: 0 on success, -1 on error.

```rust
fn main() {
    let status: Int = http_get("http://httpbin.org/get")
    if status == 0 {
        println("request succeeded")
    }
}
```

### `http_post(url, body)`

Performs an HTTP POST request with a string body. Like `http_get`, it returns 0 on success and -1 on error.

```rust
fn main() {
    let payload: String = "{\"name\": \"kodo\"}"
    let status: Int = http_post("http://httpbin.org/post", payload)
    print_int(status)
}
```

### Plain HTTP Only

In v1, HTTP builtins use plain HTTP (no TLS). If you need to reach an HTTPS endpoint, route through a local proxy or use a service that exposes an HTTP interface.

## JSON Parsing

Kodo represents parsed JSON as an opaque handle (an `Int`). You obtain a handle by parsing a JSON string, extract values from it by key, and free it when done.

### `json_parse(str)`

Parses a JSON string and returns a handle. Returns 0 if parsing fails.

```rust
let handle: Int = json_parse("{\"count\": 42, \"name\": \"kodo\"}")
if handle == 0 {
    println("parse error")
}
```

### `json_get_int(handle, key)`

Extracts an integer value from the parsed JSON object by key. Returns 0 if the key does not exist or the value is not an integer.

```rust
let count: Int = json_get_int(handle, "count")
print_int(count)  // 42
```

### `json_get_string(handle, key)`

Extracts a string value from the parsed JSON object by key. Returns the string value on success.

```rust
let name: String = json_get_string(handle, "name")
println(name)  // kodo
```

### `json_free(handle)`

Frees the memory associated with a parsed JSON handle. Every handle returned by `json_parse` must be freed when no longer needed. Using a handle after freeing it is undefined behavior.

```rust
json_free(handle)
```

Passing 0 (a null handle) to `json_free` is safe and does nothing.

## Complete Example

Here is a program that fetches data from an HTTP endpoint and parses the JSON response:

```rust
module http_client {
    meta {
        purpose: "HTTP client and JSON parsing demonstration",
        version: "0.1.0"
    }

    fn main() {
        // Perform HTTP GET request
        let status: Int = http_get("http://httpbin.org/get")

        // Parse a JSON string into an opaque handle
        let handle: Int = json_parse("{\"count\": 7, \"lang\": \"kodo\"}")

        // Extract fields by key
        let count: Int = json_get_int(handle, "count")
        print_int(count)

        // Always free the handle when done
        json_free(handle)

        println("done")
    }
}
```

Compile and run:

```bash
cargo run -p kodoc -- build http_client.ko -o http_client
./http_client
```

## Summary of Builtins

| Builtin | Parameters | Returns | Description |
|---------|-----------|---------|-------------|
| `http_get` | `url: String` | `Int` (0 = success) | GET request |
| `http_post` | `url: String, body: String` | `Int` (0 = success) | POST request |
| `json_parse` | `str: String` | `Int` (handle, 0 = error) | Parse JSON string |
| `json_get_int` | `handle: Int, key: String` | `Int` | Extract integer field |
| `json_get_string` | `handle: Int, key: String` | `String` | Extract string field |
| `json_free` | `handle: Int` | `()` | Free parsed JSON |

## Next Steps

- [Concurrency](concurrency.md) -- spawning tasks and the cooperative scheduler
- [Actors](actors.md) -- stateful actors with message passing
- [Modules and Imports](modules-and-imports.md) -- multi-file programs and the standard library
