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

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Position;

    /// Helper source with a function containing parameters and a let binding.
    fn sample_source() -> &'static str {
        r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn add(a: Int, b: Int) -> Int {
        let result: Int = a + b
        return result
    }
}"#
    }

    #[test]
    fn hover_over_function_name_shows_signature() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn add(a: Int, b: Int) -> Int {
        return a + b
    }
}"#;
        // Position on "return" keyword in the function body (not a param or variable name)
        // which falls through to the function-level hover
        let hover = hover_at_position(source, Position::new(7, 10));
        assert!(
            hover.is_some(),
            "should find hover info inside function body"
        );
        let info = hover.unwrap();
        assert!(
            info.contains("fn add"),
            "hover should contain function name, got: {info}"
        );
        assert!(
            info.contains("Returns:"),
            "hover should contain return type, got: {info}"
        );
    }

    #[test]
    fn hover_over_parameter_shows_type() {
        let source = sample_source();
        // Position on "a" in "let result: Int = a + b" (line 7, col ~26)
        let hover = hover_at_position(source, Position::new(7, 26));
        assert!(hover.is_some(), "should find hover info for parameter");
        let info = hover.unwrap();
        assert!(
            info.contains("**param a**"),
            "hover should show param info, got: {info}"
        );
        assert!(
            info.contains("Int"),
            "hover should show param type, got: {info}"
        );
    }

    #[test]
    fn hover_over_variable_shows_type() {
        let source = sample_source();
        // Position on "result" in "let result: Int = a + b" (line 7)
        let hover = hover_at_position(source, Position::new(7, 12));
        assert!(hover.is_some(), "should find hover info for variable");
        let info = hover.unwrap();
        assert!(
            info.contains("**let result**"),
            "hover should show let variable info, got: {info}"
        );
        assert!(
            info.contains("Int"),
            "hover should show variable type, got: {info}"
        );
    }

    #[test]
    fn hover_at_empty_position_returns_none() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }
}"#;
        // Position in the meta block — outside any function
        let hover = hover_at_position(source, Position::new(0, 0));
        assert!(
            hover.is_none(),
            "hover outside functions should return None"
        );
    }

    #[test]
    fn hover_shows_contracts_when_present() {
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
}"#;
        // Position in the function body (not on a param name)
        let hover = hover_at_position(source, Position::new(10, 17));
        assert!(hover.is_some(), "should find hover info");
        let info = hover.unwrap();
        assert!(
            info.contains("requires"),
            "hover should contain contract info, got: {info}"
        );
    }

    #[test]
    fn hover_shows_annotations() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    @confidence(0.9)
    fn process(x: Int) -> Int {
        return x
    }
}"#;
        // Position inside the function body
        let hover = hover_at_position(source, Position::new(8, 10));
        assert!(hover.is_some(), "should find hover info");
        let info = hover.unwrap();
        assert!(
            info.contains("@confidence"),
            "hover should show annotation, got: {info}"
        );
    }

    #[test]
    fn snapshot_hover_function_with_contracts_and_annotations() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    @confidence(0.95)
    @authored_by(agent: "claude")
    fn divide(a: Int, b: Int) -> Int
        requires { b > 0 }
        ensures { result >= 0 }
    {
        return a / b
    }
}"#;
        // Position on "return" in the function body — triggers function-level hover
        let hover = hover_at_position(source, Position::new(13, 10));
        assert!(hover.is_some());
        insta::assert_snapshot!(hover.unwrap());
    }

    #[test]
    fn snapshot_hover_param_type() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn process(name: String, count: Int) -> Int {
        return count
    }
}"#;
        // Hover over "count" in "return count"
        let hover = hover_at_position(source, Position::new(7, 15));
        assert!(hover.is_some());
        insta::assert_snapshot!(hover.unwrap());
    }

    #[test]
    fn hover_infers_untyped_variable() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn main() {
        let x = 42
        return x
    }
}"#;
        // Position on "x" in the let binding
        let hover = hover_at_position(source, Position::new(7, 12));
        assert!(
            hover.is_some(),
            "should find hover info for untyped variable"
        );
        let info = hover.unwrap();
        assert!(
            info.contains("**let x**"),
            "hover should show let variable info, got: {info}"
        );
        assert!(
            info.contains("Int"),
            "hover should infer Int type, got: {info}"
        );
    }
}
