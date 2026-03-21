#!/usr/bin/env bash
# validate-everything.sh — Master validation script for the Kōdo compiler.
#
# Runs all validation phases in sequence and produces a final report.
#
# Usage:
#   ./scripts/validate-everything.sh
#
# Exit code: 0 if all pass, 1 if any phase fails.

set -uo pipefail

# ── Color setup ──────────────────────────────────────────────────────────────
if [[ -t 1 ]] && command -v tput &>/dev/null && [[ $(tput colors 2>/dev/null || echo 0) -ge 8 ]]; then
    GREEN=$(tput setaf 2)
    RED=$(tput setaf 1)
    BOLD=$(tput bold)
    RESET=$(tput sgr0)
else
    GREEN="" RED="" BOLD="" RESET=""
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# ── Phase tracking ───────────────────────────────────────────────────────────
PHASES=()
RESULTS=()
TOTAL_PHASES=0
FAILED_PHASES=0

# Run a phase and record the result.
# Usage: run_phase "Phase Name" command [args...]
run_phase() {
    local name="$1"
    shift
    TOTAL_PHASES=$((TOTAL_PHASES + 1))
    local phase_num=$TOTAL_PHASES

    echo ""
    echo "${BOLD}────────────────────────────────────────${RESET}"
    echo "${BOLD}  Phase $phase_num: $name${RESET}"
    echo "${BOLD}────────────────────────────────────────${RESET}"
    echo ""

    PHASES+=("$name")

    if "$@"; then
        RESULTS+=("pass")
        echo ""
        echo "  ${GREEN}✓ $name passed${RESET}"
    else
        RESULTS+=("fail")
        FAILED_PHASES=$((FAILED_PHASES + 1))
        echo ""
        echo "  ${RED}✗ $name FAILED${RESET}"
    fi
}

# ── Banner ───────────────────────────────────────────────────────────────────
echo "${BOLD}========================================${RESET}"
echo "${BOLD}  KŌDO COMPLETE VALIDATION${RESET}"
echo "${BOLD}========================================${RESET}"
echo ""
echo "Project: $PROJECT_ROOT"
echo "Date:    $(date '+%Y-%m-%d %H:%M:%S')"

# ── Phase 1: Build ───────────────────────────────────────────────────────────
run_phase "Build" \
    cargo build --workspace --release --manifest-path "$PROJECT_ROOT/Cargo.toml"

# ── Phase 2: Linters ────────────────────────────────────────────────────────
run_phase_linters() {
    local failed=0
    echo "Running cargo fmt --all -- --check..."
    if ! cargo fmt --all --manifest-path "$PROJECT_ROOT/Cargo.toml" -- --check 2>&1; then
        echo "  fmt check failed"
        failed=1
    fi
    echo ""
    echo "Running cargo clippy --workspace -- -D warnings..."
    if ! cargo clippy --workspace --manifest-path "$PROJECT_ROOT/Cargo.toml" -- -D warnings 2>&1; then
        echo "  clippy failed"
        failed=1
    fi
    return $failed
}
run_phase "Linters (fmt + clippy)" run_phase_linters

# ── Phase 3: Tests ───────────────────────────────────────────────────────────
run_phase "Unit & Integration Tests" \
    cargo test --workspace --manifest-path "$PROJECT_ROOT/Cargo.toml"

# ── Phase 4: UI Tests ───────────────────────────────────────────────────────
run_phase "UI Tests (kotest)" \
    cargo run -p kotest --manifest-path "$PROJECT_ROOT/Cargo.toml" -- "$PROJECT_ROOT/tests/ui/"

# ── Phase 5: Example Validation ──────────────────────────────────────────────
if [[ -x "$SCRIPT_DIR/validate-all-examples.sh" ]]; then
    run_phase "Example Validation" \
        "$SCRIPT_DIR/validate-all-examples.sh"
else
    echo ""
    echo "  WARNING: validate-all-examples.sh not found or not executable"
fi

# ── Phase 6: CLI Validation ─────────────────────────────────────────────────
if [[ -x "$SCRIPT_DIR/validate-cli.sh" ]]; then
    run_phase "CLI Validation" \
        "$SCRIPT_DIR/validate-cli.sh"
else
    echo ""
    echo "  WARNING: validate-cli.sh not found or not executable"
fi

# ── Phase 7: Documentation Examples ─────────────────────────────────────────
if [[ -x "$SCRIPT_DIR/validate-doc-examples.sh" ]]; then
    run_phase "Documentation Examples" \
        "$SCRIPT_DIR/validate-doc-examples.sh"
else
    echo ""
    echo "  Skipping: validate-doc-examples.sh not found"
    TOTAL_PHASES=$((TOTAL_PHASES + 1))
    PHASES+=("Documentation Examples")
    RESULTS+=("skip")
fi

# ── Final Report ─────────────────────────────────────────────────────────────
echo ""
echo ""
echo "${BOLD}========================================${RESET}"
echo "${BOLD}  KŌDO COMPLETE VALIDATION REPORT${RESET}"
echo "${BOLD}========================================${RESET}"
echo ""

for i in "${!PHASES[@]}"; do
    local_phase_num=$((i + 1))
    local_name="${PHASES[$i]}"
    local_result="${RESULTS[$i]}"

    # Format the phase line with dots
    local_label=$(printf "[%d/%d] %s " "$local_phase_num" "$TOTAL_PHASES" "$local_name")
    local_dots_needed=$((50 - ${#local_label}))
    if [[ $local_dots_needed -lt 3 ]]; then
        local_dots_needed=3
    fi
    local_dots=$(printf '%*s' "$local_dots_needed" '' | tr ' ' '.')

    case "$local_result" in
        pass)
            printf "%s %s ${GREEN}✓${RESET}\n" "$local_label" "$local_dots"
            ;;
        fail)
            printf "%s %s ${RED}✗${RESET}\n" "$local_label" "$local_dots"
            ;;
        skip)
            printf "%s %s ⊘ (skipped)\n" "$local_label" "$local_dots"
            ;;
    esac
done

echo ""
if [[ $FAILED_PHASES -eq 0 ]]; then
    echo "${GREEN}${BOLD}RESULT: ALL PASSED${RESET}"
    exit 0
else
    echo "${RED}${BOLD}RESULT: $FAILED_PHASES PHASE(S) FAILED${RESET}"
    exit 1
fi
