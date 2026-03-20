# Concurrency

Kōdo uses green threads for lightweight concurrency. You express concurrency with `spawn` blocks and `async`/`await`, and the runtime schedules them across multiple OS threads using work-stealing.

## Green Threads

Kōdo's concurrency model is based on **green threads** — lightweight threads managed by the runtime, not the OS. Each green thread has a 64KB stack and is multiplexed onto a pool of OS worker threads (M:N scheduling).

Key properties:
- **Lightweight**: thousands of green threads can run on a few OS threads
- **Cooperative scheduling**: green threads yield at loop iterations and function calls
- **Work-stealing**: idle workers steal tasks from busy workers for load balancing

## Basic Spawn

A `spawn` block creates a new green thread:

```rust
fn main() -> Int {
    spawn {
        println("hello from green thread 1")
    }

    spawn {
        println("hello from green thread 2")
    }

    println("main thread")
    return 0
}
```

Output order may vary — green threads run concurrently:
```
main thread
hello from green thread 1
hello from green thread 2
```

## Spawn with Captures

Spawned blocks can capture variables from the enclosing scope (by value):

```rust
fn main() -> Int {
    let greeting: String = "hello"
    let count: Int = 42

    spawn {
        print(greeting)
        print_int(count)
    }

    return 0
}
```

Captures are copied at the time of `spawn` — changes to the original variable after spawn do not affect the spawned thread.

## Async/Await

Use `async fn` for concurrent computations that return a value:

```rust
async fn compute(x: Int) -> Int {
    return x * x
}

fn main() -> Int {
    let result: Int = compute(5).await
    print_int(result)  // 25
    return 0
}
```

- `async fn` runs its body on a green thread and returns a `Future<T>`
- `.await` suspends the calling green thread until the result is available
- While waiting, the worker thread runs other green threads

## Parallel Blocks

For structured parallelism with real OS threads, use `parallel`:

```rust
fn main() -> Int {
    parallel {
        spawn { heavy_computation_a() }
        spawn { heavy_computation_b() }
    }
    // Both tasks guaranteed complete here
    return 0
}
```

`parallel` uses `std::thread::scope` — real OS threads with structured concurrency guarantees.

## Channels

Channels enable communication between green threads. They support any type:

```rust
fn main() -> Int {
    let ch: Channel<Int> = channel_new()

    spawn {
        channel_send(ch, 42)
    }

    let val: Int = channel_recv(ch)
    print_int(val)  // 42
    return 0
}
```

`channel_recv` yields the green thread while waiting — it does not block the OS thread.

## Configuration

Control the number of worker threads:

```bash
# Auto-detect (default: one per CPU core)
kodoc build myfile.ko

# Specific thread count
kodoc build myfile.ko --threads 4

# Disable green threads (legacy sequential mode)
kodoc build myfile.ko --no-green-threads
```

## How It Works

The compiler inserts **yield points** at:
- Loop back-edges (start of each `while`/`for` iteration)
- Function calls (before calling user-defined functions)
- I/O operations (`http_get`, `file_read`, `channel_recv`)

At each yield point, the runtime checks if the current green thread should yield. If yes, it saves the thread's CPU registers, picks another thread from the work-stealing queue, and resumes it. This is transparent to the programmer.

## Known Limitations

- **Async execution**: In v1, `async fn` calls execute synchronously and return their result directly. The runtime infrastructure for true futures exists but is not yet wired end-to-end.
- **No `select`**: Cannot wait on multiple channels simultaneously (planned for v2).
- **Fixed stack size**: Each green thread gets 64KB. Deep recursion may overflow.
- **No preemption**: A green thread in a tight loop without yield points (e.g., inline assembly) will not yield. Use `--no-green-threads` if this is a problem.

## Examples

- [`examples/green_threads.ko`](https://github.com/rfunix/kodo/blob/main/examples/green_threads.ko) — basic spawn with green threads
- [`examples/async_await.ko`](https://github.com/rfunix/kodo/blob/main/examples/async_await.ko) — async/await demonstration
- [`examples/parallel_demo.ko`](https://github.com/rfunix/kodo/blob/main/examples/parallel_demo.ko) — parallel blocks with OS threads
- [`examples/channel_demo.ko`](https://github.com/rfunix/kodo/blob/main/examples/channel_demo.ko) — channel communication
