//! Hover information provider for the Kōdo LSP server.
//!
//! Provides type information, contracts, and confidence annotations
//! when the user hovers over functions, parameters, and variables.

use tower_lsp::lsp_types::Position;

use crate::utils::{
    format_annotation, format_expr, format_type_expr, infer_type_hint, line_col_to_offset,
    word_at_offset,
};

/// Formats a full function signature as a Kōdo code block for hover display.
fn format_function_signature(func: &kodo_ast::Function) -> String {
    let params_str: Vec<String> = func
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, format_type_expr(&p.ty)))
        .collect();
    let ret = format_type_expr(&func.return_type);
    format!("fn {}({}) -> {}", func.name, params_str.join(", "), ret)
}

/// Builds a hover info string for a struct declaration.
fn format_struct_hover(type_decl: &kodo_ast::TypeDecl) -> String {
    use std::fmt::Write;
    let mut info = format!("```kodo\nstruct {}", type_decl.name);
    if !type_decl.fields.is_empty() {
        info.push_str(" {\n");
        for field in &type_decl.fields {
            let _ = writeln!(info, "    {}: {},", field.name, format_type_expr(&field.ty));
        }
        info.push('}');
    }
    info.push_str("\n```");
    info
}

/// Builds a hover info string for an enum declaration.
fn format_enum_hover(enum_decl: &kodo_ast::EnumDecl) -> String {
    use std::fmt::Write;
    let mut info = format!("```kodo\nenum {}", enum_decl.name);
    if !enum_decl.variants.is_empty() {
        info.push_str(" {\n");
        for variant in &enum_decl.variants {
            if variant.fields.is_empty() {
                let _ = writeln!(info, "    {},", variant.name);
            } else {
                let fields_str: Vec<String> = variant.fields.iter().map(format_type_expr).collect();
                let _ = writeln!(info, "    {}({}),", variant.name, fields_str.join(", "));
            }
        }
        info.push('}');
    }
    info.push_str("\n```");
    info
}

/// Finds the symbol at a given position in the source and returns
/// hover information including type, contracts, and annotations.
///
/// Supports hovering over:
/// - Function bodies (shows signature, params, contracts, annotations)
/// - Parameters (shows param name and type)
/// - Local variables (shows variable name and type)
/// - Function calls (shows callee signature and contracts)
/// - Struct names (shows struct definition with fields)
/// - Enum names (shows enum definition with variants)
pub(crate) fn hover_at_position(source: &str, position: Position) -> Option<String> {
    let offset = line_col_to_offset(source, position.line, position.character)?;

    // Parse
    let module = kodo_parser::parse(source).ok()?;

    #[allow(clippy::cast_possible_truncation)]
    let offset_u32 = offset as u32;

    // Check if the cursor is on a struct or enum name anywhere in the source
    let word = word_at_offset(source, offset);
    if !word.is_empty() {
        // Check struct declarations and usages
        for type_decl in &module.type_decls {
            if type_decl.name == word {
                return Some(format_struct_hover(type_decl));
            }
        }
        // Check enum declarations and usages
        for enum_decl in &module.enum_decls {
            if enum_decl.name == word {
                return Some(format_enum_hover(enum_decl));
            }
        }
    }

    // Find function at offset
    for func in &module.functions {
        if func.span.start <= offset_u32 && offset_u32 <= func.span.end {
            // Check if the cursor is on a parameter or local variable first
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

                // Check if cursor is on a function call — show callee's contracts
                if let Some(callee_info) = find_callee_hover(word, &module.functions, func) {
                    return Some(callee_info);
                }
            }

            // Default: show the enclosing function's full hover
            return Some(build_function_hover(func));
        }
    }

    None
}

/// Builds hover information for a function call site by looking up the callee.
///
/// When hovering over a function name at a call site, this shows the callee's
/// full signature including contracts and annotations, which helps agents
/// understand what preconditions they need to satisfy.
fn find_callee_hover(
    word: &str,
    all_functions: &[kodo_ast::Function],
    current_func: &kodo_ast::Function,
) -> Option<String> {
    // Don't match the current function's own name (let that fall through to function hover)
    if word == current_func.name {
        return None;
    }
    // Look for a matching function definition
    for func in all_functions {
        if func.name == word {
            return Some(build_function_hover(func));
        }
    }
    None
}

/// Builds the full hover display for a function, including signature,
/// contracts, and annotations formatted as a Kōdo code block.
fn build_function_hover(func: &kodo_ast::Function) -> String {
    use std::fmt::Write;

    let sig = format_function_signature(func);

    let mut info = format!("```kodo\n{sig}");

    // Add contracts in the code block
    for req in &func.requires {
        let _ = write!(info, "\n    requires {{ {} }}", format_expr(req));
    }
    for ens in &func.ensures {
        let _ = write!(info, "\n    ensures {{ {} }}", format_expr(ens));
    }
    info.push_str("\n```");

    // Add annotations below the code block
    if !func.annotations.is_empty() {
        info.push_str("\n\n---\n");
        for ann in &func.annotations {
            let _ = write!(info, "\n`{}`", format_annotation(ann));
        }
    }

    info
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
            info.contains("fn add(a: Int, b: Int) -> Int"),
            "hover should contain full function signature, got: {info}"
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

    #[test]
    fn hover_over_struct_name_shows_definition() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    struct Point {
        x: Int,
        y: Int
    }

    fn main() {
        let p: Point = Point { x: 1, y: 2 }
    }
}"#;
        // Position on "Point" in the struct declaration (line 6)
        let hover = hover_at_position(source, Position::new(6, 11));
        assert!(hover.is_some(), "should find hover for struct name");
        let info = hover.unwrap();
        assert!(
            info.contains("struct Point"),
            "hover should show struct definition, got: {info}"
        );
        assert!(
            info.contains("x: Int"),
            "hover should show struct fields, got: {info}"
        );
        assert!(
            info.contains("y: Int"),
            "hover should show all struct fields, got: {info}"
        );
    }

    #[test]
    fn hover_over_enum_name_shows_definition() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    enum Color {
        Red,
        Green,
        Blue
    }

    fn main() {
        let c: Color = Color::Red
    }
}"#;
        // Position on "Color" in the enum declaration (line 6)
        let hover = hover_at_position(source, Position::new(6, 9));
        assert!(hover.is_some(), "should find hover for enum name");
        let info = hover.unwrap();
        assert!(
            info.contains("enum Color"),
            "hover should show enum definition, got: {info}"
        );
        assert!(
            info.contains("Red"),
            "hover should show enum variants, got: {info}"
        );
    }

    #[test]
    fn hover_over_function_call_shows_callee_contracts() {
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
        // Position on "divide" in the call at line 14
        let hover = hover_at_position(source, Position::new(14, 21));
        assert!(hover.is_some(), "should find hover for function call");
        let info = hover.unwrap();
        assert!(
            info.contains("fn divide(a: Int, b: Int) -> Int"),
            "hover should show callee signature, got: {info}"
        );
        assert!(
            info.contains("requires { b > 0 }"),
            "hover should show callee contracts, got: {info}"
        );
        assert!(
            info.contains("ensures { result >= 0 }"),
            "hover should show callee ensures, got: {info}"
        );
    }

    #[test]
    fn snapshot_hover_struct_definition() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    struct Person {
        name: String,
        age: Int,
        active: Bool
    }

    fn main() {
        let p: Person = Person { name: "Alice", age: 30, active: true }
    }
}"#;
        let hover = hover_at_position(source, Position::new(6, 11));
        assert!(hover.is_some());
        insta::assert_snapshot!(hover.unwrap());
    }

    #[test]
    fn snapshot_hover_enum_definition() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    enum Shape {
        Circle(Float64),
        Rectangle(Float64, Float64),
        Point
    }

    fn main() {
        let s: Shape = Shape::Point
    }
}"#;
        let hover = hover_at_position(source, Position::new(6, 9));
        assert!(hover.is_some());
        insta::assert_snapshot!(hover.unwrap());
    }
}
