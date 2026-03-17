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

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Position;

    fn module_with_function() -> &'static str {
        r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn my_func(x: Int) -> Int {
        return x
    }
}"#
    }

    #[test]
    fn completions_include_function_names() {
        let source = module_with_function();
        let items = completions_for_source(source, Position::new(0, 0));
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            labels.contains(&"my_func"),
            "completions should include user-defined function names"
        );
        let func_item = items.iter().find(|i| i.label == "my_func").unwrap();
        assert_eq!(func_item.kind, Some(CompletionItemKind::FUNCTION));
        assert!(
            func_item
                .detail
                .as_deref()
                .unwrap_or("")
                .contains("fn my_func"),
            "function detail should show signature"
        );
    }

    #[test]
    fn completions_include_struct_names() {
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
        let items = completions_for_source(source, Position::new(0, 0));
        let struct_items: Vec<&CompletionItem> = items
            .iter()
            .filter(|i| i.kind == Some(CompletionItemKind::STRUCT))
            .collect();
        assert!(
            !struct_items.is_empty(),
            "completions should include struct names"
        );
        assert_eq!(struct_items[0].label, "Point");
    }

    #[test]
    fn completions_include_builtin_functions() {
        let source = module_with_function();
        let items = completions_for_source(source, Position::new(0, 0));
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        for builtin in &["println", "abs", "min", "max", "list_new", "map_new"] {
            assert!(
                labels.contains(builtin),
                "completions should include builtin function: {builtin}"
            );
        }
    }

    #[test]
    fn qualified_prefix_extraction() {
        // "Shape::" should extract "Shape"
        let source = "let x = Shape::";
        let result = qualified_prefix_at(source, Position::new(0, 15));
        assert_eq!(result, Some("Shape".to_string()));

        // Without :: should return None
        let source_no_colon = "let x = Shape.";
        let result_none = qualified_prefix_at(source_no_colon, Position::new(0, 14));
        assert!(
            result_none.is_none(),
            "dot should not trigger qualified prefix"
        );

        // Nested namespace like "a::b::" — extracts "b"
        let source_nested = "let x = a::b::";
        let result_nested = qualified_prefix_at(source_nested, Position::new(0, 14));
        assert_eq!(result_nested, Some("b".to_string()));
    }

    #[test]
    fn completions_for_enum_variants_after_double_colon() {
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
        let c: Color = Color::
    }
}"#;
        let items = completions_for_source(source, Position::new(13, 30));
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"Red"));
        assert!(labels.contains(&"Green"));
        assert!(labels.contains(&"Blue"));
        assert_eq!(items.len(), 3, "should only return enum variants");
        for item in &items {
            assert_eq!(item.kind, Some(CompletionItemKind::ENUM_MEMBER));
        }
    }

    #[test]
    fn completions_for_unknown_prefix_returns_empty() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn main() {
        let c: Int = Unknown::
    }
}"#;
        let items = completions_for_source(source, Position::new(7, 30));
        assert!(
            items.is_empty(),
            "unknown prefix after :: should return empty"
        );
    }

    #[test]
    fn completions_include_enum_names() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    enum Direction {
        North,
        South
    }

    fn main() {
        let d: Direction = Direction::North
    }
}"#;
        let items = completions_for_source(source, Position::new(0, 0));
        let enum_items: Vec<&CompletionItem> = items
            .iter()
            .filter(|i| i.kind == Some(CompletionItemKind::ENUM))
            .collect();
        assert!(!enum_items.is_empty(), "completions should include enums");
        assert_eq!(enum_items[0].label, "Direction");
    }

    #[test]
    fn completions_include_string_methods() {
        let source = module_with_function();
        let items = completions_for_source(source, Position::new(0, 0));
        let method_labels: Vec<&str> = items
            .iter()
            .filter(|i| i.kind == Some(CompletionItemKind::METHOD))
            .map(|i| i.label.as_str())
            .collect();
        for method in &[
            "length",
            "contains",
            "trim",
            "to_upper",
            "to_lower",
            "substring",
        ] {
            assert!(
                method_labels.contains(method),
                "completions should include string method: {method}"
            );
        }
    }

    #[test]
    fn completions_show_contracts_in_documentation() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn divide(a: Int, b: Int) -> Int
        requires { b > 0 }
    {
        return a
    }
}"#;
        let items = completions_for_source(source, Position::new(0, 0));
        let divide_item = items.iter().find(|i| i.label == "divide");
        assert!(divide_item.is_some(), "should find divide in completions");
        let doc = match &divide_item.unwrap().documentation {
            Some(Documentation::String(s)) => s.clone(),
            _ => String::new(),
        };
        assert!(
            doc.contains("requires"),
            "documentation should mention contracts, got: {doc}"
        );
    }
}
