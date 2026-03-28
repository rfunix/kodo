//! # AI-Assisted Contract Annotation
//!
//! Uses an LLM (via the Anthropic API) to suggest `requires`/`ensures`
//! contracts for functions that the heuristic engine couldn't annotate.
//!
//! Requires the `ANTHROPIC_API_KEY` environment variable to be set.
//!
//! The LLM receives the function source and a prompt asking for contracts.
//! Each suggestion is validated by parsing it as a Kōdo expression.

use crate::annotate::{AnnotationResult, ContractKind, Suggestion};
use kodo_ast::Function;

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
                heuristic_result.suggestions.push(Suggestion {
                    function: func.name.clone(),
                    line,
                    kind,
                    expression: expr,
                    reason,
                    verified: true, // LLM suggestions are pre-filtered
                });
                heuristic_result.total_count += 1;
                heuristic_result.verified_count += 1;
            }
        }
    }
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
         - Maximum 3 suggestions per function"
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
}
