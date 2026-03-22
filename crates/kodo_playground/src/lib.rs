//! WebAssembly playground for the Kōdo compiler.
//!
//! Exposes the compiler frontend (lexer, parser, type checker) as WASM
//! functions that can be called from JavaScript in the browser.

use serde::Serialize;
use wasm_bindgen::prelude::*;

/// Result of checking Kōdo source code.
#[derive(Serialize)]
struct CheckResult {
    /// Whether the code passed all checks without errors.
    success: bool,
    /// The module name (if parsing succeeded).
    module_name: Option<String>,
    /// Function signatures found in the module.
    functions: Vec<FunctionInfo>,
    /// Diagnostic messages (errors and warnings).
    diagnostics: Vec<Diagnostic>,
}

/// Information about a function in the module.
#[derive(Serialize)]
struct FunctionInfo {
    /// Function name.
    name: String,
    /// Parameter names and types.
    params: Vec<ParamInfo>,
    /// Return type as string.
    return_type: String,
    /// Number of requires clauses.
    requires_count: usize,
    /// Number of ensures clauses.
    ensures_count: usize,
}

/// Parameter name and type.
#[derive(Serialize)]
struct ParamInfo {
    /// Parameter name.
    name: String,
    /// Type as string.
    ty: String,
}

/// A diagnostic message from the compiler.
#[derive(Serialize)]
struct Diagnostic {
    /// Error code (e.g., "E0201").
    code: String,
    /// Severity: "error" or "warning".
    severity: String,
    /// Human-readable message.
    message: String,
    /// Start byte offset in the source.
    start: u32,
    /// End byte offset in the source.
    end: u32,
    /// Line number (0-based).
    line: u32,
    /// Column number (0-based).
    col: u32,
    /// Optional fix suggestion.
    suggestion: Option<String>,
}

/// Checks Kōdo source code and returns diagnostics as JSON.
///
/// Runs the full frontend pipeline: lexing, parsing, and type checking.
#[wasm_bindgen]
pub fn check(source: &str) -> String {
    let result = run_check(source);
    serde_json::to_string(&result).unwrap_or_else(|_| {
        r#"{"success":false,"module_name":null,"functions":[],"diagnostics":[]}"#.to_string()
    })
}

/// Internal check implementation.
fn run_check(source: &str) -> CheckResult {
    // Step 1: Parse with error recovery
    let output = kodo_parser::parse_with_recovery(source);

    // Collect parse errors
    let mut diagnostics: Vec<Diagnostic> = output
        .errors
        .iter()
        .map(|e| {
            let span = e.span().unwrap_or(kodo_ast::Span::new(0, 0));
            let (line, col) = offset_to_line_col(source, span.start);
            Diagnostic {
                code: e.code().to_string(),
                severity: "error".to_string(),
                message: e.to_string(),
                start: span.start,
                end: span.end,
                line,
                col,
                suggestion: None,
            }
        })
        .collect();

    let module = &output.module;

    // Step 2: Type check (even if parse had errors — recovery gives partial AST)
    let mut checker = kodo_types::TypeChecker::new();
    let type_errors = checker.check_module_collecting(module);

    for e in &type_errors {
        let span = kodo_ast::Diagnostic::span(e).unwrap_or(kodo_ast::Span::new(0, 0));
        let (line, col) = offset_to_line_col(source, span.start);
        let suggestion = kodo_ast::Diagnostic::fix_patch(e).map(|p| p.description.clone());
        diagnostics.push(Diagnostic {
            code: kodo_ast::Diagnostic::code(e).to_string(),
            severity: "error".to_string(),
            message: e.to_string(),
            start: span.start,
            end: span.end,
            line,
            col,
            suggestion,
        });
    }

    let functions: Vec<FunctionInfo> = module
        .functions
        .iter()
        .map(|f| FunctionInfo {
            name: f.name.clone(),
            params: f
                .params
                .iter()
                .map(|p| ParamInfo {
                    name: p.name.clone(),
                    ty: format_type_expr(&p.ty),
                })
                .collect(),
            return_type: format_type_expr(&f.return_type),
            requires_count: f.requires.len(),
            ensures_count: f.ensures.len(),
        })
        .collect();

    CheckResult {
        success: diagnostics.is_empty(),
        module_name: Some(module.name.clone()),
        functions,
        diagnostics,
    }
}

/// Tokenizes Kōdo source code and returns tokens as JSON.
#[wasm_bindgen]
pub fn tokenize(source: &str) -> String {
    let tokens = match kodo_lexer::tokenize(source) {
        Ok(t) => t,
        Err(_) => return "[]".to_string(),
    };

    let token_list: Vec<serde_json::Value> = tokens
        .iter()
        .map(|t| {
            serde_json::json!({
                "kind": format!("{:?}", t.kind),
                "start": t.span.start,
                "end": t.span.end,
            })
        })
        .collect();

    serde_json::to_string(&token_list).unwrap_or_else(|_| "[]".to_string())
}

/// Converts a byte offset to (line, col), both 0-based.
fn offset_to_line_col(source: &str, offset: u32) -> (u32, u32) {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in source.char_indices() {
        #[allow(clippy::cast_possible_truncation)]
        if i as u32 >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Formats a `TypeExpr` as a human-readable string.
fn format_type_expr(ty: &kodo_ast::TypeExpr) -> String {
    match ty {
        kodo_ast::TypeExpr::Named(name) => name.clone(),
        kodo_ast::TypeExpr::Generic(name, args) => {
            let args_str: Vec<String> = args.iter().map(format_type_expr).collect();
            format!("{}<{}>", name, args_str.join(", "))
        }
        kodo_ast::TypeExpr::Function(params, ret) => {
            let params_str: Vec<String> = params.iter().map(format_type_expr).collect();
            format!("({}) -> {}", params_str.join(", "), format_type_expr(ret))
        }
        kodo_ast::TypeExpr::Optional(inner) => format!("{}?", format_type_expr(inner)),
        kodo_ast::TypeExpr::DynTrait(name) => format!("dyn {name}"),
        kodo_ast::TypeExpr::Tuple(elems) => {
            let elems_str: Vec<String> = elems.iter().map(format_type_expr).collect();
            format!("({})", elems_str.join(", "))
        }
        kodo_ast::TypeExpr::Unit => "Unit".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_valid_module() {
        let source = r#"module test {
    meta { purpose: "test", version: "1.0.0" }
    fn main() -> Int { return 42 }
}"#;
        let json = check(source);
        let result: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(result["success"], true);
        assert_eq!(result["module_name"], "test");
        assert!(result["diagnostics"].as_array().unwrap().is_empty());
    }

    #[test]
    fn check_type_error() {
        let source = r#"module test {
    meta { purpose: "test", version: "1.0.0" }
    fn main() -> Int { return true }
}"#;
        let json = check(source);
        let result: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(result["success"], false);
        assert!(!result["diagnostics"].as_array().unwrap().is_empty());
    }

    #[test]
    fn check_parse_error() {
        let source = "module test { fn }";
        let json = check(source);
        let result: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(result["success"], false);
    }

    #[test]
    fn tokenize_returns_tokens() {
        let source = "let x: Int = 42";
        let json = tokenize(source);
        let tokens: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert!(!tokens.is_empty());
    }

    #[test]
    fn check_shows_function_info() {
        let source = r#"module test {
    meta { purpose: "test", version: "1.0.0" }
    fn add(a: Int, b: Int) -> Int
        requires { a > 0 }
    {
        return a + b
    }
}"#;
        let json = check(source);
        let result: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(result["functions"][0]["name"], "add");
        assert_eq!(result["functions"][0]["requires_count"], 1);
    }
}

#[test]
fn check_option_example() {
    let source = r#"module error_handling {
    meta { purpose: "Option and Result types", version: "1.0.0" }
    fn safe_get(idx: Int) -> Option<Int> {
        if idx >= 0 {
            return Option::Some(idx * 10)
        }
        return Option::None
    }
    fn main() -> Int {
        let val: Option<Int> = safe_get(3)
        match val {
            Option::Some(x) => { print_int(x) }
            Option::None => { println("not found") }
        }
        return 0
    }
}"#;
    let json = check(source);
    let result: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        result["success"], true,
        "Option example should pass: {}",
        result["diagnostics"]
    );
}
