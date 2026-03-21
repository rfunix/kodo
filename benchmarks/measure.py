#!/usr/bin/env python3
"""
Benchmark Measurement Script — Kōdo vs Python vs TypeScript vs Rust vs Go

Measures:
  1. Token count (GPT-4 tokenizer via tiktoken)
  2. Lines of code (total, non-empty, non-comment)
  3. Compile-time safety score
  4. Machine-readability of errors score
  5. Agent-unique features
  6. Generates results.md with comparison tables
"""

import os
import json
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

BASE_DIR = Path(__file__).parent
KODO_DIR = BASE_DIR.parent / "examples" / "task_manager"

LANGUAGES = {
    "Kōdo": {
        "dir": KODO_DIR,
        "extensions": [".ko"],
        "comment_prefix": "//",
    },
    "Python": {
        "dir": BASE_DIR / "python" / "task_manager",
        "extensions": [".py"],
        "comment_prefix": "#",
    },
    "TypeScript": {
        "dir": BASE_DIR / "typescript" / "src",
        "extensions": [".ts"],
        "comment_prefix": "//",
    },
    "Rust": {
        "dir": BASE_DIR / "rust" / "src",
        "extensions": [".rs"],
        "comment_prefix": "//",
    },
    "Go": {
        "dir": BASE_DIR / "go",
        "extensions": [".go"],
        "comment_prefix": "//",
    },
}

# Compile-time safety: which bug classes are caught before runtime?
SAFETY_SCORES = {
    "Kōdo":       {"null": True,  "type": True,  "contract": True,  "transition": True,  "range": True,  "error": True,  "move": True},
    "Python":     {"null": False, "type": False, "contract": False, "transition": False, "range": False, "error": False, "move": False},
    "TypeScript": {"null": True,  "type": True,  "contract": False, "transition": False, "range": False, "error": False, "move": False},
    "Rust":       {"null": True,  "type": True,  "contract": False, "transition": False, "range": False, "error": True,  "move": True},
    "Go":         {"null": False, "type": True,  "contract": False, "transition": False, "range": False, "error": True,  "move": False},
}

# Machine-readability of errors (score 0-5)
ERROR_READABILITY = {
    "Kōdo":       {"json_parseable": 2, "exact_spans": 1, "suggests_fix": 1, "unique_code": 1},
    "Python":     {"json_parseable": 0, "exact_spans": 1, "suggests_fix": 0, "unique_code": 0},
    "TypeScript": {"json_parseable": 0, "exact_spans": 1, "suggests_fix": 0, "unique_code": 1},
    "Rust":       {"json_parseable": 0, "exact_spans": 1, "suggests_fix": 1, "unique_code": 1},
    "Go":         {"json_parseable": 0, "exact_spans": 1, "suggests_fix": 0, "unique_code": 0},
}

# Agent-unique features
AGENT_FEATURES = {
    "Self-describing modules (meta)":     {"Kōdo": True,  "Python": False, "TypeScript": False, "Rust": False, "Go": False},
    "Agent traceability annotations":     {"Kōdo": True,  "Python": False, "TypeScript": False, "Rust": False, "Go": False},
    "Formal contract verification (Z3)":  {"Kōdo": True,  "Python": False, "TypeScript": False, "Rust": False, "Go": False},
    "Refinement types":                   {"Kōdo": True,  "Python": False, "TypeScript": False, "Rust": False, "Go": False},
    "Intent-driven code generation":      {"Kōdo": True,  "Python": False, "TypeScript": False, "Rust": False, "Go": False},
    "Compilation certificates":           {"Kōdo": True,  "Python": False, "TypeScript": False, "Rust": False, "Go": False},
    "Machine-applicable fix patches":     {"Kōdo": True,  "Python": False, "TypeScript": False, "Rust": False, "Go": False},
    "Confidence propagation":             {"Kōdo": True,  "Python": False, "TypeScript": False, "Rust": False, "Go": False},
    "MCP server (native agent support)":  {"Kōdo": True,  "Python": False, "TypeScript": False, "Rust": False, "Go": False},
}


# ---------------------------------------------------------------------------
# Measurement Functions
# ---------------------------------------------------------------------------

def collect_source_files(lang_config: dict) -> list[Path]:
    """Collect all source files for a language."""
    files = []
    src_dir = lang_config["dir"]
    if not src_dir.exists():
        return files
    for ext in lang_config["extensions"]:
        files.extend(src_dir.rglob(f"*{ext}"))
    return sorted(files)


def read_all_source(files: list[Path]) -> str:
    """Read and concatenate all source files."""
    content = ""
    for f in files:
        content += f.read_text(encoding="utf-8") + "\n"
    return content


def count_loc(source: str, comment_prefix: str) -> dict:
    """Count lines of code."""
    lines = source.split("\n")
    total = len(lines)
    empty = sum(1 for l in lines if not l.strip())
    comments = sum(1 for l in lines if l.strip().startswith(comment_prefix))
    code = total - empty - comments
    return {
        "total": total,
        "empty": empty,
        "comments": comments,
        "code": code,
    }


def count_tokens(source: str) -> int:
    """Count tokens using tiktoken (GPT-4 tokenizer)."""
    try:
        import tiktoken
        enc = tiktoken.encoding_for_model("gpt-4")
        return len(enc.encode(source))
    except ImportError:
        # Fallback: approximate tokens as words / 0.75
        words = len(source.split())
        return int(words / 0.75)


def safety_score(lang: str) -> tuple[int, int]:
    """Return (caught, total) for compile-time safety."""
    scores = SAFETY_SCORES[lang]
    total = len(scores)
    caught = sum(1 for v in scores.values() if v)
    return caught, total


def error_score(lang: str) -> int:
    """Return machine-readability score (0-5)."""
    return sum(ERROR_READABILITY[lang].values())


def agent_feature_count(lang: str) -> int:
    """Count agent-unique features supported."""
    return sum(1 for feat in AGENT_FEATURES.values() if feat.get(lang, False))


# ---------------------------------------------------------------------------
# Report Generation
# ---------------------------------------------------------------------------

def generate_report(results: dict) -> str:
    """Generate markdown report."""
    langs = ["Kōdo", "Python", "TypeScript", "Rust", "Go"]

    report = """# Benchmark Results: Task Management API

> Full methodology and honest limitations: https://kodo-lang.dev/docs/reference/benchmarks/

## What this measures

This benchmark compares **compile-time safety** and **agent-specific features** — areas where
Kōdo was designed to excel. It does NOT measure runtime performance, ecosystem maturity, or
general-purpose productivity. All source code is open: https://github.com/rfunix/kodo/tree/main/benchmarks

## The project

Same Task Management REST API in all 5 languages: CRUD, priority validation (1-5),
status workflow (pending → in_progress → done), JSON API, persistence, tests.
All implementations are idiomatic for their language.

## Token Count (GPT-4 `cl100k_base` tokenizer)

| Metric | """ + " | ".join(langs) + """ |
|--------|""" + "|".join(["-----:" for _ in langs]) + """|
"""

    # Token count row
    row = "| **Tokens** |"
    for lang in langs:
        tokens = results[lang].get("tokens", "N/A")
        row += f" {tokens:,} |" if isinstance(tokens, int) else f" {tokens} |"
    report += row + "\n"

    # LOC rows
    for metric, key in [("Total Lines", "total"), ("Code Lines", "code"), ("Comments", "comments")]:
        row = f"| {metric} |"
        for lang in langs:
            val = results[lang].get("loc", {}).get(key, "N/A")
            row += f" {val:,} |" if isinstance(val, int) else f" {val} |"
        report += row + "\n"

    report += """
**Python wins on raw token count.** This is expected — Python is concise by design, and
FastAPI generates minimal boilerplate. We don't claim Kōdo is more concise than Python.

Kōdo's extra tokens include contracts, refinement types, agent traceability annotations, and
inline tests — features the other implementations lack because their languages don't support them.

"""
    kodo_tokens = results["Kōdo"].get("tokens", 0)
    if kodo_tokens > 0:
        kodo_safety, kodo_total_safety = safety_score("Kōdo")
        report += "**Token count vs. compile-time guarantees:**\n\n"
        report += f"| Language | Tokens | Bug Classes Caught (compile-time) |\n"
        report += f"|----------|-------:|----------------------------------:|\n"
        for lang in langs:
            other_tokens = results[lang].get("tokens", 0)
            other_safety, _ = safety_score(lang)
            if isinstance(other_tokens, int):
                report += f"| {lang} | {other_tokens:,} | {other_safety}/{kodo_total_safety} |\n"

    report += """
## Compile-Time Safety

How many classes of bugs are caught **before** the code runs?

| Bug Class | """ + " | ".join(langs) + """ |
|-----------|""" + "|".join([":----:" for _ in langs]) + """|
"""

    bug_classes = ["null", "type", "contract", "transition", "range", "error", "move"]
    bug_labels = {
        "null": "Null/None dereference",
        "type": "Type mismatch",
        "contract": "Contract violation",
        "transition": "Invalid status transition",
        "range": "Value out of range",
        "error": "Missing error handling",
        "move": "Use after move",
    }
    for bug in bug_classes:
        row = f"| {bug_labels[bug]} |"
        for lang in langs:
            val = SAFETY_SCORES[lang][bug]
            row += " ✅ |" if val else " ❌ |"
        report += row + "\n"

    # Safety totals
    row = "| **Total** |"
    for lang in langs:
        caught, total = safety_score(lang)
        row += f" **{caught}/{total}** |"
    report += row + "\n"

    report += """
## Machine-Readability of Errors

How easily can an AI agent parse and act on compiler/linter errors?

| Criterion | """ + " | ".join(langs) + """ |
|-----------|""" + "|".join([":----:" for _ in langs]) + """|
"""

    criteria = ["json_parseable", "exact_spans", "suggests_fix", "unique_code"]
    criteria_labels = {
        "json_parseable": "JSON parseable (+2)",
        "exact_spans": "Exact source spans (+1)",
        "suggests_fix": "Suggests fix (+1)",
        "unique_code": "Unique error code (+1)",
    }
    for c in criteria:
        row = f"| {criteria_labels[c]} |"
        for lang in langs:
            val = ERROR_READABILITY[lang][c]
            row += f" +{val} |" if val > 0 else " — |"
        report += row + "\n"

    row = "| **Total** |"
    for lang in langs:
        row += f" **{error_score(lang)}/5** |"
    report += row + "\n"

    report += """
## Agent-Unique Features

Features specifically designed for AI agent workflows — not available in general-purpose languages.

| Feature | """ + " | ".join(langs) + """ |
|---------|""" + "|".join([":----:" for _ in langs]) + """|
"""

    for feat, support in AGENT_FEATURES.items():
        row = f"| {feat} |"
        for lang in langs:
            row += " ✅ |" if support.get(lang, False) else " ❌ |"
        report += row + "\n"

    row = "| **Total** |"
    for lang in langs:
        row += f" **{agent_feature_count(lang)}/9** |"
    report += row + "\n"

    report += """
## Summary

| Dimension | Result | Notes |
|-----------|--------|-------|
| **Compile-Time Safety** | Kōdo 7/7 | Contracts and refinement types catch 3 bug classes no other language covers |
| **Error Machine-Readability** | Kōdo 5/5 | Only language with native JSON error output and byte-offset fix patches |
| **Agent-Specific Features** | Kōdo 9/9 | Features designed for agent workflows — other languages weren't built for this |
| **Raw Token Count** | Python smallest | Python is more concise — expected and not a flaw |
| **Ownership Safety** | Kōdo and Rust tied | Both enforce linear ownership; Rust's system is more mature |

### What this means

Kōdo's advantage is in **compile-time verification of business logic** (contracts, refinement
types, state machine transitions) and **agent-specific infrastructure** (traceability, confidence
scores, structured errors, certificates). These features don't exist in general-purpose languages
because those languages weren't designed for this use case.

This is not a claim that Kōdo is "better" than Python, Rust, Go, or TypeScript in any absolute
sense. Those are mature, battle-tested languages. Kōdo is purpose-built for a specific niche.

For full methodology, limitations, and honest discussion of scoring:
https://kodo-lang.dev/docs/reference/benchmarks/

---

*Generated by `benchmarks/measure.py` — Kōdo Language Benchmark Suite*
"""

    return report


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    print("📊 Kōdo Language Benchmark — Measuring...")
    print()

    results = {}

    for lang, config in LANGUAGES.items():
        print(f"  Measuring {lang}...")
        files = collect_source_files(config)

        if not files:
            print(f"    ⚠️  No source files found in {config['dir']}")
            results[lang] = {"tokens": "N/A", "loc": {"total": 0, "code": 0, "comments": 0, "empty": 0}, "files": 0}
            continue

        source = read_all_source(files)
        tokens = count_tokens(source)
        loc = count_loc(source, config["comment_prefix"])

        results[lang] = {
            "tokens": tokens,
            "loc": loc,
            "files": len(files),
        }

        print(f"    Files: {len(files)}")
        print(f"    Tokens: {tokens:,}")
        print(f"    LOC (code): {loc['code']:,}")

    print()
    print("  Generating report...")

    report = generate_report(results)
    output_path = BASE_DIR / "results.md"
    output_path.write_text(report, encoding="utf-8")
    print(f"  ✅ Report written to {output_path}")

    # Also write JSON results
    json_path = BASE_DIR / "results.json"
    json_results = {}
    for lang, data in results.items():
        json_results[lang] = {
            "tokens": data["tokens"],
            "loc": data["loc"],
            "files": data["files"],
            "safety_score": safety_score(lang)[0],
            "safety_total": safety_score(lang)[1],
            "error_readability": error_score(lang),
            "agent_features": agent_feature_count(lang),
        }
    json_path.write_text(json.dumps(json_results, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"  ✅ JSON data written to {json_path}")
    print()
    print("Done!")


if __name__ == "__main__":
    main()
