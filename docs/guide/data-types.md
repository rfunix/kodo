# Data Types and Pattern Matching

Kōdo lets you define custom data types using `struct` and `enum`. Pattern matching with `match` provides a safe way to destructure values and handle every case.

## Structs

A struct groups related values under named fields:

```rust
struct Point {
    x: Int,
    y: Int
}
```

### Creating Structs

Create a struct value by providing all fields:

```rust
let origin: Point = Point { x: 0, y: 0 }
let p: Point = Point { x: 3, y: 4 }
```

### Accessing Fields

Use dot notation to access individual fields:

```rust
fn get_x(p: Point) -> Int {
    return p.x
}

fn main() {
    let p: Point = Point { x: 10, y: 20 }
    print_int(p.x)
    print_int(p.y)
}
```

### Structs as Function Parameters

Structs can be passed to and returned from functions:

```rust
struct Point {
    x: Int,
    y: Int
}

fn translate(p: Point, dx: Int, dy: Int) -> Point {
    return Point { x: p.x + dx, y: p.y + dy }
}

fn main() {
    let p: Point = Point { x: 1, y: 2 }
    let q: Point = translate(p, 10, 20)
    print_int(q.x)
    print_int(q.y)
}
```

Output:
```rust
11
22
```

Internally, structs are passed by pointer — there is no copy overhead for large structs.

## Enums

An enum defines a type that can be one of several named variants. Each variant can optionally carry data:

```rust
enum Color {
    Red,
    Green,
    Blue
}

enum Shape {
    Circle(Int),
    Rectangle(Int, Int)
}
```

`Color` has three variants with no data. `Shape` has two variants: `Circle` carries a radius (`Int`) and `Rectangle` carries width and height (`Int, Int`).

### Creating Enum Values

Use the `EnumName::Variant(args)` syntax:

```rust
let s: Shape = Shape::Circle(5)
let r: Shape = Shape::Rectangle(10, 20)
```

### Pattern Matching with `match`

`match` is the primary way to work with enums. It destructures the value and binds contained data to variables:

```rust
fn area(s: Shape) -> Int {
    match s {
        Shape::Circle(r) => {
            return r * r * 3
        }
        Shape::Rectangle(w, h) => {
            return w * h
        }
    }
}
```

Each arm of a `match` specifies a pattern (`Shape::Circle(r)`) and a body to execute when the value matches that pattern. The contained values are bound to the names you choose (`r`, `w`, `h`).

### Match is Exhaustive

Every `match` must cover all variants. If you forget one, the compiler will tell you. This prevents a common category of bugs where a new variant is added but not handled everywhere.

### Enums as Function Parameters

Like structs, enums can be passed to and returned from functions:

```rust
enum Direction {
    Up,
    Down,
    Left,
    Right
}

fn move_point(p: Point, dir: Direction, amount: Int) -> Point {
    match dir {
        Direction::Up => {
            return Point { x: p.x, y: p.y + amount }
        }
        Direction::Down => {
            return Point { x: p.x, y: p.y - amount }
        }
        Direction::Left => {
            return Point { x: p.x - amount, y: p.y }
        }
        Direction::Right => {
            return Point { x: p.x + amount, y: p.y }
        }
    }
}
```

## Complete Example

Here's a program that combines structs, enums, and pattern matching:

```rust
module geometry {
    meta {
        purpose: "Calculate areas of shapes"
        version: "0.1.0"
    }

    struct Point {
        x: Int,
        y: Int
    }

    enum Shape {
        Circle(Int),
        Square(Int),
        Rectangle(Int, Int)
    }

    fn area(s: Shape) -> Int {
        match s {
            Shape::Circle(r) => {
                return r * r * 3
            }
            Shape::Square(side) => {
                return side * side
            }
            Shape::Rectangle(w, h) => {
                return w * h
            }
        }
    }

    fn main() {
        let c: Shape = Shape::Circle(5)
        let sq: Shape = Shape::Square(4)
        let r: Shape = Shape::Rectangle(3, 7)

        print_int(area(c))
        print_int(area(sq))
        print_int(area(r))
    }
}
```

Output:
```rust
75
16
21
```

## Float64

Kōdo supports 64-bit floating-point numbers with full arithmetic:

```rust
let pi: Float64 = 3.14159
let radius: Float64 = 5.0
let area: Float64 = pi * radius * radius

println_float(area)
```

All arithmetic (`+`, `-`, `*`, `/`, `%`), comparison (`==`, `!=`, `<`, `>`, `<=`, `>=`), and negation (`-x`) operators work with `Float64` values.

## String Operators

Strings support concatenation with the `+` operator:

```rust
let greeting: String = "Hello, " + "world!"
println(greeting)

let name: String = "Kōdo"
let msg: String = "Welcome to " + name
println(msg)
```

Strings also support equality comparison with `==` and `!=`, which compares by content (not by pointer):

```rust
let a: String = "hello"
let b: String = "hello"
if a == b {
    println("strings are equal")
}
```

See [Modules and Imports](modules-and-imports.md) for the full list of string methods (`length`, `contains`, `split`, `trim`, `concat`, `index_of`, `replace`, etc.).

## Next Steps

- [Closures](closures.md) — closures, lambda lifting, and higher-order functions
- [Generics](generics.md) — parameterize your types with type variables
- [Error Handling](error-handling.md) — use `Option<T>` and `Result<T, E>` for safe error handling
- [Contracts](contracts.md) — add runtime verification to your functions
