# Real-World Examples

These examples demonstrate complete, practical programs built with Kōdo's features. Each one combines multiple language features to solve a realistic problem.

## Todo App

A task manager with CRUD operations using structs, contracts, string interpolation, and functional combinators (`fold`).

**Features used:** structs, contracts (`requires`), `List<Int>`, `fold`, `ref` borrowing, string interpolation

```rust
module todo_app {
    meta {
        purpose: "CLI todo app with CRUD operations"
        version: "1.0.0"
        author: "Kodo Team"
    }

    struct Task {
        id: Int,
        title: String,
        done: Bool,
        priority: String
    }

    fn create_task(id: Int, title: String, priority: String) -> Task
        requires { id > 0 }
    {
        return Task {
            id: id, title: title, done: false, priority: priority
        }
    }

    fn print_task(ref task: Task) {
        if task.done {
            println(f"[x] #{task.id} [{task.priority}] {task.title}")
        } else {
            println(f"[ ] #{task.id} [{task.priority}] {task.title}")
        }
    }

    fn main() -> Int {
        let t1: Task = create_task(1, "Write unit tests", "HIGH")
        let t2: Task = create_task(2, "Update README", "MED")
        print_task(t1)
        print_task(t2)

        // Count completed with fold
        let done_flags: List<Int> = list_new()
        list_push(done_flags, 1)
        list_push(done_flags, 0)
        let completed: Int = done_flags.fold(0, |acc: Int, x: Int| -> Int { acc + x })
        println(f"Completed: {completed.to_string()}/2")
        return 0
    }
}
```

```bash
kodoc build examples/todo_app.ko -o todo_app
./todo_app
```

Source: [`examples/todo_app.ko`](../../examples/todo_app.ko)

---

## URL Shortener

A URL shortening service with validation contracts, `Map<Int, Int>` for visit tracking, and string methods.

**Features used:** `Map<Int, Int>`, contracts (`requires`), string methods (`starts_with`), string interpolation

```rust
module url_shortener {
    meta {
        purpose: "URL shortener with validation and tracking"
        version: "1.0.0"
        author: "Kodo Team"
    }

    fn is_valid_url(url: String) -> Bool {
        return url.starts_with("http://") || url.starts_with("https://")
    }

    fn register_url(tracker: Map<Int, Int>, code: Int)
        requires { code > 0 }
    {
        map_insert(tracker, code, 0)
    }

    fn record_visit(tracker: Map<Int, Int>, code: Int)
        requires { code > 0 }
    {
        let current: Int = map_get(tracker, code)
        map_insert(tracker, code, current + 1)
    }

    fn main() -> Int {
        let tracker: Map<Int, Int> = map_new()

        let valid: Bool = is_valid_url("https://kodo-lang.dev/docs")
        if valid {
            register_url(tracker, 1)
            println("Registered code 1")
        }

        record_visit(tracker, 1)
        record_visit(tracker, 1)

        let visits: Int = map_get(tracker, 1)
        println(f"Code 1 visits: {visits.to_string()}")
        return 0
    }
}
```

```bash
kodoc build examples/url_shortener.ko -o url_shortener
./url_shortener
```

Source: [`examples/url_shortener.ko`](../../examples/url_shortener.ko)

---

## Word Counter

Counts words and characters using string methods, `for-in` loops over split results, and `fold` for aggregation.

**Features used:** structs, `ref` borrowing, string methods (`split`, `length`), `for-in`, `fold`, string interpolation

```rust
module word_counter {
    meta {
        purpose: "Word and character counter using string operations"
        version: "1.0.0"
        author: "Kodo Team"
    }

    struct Stats {
        words: Int,
        chars: Int
    }

    fn count_words(ref text: String) -> Int {
        let mut count: Int = 0
        for word in text.split(" ") {
            if word.length() > 0 {
                count = count + 1
            }
        }
        return count
    }

    fn analyze(ref text: String) -> Stats {
        let w: Int = count_words(text)
        let c: Int = text.length()
        return Stats { words: w, chars: c }
    }

    fn main() -> Int {
        let prose: String = "The quick brown fox jumps over the lazy dog"
        let stats: Stats = analyze(prose)
        println(f"Words: {stats.words.to_string()}")
        println(f"Chars: {stats.chars.to_string()}")
        return 0
    }
}
```

```bash
kodoc build examples/word_counter.ko -o word_counter
./word_counter
```

Source: [`examples/word_counter.ko`](../../examples/word_counter.ko)

---

## Config Validator

Validates application configuration using contracts, string methods, and struct field access.

**Features used:** structs, contracts (`requires`), `ref` borrowing, string methods (`length`), string interpolation

```rust
module config_validator {
    meta {
        purpose: "Configuration validator with contracts"
        version: "1.0.0"
        author: "Kodo Team"
    }

    struct AppConfig {
        port: Int,
        host: String,
        max_connections: Int,
        log_level: String,
        app_name: String
    }

    fn validate_port(port: Int) -> Bool
        requires { port >= 0 }
    {
        return port >= 1 && port <= 65535
    }

    fn validate_config(ref config: AppConfig) -> Int {
        let mut errors: Int = 0
        if !validate_port(config.port) {
            println("  ERROR: Port must be between 1 and 65535")
            errors = errors + 1
        }
        if config.app_name.length() == 0 {
            println("  ERROR: App name cannot be empty")
            errors = errors + 1
        }
        return errors
    }

    fn main() -> Int {
        let config: AppConfig = AppConfig {
            port: 8080, host: "localhost",
            max_connections: 100, log_level: "info",
            app_name: "my-service"
        }
        let errors: Int = validate_config(config)
        if errors == 0 {
            println("Config is VALID")
        }
        return 0
    }
}
```

```bash
kodoc build examples/config_validator.ko -o config_validator
./config_validator
```

Source: [`examples/config_validator.ko`](../../examples/config_validator.ko)

---

## HTTP Health Checker

Checks health of multiple HTTP endpoints and generates a status report using `http_get` and `fold`.

**Features used:** `http_get`, `List<Int>`, `fold`, string interpolation

```rust
module health_checker {
    meta {
        purpose: "HTTP endpoint health checker"
        version: "1.0.0"
        author: "Kodo Team"
    }

    fn check_and_report(name: String, url: String) -> Int {
        let status: Int = http_get(url)
        if status == 0 {
            println(f"  [OK] {name}")
            return 1
        } else {
            println(f"  [FAIL] {name}")
            return 0
        }
    }

    fn main() -> Int {
        println("=== Health Check Report ===")
        let results: List<Int> = list_new()

        let r1: Int = check_and_report("API", "https://httpbin.org/status/200")
        list_push(results, r1)

        let r2: Int = check_and_report("DB", "https://httpbin.org/status/500")
        list_push(results, r2)

        let total: Int = list_length(results)
        let healthy: Int = results.fold(0, |a: Int, x: Int| -> Int { a + x })
        println(f"Healthy: {healthy.to_string()}/{total.to_string()}")
        return 0
    }
}
```

```bash
kodoc build examples/health_checker.ko -o health_checker
./health_checker
```

Source: [`examples/health_checker.ko`](../../examples/health_checker.ko)
