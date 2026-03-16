# Standard Library Reference

This is the complete reference for all built-in functions and methods available in every Kōdo program without explicit imports.

## I/O

### `println(msg: String)`

Prints a string to stdout followed by a newline.

```rust
println("Hello, world!")
```

### `print(msg: String)`

Prints a string to stdout without a trailing newline.

```rust
print("Enter name: ")
```

### `print_int(value: Int)`

Prints an integer to stdout.

```rust
print_int(42)
```

### `print_float(value: Float64)`

Prints a float to stdout without a newline.

```rust
print_float(3.14)
```

### `println_float(value: Float64)`

Prints a float to stdout followed by a newline.

```rust
println_float(2.718)
```

### `readln() -> String`

Reads one line from stdin and returns it as a string.

```rust
let name: String = readln()
```

### `args() -> List<String>`

Returns the command-line arguments as a list of strings.

```rust
let argv: List<String> = args()
let first: String = list_get(argv, 0)
```

### `exit(code: Int)`

Exits the process with the given exit code.

```rust
exit(1)
```

## Math

### `abs(x: Int) -> Int`

Returns the absolute value of `x`.

```rust
let n: Int = abs(-5)  // 5
```

### `min(a: Int, b: Int) -> Int`

Returns the smaller of two integers.

```rust
let m: Int = min(3, 7)  // 3
```

### `max(a: Int, b: Int) -> Int`

Returns the larger of two integers.

```rust
let m: Int = max(3, 7)  // 7
```

### `clamp(x: Int, lo: Int, hi: Int) -> Int`

Clamps `x` to the range `[lo, hi]`.

```rust
let c: Int = clamp(50, 0, 25)  // 25
```

### `rand_int(min: Int, max: Int) -> Int`

Returns a random integer in the range `[min, max]`.

```rust
let r: Int = rand_int(1, 100)
```

### `sqrt(x: Float64) -> Float64`

Returns the square root of `x`.

```rust
let s: Float64 = sqrt(9.0)  // 3.0
```

### `pow(base: Float64, exp: Float64) -> Float64`

Returns `base` raised to the power `exp`.

```rust
let p: Float64 = pow(2.0, 10.0)  // 1024.0
```

### `sin(x: Float64) -> Float64`

Returns the sine of `x` (in radians).

```rust
let s: Float64 = sin(3.14159)
```

### `cos(x: Float64) -> Float64`

Returns the cosine of `x` (in radians).

```rust
let c: Float64 = cos(0.0)  // 1.0
```

### `log(x: Float64) -> Float64`

Returns the natural logarithm of `x`.

```rust
let l: Float64 = log(2.718)  // ~1.0
```

### `floor(x: Float64) -> Float64`

Rounds `x` down to the nearest integer.

```rust
let f: Float64 = floor(3.7)  // 3.0
```

### `ceil(x: Float64) -> Float64`

Rounds `x` up to the nearest integer.

```rust
let c: Float64 = ceil(3.2)  // 4.0
```

### `round(x: Float64) -> Float64`

Rounds `x` to the nearest integer.

```rust
let r: Float64 = round(3.5)  // 4.0
```

## String Methods

String methods are called with dot syntax on string values.

### `s.length() -> Int`

Returns the byte length of the string.

```rust
let len: Int = "hello".length()  // 5
```

### `s.contains(substr: String) -> Bool`

Returns `true` if the string contains `substr`.

```rust
let has: Bool = "hello world".contains("world")  // true
```

### `s.starts_with(prefix: String) -> Bool`

Returns `true` if the string starts with `prefix`.

```rust
let yes: Bool = "hello".starts_with("hel")  // true
```

### `s.ends_with(suffix: String) -> Bool`

Returns `true` if the string ends with `suffix`.

```rust
let yes: Bool = "hello".ends_with("llo")  // true
```

### `s.trim() -> String`

Returns a copy with leading and trailing whitespace removed.

```rust
let t: String = "  hello  ".trim()  // "hello"
```

### `s.to_upper() -> String`

Returns an uppercase copy of the string.

```rust
let u: String = "hello".to_upper()  // "HELLO"
```

### `s.to_lower() -> String`

Returns a lowercase copy of the string.

```rust
let l: String = "HELLO".to_lower()  // "hello"
```

### `s.substring(start: Int, end: Int) -> String`

Extracts a substring from byte index `start` to `end` (exclusive). Note: byte-based, not Unicode-aware.

```rust
let sub: String = "hello".substring(1, 4)  // "ell"
```

### `s.split(sep: String) -> List<String>`

Splits the string by `sep` and returns a list of parts.

```rust
let parts: List<String> = "a,b,c".split(",")
// parts contains ["a", "b", "c"]
```

### `s.lines() -> List<String>`

Splits the string by newline characters.

```rust
let ls: List<String> = "line1\nline2".lines()
```

### `s.char_at(index: Int) -> Int`

Returns the byte value at the given index.

```rust
let ch: Int = "ABC".char_at(0)  // 65
```

### `s.repeat(count: Int) -> String`

Returns the string repeated `count` times.

```rust
let r: String = "ab".repeat(3)  // "ababab"
```

### `s.parse_int() -> Int`

Parses the string as an integer. Returns `0` on failure.

```rust
let n: Int = "42".parse_int()  // 42
```

## Type Conversions

### `i.to_string() -> String` (on Int)

Converts an integer to its string representation.

```rust
let s: String = 42.to_string()  // "42"
```

### `i.to_float64() -> Float64` (on Int)

Converts an integer to a Float64.

```rust
let f: Float64 = 42.to_float64()  // 42.0
```

### `f.to_string() -> String` (on Float64)

Converts a float to its string representation.

```rust
let s: String = 3.14.to_string()
```

### `f.to_int() -> Int` (on Float64)

Truncates a float to an integer (toward zero).

```rust
let n: Int = 3.7.to_int()  // 3
```

### `b.to_string() -> String` (on Bool)

Converts a boolean to `"true"` or `"false"`.

```rust
let s: String = true.to_string()  // "true"
```

## File I/O

### `file_read(path: String) -> Result<String, String>`

Reads the entire contents of a file.

```rust
let content: Result<String, String> = file_read("data.txt")
```

### `file_write(path: String, content: String) -> Result<Unit, String>`

Writes content to a file, overwriting any existing content.

```rust
let r: Result<Unit, String> = file_write("out.txt", "hello")
```

### `file_append(path: String, content: String) -> Result<Unit, String>`

Appends content to a file.

```rust
let r: Result<Unit, String> = file_append("log.txt", "new line\n")
```

### `file_delete(path: String) -> Bool`

Deletes a file. Returns `true` on success.

```rust
let ok: Bool = file_delete("temp.txt")
```

### `file_exists(path: String) -> Bool`

Checks if a file exists.

```rust
let exists: Bool = file_exists("config.json")
```

### `dir_list(path: String) -> List<String>`

Lists the contents of a directory.

```rust
let entries: List<String> = dir_list(".")
```

### `dir_exists(path: String) -> Bool`

Checks if a directory exists.

```rust
let exists: Bool = dir_exists("src/")
```

## Collections — List\<T\>

Lists are created and manipulated via free functions. Methods like `.iter()`, `.map()`, `.filter()` are available as dot-syntax on list values.

### `list_new() -> List<T>`

Creates a new empty list. The element type is inferred from the type annotation.

```rust
let nums: List<Int> = list_new()
let names: List<String> = list_new()
```

### `list_push(list: List<T>, value: T)`

Appends a value to the end of the list.

```rust
list_push(nums, 42)
```

### `list_get(list: List<T>, index: Int) -> T`

Returns the element at the given index.

```rust
let first: Int = list_get(nums, 0)
```

### `list_length(list: List<T>) -> Int`

Returns the number of elements in the list.

```rust
let len: Int = list_length(nums)
```

### `list_contains(list: List<T>, value: T) -> Bool`

Returns `true` if the list contains the value.

```rust
let has: Bool = list_contains(nums, 42)
```

### `list_pop(list: List<T>) -> T`

Removes and returns the last element.

```rust
let last: Int = list_pop(nums)
```

### `list_remove(list: List<T>, index: Int) -> Bool`

Removes the element at the given index. Returns `true` on success.

```rust
let ok: Bool = list_remove(nums, 0)
```

### `list_set(list: List<T>, index: Int, value: T) -> Bool`

Sets the element at the given index. Returns `true` on success.

```rust
let ok: Bool = list_set(nums, 0, 99)
```

### `list_is_empty(list: List<T>) -> Bool`

Returns `true` if the list has no elements.

```rust
let empty: Bool = list_is_empty(nums)
```

### `list_reverse(list: List<T>)`

Reverses the list in place.

```rust
list_reverse(nums)
```

### List Methods (dot syntax)

These are called as methods on list values:

| Method | Signature | Description |
|--------|-----------|-------------|
| `list.slice(start, end)` | `(Int, Int) -> List<T>` | Extract a sub-list |
| `list.sort()` | `() -> Unit` | Sort in place (ascending) |
| `list.join(sep)` | `(String) -> String` | Join elements with separator (for `List<String>`) |
| `list.iter()` | `() -> Iterator` | Create an iterator for `for-in` or combinators |
| `list.map(f)` | `((T) -> U) -> List<U>` | Transform each element |
| `list.filter(f)` | `((T) -> Bool) -> List<T>` | Keep elements matching predicate |
| `list.fold(init, f)` | `(U, (U, T) -> U) -> U` | Reduce to a single value |
| `list.count()` | `() -> Int` | Count elements |
| `list.any(f)` | `((T) -> Bool) -> Bool` | True if any element matches |
| `list.all(f)` | `((T) -> Bool) -> Bool` | True if all elements match |

## Collections — Map\<K, V\>

Maps support `Int` and `String` as key and value types, in any combination. The type is determined by the annotation on the `let` binding.

### `map_new() -> Map<K, V>`

Creates a new empty map.

```rust
let scores: Map<String, Int> = map_new()
```

### `map_insert(map: Map<K, V>, key: K, value: V)`

Inserts or updates a key-value pair.

```rust
map_insert(scores, "alice", 95)
```

### `map_get(map: Map<K, V>, key: K) -> V`

Returns the value associated with the key.

```rust
let score: Int = map_get(scores, "alice")
```

### `map_contains_key(map: Map<K, V>, key: K) -> Bool`

Returns `true` if the key exists.

```rust
let has: Bool = map_contains_key(scores, "alice")
```

### `map_length(map: Map<K, V>) -> Int`

Returns the number of entries.

```rust
let n: Int = map_length(scores)
```

### `map_remove(map: Map<K, V>, key: K) -> Bool`

Removes the entry with the given key. Returns `true` on success.

```rust
let ok: Bool = map_remove(scores, "alice")
```

### `map_is_empty(map: Map<K, V>) -> Bool`

Returns `true` if the map has no entries.

```rust
let empty: Bool = map_is_empty(scores)
```

### Map Iteration

Use `.keys()` and `.values()` with `for-in` to iterate:

```rust
for key in scores.keys() {
    println(key)
}

for value in scores.values() {
    print_int(value)
}
```

## JSON

### Parsing

### `json_parse(text: String) -> Int`

Parses a JSON string and returns an opaque handle.

```rust
let doc: Int = json_parse("{\"name\": \"alice\"}")
```

### `json_get_string(handle: Int, key: String) -> String`

Extracts a string field from a JSON object.

```rust
let name: String = json_get_string(doc, "name")  // "alice"
```

### `json_get_int(handle: Int, key: String) -> Int`

Extracts an integer field.

```rust
let age: Int = json_get_int(doc, "age")
```

### `json_get_bool(handle: Int, key: String) -> Int`

Extracts a boolean field. Returns `1` for true, `0` for false, `-1` if not found.

```rust
let active: Int = json_get_bool(doc, "active")
```

### `json_get_float(handle: Int, key: String) -> Float64`

Extracts a float field.

```rust
let score: Float64 = json_get_float(doc, "score")
```

### `json_get_array(handle: Int, key: String) -> List<Int>`

Extracts an array field as a list of JSON handles.

```rust
let items: List<Int> = json_get_array(doc, "items")
```

### `json_get_object(handle: Int, key: String) -> Int`

Extracts a nested JSON object as a handle.

```rust
let nested: Int = json_get_object(doc, "metadata")
```

### `json_free(handle: Int)`

Releases memory associated with a JSON handle.

```rust
json_free(doc)
```

### `json_stringify(handle: Int) -> String`

Serializes a JSON handle back to a JSON string.

```rust
let text: String = json_stringify(doc)
```

### Building

### `json_new_object() -> Int`

Creates a new empty JSON object and returns a handle.

```rust
let obj: Int = json_new_object()
```

### `json_set_string(handle: Int, key: String, value: String)`

Sets a string field on a JSON object.

```rust
json_set_string(obj, "name", "alice")
```

### `json_set_int(handle: Int, key: String, value: Int)`

Sets an integer field.

```rust
json_set_int(obj, "age", 30)
```

### `json_set_bool(handle: Int, key: String, value: Bool)`

Sets a boolean field.

```rust
json_set_bool(obj, "active", true)
```

### `json_set_float(handle: Int, key: String, value: Float64)`

Sets a float field.

```rust
json_set_float(obj, "score", 9.5)
```

## Time & Environment

### `time_now() -> Int`

Returns the current Unix timestamp in seconds.

```rust
let ts: Int = time_now()
```

### `time_now_ms() -> Int`

Returns the current Unix timestamp in milliseconds.

```rust
let ms: Int = time_now_ms()
```

### `time_format(timestamp: Int) -> String`

Formats a Unix timestamp as an ISO 8601 string.

```rust
let formatted: String = time_format(time_now())
```

### `time_elapsed_ms(start: Int) -> Int`

Returns the number of milliseconds elapsed since `start`.

```rust
let start: Int = time_now_ms()
// ... work ...
let elapsed: Int = time_elapsed_ms(start)
```

### `env_get(name: String) -> String`

Returns the value of an environment variable.

```rust
let home: String = env_get("HOME")
```

### `env_set(name: String, value: String)`

Sets an environment variable.

```rust
env_set("MY_VAR", "value")
```

## Channels

Channels provide typed communication between spawned tasks. Currently limited to `Int`, `Bool`, and `String` types.

### Int Channels

```rust
let ch: Channel<Int> = channel_new()
channel_send(ch, 42)
let val: Int = channel_recv(ch)
channel_free(ch)
```

### Bool Channels

```rust
let ch: Channel<Bool> = channel_new_bool()
channel_send_bool(ch, true)
let val: Bool = channel_recv_bool(ch)
```

### String Channels

```rust
let ch: Channel<String> = channel_new_string()
channel_send_string(ch, "hello")
let val: String = channel_recv_string(ch)
```

### `channel_free(ch: Int)`

Releases a channel's resources.

## HTTP Client

### `http_get(url: String) -> Result<String, String>`

Performs an HTTP GET request and returns the response body.

```rust
let resp: Result<String, String> = http_get("https://api.example.com/data")
```

### `http_post(url: String, body: String) -> Result<String, String>`

Performs an HTTP POST request with the given body.

```rust
let resp: Result<String, String> = http_post("https://api.example.com/data", "{\"key\": \"value\"}")
```

## HTTP Server

### `http_server_new(port: Int) -> Int`

Creates an HTTP server listening on the given port. Returns a server handle.

```rust
let server: Int = http_server_new(8080)
```

### `http_server_recv(server: Int) -> Int`

Waits for an incoming HTTP request. Returns a request handle.

```rust
let req: Int = http_server_recv(server)
```

### `http_request_method(request: Int) -> String`

Returns the HTTP method (e.g., `"GET"`, `"POST"`).

```rust
let method: String = http_request_method(req)
```

### `http_request_path(request: Int) -> String`

Returns the request URL path.

```rust
let path: String = http_request_path(req)
```

### `http_request_body(request: Int) -> String`

Returns the request body.

```rust
let body: String = http_request_body(req)
```

### `http_respond(request: Int, status: Int, body: String)`

Sends an HTTP response with the given status code and body.

```rust
http_respond(req, 200, "{\"status\": \"ok\"}")
```

### `http_server_free(server: Int)`

Closes the HTTP server and releases resources.

```rust
http_server_free(server)
```

## Database (SQLite)

### `db_open(path: String) -> Int`

Opens a SQLite database and returns a handle.

```rust
let db: Int = db_open("app.db")
```

### `db_execute(db: Int, sql: String) -> Int`

Executes a SQL statement. Returns `0` on success, `1` on error.

```rust
let ok: Int = db_execute(db, "CREATE TABLE users (id INTEGER, name TEXT)")
```

### `db_query(db: Int, sql: String) -> Int`

Executes a query and returns a result handle.

```rust
let result: Int = db_query(db, "SELECT * FROM users")
```

### `db_row_next(result: Int) -> Int`

Returns `1` if there is a next row, `0` if exhausted.

```rust
let has_row: Int = db_row_next(result)
```

### `db_row_get_string(result: Int, col: Int) -> String`

Returns a string column value from the current row.

```rust
let name: String = db_row_get_string(result, 1)
```

### `db_row_get_int(result: Int, col: Int) -> Int`

Returns an integer column value from the current row.

```rust
let id: Int = db_row_get_int(result, 0)
```

### `db_row_advance(result: Int) -> Int`

Advances to the next row. Returns `1` if successful, `0` if done.

```rust
let more: Int = db_row_advance(result)
```

### `db_result_free(result: Int)`

Releases a query result set.

```rust
db_result_free(result)
```

### `db_close(db: Int)`

Closes the database connection.

```rust
db_close(db)
```

## Option\<T\> Methods

### `opt.is_some() -> Bool`

Returns `true` if the option contains a value.

```rust
let x: Option<Int> = Option::Some(42)
let yes: Bool = x.is_some()  // true
```

### `opt.is_none() -> Bool`

Returns `true` if the option is `None`.

```rust
let x: Option<Int> = Option::None
let yes: Bool = x.is_none()  // true
```

### `opt.unwrap_or(default: T) -> T`

Returns the contained value, or `default` if `None`.

```rust
let x: Option<Int> = Option::None
let val: Int = x.unwrap_or(0)  // 0
```

## Result\<T, E\> Methods

### `res.is_ok() -> Bool`

Returns `true` if the result is `Ok`.

```rust
let r: Result<Int, String> = Result::Ok(42)
let yes: Bool = r.is_ok()  // true
```

### `res.is_err() -> Bool`

Returns `true` if the result is `Err`.

```rust
let r: Result<Int, String> = Result::Err("failed")
let yes: Bool = r.is_err()  // true
```

### `res.unwrap_or(default: T) -> T`

Returns the `Ok` value, or `default` if `Err`.

```rust
let r: Result<Int, String> = Result::Err("oops")
let val: Int = r.unwrap_or(0)  // 0
```

## Test Assertions

These functions are available inside `test` blocks.

| Function | Signature | Description |
|----------|-----------|-------------|
| `assert(condition)` | `(Bool)` | Fails if `false` |
| `assert_true(condition)` | `(Bool)` | Fails if `false` (alias for `assert`) |
| `assert_false(condition)` | `(Bool)` | Fails if `true` |
| `assert_eq(left, right)` | `(T, T)` | Fails if left != right |
| `assert_ne(left, right)` | `(T, T)` | Fails if left == right |

`assert_eq` and `assert_ne` support `Int`, `String`, `Bool`, and `Float64`.
