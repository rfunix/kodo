# Real-World Examples

These examples demonstrate complete programs that showcase **what makes Kōdo different from every other language**: compiler-enforced trust policies, contract verification, ownership safety, and agent traceability. Each example shows features that only exist in Kōdo.

## Why These Examples Matter

In other languages, an AI agent generates code and hopes it's correct. In Kōdo:

- **The compiler tracks who wrote each function** (`@authored_by`, `@confidence`)
- **Low-confidence code won't compile without human review** (`@reviewed_by`)
- **Security-sensitive code must have contracts** (`@security_sensitive`)
- **Contracts are verified by Z3 at compile time** — not just type-checked
- **Ownership prevents use-after-move bugs** that agents commonly introduce
- **Every module is self-describing** — agents can understand modules without reading implementation

Run `kodoc confidence-report <file>` on any example to see the full trust audit.

---

## Todo App — Agent Traceability

An AI agent builds a task manager. The compiler enforces quality guarantees on every function the agent writes.

**Kōdo-unique features:** `@authored_by`, `@confidence`, `@reviewed_by` (compiler-enforced review), contracts (`requires`), `ref` borrowing

```rust
// High confidence: agent is sure this is correct.
@authored_by(agent: "claude")
@confidence(0.99)
fn create_task(id: Int, title: String, priority: String) -> Task
    requires { id > 0 }
{
    return Task { id: id, title: title, done: false, priority: priority }
}

// 0.7 < 0.8 → compiler REQUIRES @reviewed_by or won't compile.
@authored_by(agent: "claude")
@confidence(0.7)
@reviewed_by(human: "rafael")
fn count_completed(flags: List<Int>) -> Int {
    return flags.fold(0, |acc: Int, x: Int| -> Int { acc + x })
}
```

```bash
kodoc build examples/todo_app.ko -o todo_app && ./todo_app
kodoc confidence-report examples/todo_app.ko
```

Source: [`examples/todo_app.ko`](../../examples/todo_app.ko)

---

## URL Shortener — Security Contracts + Refinement Types

Every URL validation function is `@security_sensitive` — the compiler **refuses to compile** it without a contract. Refinement types make invalid values impossible to construct.

**Kōdo-unique features:** `@security_sensitive` (E0262 without contracts), refinement types (`type ShortCode = Int requires { ... }`), contract chains, `@authored_by`

```rust
// Refinement type: codes must be positive
type ShortCode = Int requires { self > 0 }

// @security_sensitive → compiler rejects this without a contract (E0262).
@security_sensitive
@authored_by(agent: "claude")
@confidence(0.95)
fn is_valid_url(url: String) -> Bool
    requires { url.length() > 0 }
{
    return url.starts_with("http://") || url.starts_with("https://")
}

// Contract chain: every function that touches codes requires code > 0.
// Agent can't accidentally call with code=0 — the compiler catches it.
fn register_url(tracker: Map<Int, Int>, code: Int)
    requires { code > 0 }
{
    map_insert(tracker, code, 0)
}
```

```bash
kodoc build examples/url_shortener.ko -o url_shortener && ./url_shortener
kodoc confidence-report examples/url_shortener.ko
```

Source: [`examples/url_shortener.ko`](../../examples/url_shortener.ko)

---

## Word Counter — Ownership Prevents Agent Bugs

Demonstrates how `ref` borrowing prevents the most common bug AI agents introduce in systems languages: **use-after-move**. Without `ref`, calling `count_words(text)` would consume `text`, and the subsequent `text.length()` would fail with E0240.

**Kōdo-unique features:** `ref` borrowing (E0240 prevention), linear ownership (`own`), contracts on text processing, `@confidence` with forced review

```rust
// `ref text` borrows instead of moving — text stays usable after the call.
// Without ref, the caller couldn't use `text` again (E0240: use-after-move).
@authored_by(agent: "claude")
@confidence(0.95)
fn count_words(ref text: String) -> Int
    requires { text.length() > 0 }
{
    let mut count: Int = 0
    for word in text.split(" ") {
        if word.length() > 0 {
            count = count + 1
        }
    }
    return count
}

// ref borrows text — we can call count_words AND text.length()
// on the same value. In a move-based call, the second use would fail.
fn analyze(ref text: String) -> Stats {
    let w: Int = count_words(text)
    let c: Int = text.length()
    return Stats { words: w, chars: c }
}
```

```bash
kodoc build examples/word_counter.ko -o word_counter && ./word_counter
kodoc confidence-report examples/word_counter.ko
```

Source: [`examples/word_counter.ko`](../../examples/word_counter.ko)

---

## Config Validator — Refinement Types + Module Invariants

Configuration validation with **three layers of safety** that no other language provides simultaneously: refinement types constrain values at the type level, contracts verify logic at function boundaries, and module invariants enforce global conditions.

**Kōdo-unique features:** refinement types (`type Port`, `type MaxConns`), `invariant { }` (module-level), `@security_sensitive`, contracts, `ref` borrowing

```rust
// Module invariant: checked at entry of every function in this module.
invariant { 1 > 0 }

// Refinement types — invalid values can't even be constructed
type Port = Int requires { self >= 1 && self <= 65535 }
type MaxConns = Int requires { self > 0 && self <= 10000 }

// @security_sensitive: config validation is a trust boundary.
// The compiler won't let this function exist without contracts (E0262).
@security_sensitive
@authored_by(agent: "claude")
@confidence(0.95)
fn validate_port(port: Int) -> Bool
    requires { port >= 0 }
{
    return port >= 1 && port <= 65535
}
```

```bash
kodoc build examples/config_validator.ko -o config_validator && ./config_validator
kodoc confidence-report examples/config_validator.ko
```

Source: [`examples/config_validator.ko`](../../examples/config_validator.ko)

---

## HTTP Health Checker — Closed-Loop Agent Workflow

Shows the complete agent workflow: the agent writes code, the compiler verifies it, and when something breaks, `--json-errors` gives the agent structured output to fix it automatically. No other language has this **machine-first error protocol**.

**Kōdo-unique features:** `@security_sensitive` on network code, `@confidence` < 0.8 forcing `@reviewed_by`, `--json-errors` for machine consumption, `kodoc fix` for auto-repair

```rust
// @security_sensitive: network-facing code MUST have a contract.
// Without `requires`, the compiler rejects this function (E0262).
@security_sensitive
@authored_by(agent: "claude")
@confidence(0.9)
fn check_endpoint(name: String, url: String) -> Int
    requires { url.length() > 0 }
{
    let status: Int = http_get(url)
    if status == 0 {
        println(f"  [OK] {name}")
        return 1
    } else {
        println(f"  [FAIL] {name}")
        return 0
    }
}

// 0.75 < 0.8 → compiler requires @reviewed_by or won't compile.
@authored_by(agent: "claude")
@confidence(0.75)
@reviewed_by(human: "rafael")
fn summarize(results: List<Int>) {
    let total: Int = list_length(results)
    let healthy: Int = results.fold(0, |acc: Int, x: Int| -> Int { acc + x })
    println(f"Healthy: {healthy.to_string()}/{total.to_string()}")
}
```

```bash
kodoc build examples/health_checker.ko -o health_checker && ./health_checker
kodoc confidence-report examples/health_checker.ko
# Machine-readable errors for agent consumption:
kodoc check examples/health_checker.ko --json-errors
```

Source: [`examples/health_checker.ko`](../../examples/health_checker.ko)

---

## Audit Log — Multi-File Showcase (15+ Features)

A complete audit log system for AI agents — meta-relevant: an auditable system written in the language designed for auditability. This is the most comprehensive example, combining 15+ Kōdo features across multiple files.

**Kōdo-unique features:** multi-file imports, refinement types (`Timestamp`, `ConfidenceScore`), `@security_sensitive`, `@reviewed_by` (forced by low confidence), contracts, module `invariant`, enums with pattern matching, `List<T>` pipelines (`filter`/`map`/`fold`/`any`/`all`), `Map<String, Int>`, closures, f-strings, `for-in`, `let mut`

```rust
// validation.ko — refinement types constrain values at the type level
type Timestamp = Int requires { self > 0 }
type ConfidenceScore = Int requires { self >= 0 && self <= 100 }

// @security_sensitive forces contracts — no contract = no compilation (E0262)
pub
@security_sensitive
@authored_by(agent: "claude")
@confidence(0.92)
fn validate_entry(agent_name_len: Int, severity_level: Int, timestamp: Int, confidence: Int) -> Bool
    requires { timestamp > 0 }
    requires { severity_level >= 1 }
    requires { severity_level <= 4 }
{ ... }

// main.ko — functional pipelines on audit data
import validation

let critical_levels: List<Int> = levels.filter(|lvl: Int| -> Bool { lvl >= 3 })
let risk_scores: List<Int> = levels.map(|lvl: Int| -> Int { lvl * 25 })
let total_risk: Int = risk_scores.fold(0, |acc: Int, r: Int| -> Int { acc + r })
let has_critical: Bool = levels.any(|lvl: Int| -> Bool { lvl >= 4 })
```

```bash
kodoc build examples/audit_log/main.ko -o audit_log && ./audit_log
kodoc test examples/audit_log/audit_log_test.ko
kodoc confidence-report examples/audit_log/main.ko
```

Source: [`examples/audit_log/`](../../examples/audit_log/)
