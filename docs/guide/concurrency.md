# Concurrency

Kodo uses a cooperative scheduler to run spawned tasks. You express concurrency with `spawn` blocks, and the runtime executes them after `main` returns.

## The Cooperative Scheduler

Kodo's concurrency model is based on a task queue. When you use `spawn`, the block is not executed immediately. Instead, it is enqueued as a task. After `main` finishes, the runtime calls `kodo_run_scheduler`, which drains the queue and runs each task in order.

Tasks may spawn additional tasks. The scheduler loops until the queue is empty, so nested spawns are fully supported.

## Basic Spawn

A `spawn` block runs a piece of code as a deferred task:

```rust
fn main() -> Int {
    spawn {
        println("hello from a task")
    }

    println("main finishing")
    return 0
}
```

Output:

```rust
main finishing
hello from a task
```

The spawned task runs after `main` returns, which is why "main finishing" appears first.

## Spawn with Captures

Spawned blocks can reference variables from the enclosing scope. The compiler performs capture analysis and packs the captured values into an environment buffer that is passed to the task function at execution time.

```rust
fn main() -> Int {
    let greeting: Int = 42
    spawn {
        print_int(greeting)
    }

    let a: Int = 10
    let b: Int = 32
    spawn {
        print_int(a + b)
    }

    println("all tasks queued")
    return 0
}
```

Output:

```rust
all tasks queued
42
42
```

### How Captures Work

Internally, the compiler transforms a `spawn` block into a top-level function via lambda lifting. Captured variables are serialized into an environment buffer (env packing):

1. The compiler identifies free variables in the spawn body.
2. Each captured value is written to a contiguous byte buffer at a known offset.
3. The task function receives a pointer to this buffer and reads the values back.

This means captures are by value -- the task receives a copy of each variable at the time of the `spawn`, not a reference to the original.

## Multiple Tasks

You can spawn as many tasks as you need. They execute in the order they were enqueued:

```rust
fn main() -> Int {
    spawn {
        println("[Task 1] no captures")
    }

    let x: Int = 10
    spawn {
        print_int(x)
    }

    let a: Int = 10
    let b: Int = 32
    spawn {
        print_int(a + b)
    }

    println("[main] all tasks queued, finishing main")
    return 0
}
```

## Complete Example

```rust
module async_tasks {
    meta {
        purpose: "Demonstrate spawn with captured variables",
        version: "1.0.0",
        author: "Kodo Team"
    }

    fn main() -> Int {
        spawn {
            println("[Task 1] no captures")
        }

        let greeting: Int = 42
        spawn {
            print_int(greeting)
        }

        let a: Int = 10
        let b: Int = 32
        spawn {
            print_int(a + b)
        }

        println("[main] all tasks queued, finishing main")
        return 0
    }
}
```

Compile and run:

```bash
cargo run -p kodoc -- build async_tasks.ko -o async_tasks
./async_tasks
```

## Async Syntax Preview

Kodo supports `async fn` and `.await` as syntax, but in v1 these compile synchronously. The syntax exists to establish conventions for future versions where true async I/O will be available. For now, use `spawn` for concurrency.

## Next Steps

- [Actors](actors.md) -- stateful actors with message passing and the scheduler
- [HTTP & JSON](http.md) -- making HTTP requests and parsing JSON
- [Closures](closures.md) -- closures, lambda lifting, and higher-order functions
