#!/usr/bin/env bash
# kodo-bench runner — validates solutions against expected output
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
KODOC="${KODOC:-$(dirname "$SCRIPT_DIR")/target/release/kodoc}"
RESULTS_DIR="$SCRIPT_DIR/results"

mkdir -p "$RESULTS_DIR"

pass=0
fail=0
skip=0
total=0

echo "=== kodo-bench ==="
echo "Compiler: $KODOC"
echo ""

for task_file in "$SCRIPT_DIR"/tasks/*.json; do
    total=$((total + 1))
    id=$(python3 -c "import json; print(json.load(open('$task_file'))['id'])")
    title=$(python3 -c "import json; print(json.load(open('$task_file'))['title'])")
    expected=$(python3 -c "import json; print(json.load(open('$task_file'))['checks'].get('output_matches', ''))")

    solution="$SCRIPT_DIR/solutions/$(echo "$id" | cut -d'-' -f1).ko"

    if [ ! -f "$solution" ]; then
        echo "SKIP  $id — $title (no solution file)"
        skip=$((skip + 1))
        continue
    fi

    # Step 1: Check if it compiles
    if ! "$KODOC" check "$solution" > /dev/null 2>&1; then
        echo "FAIL  $id — $title (compile error)"
        fail=$((fail + 1))
        continue
    fi

    # Step 2: Check contracts if required
    contracts_check=$(python3 -c "
import json
checks = json.load(open('$task_file'))['checks']
contracts = checks.get('contracts_present', [])
if contracts:
    print(' '.join(contracts))
else:
    print('')
" 2>/dev/null || echo "")

    if [ -n "$contracts_check" ]; then
        for keyword in $contracts_check; do
            if ! grep -q "$keyword" "$solution"; then
                echo "FAIL  $id — $title (missing contract: $keyword)"
                fail=$((fail + 1))
                continue 2
            fi
        done
    fi

    # Step 3: Build and run if output check needed
    if [ -n "$expected" ]; then
        tmpbin=$(mktemp /tmp/kodo-bench-XXXXXX)
        if "$KODOC" build "$solution" -o "$tmpbin" > /dev/null 2>&1; then
            actual=$(perl -e 'alarm 10; exec @ARGV' "$tmpbin" 2>/dev/null | tr -d '\n' || echo "TIMEOUT")
            rm -f "$tmpbin"
            if [ "$actual" = "$expected" ]; then
                echo "PASS  $id — $title"
                pass=$((pass + 1))
            else
                echo "FAIL  $id — $title (expected '$expected', got '$actual')"
                fail=$((fail + 1))
            fi
        else
            rm -f "$tmpbin"
            echo "FAIL  $id — $title (build error)"
            fail=$((fail + 1))
        fi
    else
        echo "PASS  $id — $title (compile-only)"
        pass=$((pass + 1))
    fi
done

echo ""
echo "=== Results ==="
echo "PASS: $pass"
echo "FAIL: $fail"
echo "SKIP: $skip"
echo "TOTAL: $total"

# Write JSON results
cat > "$RESULTS_DIR/latest.json" << EOF
{
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "compiler": "$($KODOC --version 2>/dev/null || echo 'unknown')",
  "pass": $pass,
  "fail": $fail,
  "skip": $skip,
  "total": $total,
  "pass_rate": $(python3 -c "print(round($pass/$total*100, 1) if $total > 0 else 0)")
}
EOF

echo ""
echo "Results written to $RESULTS_DIR/latest.json"

if [ $fail -gt 0 ]; then
    exit 1
fi
