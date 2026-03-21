#!/bin/bash
# Test LLVM backend against all examples.
# Usage: ./scripts/test-llvm-backend.sh
# Requires: llc in PATH (install LLVM via brew install llvm)

set -e

# Ensure llc is available
export PATH="/opt/homebrew/opt/llvm/bin:$PATH"
if ! command -v llc &> /dev/null; then
    echo "ERROR: llc not found. Install LLVM: brew install llvm"
    exit 1
fi

# Build kodoc first
echo "Building kodoc..."
cargo build -p kodoc --quiet 2>/dev/null

PASS=0
FAIL=0
SKIP=0
FAILURES=""

# All examples sorted by complexity
EXAMPLES=(
    hello
    fibonacci
    while_loop
    variables
    string_ops
    enums
    enum_params
    option_demo
    result_demo
    result_patterns
    closures
    try_operator
    testing
    green_threads
    async_await
    self_hosted_lexer/main
    self_hosted_parser/main
)

for ex in "${EXAMPLES[@]}"; do
    KO_FILE="examples/${ex}.ko"
    LL_FILE="examples/${ex}.ll"
    OBJ_FILE="/tmp/kodo_llvm_test_$(echo $ex | tr '/' '_').o"

    if [ ! -f "$KO_FILE" ]; then
        echo "  SKIP  $ex (file not found)"
        ((SKIP++))
        continue
    fi

    # Step 1: Generate LLVM IR
    if ! cargo run -p kodoc --quiet -- build "$KO_FILE" --backend=llvm --emit-llvm 2>/dev/null; then
        echo "  FAIL  $ex (IR generation failed)"
        FAILURES="$FAILURES\n  $ex: IR generation failed"
        ((FAIL++))
        continue
    fi

    # Step 2: Validate with llc
    LLC_OUTPUT=$(llc -filetype=obj "$LL_FILE" -o "$OBJ_FILE" 2>&1)
    if [ $? -eq 0 ]; then
        echo "  PASS  $ex"
        ((PASS++))
    else
        FIRST_ERROR=$(echo "$LLC_OUTPUT" | head -1)
        echo "  FAIL  $ex"
        echo "        $FIRST_ERROR"
        FAILURES="$FAILURES\n  $ex: $FIRST_ERROR"
        ((FAIL++))
    fi

    # Cleanup
    rm -f "$LL_FILE" "$OBJ_FILE"
done

echo ""
echo "================================"
echo "LLVM Backend Test Results"
echo "================================"
echo "  Passed:  $PASS"
echo "  Failed:  $FAIL"
echo "  Skipped: $SKIP"
echo "  Total:   $((PASS + FAIL + SKIP))"

if [ $FAIL -gt 0 ]; then
    echo ""
    echo "Failures:"
    echo -e "$FAILURES"
    exit 1
fi

echo ""
echo "All examples passed!"
exit 0
