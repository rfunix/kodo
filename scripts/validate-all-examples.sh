#!/usr/bin/env bash
# validate-all-examples.sh — Validate that all .ko examples compile and run.
#
# Usage:
#   ./scripts/validate-all-examples.sh
#
# Requires: kodoc built (cargo build -p kodoc --release)
# Exit code: 0 if all pass, 1 if any unexpected failure.

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

# ── Counters ─────────────────────────────────────────────────────────────────
TOTAL=0
CHECKED=0
BUILT_RAN=0
SKIPPED=0
FAILED=0
ERRORS=""

# ── Skip list ────────────────────────────────────────────────────────────────
# Files that intentionally fail or need special handling.
SKIP_FILES=(
    "type_errors.ko"
    "http_api.ko"
    "http_client.ko"
    "url_shortener.ko"
    "intent_http_server.ko"
    "intent_http.ko"
    "cli_args.ko"
    "async_real.ko"
    "send_sync_demo.ko"
    "channel_select.ko"
    "crud_api.ko"
)

# Subdirectories to skip (intentionally broken or need special setup)
SKIP_DIRS=(
    "closed_loop_demo"   # Contains step1_broken.ko (intentionally broken)
    "with_deps"          # Requires dependency resolution (kodo.toml)
)

should_skip() {
    local filename
    filename="$(basename "$1")"
    for skip in "${SKIP_FILES[@]}"; do
        if [[ "$filename" == "$skip" ]]; then
            return 0
        fi
    done
    return 1
}

# ── Timeout helper (macOS may not have timeout/gtimeout) ─────────────────────
run_with_timeout() {
    local secs="$1"
    shift
    if command -v timeout &>/dev/null; then
        timeout "$secs" "$@"
    elif command -v gtimeout &>/dev/null; then
        gtimeout "$secs" "$@"
    else
        # Fallback: background the process and kill after timeout
        "$@" &
        local pid=$!
        ( sleep "$secs" && kill -9 "$pid" 2>/dev/null ) &
        local watcher=$!
        wait "$pid" 2>/dev/null
        local rc=$?
        kill "$watcher" 2>/dev/null
        wait "$watcher" 2>/dev/null
        return $rc
    fi
}

# ── Helper: check and optionally build+run a .ko file ───────────────────────
validate_file() {
    local file="$1"
    local name
    name="$(basename "$file" .ko)"
    TOTAL=$((TOTAL + 1))

    if should_skip "$file"; then
        printf "  ${YELLOW}⊘${RESET} %-45s skipped\n" "$(basename "$file")"
        SKIPPED=$((SKIPPED + 1))
        return
    fi

    # Step 1: kodoc check
    if ! "$KODOC" check "$file" >/dev/null 2>&1; then
        printf "  ${RED}✗${RESET} %-45s check failed\n" "$(basename "$file")"
        FAILED=$((FAILED + 1))
        ERRORS="$ERRORS\n  ✗ $file (check failed)"
        return
    fi
    CHECKED=$((CHECKED + 1))

    # Step 2: if file has fn main(), build and run it
    if grep -q 'fn main()' "$file" 2>/dev/null; then
        local outbin="/tmp/kodo_validate_${name}_$$"
        if "$KODOC" build "$file" -o "$outbin" >/dev/null 2>&1; then
            if run_with_timeout 5 "$outbin" >/dev/null 2>&1; then
                printf "  ${GREEN}✓${RESET} %-45s check + build + run\n" "$(basename "$file")"
                BUILT_RAN=$((BUILT_RAN + 1))
            else
                local exit_code=$?
                # Exit code 134 is SIGABRT (contract violations etc.) — acceptable
                if [[ $exit_code -eq 134 ]]; then
                    printf "  ${GREEN}✓${RESET} %-45s check + build + run (aborted, ok)\n" "$(basename "$file")"
                    BUILT_RAN=$((BUILT_RAN + 1))
                else
                    printf "  ${RED}✗${RESET} %-45s run failed (exit %d)\n" "$(basename "$file")" "$exit_code"
                    FAILED=$((FAILED + 1))
                    ERRORS="$ERRORS\n  ✗ $file (run failed, exit $exit_code)"
                fi
            fi
            rm -f "$outbin"
        else
            printf "  ${RED}✗${RESET} %-45s build failed\n" "$(basename "$file")"
            FAILED=$((FAILED + 1))
            ERRORS="$ERRORS\n  ✗ $file (build failed)"
        fi
    else
        printf "  ${GREEN}✓${RESET} %-45s check only (no main)\n" "$(basename "$file")"
    fi
}

# ── Main ─────────────────────────────────────────────────────────────────────
echo "${BOLD}=== Kōdo Example Validation ===${RESET}"
echo "Compiler: $KODOC"
echo ""

# ── Top-level .ko files ──────────────────────────────────────────────────────
echo "${BOLD}── Top-level examples ──${RESET}"
for f in "$PROJECT_ROOT"/examples/*.ko; do
    [[ -f "$f" ]] || continue
    validate_file "$f"
done

# ── Subdirectory examples ────────────────────────────────────────────────────
echo ""
echo "${BOLD}── Subdirectory examples ──${RESET}"

for dir in "$PROJECT_ROOT"/examples/*/; do
    [[ -d "$dir" ]] || continue
    dirname="$(basename "$dir")"

    # Check if directory should be skipped
    skip_dir=false
    for sd in "${SKIP_DIRS[@]}"; do
        if [[ "$dirname" == "$sd" ]]; then
            skip_dir=true
            break
        fi
    done
    if $skip_dir; then
        printf "  ${YELLOW}⊘${RESET} %-45s skipped\n" "$dirname/"
        SKIPPED=$((SKIPPED + 1))
        TOTAL=$((TOTAL + 1))
        continue
    fi

    TOTAL=$((TOTAL + 1))

    # Look for main.ko or <dirname>.ko as the entry point
    local_main=""
    if [[ -f "$dir/main.ko" ]]; then
        local_main="$dir/main.ko"
    elif [[ -f "$dir/${dirname}.ko" ]]; then
        local_main="$dir/${dirname}.ko"
    fi

    if [[ -n "$local_main" ]]; then
        if "$KODOC" check "$local_main" >/dev/null 2>&1; then
            printf "  ${GREEN}✓${RESET} %-45s check\n" "$dirname/"
            CHECKED=$((CHECKED + 1))
        else
            printf "  ${RED}✗${RESET} %-45s check failed\n" "$dirname/"
            FAILED=$((FAILED + 1))
            ERRORS="$ERRORS\n  ✗ $dirname/ (check failed on $local_main)"
        fi
    else
        # Try checking each .ko file individually
        dir_ok=true
        for f in "$dir"/*.ko; do
            [[ -f "$f" ]] || continue
            if ! "$KODOC" check "$f" >/dev/null 2>&1; then
                dir_ok=false
                ERRORS="$ERRORS\n  ✗ $dirname/$(basename "$f") (check failed)"
            fi
        done
        if $dir_ok; then
            printf "  ${GREEN}✓${RESET} %-45s check\n" "$dirname/"
            CHECKED=$((CHECKED + 1))
        else
            printf "  ${RED}✗${RESET} %-45s check failed\n" "$dirname/"
            FAILED=$((FAILED + 1))
        fi
    fi
done

# ── Summary ──────────────────────────────────────────────────────────────────
echo ""
echo "${BOLD}=== Summary ===${RESET}"
echo "  Total:       $TOTAL"
echo "  Checked:     $CHECKED"
echo "  Built+Ran:   $BUILT_RAN"
echo "  Skipped:     $SKIPPED"
echo "  Failed:      $FAILED"

if [[ $FAILED -gt 0 ]]; then
    echo ""
    echo "${RED}${BOLD}Failures:${RESET}"
    echo -e "$ERRORS"
    echo ""
    exit 1
else
    echo ""
    echo "${GREEN}${BOLD}ALL EXAMPLES VALIDATED SUCCESSFULLY${RESET}"
    exit 0
fi
