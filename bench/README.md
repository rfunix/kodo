# kodo-bench — Agent Coding Benchmark Suite

Measures AI agent success rate on realistic Kōdo coding tasks.

## Why

SWE-bench 2026 shows agents achieve 78% on verified benchmarks but only 23% on realistic tasks. kodo-bench measures whether Kōdo's contracts, structured errors, and fix patches actually close this gap.

## Structure

```
bench/
  tasks/          # Task definitions (JSON)
  solutions/      # Reference solutions (.ko)
  templates/      # Starter code for agents (.ko)
  run.sh          # Runner script
  results/        # Output directory (gitignored)
```

## Task Format

Each task is a JSON file:

```json
{
  "id": "001-fibonacci-contracts",
  "category": "contracts",
  "difficulty": "easy",
  "prompt": "Write a recursive fibonacci function with contracts...",
  "template": "templates/001.ko",
  "solution": "solutions/001.ko",
  "checks": {
    "compiles": true,
    "contracts_present": true,
    "tests_pass": true,
    "output_matches": "55"
  }
}
```

## Running

```bash
# Run all tasks with kodoc check
./bench/run.sh

# Results in bench/results/
```

## Categories

| Category | Tasks | What it measures |
|----------|-------|-----------------|
| contracts | 3 | Can agent write correct requires/ensures? |
| error-fix | 3 | Can agent fix type errors using structured messages? |
| ownership | 2 | Can agent handle own/ref/mut correctly? |
| collections | 1 | Can agent use List/Map/Set APIs? |
| concurrency | 1 | Can agent use channels + select? |

## Goal

Prove: "Agents achieve X% on Kōdo tasks" — the definitive proof of value.
