# Inherent Impl Blocks

Kōdo supports inherent impl blocks, which allow you to define methods directly on struct types without requiring a trait.

## Defining Methods

Use `impl TypeName { ... }` to add methods to a struct:

```
struct Point {
    x: Int,
    y: Int,
}

impl Point {
    fn translate(self, dx: Int, dy: Int) -> Point {
        return Point { x: self.x + dx, y: self.y + dy }
    }

    fn manhattan_distance(self) -> Int {
        let ax: Int = self.x
        if ax < 0 { ax = 0 - ax }
        let ay: Int = self.y
        if ay < 0 { ay = 0 - ay }
        return ax + ay
    }
}
```

## Calling Methods

Methods are called using dot notation:

```
let p: Point = Point { x: 3, y: 4 }
let moved: Point = p.translate(1, 2)
let dist: Int = moved.manhattan_distance()
```

## Static Methods

Methods that don't take `self` as the first parameter act as static/associated functions:

```
impl Counter {
    fn new() -> Counter {
        return Counter { value: 0 }
    }
}

let c: Counter = Counter.new()
```

## Inherent vs Trait Impl

- `impl Point { ... }` — inherent impl (no trait required)
- `impl Printable for Point { ... }` — trait impl (implements a trait)

Both can coexist for the same type.
