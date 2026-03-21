#!/usr/bin/env bash
# validate-cli.sh — Validate all kodoc CLI commands work correctly.
#
# Usage:
#   ./scripts/validate-cli.sh
#
# Requires: kodoc built (cargo build -p kodoc --release)
# Exit code: 0 if all pass, 1 if any fail.

set -uo pipefail

# ── Color setup ──────────────────────────────────────────────────────────────
if [[ -t 1 ]] && command -v tput &>/dev/null && [[ $(tput colors 2>/dev/null || echo 0) -ge 8 ]]; then
    GREEN=$(tput setaf 2)
    RED=$(tput setaf 1)
    YELLOW=$(tput setaf 3)
    BOLD=$(tput bold)
    RESET=$(tput sgr0)
else
    GREEN="" RED="" YELLOW="" BOLD="" RESET=""
fi

# ── Locate kodoc ─────────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
KODOC="$PROJECT_ROOT/target/release/kodoc"

if [[ ! -x "$KODOC" ]]; then
    echo "kodoc not found at $KODOC — building release..."
    (cd "$PROJECT_ROOT" && cargo build -p kodoc --release)
    if [[ ! -x "$KODOC" ]]; then
        echo "${RED}ERROR: Failed to build kodoc${RESET}"
        exit 1
    fi
fi

# ── Temp directory for outputs ───────────────────────────────────────────────
TMPDIR="$(mktemp -d /tmp/kodo-cli-validate.XXXXXX)"
trap 'rm -rf "$TMPDIR"' EXIT

# ── Counters ─────────────────────────────────────────────────────────────────
PASS=0
FAIL=0
SKIP=0
ERRORS=""

# ── Helper: run a CLI test ───────────────────────────────────────────────────
# Usage: test_cmd "description" command [args...]
test_cmd() {
    local desc="$1"
    shift
    if "$@" >/dev/null 2>&1; then
        printf "  ${GREEN}✓${RESET} %s\n" "$desc"
        PASS=$((PASS + 1))
    else
        printf "  ${RED}✗${RESET} %s\n" "$desc"
        FAIL=$((FAIL + 1))
        ERRORS="$ERRORS\n  ✗ $desc"
    fi
}

# Usage: test_cmd_skip "description" "reason"
test_cmd_skip() {
    local desc="$1" reason="$2"
    printf "  ${YELLOW}⊘${RESET} %s (%s)\n" "$desc" "$reason"
    SKIP=$((SKIP + 1))
}

# ── Main ─────────────────────────────────────────────────────────────────────
echo "${BOLD}=== Kōdo CLI Validation ===${RESET}"
echo "Compiler: $KODOC"
echo ""

EXAMPLES="$PROJECT_ROOT/examples"

# ── Core compilation commands ────────────────────────────────────────────────
echo "${BOLD}── Core commands ──${RESET}"
test_cmd "kodoc lex examples/hello.ko" \
    "$KODOC" lex "$EXAMPLES/hello.ko"

test_cmd "kodoc parse examples/hello.ko" \
    "$KODOC" parse "$EXAMPLES/hello.ko"

test_cmd "kodoc check examples/hello.ko" \
    "$KODOC" check "$EXAMPLES/hello.ko"

test_cmd "kodoc build examples/hello.ko" \
    "$KODOC" build "$EXAMPLES/hello.ko" -o "$TMPDIR/hello"

# Run the built binary
if [[ -x "$TMPDIR/hello" ]]; then
    test_cmd "run built hello binary" \
        "$TMPDIR/hello"
else
    printf "  ${RED}✗${RESET} run built hello binary (binary not found)\n"
    FAIL=$((FAIL + 1))
    ERRORS="$ERRORS\n  ✗ run built hello binary (binary not found)"
fi

test_cmd "kodoc mir examples/fibonacci.ko" \
    "$KODOC" mir "$EXAMPLES/fibonacci.ko"

# ── Tool commands ────────────────────────────────────────────────────────────
echo ""
echo "${BOLD}── Tool commands ──${RESET}"

test_cmd "kodoc explain E0201" \
    "$KODOC" explain E0201

# fmt --check returns 0 if no changes needed, 1 if reformatting needed.
# Both are valid outputs; we just test that it doesn't crash (exit 0 or 1).
fmt_output=$("$KODOC" fmt "$EXAMPLES/hello.ko" --check 2>&1) ; fmt_exit=$?
if [[ $fmt_exit -eq 0 || $fmt_exit -eq 1 ]]; then
    printf "  ${GREEN}✓${RESET} %s\n" "kodoc fmt examples/hello.ko --check"
    PASS=$((PASS + 1))
else
    printf "  ${RED}✗${RESET} %s\n" "kodoc fmt examples/hello.ko --check"
    FAIL=$((FAIL + 1))
    ERRORS="$ERRORS\n  ✗ kodoc fmt --check (exit $fmt_exit)"
fi

# ── Analysis commands ────────────────────────────────────────────────────────
echo ""
echo "${BOLD}── Analysis commands ──${RESET}"

if [[ -f "$EXAMPLES/confidence_demo.ko" ]]; then
    test_cmd "kodoc confidence-report --json" \
        "$KODOC" confidence-report "$EXAMPLES/confidence_demo.ko" --json
else
    test_cmd_skip "kodoc confidence-report --json" "confidence_demo.ko not found"
fi

# describe operates on compiled binaries, so we build first
if [[ -x "$TMPDIR/hello" ]]; then
    test_cmd "kodoc describe <binary> --json" \
        "$KODOC" describe "$TMPDIR/hello" --json
else
    test_cmd_skip "kodoc describe <binary> --json" "hello binary not available"
fi

if [[ -f "$EXAMPLES/confidence_demo.ko" ]]; then
    test_cmd "kodoc audit --json" \
        "$KODOC" audit "$EXAMPLES/confidence_demo.ko" --json
else
    test_cmd_skip "kodoc audit --json" "confidence_demo.ko not found"
fi

# ── Testing commands ─────────────────────────────────────────────────────────
echo ""
echo "${BOLD}── Testing commands ──${RESET}"

if [[ -f "$EXAMPLES/recoverable_contracts.ko" ]]; then
    test_cmd "kodoc test --contracts=runtime" \
        "$KODOC" test "$EXAMPLES/recoverable_contracts.ko" --contracts=runtime
else
    test_cmd_skip "kodoc test --contracts=runtime" "recoverable_contracts.ko not found"
fi

# ── Project scaffolding ──────────────────────────────────────────────────────
echo ""
echo "${BOLD}── Project scaffolding ──${RESET}"

INIT_DIR="$TMPDIR/kodo_init_test"
rm -rf "$INIT_DIR"
test_cmd "kodoc init /tmp/kodo_init_test" \
    "$KODOC" init "$INIT_DIR"

# ── LLVM backend (optional) ─────────────────────────────────────────────────
echo ""
echo "${BOLD}── LLVM backend (optional) ──${RESET}"

if command -v llc &>/dev/null; then
    test_cmd "kodoc build --backend=llvm" \
        "$KODOC" build "$EXAMPLES/hello.ko" --backend=llvm -o "$TMPDIR/hello_llvm"
else
    test_cmd_skip "kodoc build --backend=llvm" "llc not available"
fi

# ── Summary ──────────────────────────────────────────────────────────────────
echo ""
TOTAL=$((PASS + FAIL + SKIP))
echo "${BOLD}=== Summary ===${RESET}"
echo "  Total:   $TOTAL"
echo "  Passed:  $PASS"
echo "  Failed:  $FAIL"
echo "  Skipped: $SKIP"

if [[ $FAIL -gt 0 ]]; then
    echo ""
    echo "${RED}${BOLD}Failures:${RESET}"
    echo -e "$ERRORS"
    echo ""
    exit 1
else
    echo ""
    echo "${GREEN}${BOLD}ALL CLI COMMANDS VALIDATED SUCCESSFULLY${RESET}"
    exit 0
fi
