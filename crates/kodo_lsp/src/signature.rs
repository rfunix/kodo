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
