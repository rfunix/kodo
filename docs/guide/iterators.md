# Iterators

Kōdo provides a built-in iterator protocol that lets you traverse collections using `for-in` loops. Lists, Maps, and Strings all support iteration.

## The `for-in` Loop

The simplest way to iterate is with `for-in`:

```rust
let items: List<Int> = list_new()
list_push(items, 10)
list_push(items, 20)
list_push(items, 30)

for item in items {
    print_int(item)
}
```

Output:
```
10
20
30
```

## Iterating Over Lists

`for-in` on a `List<T>` visits each element in order:

```rust
let names: List<String> = list_new()
list_push(names, "alice")
list_push(names, "bob")
list_push(names, "carol")

for name in names {
    print(name)
}
```

## Iterating Over Maps

Iterating over a `Map<K, V>` visits keys:

```rust
let scores: Map<String, Int> = map_new()
map_insert(scores, "alice", 95)
map_insert(scores, "bob", 87)

for key in scores.keys() {
    print(key)
}

for value in scores.values() {
    print_int(value)
}
```

## Iterating Over Strings

`for-in` on a `String` visits each character (as a single-character string):

```rust
let word: String = "hello"
for ch in word {
    print(ch)
}
```

Output:
```
h
e
l
l
o
```

## Iterator Protocol

Under the hood, `for-in` desugars to the iterator protocol:

1. Call `.iter()` on the collection to get an iterator handle
2. Call `advance()` on the iterator — returns `1` if an element is available, `0` if exhausted
3. Call `value()` on the iterator to get the current element
4. Repeat until `advance()` returns `0`
5. Call `free()` on the iterator to clean up

You don't normally need to use the protocol directly — `for-in` handles it automatically.

## Range-Based Iteration

You can iterate over a range of integers using a `while` loop or by building a list:

```rust
let i: Int = 0
while i < 10 {
    print_int(i)
    i = i + 1
}
```

## Examples

- [`for_in.ko`](../../examples/for_in.ko) — for-in loops over collections
- [`iterator_basic.ko`](../../examples/iterator_basic.ko) — basic iterator protocol
- [`iterator_list.ko`](../../examples/iterator_list.ko) — iterating over `List<T>`
- [`iterator_map_filter.ko`](../../examples/iterator_map_filter.ko) — `map` and `filter` on iterators
- [`iterator_fold.ko`](../../examples/iterator_fold.ko) — `fold` for aggregation
- [`iterator_map.ko`](../../examples/iterator_map.ko) — iterating over `Map` keys
- [`iterator_string.ko`](../../examples/iterator_string.ko) — iterating over `String` characters

## Next Steps

- [Functional Combinators](functional.md) — `map`, `filter`, `fold`, and pipeline composition
- [Closures](closures.md) — closures and higher-order functions
- [Data Types](data-types.md) — structs, enums, and pattern matching
