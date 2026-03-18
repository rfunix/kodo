#!/usr/bin/env bash
# validate-doc-examples.sh — Compile and run every code example from the
# kodo-lang.dev documentation against the real compiler.
#
# Usage:
#   ./scripts/validate-doc-examples.sh [WEBSITE_DIR]
#
# Requires: kodoc built (cargo build -p kodoc --release)
# Exit code: 0 if all pass, 1 if any fail.

set -euo pipefail

WEBSITE_DIR="${1:-$HOME/dev/kodo-website}"
KODOC="$(cd "$(dirname "$0")/.." && pwd)/target/release/kodoc"
TMPDIR="$(mktemp -d /tmp/kodo-doc-validate.XXXXXX)"

if [[ ! -x "$KODOC" ]]; then
    echo "ERROR: kodoc not found at $KODOC — run 'cargo build -p kodoc --release' first"
    exit 1
fi

if [[ ! -d "$WEBSITE_DIR/src/content/docs" ]]; then
    echo "ERROR: website dir not found at $WEBSITE_DIR"
    exit 1
fi

PASS=0
FAIL=0
ERRORS=""

# Helper: test a .ko file (check + build + run)
test_ko() {
    local name="$1" file="$2" mode="${3:-run}"
    local out="$TMPDIR/out_$name"

    if ! "$KODOC" check "$file" >/dev/null 2>&1; then
        FAIL=$((FAIL + 1))
        ERRORS="$ERRORS\n  FAIL: $name (check failed)"
        return
    fi

    if [[ "$mode" == "check-only" ]]; then
        PASS=$((PASS + 1))
        return
    fi

    if [[ "$mode" == "test" ]]; then
        if "$KODOC" test "$file" >/dev/null 2>&1; then
            PASS=$((PASS + 1))
        else
            FAIL=$((FAIL + 1))
            ERRORS="$ERRORS\n  FAIL: $name (kodoc test failed)"
        fi
        return
    fi

    if ! "$KODOC" build "$file" -o "$out" >/dev/null 2>&1; then
        FAIL=$((FAIL + 1))
        ERRORS="$ERRORS\n  FAIL: $name (build failed)"
        return
    fi

    if [[ "$mode" == "build-only" ]]; then
        PASS=$((PASS + 1))
        return
    fi

    if "$out" >/dev/null 2>&1; then
        PASS=$((PASS + 1))
    else
        FAIL=$((FAIL + 1))
        ERRORS="$ERRORS\n  FAIL: $name (runtime exit code $?)"
    fi
}

# Helper: test that a .ko file produces a specific error code
test_error() {
    local name="$1" file="$2" expected_code="$3"
    local output
    output=$("$KODOC" check "$file" 2>&1 || true)
    if echo "$output" | LC_ALL=C tr -d '\033' | grep -q "$expected_code"; then
        PASS=$((PASS + 1))
    else
        FAIL=$((FAIL + 1))
        ERRORS="$ERRORS\n  FAIL: $name (expected error $expected_code not found)"
    fi
}

echo "=== Kodo Documentation Example Validation ==="
echo "Compiler: $KODOC"
echo "Website:  $WEBSITE_DIR"
echo ""

# ─── P0: Getting Started ──────────────────────────────────────
echo -n "getting-started.md ... "
cat > "$TMPDIR/gs_hello.ko" << 'KO'
module hello {
    meta { purpose: "My first Kodo program", version: "0.1.0" }
    fn main() { println("Hello, World!") }
}
KO
test_ko gs_hello "$TMPDIR/gs_hello.ko"

cat > "$TMPDIR/gs_fib.ko" << 'KO'
module fibonacci {
    meta { purpose: "Fibonacci", version: "0.1.0" }
    fn fib(n: Int) -> Int {
        if n <= 1 { return n }
        return fib(n - 1) + fib(n - 2)
    }
    fn main() { print_int(fib(10)) }
}
KO
test_ko gs_fib "$TMPDIR/gs_fib.ko"
echo "done"

# ─── P0: Tour ─────────────────────────────────────────────────
echo -n "tour.md ... "
cat > "$TMPDIR/tour_greeter.ko" << 'KO'
module greeter {
    meta { purpose: "Greet users", version: "0.1.0" }
    fn main() { println("Hello from Kodo!") }
}
KO
test_ko tour_greeter "$TMPDIR/tour_greeter.ko"

cat > "$TMPDIR/tour_contracts.ko" << 'KO'
module tour_contracts {
    meta { purpose: "test", version: "1.0" }
    fn safe_divide(a: Int, b: Int) -> Int
        requires { b != 0 }
        ensures { result * b <= a }
    { return a / b }
    fn main() { print_int(safe_divide(10, 3)) }
}
KO
test_ko tour_contracts "$TMPDIR/tour_contracts.ko"

cat > "$TMPDIR/tour_option.ko" << 'KO'
module tour_option {
    meta { purpose: "test", version: "1.0" }
    fn find_first_positive(a: Int, b: Int) -> Option<Int> {
        if a > 0 { return Option::Some(a) }
        if b > 0 { return Option::Some(b) }
        return Option::None
    }
    fn main() {
        let result: Option<Int> = find_first_positive(42, -1)
        match result {
            Option::Some(v) => { print_int(v) }
            Option::None => { println("none") }
        }
    }
}
KO
test_ko tour_option "$TMPDIR/tour_option.ko"

cat > "$TMPDIR/tour_math.ko" << 'KO'
module tour_math {
    meta { purpose: "test", version: "1.0" }
    fn main() {
        print_int(abs(-42))
        print_int(min(10, 20))
        print_int(max(10, 20))
        print_int(clamp(50, 0, 25))
    }
}
KO
test_ko tour_math "$TMPDIR/tour_math.ko"

cat > "$TMPDIR/tour_spawn.ko" << 'KO'
module tour_spawn {
    meta { purpose: "test", version: "1.0" }
    fn main() {
        print_int(1)
        spawn { print_int(2) }
        spawn { print_int(3) }
        print_int(4)
    }
}
KO
test_ko tour_spawn "$TMPDIR/tour_spawn.ko"
echo "done"

# ─── P1: Language Basics ──────────────────────────────────────
echo -n "language-basics.md ... "
cat > "$TMPDIR/lb_fizzbuzz.ko" << 'KO'
module fizzbuzz {
    meta { purpose: "FizzBuzz", version: "0.1.0" }
    fn fizzbuzz(n: Int) -> String {
        if n % 15 == 0 { return "FizzBuzz" }
        if n % 3 == 0 { return "Fizz" }
        if n % 5 == 0 { return "Buzz" }
        return "other"
    }
    fn main() { println(fizzbuzz(15)) }
}
KO
test_ko lb_fizzbuzz "$TMPDIR/lb_fizzbuzz.ko"
echo "done"

# ─── P1: Data Types ───────────────────────────────────────────
echo -n "data-types.md ... "
cat > "$TMPDIR/dt_structs.ko" << 'KO'
module dt_structs {
    meta { purpose: "test", version: "1.0" }
    struct Point { x: Int, y: Int }
    fn translate(p: Point, dx: Int, dy: Int) -> Point {
        return Point { x: p.x + dx, y: p.y + dy }
    }
    fn main() {
        let p: Point = Point { x: 10, y: 20 }
        let moved: Point = translate(p, 1, 2)
        print_int(moved.x)
        print_int(moved.y)
    }
}
KO
test_ko dt_structs "$TMPDIR/dt_structs.ko"

cat > "$TMPDIR/dt_enums.ko" << 'KO'
module dt_enums {
    meta { purpose: "test", version: "1.0" }
    enum Shape { Circle(Int), Rectangle(Int, Int) }
    fn area(s: Shape) -> Int {
        match s {
            Shape::Circle(r) => { return r * r * 3 }
            Shape::Rectangle(w, h) => { return w * h }
        }
    }
    fn main() {
        let c: Shape = Shape::Circle(5)
        print_int(area(c))
    }
}
KO
test_ko dt_enums "$TMPDIR/dt_enums.ko"
echo "done"

# ─── P1: Error Handling ───────────────────────────────────────
echo -n "error-handling.md ... "
cat > "$TMPDIR/eh_complete.ko" << 'KO'
module error_handling {
    meta { purpose: "test", version: "0.1.0" }
    fn safe_divide(a: Int, b: Int) -> Result<Int, Int> {
        if b == 0 { return Result::Err(0) }
        return Result::Ok(a / b)
    }
    fn first_positive(a: Int, b: Int) -> Option<Int> {
        if a > 0 { return Option::Some(a) }
        if b > 0 { return Option::Some(b) }
        return Option::None
    }
    fn main() {
        let div: Result<Int, Int> = safe_divide(100, 5)
        match div {
            Result::Ok(v) => { print_int(v) }
            Result::Err(e) => { println("error") }
        }
        let found: Option<Int> = first_positive(-1, 42)
        match found {
            Option::Some(v) => { print_int(v) }
            Option::None => { println("none") }
        }
    }
}
KO
test_ko eh_complete "$TMPDIR/eh_complete.ko"
echo "done"

# ─── P1: Ownership ────────────────────────────────────────────
echo -n "ownership.md ... "
cat > "$TMPDIR/own_e0240.ko" << 'KO'
module own_e0240 {
    meta { purpose: "test", version: "1.0" }
    fn consume(s: String) { println(s) }
    fn main() {
        let s: String = "hello"
        consume(s)
        consume(s)
    }
}
KO
test_error own_e0240 "$TMPDIR/own_e0240.ko" "E0240"

cat > "$TMPDIR/own_ref.ko" << 'KO'
module own_ref {
    meta { purpose: "test", version: "1.0" }
    fn show(ref s: String) { println(s) }
    fn main() {
        let s: String = "hello"
        show(s)
        show(s)
    }
}
KO
test_ko own_ref "$TMPDIR/own_ref.ko"
echo "done"

# ─── P1: Generics ─────────────────────────────────────────────
echo -n "generics.md ... "
cat > "$TMPDIR/gen_pair.ko" << 'KO'
module gen_pair {
    meta { purpose: "test", version: "1.0" }
    struct Pair<T> { first: T, second: T }
    fn main() {
        let p: Pair<Int> = Pair { first: 10, second: 20 }
        print_int(p.first)
        print_int(p.second)
    }
}
KO
test_ko gen_pair "$TMPDIR/gen_pair.ko"
echo "done"

# ─── P1: Traits ───────────────────────────────────────────────
echo -n "traits.md ... "
cat > "$TMPDIR/traits_dyn.ko" << 'KO'
module traits_dyn {
    meta { purpose: "test", version: "1.0" }
    trait Describable { fn name(self) -> Int }
    struct Point { x: Int, y: Int }
    impl Describable for Point {
        fn name(self) -> Int { return self.x }
    }
    fn get_name(obj: dyn Describable) -> Int { return obj.name() }
    fn main() {
        let p: Point = Point { x: 42, y: 10 }
        print_int(get_name(p))
    }
}
KO
test_ko traits_dyn "$TMPDIR/traits_dyn.ko"
echo "done"

# ─── P1: Methods ──────────────────────────────────────────────
echo -n "methods.md ... "
cat > "$TMPDIR/methods_static.ko" << 'KO'
module methods_static {
    meta { purpose: "test", version: "1.0" }
    struct Counter { value: Int }
    impl Counter {
        fn new() -> Counter { return Counter { value: 0 } }
    }
    fn main() {
        let c: Counter = Counter.new()
        print_int(c.value)
    }
}
KO
test_ko methods_static "$TMPDIR/methods_static.ko"
echo "done"

# ─── P1: Closures ─────────────────────────────────────────────
echo -n "closures.md ... "
cat > "$TMPDIR/closures_hof.ko" << 'KO'
module closures_hof {
    meta { purpose: "test", version: "1.0" }
    fn double(x: Int) -> Int { return x * 2 }
    fn apply(f: (Int) -> Int, x: Int) -> Int { return f(x) }
    fn main() { print_int(apply(double, 5)) }
}
KO
test_ko closures_hof "$TMPDIR/closures_hof.ko"

cat > "$TMPDIR/closures_capture.ko" << 'KO'
module closures_capture {
    meta { purpose: "test", version: "1.0" }
    fn make_adder(n: Int) -> (Int) -> Int {
        return |x: Int| -> Int { return x + n }
    }
    fn main() {
        let add5 = make_adder(5)
        print_int(add5(10))
    }
}
KO
test_ko closures_capture "$TMPDIR/closures_capture.ko"
echo "done"

# ─── P1: Functional ───────────────────────────────────────────
echo -n "functional.md ... "
cat > "$TMPDIR/func_pipeline.ko" << 'KO'
module func_pipeline {
    meta { purpose: "test", version: "1.0" }
    fn main() {
        let data: List<Int> = list_new()
        list_push(data, 1)
        list_push(data, 2)
        list_push(data, 3)
        list_push(data, 4)
        list_push(data, 5)
        let evens: List<Int> = data.filter(|x: Int| -> Bool { x % 2 == 0 })
        let doubled: List<Int> = evens.map(|x: Int| -> Int { x * 2 })
        let result: Int = doubled.fold(0, |acc: Int, x: Int| -> Int { acc + x })
        print_int(result)
    }
}
KO
test_ko func_pipeline "$TMPDIR/func_pipeline.ko"
echo "done"

# ─── P1: Iterators ────────────────────────────────────────────
echo -n "iterators.md ... "
cat > "$TMPDIR/iter_map.ko" << 'KO'
module iter_map {
    meta { purpose: "test", version: "1.0" }
    fn main() {
        let m: Map<Int, Int> = map_new()
        map_insert(m, 1, 95)
        map_insert(m, 2, 87)
        for key in m { print_int(key) }
    }
}
KO
test_ko iter_map "$TMPDIR/iter_map.ko"
echo "done"

# ─── P1: String Interpolation ─────────────────────────────────
echo -n "string-interpolation.md ... "
cat > "$TMPDIR/fstring.ko" << 'KO'
module fstring {
    meta { purpose: "test", version: "1.0" }
    fn main() {
        let name: String = "World"
        println(f"Hello, {name}!")
    }
}
KO
test_ko fstring "$TMPDIR/fstring.ko"
echo "done"

# ─── P1: List<String> ─────────────────────────────────────────
echo -n "list-string ... "
cat > "$TMPDIR/list_string.ko" << 'KO'
module list_string {
    meta { purpose: "test", version: "1.0" }
    fn main() {
        let names: List<String> = list_new()
        list_push(names, "alice")
        list_push(names, "bob")
        println(list_get(names, 0))
        println(list_get(names, 1))
    }
}
KO
test_ko list_string "$TMPDIR/list_string.ko"
echo "done"

# ─── P2: Contracts ────────────────────────────────────────────
echo -n "contracts.md ... "
cat > "$TMPDIR/contracts.ko" << 'KO'
module contracts {
    meta { purpose: "test", version: "1.0" }
    fn clamp(value: Int, lo: Int, hi: Int) -> Int
        requires { lo <= hi }
        ensures { result >= lo && result <= hi }
    {
        if value < lo { return lo }
        if value > hi { return hi }
        return value
    }
    fn main() { print_int(clamp(50, 0, 25)) }
}
KO
test_ko contracts "$TMPDIR/contracts.ko"
echo "done"

# ─── P2: Agent Traceability ───────────────────────────────────
echo -n "agent-traceability.md ... "
cat > "$TMPDIR/agent_e0260.ko" << 'KO'
module agent_e0260 {
    meta { purpose: "test", version: "1.0" }
    @confidence(0.5)
    fn risky() -> Int { return 42 }
    fn main() { print_int(risky()) }
}
KO
test_error agent_e0260 "$TMPDIR/agent_e0260.ko" "E0260"
echo "done"

# ─── P2: Concurrency ──────────────────────────────────────────
echo -n "concurrency.md ... "
cat > "$TMPDIR/concurrency.ko" << 'KO'
module concurrency {
    meta { purpose: "test", version: "1.0" }
    fn main() {
        println("main")
        spawn { println("task") }
    }
}
KO
test_ko concurrency "$TMPDIR/concurrency.ko"
echo "done"

# ─── P2: Testing ──────────────────────────────────────────────
echo -n "testing.md ... "
cat > "$TMPDIR/testing.ko" << 'KO'
module math_tests {
    meta { purpose: "test", version: "1.0" }
    fn add(a: Int, b: Int) -> Int { return a + b }
    test "addition works" {
        assert_eq(add(2, 3), 5)
        assert_eq(add(-1, 1), 0)
    }
    test "addition is commutative" {
        assert_eq(add(3, 7), add(7, 3))
    }
    fn main() { println("ok") }
}
KO
test_ko testing "$TMPDIR/testing.ko" test
echo "done"

# ─── P2: Stdlib ───────────────────────────────────────────────
echo -n "stdlib-reference.md ... "
cat > "$TMPDIR/stdlib_math.ko" << 'KO'
module stdlib_math {
    meta { purpose: "test", version: "1.0" }
    fn main() {
        print_int(abs(-5))
        let s: Float64 = sqrt(9.0)
        println_float(s)
    }
}
KO
test_ko stdlib_math "$TMPDIR/stdlib_math.ko"

cat > "$TMPDIR/stdlib_strings.ko" << 'KO'
module stdlib_strings {
    meta { purpose: "test", version: "1.0" }
    fn main() {
        let len: Int = "hello".length()
        print_int(len)
        let ch: Int = "ABC".char_at(0)
        print_int(ch)
        let r: String = "ab".repeat(3)
        println(r)
    }
}
KO
test_ko stdlib_strings "$TMPDIR/stdlib_strings.ko"

cat > "$TMPDIR/stdlib_json.ko" << 'KO'
module stdlib_json {
    meta { purpose: "test", version: "1.0" }
    fn main() {
        let doc: Int = json_parse("{\"name\": \"alice\", \"age\": 30}")
        let name: String = json_get_string(doc, "name")
        println(name)
        let age: Int = json_get_int(doc, "age")
        print_int(age)
        json_free(doc)
    }
}
KO
test_ko stdlib_json "$TMPDIR/stdlib_json.ko"

cat > "$TMPDIR/stdlib_channel.ko" << 'KO'
module stdlib_channel {
    meta { purpose: "test", version: "1.0" }
    fn main() {
        let ch: Channel<Int> = channel_new()
        channel_send(ch, 42)
        let val: Int = channel_recv(ch)
        print_int(val)
        channel_free(ch)
    }
}
KO
test_ko stdlib_channel "$TMPDIR/stdlib_channel.ko"

cat > "$TMPDIR/stdlib_file.ko" << 'KO'
module stdlib_file {
    meta { purpose: "test", version: "1.0" }
    fn main() {
        file_write("/tmp/kodo_validate_test.txt", "hello kodo")
        let r: Result<String, String> = file_read("/tmp/kodo_validate_test.txt")
        match r {
            Result::Ok(content) => { println(content) }
            Result::Err(e) => { println("error") }
        }
    }
}
KO
test_ko stdlib_file "$TMPDIR/stdlib_file.ko"
echo "done"

# ─── Summary ──────────────────────────────────────────────────
echo ""
echo "=== Results ==="
echo "PASS: $PASS"
echo "FAIL: $FAIL"
TOTAL=$((PASS + FAIL))
echo "TOTAL: $TOTAL"

if [[ $FAIL -gt 0 ]]; then
    echo ""
    echo "Failures:"
    echo -e "$ERRORS"
    echo ""
fi

# Cleanup
rm -rf "$TMPDIR"

if [[ $FAIL -eq 0 ]]; then
    echo ""
    echo "ALL CLEAR — every documentation example compiles and runs correctly."
    exit 0
else
    echo ""
    echo "$FAIL example(s) FAILED."
    exit 1
fi
