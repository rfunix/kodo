//! Signature help provider for the Kōdo LSP server.
//!
//! Provides function signature information including parameter types,
//! active parameter tracking, and contract documentation when the
//! cursor is inside a function call's parentheses.

#[allow(clippy::wildcard_imports)]
use tower_lsp::lsp_types::*;

use crate::utils::{format_expr, format_type_expr, is_ident_char, line_col_to_offset};

/// Returns signature help for the function call at the given position.
pub(crate) fn signature_at_position(source: &str, position: Position) -> Option<SignatureHelp> {
    let offset = line_col_to_offset(source, position.line, position.character)?;

    // Walk backwards from cursor to find the function name before '('
    let bytes = source.as_bytes();
    let mut paren_pos = offset;
    let mut depth = 0i32;

    // Find the matching '(' by walking back
    while paren_pos > 0 {
        paren_pos -= 1;
        match bytes[paren_pos] {
            b')' => depth += 1,
            b'(' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            _ => {}
        }
    }

    if paren_pos == 0 && bytes[0] != b'(' {
        return None;
    }

    // Extract function name before '('
    let func_name = {
        let mut end = paren_pos;
        while end > 0 && bytes[end - 1] == b' ' {
            end -= 1;
        }
        let mut start = end;
        while start > 0 && is_ident_char(bytes[start - 1]) {
            start -= 1;
        }
        &source[start..end]
    };

    if func_name.is_empty() {
        return None;
    }

    // Count which parameter we're on (count commas at depth 0)
    let mut active_param = 0u32;
    let mut scan = paren_pos + 1;
    let mut scan_depth = 0i32;
    while scan < offset {
        match bytes[scan] {
            b'(' => scan_depth += 1,
            b')' => scan_depth -= 1,
            b',' if scan_depth == 0 => active_param += 1,
            _ => {}
        }
        scan += 1;
    }

    // Parse and find the function
    let module = kodo_parser::parse(source).ok()?;

    for func in &module.functions {
        if func.name == func_name {
            let params_str: Vec<String> = func
                .params
                .iter()
                .map(|p| format!("{}: {}", p.name, format_type_expr(&p.ty)))
                .collect();
            let ret_str = format_type_expr(&func.return_type);
            let label = format!("fn {}({}) -> {}", func.name, params_str.join(", "), ret_str);

            let param_infos: Vec<ParameterInformation> = func
                .params
                .iter()
                .map(|p| ParameterInformation {
                    label: ParameterLabel::Simple(format!(
                        "{}: {}",
                        p.name,
                        format_type_expr(&p.ty)
                    )),
                    documentation: None,
                })
                .collect();

            // Build documentation with contracts
            let mut doc_parts = Vec::new();
            for req in &func.requires {
                doc_parts.push(format!("requires {{ {} }}", format_expr(req)));
            }
            for ens in &func.ensures {
                doc_parts.push(format!("ensures {{ {} }}", format_expr(ens)));
            }
            let documentation = if doc_parts.is_empty() {
                None
            } else {
                Some(Documentation::String(doc_parts.join("\n")))
            };

            return Some(SignatureHelp {
                signatures: vec![SignatureInformation {
                    label,
                    documentation,
                    parameters: Some(param_infos),
                    active_parameter: Some(active_param),
                }],
                active_signature: Some(0),
                active_parameter: Some(active_param),
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Position;

    fn source_with_call() -> &'static str {
        r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn add(a: Int, b: Int) -> Int {
        return a + b
    }

    fn main() {
        let x: Int = add(1, 2)
    }
}"#
    }

    #[test]
    fn signature_help_inside_function_call() {
        let source = source_with_call();
        // Position after the opening paren of add(1, 2) on line 11
        let sig = signature_at_position(source, Position::new(11, 26));
        assert!(sig.is_some(), "should find signature help for add");
        let help = sig.unwrap();
        assert_eq!(help.signatures.len(), 1);
        assert!(help.signatures[0].label.contains("add"));
        assert!(help.signatures[0].label.contains("Int"));
        // Should have parameter info
        assert!(help.signatures[0].parameters.is_some());
        let params = help.signatures[0].parameters.as_ref().unwrap();
        assert_eq!(params.len(), 2, "add has 2 parameters");
    }

    #[test]
    fn signature_help_outside_call_returns_none() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn add(a: Int, b: Int) -> Int {
        return a + b
    }

    fn main() {
        let x: Int = 42
    }
}"#;
        // Position on `let x: Int = 42` — no parentheses
        let sig = signature_at_position(source, Position::new(11, 10));
        assert!(
            sig.is_none(),
            "should return None when cursor is not inside parentheses"
        );
    }

    #[test]
    fn active_parameter_tracking() {
        let source = source_with_call();
        // Position after first arg, before comma — active param should be 0
        let sig_first = signature_at_position(source, Position::new(11, 26));
        assert!(sig_first.is_some());
        assert_eq!(
            sig_first.unwrap().active_parameter,
            Some(0),
            "first parameter should be active"
        );

        // Position after the comma — active param should be 1
        let sig_second = signature_at_position(source, Position::new(11, 29));
        assert!(sig_second.is_some());
        assert_eq!(
            sig_second.unwrap().active_parameter,
            Some(1),
            "second parameter should be active after the comma"
        );
    }

    #[test]
    fn signature_help_includes_contract_documentation() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn divide(a: Int, b: Int) -> Int
        requires { b > 0 }
        ensures { result >= 0 }
    {
        return a / b
    }

    fn main() {
        let x: Int = divide(10, 2)
    }
}"#;
        // Position inside divide(10, 2)
        let sig = signature_at_position(source, Position::new(14, 28));
        assert!(sig.is_some(), "should find signature help for divide");
        let help = sig.unwrap();
        assert!(help.signatures[0].label.contains("divide"));
        assert!(
            help.signatures[0].documentation.is_some(),
            "signature help should include contract documentation"
        );
        let doc = match &help.signatures[0].documentation {
            Some(Documentation::String(s)) => s.clone(),
            _ => String::new(),
        };
        assert!(
            doc.contains("requires"),
            "documentation should mention requires, got: {doc}"
        );
        assert!(
            doc.contains("ensures"),
            "documentation should mention ensures, got: {doc}"
        );
    }

    #[test]
    fn signature_help_no_documentation_without_contracts() {
        let source = source_with_call();
        let sig = signature_at_position(source, Position::new(11, 26));
        assert!(sig.is_some());
        let help = sig.unwrap();
        assert!(
            help.signatures[0].documentation.is_none(),
            "function without contracts should have no documentation"
        );
    }

    #[test]
    fn signature_help_on_source_without_calls_returns_none() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }
}"#;
        // Position in the meta block — no function call
        let sig = signature_at_position(source, Position::new(2, 10));
        assert!(
            sig.is_none(),
            "should return None when no function call is nearby"
        );
    }
}
