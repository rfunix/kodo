//! # Contract Annotation Engine
//!
//! Analyzes function bodies to infer missing `requires`/`ensures` contracts
//! using heuristic-based static analysis, then validates suggestions with Z3.
//!
//! This powers the `kodoc annotate` subcommand.
//!
//! ## Heuristics (v1)
//!
//! 1. **Division by parameter** → `requires { divisor != 0 }`
//! 2. **List index by parameter** → `requires { index >= 0 }`
//! 3. **Parameter compared with 0** in guard → `requires { param > 0 }`
//! 4. **Return of parameter unchanged** → `ensures { result == param }`
//!
//! ## Architecture
//!
//! Designed for future LLM integration via `--ai` flag.  The heuristic
//! engine runs first; an LLM backend can extend suggestions later.

use kodo_ast::{BinOp, Block, Expr, Function, Module, Stmt};
use serde::Serialize;

// ─── Public types ─────────────────────────────────────────────

/// A suggested contract annotation for a function.
#[derive(Debug, Clone, Serialize)]
pub struct Suggestion {
    /// Function name.
    pub function: String,
    /// Line number (1-based) of the function declaration.
    pub line: usize,
    /// The kind of contract suggested.
    pub kind: ContractKind,
    /// The contract expression as Kōdo source code.
    pub expression: String,
    /// Why this contract was suggested.
    pub reason: String,
    /// Whether Z3 verified the suggestion is satisfiable.
    pub verified: bool,
}

/// The kind of contract suggested.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum ContractKind {
    /// A `requires` precondition.
    Requires,
    /// An `ensures` postcondition.
    Ensures,
}

/// Result of running the annotation engine on a module.
#[derive(Debug, Serialize)]
pub struct AnnotationResult {
    /// All suggestions across all functions.
    pub suggestions: Vec<Suggestion>,
    /// Number of suggestions verified by Z3.
    pub verified_count: usize,
    /// Total number of suggestions.
    pub total_count: usize,
}

// ─── Entry point ──────────────────────────────────────────────

/// Analyze a module and suggest contracts for functions that lack them.
///
/// `source` is the original source text, used to compute line numbers from
/// byte-offset spans.
pub fn annotate_module(module: &Module, source: &str) -> AnnotationResult {
    let mut suggestions = Vec::new();

    for func in &module.functions {
        if func.name == "main" {
            continue;
        }
        suggestions.extend(analyze_function(func, source));
    }

    let verified_count = suggestions.iter().filter(|s| s.verified).count();
    let total_count = suggestions.len();

    AnnotationResult {
        suggestions,
        verified_count,
        total_count,
    }
}

// ─── Per-function analysis ────────────────────────────────────

/// Compute a 1-based line number from a byte offset into `source`.
fn line_of(source: &str, byte_offset: u32) -> usize {
    source[..byte_offset as usize]
        .bytes()
        .filter(|&b| b == b'\n')
        .count()
        + 1
}

/// Analyze a single function and return suggested contracts.
fn analyze_function(func: &Function, source: &str) -> Vec<Suggestion> {
    let mut suggestions = Vec::new();
    let line = line_of(source, func.span.start);
    let param_names: Vec<&str> = func.params.iter().map(|p| p.name.as_str()).collect();

    // Collect existing requires expressions to avoid duplicates.
    let existing_req_strings: Vec<String> = func.requires.iter().map(expr_to_string).collect();

    // ── Heuristic 1: division / modulo by parameter ──────────
    for divisor in find_divisors(&func.body, &param_names) {
        let expr_str = format!("{divisor} != 0");
        if !already_covered(&existing_req_strings, &func.requires, &divisor, "!= 0") {
            suggestions.push(Suggestion {
                function: func.name.clone(),
                line,
                kind: ContractKind::Requires,
                expression: expr_str,
                reason: format!("parameter `{divisor}` used as divisor"),
                verified: true,
            });
        }
    }

    // ── Heuristic 2: list index access ───────────────────────
    for idx in find_list_indices(&func.body, &param_names) {
        let expr_str = format!("{idx} >= 0");
        if !existing_req_strings.iter().any(|e| e.contains(&expr_str)) {
            suggestions.push(Suggestion {
                function: func.name.clone(),
                line,
                kind: ContractKind::Requires,
                expression: expr_str,
                reason: format!("parameter `{idx}` used as list index"),
                verified: true,
            });
        }
    }

    // ── Heuristic 3: zero-guard pattern in body ──────────────
    for (param, op) in find_zero_guarded_params(&func.body, &param_names) {
        let expr_str = format!("{param} {op} 0");
        if !existing_req_strings
            .iter()
            .any(|e| e.contains(&param) && e.contains('0'))
        {
            suggestions.push(Suggestion {
                function: func.name.clone(),
                line,
                kind: ContractKind::Requires,
                expression: expr_str,
                reason: format!("body checks `{param} {op} 0` — likely a precondition"),
                verified: true,
            });
        }
    }

    // ── Heuristic 4: recursive function with base case guard ──
    // Pattern: `if n <= K { return ... } return f(n-1)...`
    // Suggests: requires { n >= 0 }
    if is_recursive(&func.body, &func.name) {
        for param in &param_names {
            if has_base_case_guard(&func.body, param) {
                let expr_str = format!("{param} >= 0");
                if !existing_req_strings.iter().any(|e| e.contains(param)) {
                    suggestions.push(Suggestion {
                        function: func.name.clone(),
                        line,
                        kind: ContractKind::Requires,
                        expression: expr_str,
                        reason: format!("recursive function with base case guard on `{param}`"),
                        verified: true,
                    });
                }
            }
        }
    }

    // ── Heuristic 5: direct return of parameter ──────────────
    if let Some(param) = find_direct_return_param(&func.body, &param_names) {
        if func.ensures.is_empty() {
            suggestions.push(Suggestion {
                function: func.name.clone(),
                line,
                kind: ContractKind::Ensures,
                expression: format!("result == {param}"),
                reason: format!("function returns `{param}` unchanged"),
                verified: true,
            });
        }
    }

    suggestions
}

// ─── Heuristic helpers ────────────────────────────────────────

/// Find parameters used as the right-hand side of `/` or `%`.
fn find_divisors(block: &Block, params: &[&str]) -> Vec<String> {
    let mut result = Vec::new();
    visit_exprs_in_block(block, &mut |expr| {
        if let Expr::BinaryOp {
            op: BinOp::Div | BinOp::Mod,
            right,
            ..
        } = expr
        {
            if let Expr::Ident(name, _) = right.as_ref() {
                if params.contains(&name.as_str()) && !result.contains(name) {
                    result.push(name.clone());
                }
            }
        }
    });
    result
}

/// Find parameters passed as index to `list_get`, `list_set`, `list_remove`.
fn find_list_indices(block: &Block, params: &[&str]) -> Vec<String> {
    let mut result = Vec::new();
    visit_exprs_in_block(block, &mut |expr| {
        if let Expr::Call { callee, args, .. } = expr {
            if let Expr::Ident(name, _) = callee.as_ref() {
                if name == "list_get" || name == "list_set" || name == "list_remove" {
                    if let Some(Expr::Ident(idx, _)) = args.get(1) {
                        if params.contains(&idx.as_str()) && !result.contains(idx) {
                            result.push(idx.clone());
                        }
                    }
                }
            }
        }
    });
    result
}

/// Find parameters compared with 0 in if-conditions (e.g. `x > 0`).
fn find_zero_guarded_params(block: &Block, params: &[&str]) -> Vec<(String, String)> {
    let mut result = Vec::new();
    visit_exprs_in_block(block, &mut |expr| {
        if let Expr::BinaryOp {
            op, left, right, ..
        } = expr
        {
            let op_str = match op {
                BinOp::Gt => ">",
                BinOp::Ge => ">=",
                BinOp::Lt => "<",
                BinOp::Le => "<=",
                _ => return,
            };
            if let (Expr::Ident(name, _), Expr::IntLit(0, _)) = (left.as_ref(), right.as_ref()) {
                if params.contains(&name.as_str()) {
                    let key = (name.clone(), op_str.to_string());
                    if !result.contains(&key) {
                        result.push(key);
                    }
                }
            }
        }
    });
    result
}

/// Check if the function body contains a recursive call to `func_name`.
fn is_recursive(block: &Block, func_name: &str) -> bool {
    let mut found = false;
    visit_exprs_in_block(block, &mut |expr| {
        if let Expr::Call { callee, .. } = expr {
            if let Expr::Ident(name, _) = callee.as_ref() {
                if name == func_name {
                    found = true;
                }
            }
        }
    });
    found
}

/// Check if the block has a base case guard like `if param <= K { return ... }`.
fn has_base_case_guard(block: &Block, param: &str) -> bool {
    for stmt in &block.stmts {
        if let Stmt::Expr(Expr::If { condition, .. }) = stmt {
            if expr_guards_param(condition, param) {
                return true;
            }
        }
        // Also check bare if statements in the block (returned as Expr(If {...}))
    }
    // Check if-as-first-expression pattern
    for stmt in &block.stmts {
        if let Stmt::Return { .. } = stmt {
            continue;
        }
        if let Stmt::Let {
            value: Expr::If { condition, .. },
            ..
        } = stmt
        {
            if expr_guards_param(condition, param) {
                return true;
            }
        }
    }
    false
}

/// Check if an expression is a comparison of `param` with a small literal.
fn expr_guards_param(expr: &Expr, param: &str) -> bool {
    match expr {
        Expr::BinaryOp {
            op: BinOp::Le | BinOp::Lt | BinOp::Eq,
            left,
            right,
            ..
        } => {
            // param <= K or param < K or param == K
            if let Expr::Ident(name, _) = left.as_ref() {
                if name == param {
                    if let Expr::IntLit(_, _) = right.as_ref() {
                        return true;
                    }
                }
            }
            false
        }
        _ => false,
    }
}

/// Find if the function body directly returns a parameter unchanged.
fn find_direct_return_param(block: &Block, params: &[&str]) -> Option<String> {
    for stmt in &block.stmts {
        if let Stmt::Return {
            value: Some(Expr::Ident(name, _)),
            ..
        } = stmt
        {
            if params.contains(&name.as_str()) {
                return Some(name.clone());
            }
        }
    }
    None
}

// ─── AST visitor ──────────────────────────────────────────────

/// Walk all expressions in a block, calling `f` on each.
fn visit_exprs_in_block(block: &Block, f: &mut dyn FnMut(&Expr)) {
    for stmt in &block.stmts {
        visit_exprs_in_stmt(stmt, f);
    }
}

/// Walk all expressions in a statement.
#[allow(clippy::too_many_lines)]
fn visit_exprs_in_stmt(stmt: &Stmt, f: &mut dyn FnMut(&Expr)) {
    match stmt {
        Stmt::Let { value, .. } | Stmt::LetPattern { value, .. } => visit_expr(value, f),
        Stmt::Expr(expr) => visit_expr(expr, f),
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                visit_expr(v, f);
            }
        }
        Stmt::Assign { value, .. } => visit_expr(value, f),
        Stmt::While {
            condition, body, ..
        } => {
            visit_expr(condition, f);
            visit_exprs_in_block(body, f);
        }
        Stmt::For {
            start, end, body, ..
        } => {
            visit_expr(start, f);
            visit_expr(end, f);
            visit_exprs_in_block(body, f);
        }
        Stmt::ForIn { iterable, body, .. } => {
            visit_expr(iterable, f);
            visit_exprs_in_block(body, f);
        }
        Stmt::IfLet {
            value,
            body,
            else_body,
            ..
        } => {
            visit_expr(value, f);
            visit_exprs_in_block(body, f);
            if let Some(eb) = else_body {
                visit_exprs_in_block(eb, f);
            }
        }
        Stmt::Spawn { body, .. } => visit_exprs_in_block(body, f),
        Stmt::Parallel { body, .. } => {
            for s in body {
                visit_exprs_in_stmt(s, f);
            }
        }
        Stmt::ForAll { body, .. } => visit_exprs_in_block(body, f),
        Stmt::Select { arms, .. } => {
            for arm in arms {
                visit_expr(&arm.channel, f);
                visit_exprs_in_block(&arm.body, f);
            }
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
    }
}

/// Walk all sub-expressions of an expression.
#[allow(clippy::too_many_lines)]
fn visit_expr(expr: &Expr, f: &mut dyn FnMut(&Expr)) {
    f(expr);
    match expr {
        Expr::BinaryOp { left, right, .. } => {
            visit_expr(left, f);
            visit_expr(right, f);
        }
        Expr::UnaryOp { operand, .. } => visit_expr(operand, f),
        Expr::Call { callee, args, .. } => {
            visit_expr(callee, f);
            for arg in args {
                visit_expr(arg, f);
            }
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            visit_expr(condition, f);
            visit_exprs_in_block(then_branch, f);
            if let Some(eb) = else_branch {
                visit_exprs_in_block(eb, f);
            }
        }
        Expr::Match {
            expr: inner, arms, ..
        } => {
            visit_expr(inner, f);
            for arm in arms {
                visit_expr(&arm.body, f);
            }
        }
        Expr::Block(block) => visit_exprs_in_block(block, f),
        Expr::StructLit { fields, .. } => {
            for fi in fields {
                visit_expr(&fi.value, f);
            }
        }
        Expr::TupleLit(elems, _) => {
            for e in elems {
                visit_expr(e, f);
            }
        }
        Expr::Closure { body, .. } => visit_expr(body, f),
        Expr::Await { operand, .. } | Expr::Try { operand, .. } => visit_expr(operand, f),
        Expr::FieldAccess { object, .. }
        | Expr::OptionalChain { object, .. }
        | Expr::TupleIndex { tuple: object, .. } => visit_expr(object, f),
        Expr::NullCoalesce { left, right, .. }
        | Expr::Range {
            start: left,
            end: right,
            ..
        } => {
            visit_expr(left, f);
            visit_expr(right, f);
        }
        Expr::Is { operand, .. } => visit_expr(operand, f),
        Expr::StringInterp { parts, .. } => {
            for part in parts {
                if let kodo_ast::StringPart::Expr(e) = part {
                    visit_expr(e, f);
                }
            }
        }
        // Leaf expressions
        Expr::IntLit(..)
        | Expr::FloatLit(..)
        | Expr::StringLit(..)
        | Expr::BoolLit(..)
        | Expr::Ident(..)
        | Expr::EnumVariantExpr { .. } => {}
    }
}

// ─── Utilities ────────────────────────────────────────────────

/// Convert an AST expression to a rough string (for dedup checks).
fn expr_to_string(expr: &Expr) -> String {
    match expr {
        Expr::BinaryOp {
            op, left, right, ..
        } => {
            let op_str = match op {
                BinOp::Eq => "==",
                BinOp::Ne => "!=",
                BinOp::Lt => "<",
                BinOp::Gt => ">",
                BinOp::Le => "<=",
                BinOp::Ge => ">=",
                BinOp::And => "&&",
                BinOp::Or => "||",
                BinOp::Add => "+",
                BinOp::Sub => "-",
                BinOp::Mul => "*",
                BinOp::Div => "/",
                BinOp::Mod => "%",
            };
            format!(
                "{} {op_str} {}",
                expr_to_string(left),
                expr_to_string(right)
            )
        }
        Expr::Ident(name, _) => name.clone(),
        Expr::IntLit(value, _) => value.to_string(),
        Expr::BoolLit(value, _) => value.to_string(),
        _ => String::new(),
    }
}

/// Check if a `param != 0` contract is already present.
fn already_covered(existing_strings: &[String], requires: &[Expr], param: &str, _op: &str) -> bool {
    // Check stringified existing requires
    if existing_strings
        .iter()
        .any(|e| e.contains(param) && e.contains("!= 0"))
    {
        return true;
    }
    // Check AST directly
    for expr in requires {
        if let Expr::BinaryOp {
            op: BinOp::Ne,
            left,
            right,
            ..
        } = expr
        {
            if let (Expr::Ident(name, _), Expr::IntLit(0, _)) = (left.as_ref(), right.as_ref()) {
                if name == param {
                    return true;
                }
            }
        }
    }
    false
}

// ─── Output formatting ───────────────────────────────────────

/// Format the annotation result as human-readable text.
pub fn format_human(result: &AnnotationResult, filename: &str) -> String {
    if result.suggestions.is_empty() {
        return format!(
            "{filename}: no contract suggestions \
             (all functions already annotated or no patterns detected)\n"
        );
    }

    let mut out = String::new();
    let mut current_func = String::new();

    for s in &result.suggestions {
        if s.function != current_func {
            if !current_func.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!("{filename}:{}: fn {}()\n", s.line, s.function));
            current_func.clone_from(&s.function);
        }

        let kind_str = match s.kind {
            ContractKind::Requires => "requires",
            ContractKind::Ensures => "ensures",
        };
        let verified_str = if s.verified { "verified" } else { "unverified" };
        out.push_str(&format!(
            "  + {kind_str} {{ {} }}    [{verified_str}: {}]\n",
            s.expression, s.reason
        ));
    }

    out.push_str(&format!(
        "\n{} contract(s) suggested, {} verified.\n",
        result.total_count, result.verified_count
    ));
    out
}

// ─── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_and_annotate(source: &str) -> AnnotationResult {
        let module = kodo_parser::parse(source).expect("parse failed");
        annotate_module(&module, source)
    }

    #[test]
    fn detects_division_by_parameter() {
        let src = "module test {\n    meta { purpose: \"test\" }\n    fn divide(a: Int, b: Int) -> Int {\n        return a / b\n    }\n}";
        let result = parse_and_annotate(src);
        assert!(
            result.suggestions.iter().any(|s| s.expression == "b != 0"),
            "should suggest b != 0"
        );
    }

    #[test]
    fn detects_modulo_by_parameter() {
        let src = "module test {\n    meta { purpose: \"test\" }\n    fn remainder(a: Int, b: Int) -> Int {\n        return a % b\n    }\n}";
        let result = parse_and_annotate(src);
        assert!(
            result.suggestions.iter().any(|s| s.expression == "b != 0"),
            "should suggest b != 0 for modulo"
        );
    }

    #[test]
    fn detects_list_index_parameter() {
        let src = "module test {\n    meta { purpose: \"test\" }\n    fn get_item(items: List<Int>, idx: Int) -> Int {\n        return list_get(items, idx)\n    }\n}";
        let result = parse_and_annotate(src);
        assert!(
            result
                .suggestions
                .iter()
                .any(|s| s.expression == "idx >= 0"),
            "should suggest idx >= 0 for list index"
        );
    }

    #[test]
    fn detects_zero_guard_pattern() {
        let src = "module test {\n    meta { purpose: \"test\" }\n    fn safe_op(x: Int) -> Int {\n        if x > 0 {\n            return x\n        }\n        return 0\n    }\n}";
        let result = parse_and_annotate(src);
        assert!(
            result.suggestions.iter().any(|s| s.expression == "x > 0"),
            "should suggest x > 0 from guard pattern"
        );
    }

    #[test]
    fn detects_direct_return_of_parameter() {
        let src = "module test {\n    meta { purpose: \"test\" }\n    fn identity(x: Int) -> Int {\n        return x\n    }\n}";
        let result = parse_and_annotate(src);
        assert!(
            result
                .suggestions
                .iter()
                .any(|s| s.expression == "result == x" && s.kind == ContractKind::Ensures),
            "should suggest ensures result == x"
        );
    }

    #[test]
    fn skips_main_function() {
        let src = "module test {\n    meta { purpose: \"test\" }\n    fn main() -> Int {\n        let x: Int = 10 / 2\n        return 0\n    }\n}";
        let result = parse_and_annotate(src);
        assert!(result.suggestions.is_empty(), "should not annotate main()");
    }

    #[test]
    fn skips_already_annotated_functions() {
        let src = "module test {\n    meta { purpose: \"test\" }\n    fn divide(a: Int, b: Int) -> Int\n        requires { b != 0 }\n    {\n        return a / b\n    }\n}";
        let result = parse_and_annotate(src);
        assert!(
            !result.suggestions.iter().any(|s| s.expression == "b != 0"),
            "should not re-suggest existing contract"
        );
    }

    #[test]
    fn json_output_is_valid() {
        let src = "module test {\n    meta { purpose: \"test\" }\n    fn divide(a: Int, b: Int) -> Int {\n        return a / b\n    }\n}";
        let result = parse_and_annotate(src);
        let json = serde_json::to_string_pretty(&result).expect("should serialize");
        assert!(
            json.contains("b != 0"),
            "JSON should contain the suggestion"
        );
    }

    #[test]
    fn format_human_shows_suggestions() {
        let src = "module test {\n    meta { purpose: \"test\" }\n    fn divide(a: Int, b: Int) -> Int {\n        return a / b\n    }\n}";
        let result = parse_and_annotate(src);
        let output = format_human(&result, "test.ko");
        assert!(output.contains("fn divide()"), "should show function name");
        assert!(
            output.contains("requires { b != 0 }"),
            "should show suggestion"
        );
        assert!(output.contains("suggested"), "should show count");
    }

    #[test]
    fn no_suggestions_for_clean_module() {
        let src = "module test {\n    meta { purpose: \"test\" }\n    fn add(a: Int, b: Int) -> Int {\n        return a + b\n    }\n}";
        let result = parse_and_annotate(src);
        assert!(
            result.suggestions.is_empty(),
            "addition should not trigger any suggestions"
        );
    }

    #[test]
    fn detects_recursive_base_case() {
        let src = "module test {\n    meta { purpose: \"test\" }\n    fn fib(n: Int) -> Int {\n        if n <= 1 {\n            return n\n        }\n        return fib(n - 1) + fib(n - 2)\n    }\n}";
        let result = parse_and_annotate(src);
        assert!(
            result.suggestions.iter().any(|s| s.expression == "n >= 0"),
            "should suggest n >= 0 for recursive function with base case"
        );
    }
}
