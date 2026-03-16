//! Hover information provider for the Kōdo LSP server.
//!
//! Provides type information, contracts, and confidence annotations
//! when the user hovers over functions, parameters, and variables.

use tower_lsp::lsp_types::Position;

use crate::utils::{
    format_annotation, format_expr, format_type_expr, infer_type_hint, line_col_to_offset,
    word_at_offset,
};

/// Finds the function at a given position in the source and returns
/// hover information including type, contracts, and annotations.
pub(crate) fn hover_at_position(source: &str, position: Position) -> Option<String> {
    let offset = line_col_to_offset(source, position.line, position.character)?;

    // Parse
    let module = kodo_parser::parse(source).ok()?;

    // Find function at offset
    #[allow(clippy::cast_possible_truncation)]
    let offset_u32 = offset as u32;
    for func in &module.functions {
        if func.span.start <= offset_u32 && offset_u32 <= func.span.end {
            use std::fmt::Write;

            // Check if the cursor is on a parameter or local variable first
            let word = word_at_offset(source, offset);
            if !word.is_empty() {
                // Check parameters
                for p in &func.params {
                    if p.name == word {
                        return Some(format!("**param {}**: {}", p.name, format_type_expr(&p.ty)));
                    }
                }
                // Check let bindings
                for stmt in &func.body.stmts {
                    if let kodo_ast::Stmt::Let {
                        name, ty, value, ..
                    } = stmt
                    {
                        if name == word {
                            let type_str = if let Some(ty) = ty {
                                format_type_expr(ty)
                            } else {
                                infer_type_hint(value)
                            };
                            return Some(format!("**let {name}**: {type_str}"));
                        }
                    }
                }
            }

            let mut info = format!("**fn {}**", func.name);

            // Add parameter types
            if !func.params.is_empty() {
                info.push_str("\n\nParameters:\n");
                for p in &func.params {
                    let _ = writeln!(info, "- `{}: {:?}`", p.name, p.ty);
                }
            }

            // Add return type
            let _ = write!(info, "\nReturns: `{:?}`", func.return_type);

            // Add contracts
            if !func.requires.is_empty() {
                info.push_str("\n\n**Contracts:**\n");
                for req in &func.requires {
                    let _ = writeln!(info, "- `requires {{ {} }}`", format_expr(req));
                }
            }
            if !func.ensures.is_empty() {
                for ens in &func.ensures {
                    let _ = writeln!(info, "- `ensures {{ {} }}`", format_expr(ens));
                }
            }

            // Add annotations
            for ann in &func.annotations {
                let _ = write!(info, "\n{}", format_annotation(ann));
            }

            return Some(info);
        }
    }

    None
}
