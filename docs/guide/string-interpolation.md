# String Interpolation

Kōdo supports f-strings for embedding expressions inside string literals.

## Basic Usage

Prefix a string literal with `f` and use `{expression}` to embed values:

```
let name: String = "World"
let msg: String = f"Hello, {name}!"
// msg == "Hello, World!"
```

## Expressions in Interpolation

Any expression can be embedded, including function calls, arithmetic, and field access:

```
let x: Int = 3
let y: Int = 4
let desc: String = f"Point({x}, {y})"

let sum: String = f"Total: {x + y}"
```

## Type Conversion

Non-string types are automatically converted using built-in `to_string` functions:

- `Int` → `kodo_int_to_string`
- `Float64` → `kodo_float_to_string`
- `Bool` → `kodo_bool_to_string`

## How It Works

F-strings are desugared during compilation into string concatenation:

```
f"Hello, {name}!"
// becomes:
"Hello, " + name + "!"
```

For non-string expressions, a `to_string` call is inserted automatically.

## Restrictions

- F-strings cannot be used inside contract expressions (`requires`/`ensures`).
- Nested f-strings are not supported in v1.
