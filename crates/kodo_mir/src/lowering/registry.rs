//! Type registry construction for MIR lowering.
//!
//! Builds the struct, enum, function return type, actor, and type alias
//! registries needed by the MIR lowering pass. Also registers return types
//! for all builtin runtime functions so that MIR locals receiving their
//! results get the correct type.

use std::collections::{HashMap, HashSet};

use kodo_ast::Module;
use kodo_types::{resolve_type, resolve_type_with_enums, Type};

use crate::{MirError, Result};

/// Registers return types for builtin functions so that MIR locals receiving
/// their results get the correct type (e.g. `Type::String` for `Int_to_string`).
#[allow(clippy::too_many_lines)]
pub(super) fn register_builtin_return_types(fn_return_types: &mut HashMap<String, Type>) {
    // Builtins that return String.
    for name in &[
        "Int_to_string",
        "Float64_to_string",
        "Bool_to_string",
        "String_trim",
        "String_to_upper",
        "String_to_lower",
        "String_substring",
        "String_replace",
        "String_repeat",
        "list_join",
        "map_get_sv",
        "map_get_ss",
    ] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(Type::String);
    }
    // Builtins that return Int.
    for name in &[
        "String_length",
        "String_contains",
        "String_starts_with",
        "String_ends_with",
        "String_char_at",
        "String_index_of",
        "String_eq",
        "String_parse_int",
        "abs",
        "min",
        "max",
        "clamp",
        "list_length",
        "list_contains",
        "list_is_empty",
        "list_slice",
        "list_set",
        "list_remove",
        "map_length",
        "map_contains_key",
        "map_contains_key_sk",
        "map_remove",
        "map_remove_sk",
        "map_is_empty",
        "map_get_sk",
        "json_get_int",
        "json_get_bool",
        "json_get_array",
        "json_get_object",
        "file_delete",
        "String_lines",
        "String_split",
    ] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(Type::Int);
    }
    // Time builtins.
    for name in &["time_now", "time_now_ms", "time_elapsed_ms"] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(Type::Int);
    }
    fn_return_types
        .entry("time_format".to_string())
        .or_insert(Type::String);
    fn_return_types
        .entry("env_get".to_string())
        .or_insert(Type::String);

    // Actor runtime builtins.
    fn_return_types
        .entry("kodo_actor_new".to_string())
        .or_insert(Type::Int);
    fn_return_types
        .entry("kodo_actor_get_field".to_string())
        .or_insert(Type::Int);

    // Channel builtins.
    // All channel_new variants return Int (opaque handle — same runtime function).
    for name in &["channel_new", "channel_new_bool", "channel_new_string"] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(Type::Int);
    }
    fn_return_types
        .entry("channel_recv".to_string())
        .or_insert(Type::Int);
    fn_return_types
        .entry("channel_recv_bool".to_string())
        .or_insert(Type::Bool);
    fn_return_types
        .entry("channel_recv_string".to_string())
        .or_insert(Type::String);
    // channel_send variants return Unit.
    for name in &["channel_send", "channel_send_bool", "channel_send_string"] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(Type::Unit);
    }
    fn_return_types
        .entry("channel_select_2".to_string())
        .or_insert(Type::Int);
    fn_return_types
        .entry("channel_select_3".to_string())
        .or_insert(Type::Int);

    // Iterator builtins — all return Int (opaque handles or 0/1 flags).
    for name in &[
        "list_iter",
        "list_iterator_advance",
        "list_iterator_value",
        "list_new",
        "String_chars",
        "string_chars_advance",
        "string_chars_value",
        "Map_keys",
        "map_keys_advance",
        "map_keys_value",
        "Map_values",
        "map_values_advance",
        "map_values_value",
    ] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(Type::Int);
    }

    // Combinator methods on List.
    let list_int = Type::Generic("List".to_string(), vec![Type::Int]);
    fn_return_types
        .entry("List_map".to_string())
        .or_insert(list_int.clone());
    fn_return_types
        .entry("List_filter".to_string())
        .or_insert(list_int);
    fn_return_types
        .entry("List_fold".to_string())
        .or_insert(Type::Int);
    fn_return_types
        .entry("List_reduce".to_string())
        .or_insert(Type::Int);
    fn_return_types
        .entry("List_count".to_string())
        .or_insert(Type::Int);
    fn_return_types
        .entry("List_any".to_string())
        .or_insert(Type::Bool);
    fn_return_types
        .entry("List_all".to_string())
        .or_insert(Type::Bool);

    // List.sort_by — sorts in place with a comparator, returns Unit.
    fn_return_types
        .entry("List_sort_by".to_string())
        .or_insert(Type::Unit);

    // Closure handle builtins.
    fn_return_types
        .entry("kodo_closure_new".to_string())
        .or_insert(Type::Int);
    fn_return_types
        .entry("kodo_closure_func".to_string())
        .or_insert(Type::Int);
    fn_return_types
        .entry("kodo_closure_env".to_string())
        .or_insert(Type::Int);

    // Future runtime builtins.
    fn_return_types
        .entry("kodo_future_new".to_string())
        .or_insert(Type::Int);
    fn_return_types
        .entry("kodo_future_await".to_string())
        .or_insert(Type::Int);
    fn_return_types
        .entry("kodo_future_complete_bytes".to_string())
        .or_insert(Type::Unit);
    fn_return_types
        .entry("kodo_future_await_bytes".to_string())
        .or_insert(Type::Unit);

    // Green thread cooperative yield — returns Unit (void).
    fn_return_types
        .entry("kodo_green_maybe_yield".to_string())
        .or_insert(Type::Unit);

    // Channel select builtins — return Int (index of the ready channel).
    fn_return_types
        .entry("channel_select_2".to_string())
        .or_insert(Type::Int);
    fn_return_types
        .entry("channel_select_3".to_string())
        .or_insert(Type::Int);

    // Generic channel builtins.
    fn_return_types
        .entry("channel_generic_new".to_string())
        .or_insert(Type::Int);
    fn_return_types
        .entry("channel_generic_recv".to_string())
        .or_insert(Type::Int);

    // Set<Int> builtins.
    let set_int = Type::Generic("Set".to_string(), vec![Type::Int]);
    fn_return_types
        .entry("set_new".to_string())
        .or_insert(set_int.clone());
    for name in &["set_contains", "set_remove", "set_is_empty"] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(Type::Bool);
    }
    fn_return_types
        .entry("set_length".to_string())
        .or_insert(Type::Int);
    fn_return_types
        .entry("set_add".to_string())
        .or_insert(Type::Unit);
    fn_return_types
        .entry("set_free".to_string())
        .or_insert(Type::Unit);
    for name in &["set_union", "set_intersection", "set_difference"] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(set_int.clone());
    }
    // set_to_list converts a Set<Int> to a List<Int> for iteration.
    let list_int = Type::Generic("List".to_string(), vec![Type::Int]);
    fn_return_types
        .entry("set_to_list".to_string())
        .or_insert(list_int);

    // Map higher-order builtins — merge and filter both return Map<Int, Int>.
    let map_int_int = Type::Generic("Map".to_string(), vec![Type::Int, Type::Int]);
    for name in &["map_merge", "map_filter"] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(map_int_int.clone());
    }

    register_stdlib_expansion_return_types(fn_return_types);

    register_sprint5_return_types(fn_return_types);
    register_test_return_types(fn_return_types);
}

/// Registers return types for stdlib expansion builtins (Milestone 8).
///
/// Includes character classification, `StringBuilder`, `format_int`, `timestamp`, `sleep`.
fn register_stdlib_expansion_return_types(fn_return_types: &mut HashMap<String, Type>) {
    // Character classification — return Int (used as Bool: 0/1)
    for name in &[
        "char_at",
        "is_alpha",
        "is_digit",
        "is_alphanumeric",
        "is_whitespace",
    ] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(Type::Int);
    }
    // char_from_code returns String
    fn_return_types
        .entry("char_from_code".to_string())
        .or_insert(Type::String);

    // StringBuilder
    fn_return_types
        .entry("string_builder_new".to_string())
        .or_insert(Type::Int);
    fn_return_types
        .entry("string_builder_to_string".to_string())
        .or_insert(Type::String);
    fn_return_types
        .entry("string_builder_len".to_string())
        .or_insert(Type::Int);

    // Extended stdlib
    fn_return_types
        .entry("format_int".to_string())
        .or_insert(Type::String);
    fn_return_types
        .entry("timestamp".to_string())
        .or_insert(Type::Int);
}

/// Registers return types for Sprint 5 builtins (CLI, JSON, HTTP server, math).
#[allow(clippy::too_many_lines)]
fn register_sprint5_return_types(fn_return_types: &mut HashMap<String, Type>) {
    // CLI builtins.
    fn_return_types
        .entry("readln".to_string())
        .or_insert(Type::String);
    let list_string = Type::Generic("List".to_string(), vec![Type::String]);
    fn_return_types
        .entry("args".to_string())
        .or_insert(list_string.clone());
    fn_return_types
        .entry("dir_list".to_string())
        .or_insert(list_string);

    // JSON builtins.
    for name in &["json_new_object", "json_parse"] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(Type::Int);
    }
    fn_return_types
        .entry("json_stringify".to_string())
        .or_insert(Type::String);
    for name in &["json_get_string", "json_get"] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(Type::String);
    }

    // HTTP server builtins.
    for name in &["http_server_new", "http_server_recv"] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(Type::Int);
    }
    for name in &[
        "http_request_method",
        "http_request_path",
        "http_request_body",
    ] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(Type::String);
    }

    // Math + collections builtins.
    fn_return_types
        .entry("rand_int".to_string())
        .or_insert(Type::Int);
    fn_return_types
        .entry("list_pop".to_string())
        .or_insert(Type::Int);

    // Float64-returning math builtins.
    for name in &[
        "Int_to_float64",
        "sqrt",
        "pow",
        "sin",
        "cos",
        "log",
        "floor",
        "ceil",
        "round",
        "json_get_float",
    ] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(Type::Float64);
    }

    // File extended builtins returning Bool.
    fn_return_types
        .entry("dir_exists".to_string())
        .or_insert(Type::Bool);
    fn_return_types
        .entry("file_exists".to_string())
        .or_insert(Type::Bool);

    // SQLite database builtins.
    for name in &[
        "db_open",
        "db_execute",
        "db_query",
        "db_row_next",
        "db_row_get_int",
        "db_row_advance",
    ] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(Type::Int);
    }
    fn_return_types
        .entry("db_row_get_string".to_string())
        .or_insert(Type::String);

    // IO builtins that return Result<T, E> (enum types).
    fn_return_types
        .entry("file_read".to_string())
        .or_insert(Type::Enum("Result__String_String".to_string()));
    fn_return_types
        .entry("file_write".to_string())
        .or_insert(Type::Enum("Result__Unit_String".to_string()));
    fn_return_types
        .entry("file_append".to_string())
        .or_insert(Type::Enum("Result__Unit_String".to_string()));

    // Result/Option discriminant checks return Bool (but they are inlined in codegen).
    for name in &[
        "Result_is_ok",
        "Result_is_err",
        "Option_is_some",
        "Option_is_none",
    ] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(Type::Bool);
    }

    // Result/Option unwrap methods — return types are polymorphic and resolved
    // per-callsite during MIR lowering based on the actual generic parameters.
    // These defaults cover the common case; the lowering pass overrides them
    // when it sees the concrete types.
    fn_return_types
        .entry("Result_unwrap".to_string())
        .or_insert(Type::String);
    fn_return_types
        .entry("Result_unwrap_err".to_string())
        .or_insert(Type::String);
    fn_return_types
        .entry("Option_unwrap".to_string())
        .or_insert(Type::Int);
}

/// Registers return types for test framework builtins (assertions and lifecycle).
pub(super) fn register_test_return_types(fn_return_types: &mut HashMap<String, Type>) {
    // All assertion builtins return Unit.
    for name in &[
        "assert",
        "assert_true",
        "assert_false",
        "assert_eq",
        "assert_ne",
        "kodo_assert_eq_int",
        "kodo_assert_eq_string",
        "kodo_assert_eq_bool",
        "kodo_assert_eq_float",
        "kodo_assert_ne_int",
        "kodo_assert_ne_string",
        "kodo_assert_ne_bool",
        "kodo_assert_ne_float",
    ] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(Type::Unit);
    }

    // Test lifecycle: kodo_test_start returns Unit, kodo_test_end returns Int.
    fn_return_types
        .entry("kodo_test_start".to_string())
        .or_insert(Type::Unit);
    fn_return_types
        .entry("kodo_test_end".to_string())
        .or_insert(Type::Int);
    fn_return_types
        .entry("kodo_test_skip".to_string())
        .or_insert(Type::Unit);
    fn_return_types
        .entry("kodo_test_summary".to_string())
        .or_insert(Type::Unit);

    // Property testing builtins — lifecycle and generator functions.
    for name in &[
        "kodo_prop_start",
        "kodo_test_set_timeout",
        "kodo_test_clear_timeout",
        "kodo_test_isolate_start",
        "kodo_test_isolate_end",
    ] {
        fn_return_types
            .entry((*name).to_string())
            .or_insert(Type::Unit);
    }
    fn_return_types
        .entry("kodo_prop_gen_int".to_string())
        .or_insert(Type::Int);
    fn_return_types
        .entry("kodo_prop_gen_bool".to_string())
        .or_insert(Type::Bool);
    fn_return_types
        .entry("kodo_prop_gen_float".to_string())
        .or_insert(Type::Float64);
    fn_return_types
        .entry("kodo_prop_gen_string".to_string())
        .or_insert(Type::String);
}

/// Builds all type registries needed for lowering a module: struct fields,
/// enum variants, function return types, actor names, and type aliases.
#[allow(clippy::type_complexity)]
pub(super) fn build_module_registries(
    module: &Module,
) -> Result<(
    HashMap<String, Vec<(String, Type)>>,
    HashMap<String, Vec<(String, Vec<Type>)>>,
    HashMap<String, Type>,
    HashSet<String>,
    HashMap<String, (Type, Option<kodo_ast::Expr>)>,
)> {
    // Build struct registry from type declarations.
    let mut struct_registry: HashMap<String, Vec<(String, Type)>> = HashMap::new();
    for type_decl in &module.type_decls {
        let mut fields = Vec::new();
        for field in &type_decl.fields {
            let ty = resolve_type(&field.ty, field.span)
                .map_err(|e| MirError::TypeResolution(e.to_string()))?;
            fields.push((field.name.clone(), ty));
        }
        struct_registry.insert(type_decl.name.clone(), fields);
    }

    // Register actor fields in the struct registry.
    for actor_decl in &module.actor_decls {
        let mut fields = Vec::new();
        for field in &actor_decl.fields {
            let ty = resolve_type(&field.ty, field.span)
                .map_err(|e| MirError::TypeResolution(e.to_string()))?;
            fields.push((field.name.clone(), ty));
        }
        struct_registry.insert(actor_decl.name.clone(), fields);
    }

    // Build enum registry from enum declarations.
    let mut enum_registry: HashMap<String, Vec<(String, Vec<Type>)>> = HashMap::new();
    for enum_decl in &module.enum_decls {
        let mut variants = Vec::new();
        for variant in &enum_decl.variants {
            let field_types: std::result::Result<Vec<_>, _> = variant
                .fields
                .iter()
                .map(|f| {
                    resolve_type(f, variant.span)
                        .map_err(|e| MirError::TypeResolution(e.to_string()))
                })
                .collect();
            variants.push((variant.name.clone(), field_types?));
        }
        enum_registry.insert(enum_decl.name.clone(), variants);
    }

    // Build function return type registry.
    let enum_names: std::collections::HashSet<String> = enum_registry.keys().cloned().collect();
    let mut fn_return_types: HashMap<String, Type> = HashMap::new();
    for func in &module.functions {
        if !func.generic_params.is_empty() {
            continue;
        }
        let ret_ty = resolve_type_with_enums(&func.return_type, func.span, &enum_names)
            .map_err(|e| MirError::TypeResolution(e.to_string()))?;
        fn_return_types.insert(func.name.clone(), ret_ty);
    }
    register_builtin_return_types(&mut fn_return_types);

    // Collect actor names.
    let actor_names: HashSet<String> = module.actor_decls.iter().map(|a| a.name.clone()).collect();

    // Build type alias registry.
    let mut alias_reg: HashMap<String, (Type, Option<kodo_ast::Expr>)> = HashMap::new();
    for alias in &module.type_aliases {
        let base_ty = resolve_type(&alias.base_type, alias.span)
            .map_err(|e| MirError::TypeResolution(e.to_string()))?;
        alias_reg.insert(alias.name.clone(), (base_ty, alias.constraint.clone()));
    }

    Ok((
        struct_registry,
        enum_registry,
        fn_return_types,
        actor_names,
        alias_reg,
    ))
}
