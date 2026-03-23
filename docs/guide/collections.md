---
title: Collections
description: Working with List, Map, Set, and JSON in Kōdo
---

# Collections

Kōdo provides three built-in generic collection types — `List<T>`, `Map<K, V>`, and `Set<T>` — plus a JSON API for working with structured data. Collections are mutable, heap-allocated, and passed by reference.

## List\<T\>

A `List<T>` is an ordered, growable sequence of elements.

### Creating a List

```rust
let nums: List<Int> = list_new()
```

### Adding Elements

```rust
list_push(nums, 10)
list_push(nums, 20)
list_push(nums, 30)
```

Or using method syntax:

```rust
nums.push(40)
```

### Accessing Elements

```rust
let first: Int = list_get(nums, 0)
let second: Int = nums.get(1)
```

### Core Methods

| Method | Signature | Description |
|--------|-----------|-------------|
| `push(elem)` | `(T) -> ()` | Append an element to the end |
| `get(index)` | `(Int) -> T` | Get element at index |
| `set(index, elem)` | `(Int, T) -> Bool` | Set element at index, returns `true` if valid |
| `length()` | `() -> Int` | Number of elements |
| `is_empty()` | `() -> Bool` | Whether the list has no elements |
| `contains(elem)` | `(T) -> Bool` | Whether the list contains the element |
| `pop()` | `() -> T` | Remove and return the last element |
| `remove(index)` | `(Int) -> Bool` | Remove element at index, returns `true` if valid |
| `reverse()` | `() -> ()` | Reverse the list in place |
| `slice(start, end)` | `(Int, Int) -> List<T>` | Return a new list from `start` to `end` (exclusive) |
| `sort()` | `() -> ()` | Sort the list in place (ascending) |
| `sort_by(cmp)` | `((T, T) -> Int) -> ()` | Sort using a custom comparator |
| `join(sep)` | `(String) -> String` | Join elements with separator (for `List<String>`) |

### Example: List Operations

```rust
module list_ops {
    meta {
        purpose: "List operations demo"
        version: "1.0.0"
    }

    fn main() -> Int {
        let items: List<Int> = list_new()

        // Check empty
        if list_is_empty(items) {
            println("empty")
        }

        // Add elements
        list_push(items, 10)
        list_push(items, 20)
        list_push(items, 30)

        // Length and access
        let len: Int = list_length(items)
        print_int(len)

        let first: Int = list_get(items, 0)
        print_int(first)

        // Update element
        list_set(items, 1, 99)

        // Reverse
        list_reverse(items)

        // Check containment
        if list_contains(items, 99) {
            println("found 99")
        }

        return 0
    }
}
```

### Higher-Order Methods

`List<T>` supports functional-style operations using closures:

| Method | Signature | Description |
|--------|-----------|-------------|
| `map(f)` | `((T) -> T) -> List<T>` | Transform each element |
| `filter(f)` | `((T) -> Bool) -> List<T>` | Keep elements matching predicate |
| `fold(init, f)` | `(T, (T, T) -> T) -> T` | Reduce to single value with initial accumulator |
| `reduce(init, f)` | `(T, (T, T) -> T) -> T` | Alias for `fold` |
| `count(f)` | `((T) -> Bool) -> Int` | Count elements matching predicate |
| `any(f)` | `((T) -> Bool) -> Bool` | Whether any element matches |
| `all(f)` | `((T) -> Bool) -> Bool` | Whether all elements match |

```rust
let nums: List<Int> = list_new()
list_push(nums, 1)
list_push(nums, 2)
list_push(nums, 3)
list_push(nums, 4)
list_push(nums, 5)

// Double each element -> [2, 4, 6, 8, 10]
let doubled: List<Int> = nums.map(|x: Int| -> Int { x * 2 })

// Keep even numbers -> [2, 4]
let evens: List<Int> = nums.filter(|x: Int| -> Bool { x % 2 == 0 })

// Sum all -> 15
let total: Int = nums.fold(0, |acc: Int, x: Int| -> Int { acc + x })

// Count elements greater than 3 -> 2
let big: Int = nums.count(|x: Int| -> Bool { x > 3 })

// Any element > 4? -> true
let has_big: Bool = nums.any(|x: Int| -> Bool { x > 4 })

// All positive? -> true
let all_pos: Bool = nums.all(|x: Int| -> Bool { x > 0 })
```

### Sorting with a Custom Comparator

```rust
let to_sort: List<Int> = list_new()
list_push(to_sort, 3)
list_push(to_sort, 1)
list_push(to_sort, 5)

// Sort descending
to_sort.sort_by(|a: Int, b: Int| -> Int { b - a })
// to_sort is now [5, 3, 1]
```

### Iterating Over a List

Use `for-in` to traverse all elements:

```rust
let items: List<Int> = list_new()
list_push(items, 10)
list_push(items, 20)
list_push(items, 30)

for item in items {
    print_int(item)
}
```

## Map\<K, V\>

A `Map<K, V>` is an unordered collection of key-value pairs. Keys and values can be `Int`, `String`, or `Bool`.

### Creating a Map

```rust
let scores: Map<Int, Int> = map_new()
let names: Map<String, String> = map_new()
let config: Map<String, Int> = map_new()
```

### Inserting and Accessing

```rust
map_insert(scores, 1, 100)
map_insert(scores, 2, 200)

let val: Int = map_get(scores, 2)
print_int(val)
```

### Core Methods

| Method / Function | Signature | Description |
|-------------------|-----------|-------------|
| `map_new()` | `() -> Map<K, V>` | Create an empty map |
| `map_insert(m, k, v)` | `(Map, K, V) -> ()` | Insert or update a key-value pair |
| `map_get(m, k)` | `(Map, K) -> V` | Get value by key |
| `map_contains_key(m, k)` | `(Map, K) -> Bool` | Whether the key exists |
| `map_length(m)` | `(Map) -> Int` | Number of entries |
| `map_remove(m, k)` | `(Map, K) -> Bool` | Remove entry by key |
| `map_is_empty(m)` | `(Map) -> Bool` | Whether the map is empty |
| `m.merge(other)` | `(Map) -> Map` | Merge two maps, `other` values overwrite |
| `m.filter(f)` | `((K, V) -> Bool) -> Map` | Keep entries matching predicate |

### Example: Map Operations

```rust
module map_ops {
    meta {
        purpose: "Map operations demo"
        version: "1.0.0"
    }

    fn main() -> Int {
        let m: Map<Int, Int> = map_new()

        if map_is_empty(m) {
            println("empty")
        }

        map_insert(m, 1, 100)
        map_insert(m, 2, 200)
        map_insert(m, 3, 300)

        let len: Int = map_length(m)
        print_int(len)

        let val: Int = map_get(m, 2)
        print_int(val)

        if map_contains_key(m, 1) {
            println("has key 1")
        }

        // Remove a key
        map_remove(m, 2)

        if !map_contains_key(m, 2) {
            println("key 2 removed")
        }

        return 0
    }
}
```

### Merge and Filter

```rust
let a: Map<Int, Int> = map_new()
map_insert(a, 1, 10)
map_insert(a, 2, 20)
map_insert(a, 3, 30)

let b: Map<Int, Int> = map_new()
map_insert(b, 2, 99)
map_insert(b, 4, 40)

// Merge: b's values overwrite a's for shared keys
let merged: Map<Int, Int> = a.merge(b)
// merged has: {1: 10, 2: 99, 3: 30, 4: 40}

// Filter: keep only entries with value > 25
let big: Map<Int, Int> = merged.filter(|k: Int, v: Int| -> Bool { v > 25 })
```

### Iterating Over a Map

Maps support `.keys()` and `.values()` iterators:

```rust
let m: Map<Int, Int> = map_new()
map_insert(m, 10, 100)
map_insert(m, 20, 200)
map_insert(m, 30, 300)

// Iterate over keys
let iter: Int = m.keys()
while map_keys_advance(iter) == 1 {
    let key: Int = map_keys_value(iter)
    print_int(key)
}
map_keys_free(iter)

// Iterate over values
let viter: Int = m.values()
while map_values_advance(viter) == 1 {
    let val: Int = map_values_value(viter)
    print_int(val)
}
map_values_free(viter)
```

Maps also support `for-in` iteration over keys:

```rust
for key in scores.keys() {
    print_int(key)
}

for value in scores.values() {
    print_int(value)
}
```

## Set\<T\>

A `Set<T>` is an unordered collection of unique elements. Duplicate additions are silently ignored.

### Creating a Set

```rust
let s: Set<Int> = set_new()
```

### Core Methods

| Method | Signature | Description |
|--------|-----------|-------------|
| `add(elem)` | `(T) -> ()` | Add an element (no-op if already present) |
| `contains(elem)` | `(T) -> Bool` | Whether the element exists |
| `remove(elem)` | `(T) -> Bool` | Remove an element |
| `length()` | `() -> Int` | Number of elements |
| `is_empty()` | `() -> Bool` | Whether the set is empty |
| `union(other)` | `(Set<T>) -> Set<T>` | Elements in either set |
| `intersection(other)` | `(Set<T>) -> Set<T>` | Elements in both sets |
| `difference(other)` | `(Set<T>) -> Set<T>` | Elements in `self` but not in `other` |

### Example: Set Operations

```rust
module set_ops {
    meta {
        purpose: "Set operations demo"
        version: "1.0.0"
    }

    fn main() -> Int {
        let s1: Set<Int> = set_new()
        s1.add(1)
        s1.add(2)
        s1.add(3)
        s1.add(2)  // duplicate, ignored

        print_int(s1.length())  // 3

        let s2: Set<Int> = set_new()
        s2.add(2)
        s2.add(3)
        s2.add(4)

        // Union: {1, 2, 3, 4}
        let u: Set<Int> = s1.union(s2)
        print_int(u.length())  // 4

        // Intersection: {2, 3}
        let inter: Set<Int> = s1.intersection(s2)
        print_int(inter.length())  // 2

        // Difference: {1}
        let diff: Set<Int> = s1.difference(s2)
        print_int(diff.length())  // 1

        // Remove
        s1.remove(2)
        print_int(s1.length())  // 2

        if s1.is_empty() {
            println("empty")
        } else {
            println("not empty")
        }

        return 0
    }
}
```

### Iterating Over a Set

Sets support `for-in` iteration:

```rust
let s: Set<Int> = set_new()
s.add(10)
s.add(20)
s.add(30)

let total: Int = 0
for x in s {
    total = total + x
}
// total is 60
```

## JSON

Kōdo provides built-in functions for parsing, querying, and building JSON data. JSON values are represented as opaque `Int` handles.

### Parsing JSON

```rust
let data: Int = json_parse("{\"name\": \"kodo\", \"version\": 1}")
```

### Querying Values

| Function | Signature | Description |
|----------|-----------|-------------|
| `json_parse(s)` | `(String) -> Int` | Parse a JSON string, returns a handle |
| `json_get_string(h, key)` | `(Int, String) -> String` | Get a string field |
| `json_get_int(h, key)` | `(Int, String) -> Int` | Get an integer field |
| `json_get_bool(h, key)` | `(Int, String) -> Int` | Get a boolean field (-1 if not found) |
| `json_get_float(h, key)` | `(Int, String) -> Float64` | Get a float field |
| `json_get_array(h, key)` | `(Int, String) -> List<Int>` | Get an array as list of JSON handles |
| `json_get_object(h, key)` | `(Int, String) -> Int` | Get a nested object handle |
| `json_stringify(h)` | `(Int) -> String` | Convert handle back to JSON string |
| `json_free(h)` | `(Int) -> ()` | Free the JSON handle |

```rust
let data: Int = json_parse("{\"name\": \"kodo\", \"version\": 1}")
let name: String = json_get_string(data, "name")
let ver: Int = json_get_int(data, "version")
println(name)
print_int(ver)
json_free(data)
```

### Building JSON

Use the builder API to construct JSON objects:

| Function | Signature | Description |
|----------|-----------|-------------|
| `json_new_object()` | `() -> Int` | Create a new empty JSON object |
| `json_set_string(h, key, val)` | `(Int, String, String) -> ()` | Set a string field |
| `json_set_int(h, key, val)` | `(Int, String, Int) -> ()` | Set an integer field |
| `json_set_bool(h, key, val)` | `(Int, String, Bool) -> ()` | Set a boolean field |
| `json_set_float(h, key, val)` | `(Int, String, Float64) -> ()` | Set a float field |

```rust
let obj: Int = json_new_object()
json_set_string(obj, "name", "kodo")
json_set_int(obj, "version", 1)
json_set_bool(obj, "stable", false)

let output: String = json_stringify(obj)
println(output)
```

## Common Patterns

### Collecting Filtered Results

```rust
let nums: List<Int> = list_new()
list_push(nums, 1)
list_push(nums, 2)
list_push(nums, 3)
list_push(nums, 4)

let evens: List<Int> = nums.filter(|x: Int| -> Bool { x % 2 == 0 })
let sum: Int = evens.fold(0, |acc: Int, x: Int| -> Int { acc + x })
print_int(sum)  // 6
```

### Using a Map as a Counter

```rust
let counts: Map<String, Int> = map_new()
// Increment by inserting updated values
map_insert(counts, "apple", 1)
let current: Int = map_get(counts, "apple")
map_insert(counts, "apple", current + 1)
```

### Set Membership Checks

```rust
let allowed: Set<Int> = set_new()
allowed.add(200)
allowed.add(201)
allowed.add(204)

let code: Int = 200
if allowed.contains(code) {
    println("status OK")
}
```

## Next Steps

- [Iterators](iterators.md) -- for-in loops, iterator protocol, and collection traversal
- [Functional Combinators](functional.md) -- map, filter, fold, and pipelines
- [HTTP, JSON & Networking](http.md) -- HTTP client and server with JSON
- [Closures](closures.md) -- closures and higher-order functions
