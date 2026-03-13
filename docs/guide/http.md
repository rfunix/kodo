# HTTP, JSON & Networking

Kodo provides built-in functions for making HTTP requests, running HTTP servers, and working with JSON. These builtins enable AI agents to build real web services and CLI tools without external dependencies.

## HTTP Client

### `http_get(url) -> Result<String, String>`

Performs an HTTP GET request. Returns `Result::Ok(body)` on success or `Result::Err(message)` on failure.

```rust
let result: Result<String, String> = http_get("http://httpbin.org/get")
```

### `http_post(url, body) -> Result<String, String>`

Performs an HTTP POST request with a string body.

```rust
let result: Result<String, String> = http_post("http://httpbin.org/post", "hello")
```

### Plain HTTP Only

In v1, HTTP builtins use plain HTTP (no TLS). If you need to reach an HTTPS endpoint, route through a local proxy or use a service that exposes an HTTP interface.

## HTTP Server

Kodo includes a synchronous HTTP server powered by `tiny_http`. Servers and requests are managed via opaque `Int` handles.

### `http_server_new(port) -> Int`

Creates a new HTTP server listening on the given port. Returns a handle, or 0 on failure.

```rust
let server: Int = http_server_new(8080)
if server == 0 {
    println("failed to create server")
}
```

### `http_server_recv(server) -> Int`

Blocks until a request is received. Returns a request handle, or 0 on error.

```rust
let req: Int = http_server_recv(server)
```

### `http_request_method(req) -> String`

Returns the HTTP method (`"GET"`, `"POST"`, etc.).

### `http_request_path(req) -> String`

Returns the URL path (e.g., `"/api/data"`).

### `http_request_body(req) -> String`

Returns the request body as a string.

### `http_respond(req, status, body)`

Sends an HTTP response with a status code and body. After calling this, the request handle is consumed and must not be reused.

```rust
http_respond(req, 200, "OK")
```

### `http_server_free(server)`

Frees an HTTP server handle when done.

### Complete Server Example

```rust
module http_api {
    meta {
        purpose: "HTTP server with JSON responses"
        version: "0.1.0"
    }

    fn main() -> Int {
        let server: Int = http_server_new(8080)
        if server == 0 {
            println("failed to create server")
            return 1
        }
        println("server listening on port 8080")

        let req: Int = http_server_recv(server)
        let method: String = http_request_method(req)
        let path: String = http_request_path(req)
        println(f"received {method} {path}")

        let body: Int = json_new_object()
        json_set_string(body, "status", "ok")
        let response: String = json_stringify(body)
        http_respond(req, 200, response)

        http_server_free(server)
        return 0
    }
}
```

## JSON Parsing

Kodo represents parsed JSON as an opaque handle (an `Int`). You obtain a handle by parsing a JSON string, extract values from it by key, and free it when done.

### `json_parse(str) -> Int`

Parses a JSON string and returns a handle. Returns 0 if parsing fails.

```rust
let handle: Int = json_parse("{\"count\": 42}")
```

### `json_get_int(handle, key) -> Int`

Extracts an integer value by key. Returns 0 if the key does not exist.

### `json_get_string(handle, key) -> String`

Extracts a string value by key.

### `json_free(handle)`

Frees the memory associated with a parsed JSON handle. Passing 0 is safe.

## JSON Builder

Build JSON objects programmatically without string manipulation.

### `json_new_object() -> Int`

Creates a new empty JSON object and returns a handle.

```rust
let obj: Int = json_new_object()
```

### `json_set_string(handle, key, value)`

Sets a string field on the JSON object.

### `json_set_int(handle, key, value)`

Sets an integer field.

### `json_set_bool(handle, key, value)`

Sets a boolean field.

### `json_stringify(handle) -> String`

Serializes a JSON object to a string.

```rust
let obj: Int = json_new_object()
json_set_string(obj, "name", "kodo")
json_set_int(obj, "version", 1)
json_set_bool(obj, "stable", false)
let output: String = json_stringify(obj)
println(output)  // {"name":"kodo","stable":false,"version":1}
```

## Summary of Builtins

### HTTP Client

| Builtin | Parameters | Returns | Description |
|---------|-----------|---------|-------------|
| `http_get` | `url: String` | `Result<String, String>` | GET request |
| `http_post` | `url: String, body: String` | `Result<String, String>` | POST request |

### HTTP Server

| Builtin | Parameters | Returns | Description |
|---------|-----------|---------|-------------|
| `http_server_new` | `port: Int` | `Int` (handle) | Create server |
| `http_server_recv` | `server: Int` | `Int` (request handle) | Block for request |
| `http_request_method` | `req: Int` | `String` | Get HTTP method |
| `http_request_path` | `req: Int` | `String` | Get URL path |
| `http_request_body` | `req: Int` | `String` | Get request body |
| `http_respond` | `req: Int, status: Int, body: String` | `()` | Send response |
| `http_server_free` | `server: Int` | `()` | Free server |

### JSON

| Builtin | Parameters | Returns | Description |
|---------|-----------|---------|-------------|
| `json_parse` | `str: String` | `Int` (handle) | Parse JSON |
| `json_get_int` | `handle: Int, key: String` | `Int` | Extract integer |
| `json_get_string` | `handle: Int, key: String` | `String` | Extract string |
| `json_free` | `handle: Int` | `()` | Free JSON handle |
| `json_new_object` | — | `Int` (handle) | Create empty object |
| `json_set_string` | `handle: Int, key: String, value: String` | `()` | Set string field |
| `json_set_int` | `handle: Int, key: String, value: Int` | `()` | Set integer field |
| `json_set_bool` | `handle: Int, key: String, value: Bool` | `()` | Set boolean field |
| `json_stringify` | `handle: Int` | `String` | Serialize to string |

## Next Steps

- [Concurrency](concurrency.md) -- spawning tasks and the cooperative scheduler
- [Actors](actors.md) -- stateful actors with message passing
- [Modules and Imports](modules-and-imports.md) -- multi-file programs and the standard library
