# Actors

Actors in Kodo combine state and message handlers into a single unit. An actor holds mutable fields on the heap and exposes handler functions that can be invoked as messages through the cooperative scheduler.

## The Actor Model

An actor is declared with the `actor` keyword. It contains typed fields (the state) and handler functions that operate on that state:

```rust
actor Counter {
    count: Int

    fn increment(self) -> Int {
        return self.count + 1
    }
}
```

The `self` parameter gives the handler access to the actor's fields. Fields are read with `self.field` syntax.

## Creating Actors

Create an actor by providing initial values for all fields, similar to struct literals:

```rust
let c: Counter = Counter { count: 0 }
```

Under the hood, the runtime allocates the actor's state on the heap via `kodo_actor_new`. The state buffer is zero-initialized and then populated with the field values you provide. Each field occupies 8 bytes, accessed by offset.

## Reading Fields

You can read an actor's fields using dot notation:

```rust
let c: Counter = Counter { count: 42 }
let v: Int = c.count
print_int(v)  // 42
```

Field access compiles to a `kodo_actor_get_field` call with the appropriate byte offset.

## Sending Messages

Calling a handler on an actor queues a message that the cooperative scheduler processes:

```rust
let c: Counter = Counter { count: 10 }
c.increment(5)
```

At the runtime level, `c.increment(5)` translates to a `kodo_actor_send` call. The handler function pointer and argument are packed into an environment buffer and enqueued as a task. The scheduler runs these tasks after `main` returns, just like `spawn` blocks.

This means handler calls are asynchronous -- the call returns immediately and the handler executes later.

## Actor Lifecycle

1. **Creation**: `Counter { count: 0 }` allocates heap state via `kodo_actor_new`.
2. **Field access**: `c.count` reads from the heap buffer via `kodo_actor_get_field`.
3. **Field mutation**: Setting a field uses `kodo_actor_set_field` at the given offset.
4. **Message sending**: `c.handler(arg)` enqueues a task via `kodo_actor_send`.
5. **Execution**: The scheduler drains the task queue, running each handler in order.
6. **Cleanup**: `kodo_actor_free` releases the heap buffer when the actor is no longer needed.

## Complete Example

```rust
module actors {
    meta {
        purpose: "Demonstrate actors with state and message passing",
        version: "0.1.0",
        author: "Kodo Team"
    }

    actor Counter {
        count: Int

        fn increment(self) -> Int {
            return self.count + 1
        }
    }

    fn main() -> Int {
        // Create a Counter with initial state (heap-allocated).
        let c: Counter = Counter { count: 42 }

        // Read a field directly.
        let v: Int = c.count
        print_int(v)

        return 0
    }
}
```

Compile and run:

```bash
cargo run -p kodoc -- build actors.ko -o actors
./actors
```

Output: `42`

## Cooperative Scheduling

In v1, actors use the same cooperative scheduler as `spawn` blocks. Messages are processed sequentially after `main` returns. This means:

- There is no true concurrent mailbox -- messages execute one at a time.
- Message ordering is deterministic (FIFO within the task queue).
- An actor's state is never accessed by two handlers simultaneously.

Future versions of Kodo will introduce asynchronous mailboxes and parallel execution, but the actor declaration syntax and semantics will remain the same.

## Actors vs Structs

Actors and structs both hold fields, but they differ in important ways:

| Feature | Struct | Actor |
|---------|--------|-------|
| Storage | Stack or struct-return | Heap-allocated |
| Field access | Direct memory | `kodo_actor_get_field` / `kodo_actor_set_field` |
| Methods | Synchronous calls | Asynchronous messages via scheduler |
| Mutability | Follows ownership rules | Mutable through handlers |

Use structs for plain data. Use actors when you need mutable state combined with asynchronous message handling.

## Next Steps

- [Concurrency](concurrency.md) -- spawning tasks and the cooperative scheduler
- [HTTP & JSON](http.md) -- making HTTP requests and parsing JSON
- [Data Types and Pattern Matching](data-types.md) -- structs, enums, and `match`
