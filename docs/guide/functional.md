# Functional Combinators

Kōdo provides functional combinators on iterators that let you transform, filter, and aggregate data declaratively. These combinators compose into pipelines that are expressive and easy for both AI agents and humans to reason about.

## `map` — Transform Elements

`map` applies a closure to each element, producing a new list:

```rust
let numbers: List<Int> = list_new()
list_push(numbers, 1)
list_push(numbers, 2)
list_push(numbers, 3)

let doubled: List<Int> = numbers.iter().map(fn(x: Int) -> Int { return x * 2 })
// doubled contains [2, 4, 6]
```

## `filter` — Select Elements

`filter` keeps only elements where the predicate returns `true`:

```rust
let numbers: List<Int> = list_new()
list_push(numbers, 1)
list_push(numbers, 2)
list_push(numbers, 3)
list_push(numbers, 4)

let evens: List<Int> = numbers.iter().filter(fn(x: Int) -> Bool { return x % 2 == 0 })
// evens contains [2, 4]
```

## `fold` — Aggregate to a Single Value

`fold` reduces a collection to a single value by applying a combining function:

```rust
let numbers: List<Int> = list_new()
list_push(numbers, 1)
list_push(numbers, 2)
list_push(numbers, 3)

let sum: Int = numbers.iter().fold(0, fn(acc: Int, x: Int) -> Int { return acc + x })
// sum is 6
```

The first argument is the initial accumulator value. The closure receives the current accumulator and the next element, and returns the new accumulator.

## `count` — Count Elements

`count` returns the number of elements in the iterator:

```rust
let items: List<Int> = list_new()
list_push(items, 10)
list_push(items, 20)

let n: Int = items.iter().count()
// n is 2
```

## `any` — Check If Any Match

`any` returns `true` if at least one element satisfies the predicate:

```rust
let numbers: List<Int> = list_new()
list_push(numbers, 1)
list_push(numbers, 2)
list_push(numbers, 3)

let has_even: Bool = numbers.iter().any(fn(x: Int) -> Bool { return x % 2 == 0 })
// has_even is true
```

## `all` — Check If All Match

`all` returns `true` if every element satisfies the predicate:

```rust
let numbers: List<Int> = list_new()
list_push(numbers, 2)
list_push(numbers, 4)
list_push(numbers, 6)

let all_even: Bool = numbers.iter().all(fn(x: Int) -> Bool { return x % 2 == 0 })
// all_even is true
```

## `reduce` — Fold Without Initial Value

`reduce` is like `fold` but uses the first element as the initial accumulator:

```rust
let numbers: List<Int> = list_new()
list_push(numbers, 1)
list_push(numbers, 2)
list_push(numbers, 3)

let sum: Int = numbers.iter().reduce(fn(acc: Int, x: Int) -> Int { return acc + x })
// sum is 6
```

## Composing Pipelines

Combinators chain naturally to form data processing pipelines:

```rust
let data: List<Int> = list_new()
list_push(data, 1)
list_push(data, 2)
list_push(data, 3)
list_push(data, 4)
list_push(data, 5)

// Filter even numbers, double them, then sum
let result: Int = data.iter()
    .filter(fn(x: Int) -> Bool { return x % 2 == 0 })
    .map(fn(x: Int) -> Int { return x * 2 })
    .fold(0, fn(acc: Int, x: Int) -> Int { return acc + x })
// result is 12 (2*2 + 4*2)
```

This pipeline is:
1. **Readable**: each step describes one transformation
2. **Composable**: add or remove steps without restructuring
3. **Verifiable**: contracts can be attached at each stage

## Examples

- [`functional_pipeline.ko`](../../examples/functional_pipeline.ko) — chained combinator pipelines
- [`iterator_map_filter.ko`](../../examples/iterator_map_filter.ko) — `map` and `filter` usage
- [`iterator_fold.ko`](../../examples/iterator_fold.ko) — `fold` for aggregation
- [`closures_functional.ko`](../../examples/closures_functional.ko) — higher-order functions
- [`word_counter.ko`](../../examples/word_counter.ko) — real-world pipeline with string ops and fold

## Next Steps

- [Iterators](iterators.md) — the `for-in` loop and iterator protocol
- [Closures](closures.md) — closures and variable capture
- [Contracts](contracts.md) — add `requires`/`ensures` to pipeline functions
