# Regex Builtins

Kōdo provides three built-in functions for regular expression matching and manipulation.
They are available in every program without any import.

All three functions use the POSIX-compatible regex syntax provided by the `regex` crate.
Invalid patterns never cause a runtime panic — they return a safe default value instead.

## `regex_match`

```
fn regex_match(pattern: String, text: String) -> Bool
```

Returns `true` if `pattern` matches anywhere inside `text`, `false` otherwise.
If the pattern is invalid, returns `false`.

```ko
module example {
    meta { version: "1.0.0" }

    fn main() -> Int {
        let has_digits: Bool = regex_match("\\d+", "abc123")
        if has_digits {
            println("found digits")
        }

        let anchored: Bool = regex_match("^hello", "hello world")
        if anchored {
            println("starts with hello")
        }

        return 0
    }
}
```

## `regex_find`

```
fn regex_find(pattern: String, text: String) -> Option<String>
```

Returns `Some(first_match)` with the first substring that matches `pattern`, or `None`
when there is no match or the pattern is invalid.

```ko
module example {
    meta { version: "1.0.0" }

    fn main() -> Int {
        let first: Option<String> = regex_find("\\w+", "hello world")
        if first.is_some() {
            let word: String = first.unwrap()
            println(word)
            // prints: hello
        }

        let none: Option<String> = regex_find("\\d+", "no digits here")
        if none.is_none() {
            println("no match")
        }

        return 0
    }
}
```

## `regex_replace`

```
fn regex_replace(pattern: String, text: String, replacement: String) -> String
```

Returns a new string where **all** non-overlapping matches of `pattern` in `text` are
replaced by `replacement`. If the pattern is invalid the original `text` is returned
unchanged.

Note: the argument order is `(pattern, text, replacement)`.

```ko
module example {
    meta { version: "1.0.0" }

    fn main() -> Int {
        let result: String = regex_replace("o", "hello world", "0")
        println(result)
        // prints: hell0 w0rld

        let clean: String = regex_replace("\\s+", "hello   world", "_")
        println(clean)
        // prints: hello_world

        return 0
    }
}
```

## Common Use Cases

### Validate an email address (simple heuristic)

```ko
let is_email: Bool = regex_match("[^@]+@[^@]+\\.[^@]+", user_input)
```

### Extract the first number from a string

```ko
let num: Option<String> = regex_find("\\d+", "The answer is 42")
// num == Some("42")
```

### Sanitise a slug

```ko
let slug: String = regex_replace("[^a-z0-9]+", to_lower(title), "-")
```

## Error Handling

All three builtins treat an invalid regex pattern as a no-op:

| Builtin | Invalid pattern returns |
|---------|------------------------|
| `regex_match` | `false` |
| `regex_find` | `None` |
| `regex_replace` | original `text` unchanged |

This design means agent-generated code can never crash the program with a bad
pattern — it will simply receive an empty/false result and can inspect the
output to detect the problem.

## Full Example

See [`examples/regex_demo.ko`](../../examples/regex_demo.ko) for a complete
runnable demonstration of all three builtins.
