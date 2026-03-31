#!/usr/bin/env bash
# kodo-vericoding-bench runner
# Formally verified algorithm benchmark (AlgoVeri-equivalent for Kōdo)
# Usage: ./bench/vericoding/run.sh [--verbose] [--json]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
KODOC="${KODOC:-$(dirname "$SCRIPT_DIR")/../target/release/kodoc}"
RESULTS_DIR="$SCRIPT_DIR/results"
VERBOSE=0
JSON=0

for arg in "$@"; do
  case "$arg" in
    --verbose) VERBOSE=1 ;;
    --json)    JSON=1 ;;
  esac
done

mkdir -p "$RESULTS_DIR"

pass=0
fail=0
total=0
declare -a results_json=()

if [ "$JSON" -eq 0 ]; then
  echo "=== Kōdo Vericoding Benchmark (AlgoVeri-equivalent) ==="
  echo "Compiler: $KODOC"
  echo ""
fi

for task_file in "$SCRIPT_DIR"/tasks/v*.json; do
  id=$(python3 -c "import json,sys; d=json.load(open('$task_file')); print(d['id'])" 2>/dev/null || basename "$task_file" .json)
  category=$(python3 -c "import json,sys; d=json.load(open('$task_file')); print(d.get('category','?'))" 2>/dev/null || echo "?")
  title=$(python3 -c "import json,sys; d=json.load(open('$task_file')); print(d.get('title','?'))" 2>/dev/null || echo "?")
  expected=$(python3 -c "import json,sys; d=json.load(open('$task_file')); print(d['checks'].get('output_matches',''))" 2>/dev/null || echo "")
  sol_file="$SCRIPT_DIR/solutions/${id#v}.ko"
  # map e.g. v001-abs-value -> v001.ko
  num=$(echo "$id" | grep -oE '^v[0-9]+')
  sol_file="$SCRIPT_DIR/solutions/${num}.ko"

  total=$((total + 1))
  bin="/tmp/kodo_vbench_${num}"

  compile_ok=0
  run_ok=0
  output_ok=0
  actual_out=""

  if "$KODOC" build "$sol_file" -o "$bin" >/dev/null 2>&1; then
    compile_ok=1
    actual_out=$("$bin" 2>&1) && run_ok=1 || run_ok=0
    if [ "$run_ok" -eq 1 ] && [ "$actual_out" = "$expected" ]; then
      output_ok=1
    fi
  fi

  if [ "$compile_ok" -eq 1 ] && [ "$run_ok" -eq 1 ] && [ "$output_ok" -eq 1 ]; then
    pass=$((pass + 1))
    status="PASS"
  else
    fail=$((fail + 1))
    status="FAIL"
    if [ "$compile_ok" -eq 0 ]; then status="FAIL(compile)"; fi
    if [ "$compile_ok" -eq 1 ] && [ "$run_ok" -eq 0 ]; then status="FAIL(runtime)"; fi
    if [ "$compile_ok" -eq 1 ] && [ "$run_ok" -eq 1 ] && [ "$output_ok" -eq 0 ]; then status="FAIL(output)"; fi
  fi

  if [ "$JSON" -eq 0 ]; then
    printf "  [%-12s] %-35s %s\n" "$category" "$title" "$status"
    if [ "$VERBOSE" -eq 1 ] && [ "$status" != "PASS" ]; then
      echo "    expected: $(echo "$expected" | tr '\n' '|')"
      echo "    actual:   $(echo "$actual_out" | tr '\n' '|')"
    fi
  fi

  results_json+=("{\"id\":\"$id\",\"category\":\"$category\",\"status\":\"$status\"}")
done

rate=$(( pass * 100 / total ))

if [ "$JSON" -eq 1 ]; then
  echo "{"
  echo "  \"benchmark\": \"kodo-vericoding\","
  echo "  \"total\": $total,"
  echo "  \"passed\": $pass,"
  echo "  \"failed\": $fail,"
  echo "  \"success_rate_pct\": $rate,"
  echo "  \"results\": ["
  for i in "${!results_json[@]}"; do
    if [ $i -lt $(( ${#results_json[@]} - 1 )) ]; then
      echo "    ${results_json[$i]},"
    else
      echo "    ${results_json[$i]}"
    fi
  done
  echo "  ]"
  echo "}"
else
  echo ""
  echo "=== Results ==="
  echo "  Total:   $total"
  echo "  Passed:  $pass"
  echo "  Failed:  $fail"
  echo "  Success: ${rate}%"
  echo ""
  if [ "$fail" -eq 0 ]; then
    echo "  All vericoding tasks verified."
  else
    echo "  Some tasks failed — run with --verbose for details."
  fi
fi

exit $fail
