//! Code completion provider for the Kōdo LSP server.
//!
//! Provides context-aware completions including function names, struct/enum
//! names, builtin functions, string methods, struct fields, and enum variant
//! completions after `::` qualifiers.

#[allow(clippy::wildcard_imports)]
use tower_lsp::lsp_types::*;

use crate::utils::{format_annotation, format_type_expr, is_ident_char, line_col_to_offset};

/// Returns completion items for the current source at the given cursor position.
///
/// Provides context-aware completions:
/// - After `EnumName::`: enum variant names
/// - Otherwise: function names, struct/enum names, builtin functions,
///   string method completions, and struct field completions.
pub(crate) fn completions_for_source(source: &str, position: Position) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // Check for qualified prefix (e.g., `Color::`) before parsing,
    // since incomplete source like `Color::` may fail to parse.
    if let Some(prefix) = qualified_prefix_at(source, position) {
        // Use recovery parser to get the module even with incomplete source
        let output = kodo_parser::parse_with_recovery(source);
        for enum_decl in &output.module.enum_decls {
            if enum_decl.name == prefix {
                for variant in &enum_decl.variants {
                    let detail = if variant.fields.is_empty() {
                        format!("{}::{}", enum_decl.name, variant.name)
                    } else {
                        let fields_str: Vec<String> =
                            variant.fields.iter().map(format_type_expr).collect();
                        format!(
                            "{}::{}({})",
                            enum_decl.name,
                            variant.name,
                            fields_str.join(", ")
                        )
                    };
                    items.push(CompletionItem {
                        label: variant.name.clone(),
                        kind: Some(CompletionItemKind::ENUM_MEMBER),
                        detail: Some(detail),
                        ..Default::default()
                    });
                }
                return items;
            }
        }
        // No matching enum found — return empty
        return items;
    }

    let Ok(module) = kodo_parser::parse(source) else {
        return items;
    };

    let mut checker = kodo_types::TypeChecker::new();
    let _ = checker.check_module(&module);

    for func in &module.functions {
        let params_str: Vec<String> = func
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name, format_type_expr(&p.ty)))
            .collect();
        let ret = format_type_expr(&func.return_type);
        let detail = format!("fn {}({}) -> {}", func.name, params_str.join(", "), ret);

        let mut doc_parts = Vec::new();
        for req in &func.requires {
            doc_parts.push(format!("requires {{ {req:?} }}"));
        }
        for ens in &func.ensures {
            doc_parts.push(format!("ensures {{ {ens:?} }}"));
        }
        for ann in &func.annotations {
            doc_parts.push(format_annotation(ann));
        }

        let documentation = if doc_parts.is_empty() {
            None
        } else {
            Some(Documentation::String(doc_parts.join("\n")))
        };

        items.push(CompletionItem {
            label: func.name.clone(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some(detail),
            documentation,
            ..Default::default()
        });
    }

    for type_decl in &module.type_decls {
        items.push(CompletionItem {
            label: type_decl.name.clone(),
            kind: Some(CompletionItemKind::STRUCT),
            detail: Some(format!("struct {}", type_decl.name)),
            ..Default::default()
        });
    }

    for enum_decl in &module.enum_decls {
        items.push(CompletionItem {
            label: enum_decl.name.clone(),
            kind: Some(CompletionItemKind::ENUM),
            detail: Some(format!("enum {}", enum_decl.name)),
            ..Default::default()
        });
    }

    add_builtin_completions(&mut items);
    add_string_method_completions(&mut items);

    for type_decl in &module.type_decls {
        for field in &type_decl.fields {
            items.push(CompletionItem {
                label: field.name.clone(),
                kind: Some(CompletionItemKind::FIELD),
                detail: Some(format!("{}.{}", type_decl.name, field.name)),
                ..Default::default()
            });
        }
    }

    items
}

/// Extracts the qualified prefix before the cursor (e.g., `"Color"` from `Color::`).
///
/// Returns `Some(name)` if the text before the cursor ends with `Name::`,
/// meaning the user wants completions for variants/members of `Name`.
pub(crate) fn qualified_prefix_at(source: &str, position: Position) -> Option<String> {
    let offset = line_col_to_offset(source, position.line, position.character)?;
    let before = &source[..offset];
    let trimmed = before.trim_end();

    // Check for `Name::` pattern
    if !trimmed.ends_with("::") {
        return None;
    }
    let prefix = &trimmed[..trimmed.len() - 2];
    // Extract the identifier right before `::`
    let bytes = prefix.as_bytes();
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1] == b' ' {
        end -= 1;
    }
    let mut start = end;
    while start > 0 && is_ident_char(bytes[start - 1]) {
        start -= 1;
    }
    let name = &prefix[start..end];
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Adds builtin function completions to the list.
fn add_builtin_completions(items: &mut Vec<CompletionItem>) {
    let builtins: &[(&str, &str)] = &[
        ("println", "println(value) -> Unit"),
        ("print", "print(value) -> Unit"),
        ("print_int", "print_int(n: Int) -> Unit"),
        ("abs", "abs(n: Int) -> Int"),
        ("min", "min(a: Int, b: Int) -> Int"),
        ("max", "max(a: Int, b: Int) -> Int"),
        ("clamp", "clamp(val: Int, lo: Int, hi: Int) -> Int"),
        ("file_exists", "file_exists(path: String) -> Bool"),
        ("file_read", "file_read(path: String) -> String"),
        (
            "file_write",
            "file_write(path: String, data: String) -> Unit",
        ),
        ("list_new", "list_new() -> List<T>"),
        ("list_push", "list_push(list, value) -> Unit"),
        ("list_get", "list_get(list, index: Int) -> T"),
        ("list_length", "list_length(list) -> Int"),
        ("list_contains", "list_contains(list, value) -> Bool"),
        ("list_pop", "list_pop(list) -> T"),
        ("list_remove", "list_remove(list, index: Int) -> T"),
        ("list_set", "list_set(list, index: Int, value) -> Unit"),
        ("list_is_empty", "list_is_empty(list) -> Bool"),
        ("list_reverse", "list_reverse(list) -> Unit"),
        ("map_new", "map_new() -> Map<K, V>"),
        ("map_insert", "map_insert(map, key, value) -> Unit"),
        ("map_get", "map_get(map, key) -> V"),
        ("map_contains_key", "map_contains_key(map, key) -> Bool"),
        ("map_length", "map_length(map) -> Int"),
        ("map_remove", "map_remove(map, key) -> Bool"),
        ("map_is_empty", "map_is_empty(map) -> Bool"),
        ("json_stringify", "json_stringify(json) -> String"),
        ("json_get_bool", "json_get_bool(json, key: String) -> Bool"),
        (
            "json_get_float",
            "json_get_float(json, key: String) -> Float64",
        ),
        (
            "json_get_array",
            "json_get_array(json, key: String) -> List<Json>",
        ),
    ];
    for (name, detail) in builtins {
        items.push(CompletionItem {
            label: (*name).to_string(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some((*detail).to_string()),
            ..Default::default()
        });
    }
}

/// Adds string method completions to the list.
fn add_string_method_completions(items: &mut Vec<CompletionItem>) {
    let string_methods = [
        ("length", "Returns the length of the string", "() -> Int"),
        (
            "contains",
            "Checks if the string contains a substring",
            "(sub: String) -> Bool",
        ),
        (
            "starts_with",
            "Checks if the string starts with a prefix",
            "(prefix: String) -> Bool",
        ),
        (
            "ends_with",
            "Checks if the string ends with a suffix",
            "(suffix: String) -> Bool",
        ),
        (
            "trim",
            "Removes leading and trailing whitespace",
            "() -> String",
        ),
        ("to_upper", "Converts to uppercase", "() -> String"),
        ("to_lower", "Converts to lowercase", "() -> String"),
        (
            "substring",
            "Extracts a substring",
            "(start: Int, end: Int) -> String",
        ),
        (
            "to_string",
            "Converts to string representation",
            "() -> String",
        ),
    ];
    for (name, doc, signature) in &string_methods {
        items.push(CompletionItem {
            label: (*name).to_string(),
            kind: Some(CompletionItemKind::METHOD),
            detail: Some(format!("String.{name}{signature}")),
            documentation: Some(Documentation::String((*doc).to_string())),
            ..Default::default()
        });
    }
}
