//! # AI-Assisted Contract Annotation
//!
//! Uses an LLM (via the Anthropic API) to suggest `requires`/`ensures`
//! contracts for functions that the heuristic engine couldn't annotate.
//!
//! Requires the `ANTHROPIC_API_KEY` environment variable to be set.
//!
//! The LLM receives the function source and a prompt asking for contracts.
//! Each suggestion is filtered for quality (tautologies, complexity) and
//! marked as `verified: false` since they are not Z3-verified.

use crate::annotate::{AnnotationResult, ContractKind, Suggestion};
use kodo_ast::Function;

/// Maximum allowed length for an AI-suggested contract expression.
/// Expressions longer than this are rejected as too complex.
const MAX_EXPRESSION_LENGTH: usize = 80;

/// Patterns that are always true and thus useless as contracts.
const TAUTOLOGY_PATTERNS: &[&str] = &[
    ".length() >= 0",
    ".length() > -1",
    ".size() >= 0",
    ">= 0 && ", // catch `x >= 0 && x <= max_uint` style tautologies on unsigned-like values
];

/// Enhance annotation results with LLM-suggested contracts.
///
/// For each function that has no suggestions from heuristics, asks the LLM
/// to suggest contracts. Appends AI suggestions to the result.
pub fn enhance_with_ai(
    module: &kodo_ast::Module,
    source: &str,
    heuristic_result: &mut AnnotationResult,
) {
    let api_key = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(key) if !key.is_empty() => key,
        _ => {
            eprintln!(
                "note: --ai requires ANTHROPIC_API_KEY environment variable. \
                 Falling back to heuristics only."
            );
            return;
        }
    };

    // Collect function names that already have heuristic suggestions
    let covered: std::collections::HashSet<String> = heuristic_result
        .suggestions
        .iter()
        .map(|s| s.function.clone())
        .collect();

    for func in &module.functions {
        if func.name == "main" || covered.contains(&func.name) {
            continue;
        }
        // Skip functions that already have contracts
        if !func.requires.is_empty() || !func.ensures.is_empty() {
            continue;
        }

        if let Some(suggestions) = ask_llm_for_contracts(func, source, &api_key) {
            let line = line_of(source, func.span.start);
            for (kind, expr, reason) in suggestions {
                // Filter out tautologies and overly complex expressions
                if is_tautology(&expr) {
                    continue;
                }
                if expr.len() > MAX_EXPRESSION_LENGTH {
                    continue;
                }
                heuristic_result.suggestions.push(Suggestion {
                    function: func.name.clone(),
                    line,
                    kind,
                    expression: expr,
                    reason,
                    verified: false, // AI suggestions are NOT Z3-verified
                });
                heuristic_result.total_count += 1;
                // Do NOT increment verified_count — these are unverified
            }
        }
    }
}

/// Check if a contract expression is a tautology (always true).
///
/// Filters out expressions like `source.length() >= 0` or `len >= 0` when
/// the value is inherently non-negative.
fn is_tautology(expr: &str) -> bool {
    let normalized = expr.replace(' ', "");

    // Check known tautology patterns
    for pattern in TAUTOLOGY_PATTERNS {
        let norm_pattern = pattern.replace(' ', "");
        if normalized.contains(&norm_pattern) {
            return true;
        }
    }

    // `x.length() >= 0` or `x.length() > -1` — length is always non-negative
    if normalized.contains(".length()>=0") || normalized.contains(".size()>=0") {
        return true;
    }

    // Standalone `param >= 0` where param name suggests non-negative semantics
    // (len, length, size, count, index, pos, offset)
    let non_negative_names = [
        "len", "length", "size", "count", "pos", "offset", "idx", "index",
    ];
    for name in &non_negative_names {
        if normalized == format!("{name}>=0") || normalized == format!("{name}>-1") {
            return true;
        }
    }

    false
}

/// Compute 1-based line number from byte offset.
fn line_of(source: &str, byte_offset: u32) -> usize {
    source[..byte_offset as usize]
        .bytes()
        .filter(|&b| b == b'\n')
        .count()
        + 1
}

/// Ask the LLM to suggest contracts for a single function.
///
/// Returns a list of `(kind, expression, reason)` tuples, or None on failure.
fn ask_llm_for_contracts(
    func: &Function,
    source: &str,
    api_key: &str,
) -> Option<Vec<(ContractKind, String, String)>> {
    // Extract function source text from the module
    let start = func.span.start as usize;
    let end = func.span.end as usize;
    let func_source = &source[start.min(source.len())..end.min(source.len())];

    let prompt = format!(
        "You are a Kōdo language expert. Analyze this function and suggest \
         `requires` (preconditions) and `ensures` (postconditions) contracts.\n\n\
         Function:\n```\n{func_source}\n```\n\n\
         Respond with ONLY a JSON array of suggestions. Each suggestion:\n\
         {{\"kind\": \"requires\" or \"ensures\", \"expression\": \"<kodo expr>\", \"reason\": \"<why>\"}}\n\n\
         Rules:\n\
         - Only suggest contracts that are provably correct from the function body\n\
         - Use Kōdo syntax: `param > 0`, `result >= 0`, `param != 0`\n\
         - `result` refers to the return value in ensures clauses\n\
         - If no contracts are needed, return an empty array []\n\
         - Maximum 3 suggestions per function\n\
         - Keep expressions SHORT and SIMPLE (under 80 chars)\n\
         - Do NOT suggest tautologies (things that are always true, e.g. `.length() >= 0`)\n\
         - Do NOT enumerate every possible return value — use range constraints instead\n\
         - Prefer meaningful bounds (e.g. `result >= 1 && result <= 30`) over exhaustive lists\n\
         - Do NOT suggest `requires {{ param >= 0 }}` for lengths or sizes — they are always non-negative"
    );

    let body = serde_json::json!({
        "model": "claude-sonnet-4-20250514",
        "max_tokens": 512,
        "messages": [{
            "role": "user",
            "content": prompt
        }]
    });

    // Call API via std::process::Command (curl)
    let output = std::process::Command::new("curl")
        .args([
            "-s",
            "-X",
            "POST",
            "https://api.anthropic.com/v1/messages",
            "-H",
            &format!("x-api-key: {api_key}"),
            "-H",
            "anthropic-version: 2023-06-01",
            "-H",
            "content-type: application/json",
            "-d",
            &body.to_string(),
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let response: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;

    // Extract text from response
    let text = response["content"]
        .as_array()?
        .first()?
        .get("text")?
        .as_str()?;

    // Find JSON array in the response
    let json_start = text.find('[')?;
    let json_end = text.rfind(']')? + 1;
    let json_str = &text[json_start..json_end];

    let suggestions: Vec<serde_json::Value> = serde_json::from_str(json_str).ok()?;

    let mut result = Vec::new();
    for s in suggestions.iter().take(3) {
        let kind_str = s.get("kind")?.as_str()?;
        let expr = s.get("expression")?.as_str()?.to_string();
        let reason = s
            .get("reason")
            .and_then(|r| r.as_str())
            .unwrap_or("suggested by AI")
            .to_string();

        let kind = match kind_str {
            "requires" => ContractKind::Requires,
            "ensures" => ContractKind::Ensures,
            _ => continue,
        };

        result.push((kind, expr, format!("AI: {reason}")));
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_of_computes_correctly() {
        let source = "line1\nline2\nline3\n";
        assert_eq!(line_of(source, 0), 1);
        assert_eq!(line_of(source, 6), 2);
        assert_eq!(line_of(source, 12), 3);
    }

    #[test]
    fn enhance_skips_without_api_key() {
        // Without ANTHROPIC_API_KEY, enhance_with_ai should be a no-op
        std::env::remove_var("ANTHROPIC_API_KEY");
        let module = kodo_parser::parse(
            "module test {\n    meta { purpose: \"test\" }\n    fn add(a: Int, b: Int) -> Int {\n        return a + b\n    }\n}",
        )
        .unwrap();
        let source = "module test {\n    meta { purpose: \"test\" }\n    fn add(a: Int, b: Int) -> Int {\n        return a + b\n    }\n}";
        let mut result = AnnotationResult {
            suggestions: vec![],
            verified_count: 0,
            total_count: 0,
        };
        enhance_with_ai(&module, source, &mut result);
        // Should remain empty (no API key)
        assert!(result.suggestions.is_empty());
    }

    #[test]
    fn tautology_length_gte_zero() {
        assert!(is_tautology("source.length() >= 0"));
        assert!(is_tautology("s.length() >= 0"));
        assert!(is_tautology("items.size() >= 0"));
    }

    #[test]
    fn tautology_non_negative_names() {
        assert!(is_tautology("len >= 0"));
        assert!(is_tautology("length >= 0"));
        assert!(is_tautology("size >= 0"));
        assert!(is_tautology("count >= 0"));
        assert!(is_tautology("pos >= 0"));
        assert!(is_tautology("offset >= 0"));
        assert!(is_tautology("idx >= 0"));
        assert!(is_tautology("index >= 0"));
    }

    #[test]
    fn non_tautology_passes() {
        assert!(!is_tautology("x > 0"));
        assert!(!is_tautology("amount >= 1"));
        assert!(!is_tautology("b != 0"));
        assert!(!is_tautology("pos < len"));
        assert!(!is_tautology("result >= 0"));
    }

    #[test]
    fn max_expression_length_filter() {
        let short = "x > 0";
        let long = "result == 1 || result == 2 || result == 3 || result == 4 || result == 5 || result == 6 || result == 7";
        assert!(short.len() <= MAX_EXPRESSION_LENGTH);
        assert!(long.len() > MAX_EXPRESSION_LENGTH);
    }
}
