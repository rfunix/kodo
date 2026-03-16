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
            format!("{} {op:?} {}", format_expr(left), format_expr(right))
        }
        kodo_ast::Expr::UnaryOp { op, operand, .. } => {
            format!("{op:?}{}", format_expr(operand))
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
