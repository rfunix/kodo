//! Shared utility functions for the Kōdo LSP server.
//!
//! Provides coordinate conversion between byte offsets and LSP (line, column)
//! positions, identifier extraction, and formatting helpers for types,
//! expressions, annotations, and type inference.

#[allow(clippy::wildcard_imports)]
use tower_lsp::lsp_types::*;

/// Converts a byte offset in source to (line, column) for LSP.
pub(crate) fn offset_to_line_col(source: &str, offset: u32) -> (u32, u32) {
    let offset = offset as usize;
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in source.char_indices() {
        if i >= offset {
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

/// Converts (line, column) to a byte offset.
pub(crate) fn line_col_to_offset(source: &str, line: u32, col: u32) -> Option<usize> {
    let mut current_line = 0u32;
    let mut current_col = 0u32;
    for (i, ch) in source.char_indices() {
        if current_line == line && current_col == col {
            return Some(i);
        }
        if ch == '\n' {
            if current_line == line {
                return Some(i);
            }
            current_line += 1;
            current_col = 0;
        } else {
            current_col += 1;
        }
    }
    if current_line == line {
        Some(source.len())
    } else {
        None
    }
}

/// Extracts the word (identifier) at the given byte offset.
pub(crate) fn word_at_offset(source: &str, offset: usize) -> &str {
    let bytes = source.as_bytes();
    if offset >= bytes.len() {
        return "";
    }
    // Check if the offset is within an identifier character.
    if !is_ident_char(bytes[offset]) {
        return "";
    }
    let mut start = offset;
    while start > 0 && is_ident_char(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = offset;
    while end < bytes.len() && is_ident_char(bytes[end]) {
        end += 1;
    }
    &source[start..end]
}

/// Returns true if the byte is a valid identifier character.
pub(crate) fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Formats a type expression as a string for display.
pub(crate) fn format_type_expr(ty: &kodo_ast::TypeExpr) -> String {
    match ty {
        kodo_ast::TypeExpr::Named(name) => name.clone(),
        kodo_ast::TypeExpr::Unit => "Unit".to_string(),
        kodo_ast::TypeExpr::Generic(name, args) => {
            let args_str: Vec<String> = args.iter().map(format_type_expr).collect();
            format!("{name}<{}>", args_str.join(", "))
        }
        kodo_ast::TypeExpr::Function(params, ret) => {
            let params_str: Vec<String> = params.iter().map(format_type_expr).collect();
            format!("({}) -> {}", params_str.join(", "), format_type_expr(ret))
        }
        kodo_ast::TypeExpr::Optional(inner) => {
            format!("{}?", format_type_expr(inner))
        }
        kodo_ast::TypeExpr::Tuple(elems) => {
            let elems_str: Vec<String> = elems.iter().map(format_type_expr).collect();
            format!("({})", elems_str.join(", "))
        }
        kodo_ast::TypeExpr::DynTrait(name) => format!("dyn {name}"),
    }
}

/// Formats a binary operator as its source-level symbol.
pub(crate) fn format_binop(op: kodo_ast::BinOp) -> &'static str {
    match op {
        kodo_ast::BinOp::Add => "+",
        kodo_ast::BinOp::Sub => "-",
        kodo_ast::BinOp::Mul => "*",
        kodo_ast::BinOp::Div => "/",
        kodo_ast::BinOp::Mod => "%",
        kodo_ast::BinOp::Eq => "==",
        kodo_ast::BinOp::Ne => "!=",
        kodo_ast::BinOp::Lt => "<",
        kodo_ast::BinOp::Gt => ">",
        kodo_ast::BinOp::Le => "<=",
        kodo_ast::BinOp::Ge => ">=",
        kodo_ast::BinOp::And => "&&",
        kodo_ast::BinOp::Or => "||",
    }
}

/// Formats a unary operator as its source-level symbol.
pub(crate) fn format_unaryop(op: kodo_ast::UnaryOp) -> &'static str {
    match op {
        kodo_ast::UnaryOp::Neg => "-",
        kodo_ast::UnaryOp::Not => "!",
    }
}

/// Formats an expression as a string for display (used in contract display).
pub(crate) fn format_expr(expr: &kodo_ast::Expr) -> String {
    match expr {
        kodo_ast::Expr::Ident(name, _) => name.clone(),
        kodo_ast::Expr::IntLit(n, _) => n.to_string(),
        kodo_ast::Expr::FloatLit(f, _) => format!("{f}"),
        kodo_ast::Expr::BoolLit(b, _) => b.to_string(),
        kodo_ast::Expr::StringLit(s, _) => format!("\"{s}\""),
        kodo_ast::Expr::BinaryOp {
            left, op, right, ..
        } => {
            format!(
                "{} {} {}",
                format_expr(left),
                format_binop(*op),
                format_expr(right)
            )
        }
        kodo_ast::Expr::UnaryOp { op, operand, .. } => {
            format!("{}{}", format_unaryop(*op), format_expr(operand))
        }
        kodo_ast::Expr::Call { callee, .. } => {
            if let kodo_ast::Expr::Ident(name, _) = callee.as_ref() {
                format!("{name}(...)")
            } else {
                "call(...)".to_string()
            }
        }
        kodo_ast::Expr::FieldAccess { object, field, .. } => {
            format!("{}.{field}", format_expr(object))
        }
        _ => "...".to_string(),
    }
}

/// Formats an annotation with its arguments for display.
///
/// Renders `@confidence(0.85)`, `@authored_by(agent: "claude")`, etc.
pub(crate) fn format_annotation(ann: &kodo_ast::Annotation) -> String {
    if ann.args.is_empty() {
        return format!("@{}", ann.name);
    }
    let args: Vec<String> = ann
        .args
        .iter()
        .map(|arg| match arg {
            kodo_ast::AnnotationArg::Positional(expr) => format_annotation_expr(expr),
            kodo_ast::AnnotationArg::Named(name, expr) => {
                format!("{name}: {}", format_annotation_expr(expr))
            }
        })
        .collect();
    format!("@{}({})", ann.name, args.join(", "))
}

/// Formats an expression value for annotation display.
pub(crate) fn format_annotation_expr(expr: &kodo_ast::Expr) -> String {
    match expr {
        kodo_ast::Expr::IntLit(n, _) => n.to_string(),
        kodo_ast::Expr::FloatLit(f, _) => format!("{f}"),
        kodo_ast::Expr::StringLit(s, _) => format!("\"{s}\""),
        kodo_ast::Expr::BoolLit(b, _) => b.to_string(),
        kodo_ast::Expr::Ident(name, _) => name.clone(),
        _ => "...".to_string(),
    }
}

/// Formats a [`kodo_types::Type`] as a human-readable string for type annotations.
pub(crate) fn format_type(ty: &kodo_types::Type) -> String {
    match ty {
        kodo_types::Type::Int => "Int".to_string(),
        kodo_types::Type::Int8 => "Int8".to_string(),
        kodo_types::Type::Int16 => "Int16".to_string(),
        kodo_types::Type::Int32 => "Int32".to_string(),
        kodo_types::Type::Int64 => "Int64".to_string(),
        kodo_types::Type::Uint => "Uint".to_string(),
        kodo_types::Type::Uint8 => "Uint8".to_string(),
        kodo_types::Type::Uint16 => "Uint16".to_string(),
        kodo_types::Type::Uint32 => "Uint32".to_string(),
        kodo_types::Type::Uint64 => "Uint64".to_string(),
        kodo_types::Type::Float32 => "Float32".to_string(),
        kodo_types::Type::Float64 => "Float64".to_string(),
        kodo_types::Type::Bool => "Bool".to_string(),
        kodo_types::Type::String => "String".to_string(),
        kodo_types::Type::Byte => "Byte".to_string(),
        kodo_types::Type::Unit => "Unit".to_string(),
        kodo_types::Type::Struct(name) | kodo_types::Type::Enum(name) => name.clone(),
        kodo_types::Type::Generic(name, args) => {
            let arg_strs: Vec<String> = args.iter().map(format_type).collect();
            format!("{name}<{}>", arg_strs.join(", "))
        }
        kodo_types::Type::Function(params, ret) => {
            let param_strs: Vec<String> = params.iter().map(format_type).collect();
            format!("({}) -> {}", param_strs.join(", "), format_type(ret))
        }
        kodo_types::Type::Tuple(elems) => {
            let elem_strs: Vec<String> = elems.iter().map(format_type).collect();
            format!("({})", elem_strs.join(", "))
        }
        kodo_types::Type::DynTrait(name) => format!("dyn {name}"),
        kodo_types::Type::Future(inner) => format!("Future<{}>", format_type(inner)),
        kodo_types::Type::Unknown => "TODO".to_string(),
    }
}

/// Infers a type hint string from an expression for the "Add type annotation" code action.
///
/// Uses the `TypeChecker` to infer the actual type when possible, falling back
/// to simple pattern matching on AST literals for cases where the full type
/// checker cannot be run (e.g., expressions that depend on context).
pub(crate) fn infer_type_hint(expr: &kodo_ast::Expr) -> String {
    match expr {
        kodo_ast::Expr::IntLit(_, _) => "Int".to_string(),
        kodo_ast::Expr::FloatLit(_, _) => "Float64".to_string(),
        kodo_ast::Expr::BoolLit(_, _) => "Bool".to_string(),
        kodo_ast::Expr::StringLit(_, _) => "String".to_string(),
        kodo_ast::Expr::Call { callee, .. } => infer_type_from_call(callee),
        kodo_ast::Expr::Ident(_, _) | kodo_ast::Expr::FieldAccess { .. } => {
            // Try to infer using the TypeChecker on a minimal wrapper module.
            infer_type_via_checker(expr)
        }
        kodo_ast::Expr::BinaryOp { op, .. } => {
            let result = infer_type_via_checker(expr);
            if result != "TODO" {
                return result;
            }
            match op {
                kodo_ast::BinOp::Add
                | kodo_ast::BinOp::Sub
                | kodo_ast::BinOp::Mul
                | kodo_ast::BinOp::Div
                | kodo_ast::BinOp::Mod => "Int".to_string(),
                kodo_ast::BinOp::Lt
                | kodo_ast::BinOp::Gt
                | kodo_ast::BinOp::Le
                | kodo_ast::BinOp::Ge
                | kodo_ast::BinOp::Eq
                | kodo_ast::BinOp::Ne
                | kodo_ast::BinOp::And
                | kodo_ast::BinOp::Or => "Bool".to_string(),
            }
        }
        kodo_ast::Expr::UnaryOp { op, .. } => {
            let result = infer_type_via_checker(expr);
            if result != "TODO" {
                return result;
            }
            match op {
                kodo_ast::UnaryOp::Neg => "Int".to_string(),
                kodo_ast::UnaryOp::Not => "Bool".to_string(),
            }
        }
        _ => "TODO".to_string(),
    }
}

/// Infers a return type from a function call expression by examining the callee name.
///
/// For known builtin functions, returns their documented return types.
pub(crate) fn infer_type_from_call(callee: &kodo_ast::Expr) -> String {
    if let kodo_ast::Expr::Ident(name, _) = callee {
        #[allow(clippy::match_same_arms)]
        match name.as_str() {
            "list_new" => return "List<Int>".to_string(),
            "map_new" => return "Map<Int, Int>".to_string(),
            "channel_new" => return "Channel<Int>".to_string(),
            "channel_new_bool" => return "Channel<Bool>".to_string(),
            "channel_new_string" => return "Channel<String>".to_string(),
            "abs" | "min" | "max" | "clamp" | "list_length" | "map_length" | "json_parse"
            | "http_get" | "http_post" | "time_now" | "time_now_ms" => return "Int".to_string(),
            "list_contains" | "map_contains_key" | "file_exists" | "list_is_empty"
            | "map_is_empty" => return "Bool".to_string(),
            "file_read" | "env_get" => return "String".to_string(),
            _ => {}
        }
    }
    "TODO".to_string()
}

/// Attempts to infer the type of an expression using the full `TypeChecker`.
///
/// Wraps the expression in a minimal module and runs type inference.
/// Returns `"TODO"` if inference fails (e.g., due to missing context).
pub(crate) fn infer_type_via_checker(expr: &kodo_ast::Expr) -> String {
    let mut checker = kodo_types::TypeChecker::new();
    match checker.infer_expr(expr) {
        Ok(ty) => format_type(&ty),
        Err(_) => "TODO".to_string(),
    }
}

/// Finds all occurrences of the given identifier name in the source.
///
/// Scans the source for whole-word matches of `name` that appear as
/// identifiers (bounded by non-identifier characters).
pub(crate) fn find_all_occurrences(source: &str, name: &str) -> Vec<Range> {
    let mut results = Vec::new();
    let bytes = source.as_bytes();
    let name_bytes = name.as_bytes();
    let name_len = name_bytes.len();

    if name_len == 0 || bytes.len() < name_len {
        return results;
    }

    let mut i = 0;
    while i + name_len <= bytes.len() {
        if &bytes[i..i + name_len] == name_bytes {
            // Check word boundaries
            let before_ok = i == 0 || !is_ident_char(bytes[i - 1]);
            let after_ok = i + name_len >= bytes.len() || !is_ident_char(bytes[i + name_len]);
            if before_ok && after_ok {
                #[allow(clippy::cast_possible_truncation)]
                let start_u32 = i as u32;
                #[allow(clippy::cast_possible_truncation)]
                let end_u32 = (i + name_len) as u32;
                let (sl, sc) = offset_to_line_col(source, start_u32);
                let (el, ec) = offset_to_line_col(source, end_u32);
                results.push(Range::new(Position::new(sl, sc), Position::new(el, ec)));
            }
        }
        i += 1;
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── offset_to_line_col ──────────────────────────────────────────

    #[test]
    fn offset_to_line_col_start_of_source() {
        let (line, col) = offset_to_line_col("hello\nworld", 0);
        assert_eq!((line, col), (0, 0));
    }

    #[test]
    fn offset_to_line_col_middle_of_first_line() {
        let (line, col) = offset_to_line_col("hello\nworld", 3);
        assert_eq!((line, col), (0, 3));
    }

    #[test]
    fn offset_to_line_col_start_of_second_line() {
        let (line, col) = offset_to_line_col("hello\nworld", 6);
        assert_eq!((line, col), (1, 0));
    }

    #[test]
    fn offset_to_line_col_end_of_second_line() {
        let (line, col) = offset_to_line_col("ab\ncd\nef", 7);
        assert_eq!((line, col), (2, 1));
    }

    #[test]
    fn offset_to_line_col_at_newline_char() {
        // Offset 5 is the '\n' character itself
        let (line, col) = offset_to_line_col("hello\nworld", 5);
        assert_eq!((line, col), (0, 5));
    }

    // ── line_col_to_offset ──────────────────────────────────────────

    #[test]
    fn line_col_to_offset_start() {
        assert_eq!(line_col_to_offset("hello\nworld", 0, 0), Some(0));
    }

    #[test]
    fn line_col_to_offset_second_line() {
        assert_eq!(line_col_to_offset("hello\nworld", 1, 0), Some(6));
    }

    #[test]
    fn line_col_to_offset_middle_of_line() {
        assert_eq!(line_col_to_offset("hello\nworld", 1, 3), Some(9));
    }

    #[test]
    fn line_col_to_offset_past_last_line_returns_none() {
        assert_eq!(line_col_to_offset("hello", 5, 0), None);
    }

    #[test]
    fn line_col_to_offset_col_past_end_returns_line_end() {
        // Column beyond line length should return end of that line
        let result = line_col_to_offset("ab\ncd", 0, 99);
        // The function clamps at the newline position
        assert_eq!(result, Some(2));
    }

    // ── word_at_offset ──────────────────────────────────────────────

    #[test]
    fn word_at_offset_simple_ident() {
        assert_eq!(word_at_offset("let foo = 42", 5), "foo");
    }

    #[test]
    fn word_at_offset_start_of_word() {
        assert_eq!(word_at_offset("let foo = 42", 4), "foo");
    }

    #[test]
    fn word_at_offset_end_of_word() {
        assert_eq!(word_at_offset("let foo = 42", 6), "foo");
    }

    #[test]
    fn word_at_offset_on_space_returns_empty() {
        assert_eq!(word_at_offset("let foo = 42", 3), "");
    }

    #[test]
    fn word_at_offset_past_end_returns_empty() {
        assert_eq!(word_at_offset("hello", 99), "");
    }

    #[test]
    fn word_at_offset_underscore_ident() {
        assert_eq!(word_at_offset("let my_var = 1", 6), "my_var");
    }

    // ── is_ident_char ───────────────────────────────────────────────

    #[test]
    fn is_ident_char_letters_digits_underscore() {
        assert!(is_ident_char(b'a'));
        assert!(is_ident_char(b'Z'));
        assert!(is_ident_char(b'0'));
        assert!(is_ident_char(b'_'));
        assert!(!is_ident_char(b' '));
        assert!(!is_ident_char(b':'));
        assert!(!is_ident_char(b'('));
    }

    // ── format_type_expr ────────────────────────────────────────────

    #[test]
    fn format_type_expr_named() {
        let ty = kodo_ast::TypeExpr::Named("Int".to_string());
        assert_eq!(format_type_expr(&ty), "Int");
    }

    #[test]
    fn format_type_expr_unit() {
        assert_eq!(format_type_expr(&kodo_ast::TypeExpr::Unit), "Unit");
    }

    #[test]
    fn format_type_expr_generic() {
        let ty = kodo_ast::TypeExpr::Generic(
            "List".to_string(),
            vec![kodo_ast::TypeExpr::Named("Int".to_string())],
        );
        assert_eq!(format_type_expr(&ty), "List<Int>");
    }

    #[test]
    fn format_type_expr_optional() {
        let ty =
            kodo_ast::TypeExpr::Optional(Box::new(kodo_ast::TypeExpr::Named("String".to_string())));
        assert_eq!(format_type_expr(&ty), "String?");
    }

    #[test]
    fn format_type_expr_tuple() {
        let ty = kodo_ast::TypeExpr::Tuple(vec![
            kodo_ast::TypeExpr::Named("Int".to_string()),
            kodo_ast::TypeExpr::Named("Bool".to_string()),
        ]);
        assert_eq!(format_type_expr(&ty), "(Int, Bool)");
    }

    #[test]
    fn format_type_expr_function() {
        let ty = kodo_ast::TypeExpr::Function(
            vec![kodo_ast::TypeExpr::Named("Int".to_string())],
            Box::new(kodo_ast::TypeExpr::Named("Bool".to_string())),
        );
        assert_eq!(format_type_expr(&ty), "(Int) -> Bool");
    }

    #[test]
    fn format_type_expr_dyn_trait() {
        let ty = kodo_ast::TypeExpr::DynTrait("Printable".to_string());
        assert_eq!(format_type_expr(&ty), "dyn Printable");
    }

    // ── format_expr ─────────────────────────────────────────────────

    #[test]
    fn format_expr_ident() {
        let span = kodo_ast::Span { start: 0, end: 1 };
        let expr = kodo_ast::Expr::Ident("x".to_string(), span);
        assert_eq!(format_expr(&expr), "x");
    }

    #[test]
    fn format_expr_int_lit() {
        let span = kodo_ast::Span { start: 0, end: 1 };
        let expr = kodo_ast::Expr::IntLit(42, span);
        assert_eq!(format_expr(&expr), "42");
    }

    #[test]
    fn format_expr_bool_lit() {
        let span = kodo_ast::Span { start: 0, end: 4 };
        let expr = kodo_ast::Expr::BoolLit(true, span);
        assert_eq!(format_expr(&expr), "true");
    }

    #[test]
    fn format_expr_string_lit() {
        let span = kodo_ast::Span { start: 0, end: 5 };
        let expr = kodo_ast::Expr::StringLit("hi".to_string(), span);
        assert_eq!(format_expr(&expr), "\"hi\"");
    }

    // ── format_annotation ───────────────────────────────────────────

    #[test]
    fn format_annotation_no_args() {
        let ann = kodo_ast::Annotation {
            name: "test".to_string(),
            args: vec![],
            span: kodo_ast::Span { start: 0, end: 5 },
        };
        assert_eq!(format_annotation(&ann), "@test");
    }

    #[test]
    fn format_annotation_positional_float() {
        let span = kodo_ast::Span { start: 0, end: 3 };
        let ann = kodo_ast::Annotation {
            name: "confidence".to_string(),
            args: vec![kodo_ast::AnnotationArg::Positional(
                kodo_ast::Expr::FloatLit(0.9, span),
            )],
            span: kodo_ast::Span { start: 0, end: 16 },
        };
        assert_eq!(format_annotation(&ann), "@confidence(0.9)");
    }

    #[test]
    fn format_annotation_named_arg() {
        let span = kodo_ast::Span { start: 0, end: 7 };
        let ann = kodo_ast::Annotation {
            name: "authored_by".to_string(),
            args: vec![kodo_ast::AnnotationArg::Named(
                "agent".to_string(),
                kodo_ast::Expr::StringLit("claude".to_string(), span),
            )],
            span: kodo_ast::Span { start: 0, end: 30 },
        };
        assert_eq!(format_annotation(&ann), "@authored_by(agent: \"claude\")");
    }

    // ── format_type ─────────────────────────────────────────────────

    #[test]
    fn format_type_primitives() {
        assert_eq!(format_type(&kodo_types::Type::Int), "Int");
        assert_eq!(format_type(&kodo_types::Type::Bool), "Bool");
        assert_eq!(format_type(&kodo_types::Type::String), "String");
        assert_eq!(format_type(&kodo_types::Type::Float64), "Float64");
        assert_eq!(format_type(&kodo_types::Type::Unit), "Unit");
        assert_eq!(format_type(&kodo_types::Type::Byte), "Byte");
        assert_eq!(format_type(&kodo_types::Type::Unknown), "TODO");
    }

    #[test]
    fn format_type_generic() {
        let ty = kodo_types::Type::Generic("List".to_string(), vec![kodo_types::Type::Int]);
        assert_eq!(format_type(&ty), "List<Int>");
    }

    #[test]
    fn format_type_function() {
        let ty = kodo_types::Type::Function(
            vec![kodo_types::Type::Int, kodo_types::Type::Bool],
            Box::new(kodo_types::Type::String),
        );
        assert_eq!(format_type(&ty), "(Int, Bool) -> String");
    }

    #[test]
    fn format_type_tuple() {
        let ty = kodo_types::Type::Tuple(vec![kodo_types::Type::Int, kodo_types::Type::String]);
        assert_eq!(format_type(&ty), "(Int, String)");
    }

    #[test]
    fn format_type_dyn_trait() {
        let ty = kodo_types::Type::DynTrait("Printable".to_string());
        assert_eq!(format_type(&ty), "dyn Printable");
    }

    // ── infer_type_hint ─────────────────────────────────────────────

    #[test]
    fn infer_type_hint_literals() {
        let span = kodo_ast::Span { start: 0, end: 1 };
        assert_eq!(infer_type_hint(&kodo_ast::Expr::IntLit(42, span)), "Int");
        assert_eq!(
            infer_type_hint(&kodo_ast::Expr::FloatLit(3.14, span)),
            "Float64"
        );
        assert_eq!(
            infer_type_hint(&kodo_ast::Expr::BoolLit(true, span)),
            "Bool"
        );
        assert_eq!(
            infer_type_hint(&kodo_ast::Expr::StringLit("hi".to_string(), span)),
            "String"
        );
    }

    #[test]
    fn infer_type_from_call_builtins() {
        let span = kodo_ast::Span { start: 0, end: 1 };
        let make_call = |name: &str| kodo_ast::Expr::Ident(name.to_string(), span);

        assert_eq!(infer_type_from_call(&make_call("list_new")), "List<Int>");
        assert_eq!(infer_type_from_call(&make_call("map_new")), "Map<Int, Int>");
        assert_eq!(infer_type_from_call(&make_call("abs")), "Int");
        assert_eq!(infer_type_from_call(&make_call("list_contains")), "Bool");
        assert_eq!(infer_type_from_call(&make_call("file_read")), "String");
        assert_eq!(infer_type_from_call(&make_call("unknown_fn")), "TODO");
    }

    // ── find_all_occurrences ────────────────────────────────────────

    #[test]
    fn find_all_occurrences_basic() {
        let source = "let x = x + x";
        let ranges = find_all_occurrences(source, "x");
        assert_eq!(ranges.len(), 3, "should find 3 occurrences of x");
    }

    #[test]
    fn find_all_occurrences_word_boundary() {
        let source = "let xy = x + xyz";
        let ranges = find_all_occurrences(source, "x");
        assert_eq!(
            ranges.len(),
            1,
            "should only match whole word 'x', not 'xy' or 'xyz'"
        );
    }

    #[test]
    fn find_all_occurrences_empty_name() {
        let ranges = find_all_occurrences("hello world", "");
        assert!(ranges.is_empty(), "empty name should return no results");
    }

    #[test]
    fn find_all_occurrences_not_found() {
        let ranges = find_all_occurrences("let a = b", "z");
        assert!(ranges.is_empty());
    }

    #[test]
    fn find_all_occurrences_multiline() {
        let source = "let a = 1\nlet b = a\nreturn a";
        let ranges = find_all_occurrences(source, "a");
        assert_eq!(ranges.len(), 3);
        // First on line 0, second on line 1, third on line 2
        assert_eq!(ranges[0].start.line, 0);
        assert_eq!(ranges[1].start.line, 1);
        assert_eq!(ranges[2].start.line, 2);
    }

    // ── snapshot tests ──────────────────────────────────────────────

    #[test]
    fn snapshot_format_type_expr_nested_generic() {
        let ty = kodo_ast::TypeExpr::Generic(
            "Map".to_string(),
            vec![
                kodo_ast::TypeExpr::Named("String".to_string()),
                kodo_ast::TypeExpr::Generic(
                    "List".to_string(),
                    vec![kodo_ast::TypeExpr::Named("Int".to_string())],
                ),
            ],
        );
        insta::assert_snapshot!(format_type_expr(&ty));
    }

    #[test]
    fn snapshot_format_type_nested() {
        let ty = kodo_types::Type::Generic(
            "Result".to_string(),
            vec![
                kodo_types::Type::Generic("List".to_string(), vec![kodo_types::Type::String]),
                kodo_types::Type::String,
            ],
        );
        insta::assert_snapshot!(format_type(&ty));
    }
}
