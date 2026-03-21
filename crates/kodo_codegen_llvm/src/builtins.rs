//! Runtime builtin function declarations for LLVM IR.
//!
//! Generates `declare` statements for all Kodo runtime functions that the
//! compiled code may call. These correspond to C-ABI functions provided by
//! `libkodo_runtime`, which the linker resolves at link time.
//!
//! The declarations are organized by category (I/O, math, concurrency, etc.)
//! matching the Cranelift backend's `builtins.rs` module.

/// A single runtime builtin declaration.
struct Builtin {
    /// The C-ABI function name (e.g., `"kodo_println"`).
    name: &'static str,
    /// Parameter types as LLVM IR type strings.
    params: &'static [&'static str],
    /// Return type as LLVM IR type string (`"void"` for no return).
    ret: &'static str,
}

/// Generates all `declare` statements for runtime builtins.
pub(crate) fn emit_runtime_declarations() -> Vec<String> {
    let builtins = all_builtins();
    let mut decls: Vec<String> = builtins
        .iter()
        .map(|b| {
            let params = b.params.join(", ");
            format!("declare {} @{}({})", b.ret, b.name, params)
        })
        .collect();
    // __env_pack_N: fixed-arity variants for closure environment packing.
    // Each variant takes N i64 captures and returns a pointer (i64) to the
    // heap-allocated environment buffer.
    for n in 0..=8 {
        let params = (0..n).map(|_| "i64").collect::<Vec<_>>().join(", ");
        decls.push(format!("declare i64 @__env_pack_{n}({params})"));
    }
    decls
}

/// Returns the complete list of runtime builtins.
///
/// This list must be kept in sync with `kodo_codegen/src/builtins.rs` and
/// the actual runtime library (`kodo_runtime`).
#[allow(clippy::too_many_lines)]
fn all_builtins() -> Vec<Builtin> {
    vec![
        // -- I/O --
        Builtin {
            name: "kodo_println",
            params: &["i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_print",
            params: &["i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_print_int",
            params: &["i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_print_float",
            params: &["double"],
            ret: "void",
        },
        Builtin {
            name: "kodo_println_float",
            params: &["double"],
            ret: "void",
        },
        Builtin {
            name: "kodo_contract_fail",
            params: &["i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_contract_fail_recoverable",
            params: &["i64", "i64"],
            ret: "void",
        },
        // -- Math --
        Builtin {
            name: "kodo_abs",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_min",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_max",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_clamp",
            params: &["i64", "i64", "i64"],
            ret: "i64",
        },
        // -- Math extended --
        Builtin {
            name: "kodo_sqrt",
            params: &["double"],
            ret: "double",
        },
        Builtin {
            name: "kodo_pow",
            params: &["double", "double"],
            ret: "double",
        },
        Builtin {
            name: "kodo_sin",
            params: &["double"],
            ret: "double",
        },
        Builtin {
            name: "kodo_cos",
            params: &["double"],
            ret: "double",
        },
        Builtin {
            name: "kodo_log",
            params: &["double"],
            ret: "double",
        },
        Builtin {
            name: "kodo_floor",
            params: &["double"],
            ret: "double",
        },
        Builtin {
            name: "kodo_ceil",
            params: &["double"],
            ret: "double",
        },
        Builtin {
            name: "kodo_round",
            params: &["double"],
            ret: "double",
        },
        Builtin {
            name: "kodo_rand_int",
            params: &["i64", "i64"],
            ret: "i64",
        },
        // -- Concurrency --
        Builtin {
            name: "kodo_spawn_task",
            params: &["i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_spawn_task_with_env",
            params: &["i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_green_spawn",
            params: &["i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_green_spawn_with_env",
            params: &["i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_green_maybe_yield",
            params: &[],
            ret: "void",
        },
        Builtin {
            name: "kodo_future_new",
            params: &[],
            ret: "i64",
        },
        Builtin {
            name: "kodo_future_complete",
            params: &["i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_future_await",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_future_complete_bytes",
            params: &["i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_future_await_bytes",
            params: &["i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_parallel_begin",
            params: &[],
            ret: "i64",
        },
        Builtin {
            name: "kodo_parallel_spawn",
            params: &["i64", "i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_parallel_join",
            params: &["i64"],
            ret: "void",
        },
        // -- String query --
        Builtin {
            name: "kodo_string_length",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_string_byte_length",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_string_char_count",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_string_contains",
            params: &["i64", "i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_string_starts_with",
            params: &["i64", "i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_string_ends_with",
            params: &["i64", "i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_string_index_of",
            params: &["i64", "i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_string_eq",
            params: &["i64", "i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_string_split",
            params: &["i64", "i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_string_lines",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_string_parse_int",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_string_char_at",
            params: &["i64", "i64", "i64"],
            ret: "i64",
        },
        // -- String transform --
        Builtin {
            name: "kodo_string_trim",
            params: &["i64", "i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_string_to_upper",
            params: &["i64", "i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_string_to_lower",
            params: &["i64", "i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_string_substring",
            params: &["i64", "i64", "i64", "i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_string_concat",
            params: &["i64", "i64", "i64", "i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_string_replace",
            params: &["i64", "i64", "i64", "i64", "i64", "i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_string_repeat",
            params: &["i64", "i64", "i64", "i64", "i64"],
            ret: "void",
        },
        // -- Conversions --
        Builtin {
            name: "kodo_int_to_string",
            params: &["i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_int_to_float64",
            params: &["i64"],
            ret: "double",
        },
        Builtin {
            name: "kodo_float64_to_string",
            params: &["double", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_float64_to_int",
            params: &["double"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_bool_to_string",
            params: &["i64", "i64", "i64"],
            ret: "void",
        },
        // -- File I/O --
        Builtin {
            name: "kodo_file_exists",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_file_read",
            params: &["i64", "i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_file_write",
            params: &["i64", "i64", "i64", "i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_file_append",
            params: &["i64", "i64", "i64", "i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_file_delete",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_dir_list",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_dir_exists",
            params: &["i64", "i64"],
            ret: "i64",
        },
        // -- Collections: List --
        Builtin {
            name: "kodo_list_new",
            params: &[],
            ret: "i64",
        },
        Builtin {
            name: "kodo_list_push",
            params: &["i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_list_get",
            params: &["i64", "i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_list_length",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_list_contains",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_list_pop_simple",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_list_remove",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_list_set",
            params: &["i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_list_is_empty",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_list_reverse",
            params: &["i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_list_slice",
            params: &["i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_list_sort",
            params: &["i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_list_join",
            params: &["i64", "i64", "i64", "i64", "i64"],
            ret: "void",
        },
        // -- Collections: Map --
        Builtin {
            name: "kodo_map_new",
            params: &[],
            ret: "i64",
        },
        Builtin {
            name: "kodo_map_insert",
            params: &["i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_map_get",
            params: &["i64", "i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_map_contains_key",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_map_length",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_map_remove",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_map_is_empty",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_map_insert_sk",
            params: &["i64", "i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_map_get_sk",
            params: &["i64", "i64", "i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_map_contains_key_sk",
            params: &["i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_map_remove_sk",
            params: &["i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_map_free_sk",
            params: &["i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_map_insert_sv",
            params: &["i64", "i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_map_get_sv",
            params: &["i64", "i64", "i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_map_free_sv",
            params: &["i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_map_insert_ss",
            params: &["i64", "i64", "i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_map_get_ss",
            params: &["i64", "i64", "i64", "i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_map_free_ss",
            params: &["i64"],
            ret: "void",
        },
        // -- Collections: Set --
        Builtin {
            name: "kodo_set_new",
            params: &[],
            ret: "i64",
        },
        Builtin {
            name: "kodo_set_add",
            params: &["i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_set_contains",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_set_remove",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_set_length",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_set_is_empty",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_set_union",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_set_intersection",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_set_difference",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_set_free",
            params: &["i64"],
            ret: "void",
        },
        // -- Network / JSON --
        Builtin {
            name: "kodo_http_get",
            params: &["i64", "i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_http_post",
            params: &["i64", "i64", "i64", "i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_json_parse",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_json_get_string",
            params: &["i64", "i64", "i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_json_get_int",
            params: &["i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_json_free",
            params: &["i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_json_stringify",
            params: &["i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_json_get_bool",
            params: &["i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_json_get_float",
            params: &["i64", "i64", "i64"],
            ret: "double",
        },
        Builtin {
            name: "kodo_json_get_array",
            params: &["i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_json_get_object",
            params: &["i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_json_new_object",
            params: &[],
            ret: "i64",
        },
        Builtin {
            name: "kodo_json_set_string",
            params: &["i64", "i64", "i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_json_set_int",
            params: &["i64", "i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_json_set_bool",
            params: &["i64", "i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_json_set_float",
            params: &["i64", "i64", "i64", "double"],
            ret: "void",
        },
        // -- Actor --
        Builtin {
            name: "kodo_actor_new",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_actor_get_field",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_actor_set_field",
            params: &["i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_actor_send",
            params: &["i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_actor_free",
            params: &["i64", "i64"],
            ret: "void",
        },
        // -- Time --
        Builtin {
            name: "kodo_time_now",
            params: &[],
            ret: "i64",
        },
        Builtin {
            name: "kodo_time_now_ms",
            params: &[],
            ret: "i64",
        },
        Builtin {
            name: "kodo_time_format",
            params: &["i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_time_elapsed_ms",
            params: &["i64"],
            ret: "i64",
        },
        // -- Env --
        Builtin {
            name: "kodo_env_get",
            params: &["i64", "i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_env_set",
            params: &["i64", "i64", "i64", "i64"],
            ret: "void",
        },
        // -- Cleanup --
        Builtin {
            name: "kodo_string_free",
            params: &["i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_list_free",
            params: &["i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_map_free",
            params: &["i64"],
            ret: "void",
        },
        // -- Channels --
        Builtin {
            name: "kodo_channel_new",
            params: &[],
            ret: "i64",
        },
        Builtin {
            name: "kodo_channel_send",
            params: &["i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_channel_recv",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_channel_send_bool",
            params: &["i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_channel_recv_bool",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_channel_send_string",
            params: &["i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_channel_recv_string",
            params: &["i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_channel_free",
            params: &["i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_channel_select_2",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_channel_select_3",
            params: &["i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_channel_generic_new",
            params: &[],
            ret: "i64",
        },
        Builtin {
            name: "kodo_channel_generic_send",
            params: &["i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_channel_generic_recv",
            params: &["i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_channel_generic_free",
            params: &["i64"],
            ret: "void",
        },
        // -- RC --
        Builtin {
            name: "kodo_alloc",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_free",
            params: &["i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_rc_inc",
            params: &["i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_rc_dec",
            params: &["i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_rc_count",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_rc_inc_string",
            params: &["i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_rc_dec_string",
            params: &["i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_closure_new",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_closure_func",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_closure_env",
            params: &["i64"],
            ret: "i64",
        },
        // -- Async --
        Builtin {
            name: "kodo_spawn_async",
            params: &["i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_await",
            params: &["i64"],
            ret: "i64",
        },
        // -- Iterators --
        Builtin {
            name: "kodo_list_iter",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_list_iterator_advance",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_list_iterator_value",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_list_iterator_free",
            params: &["i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_string_chars",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_string_chars_advance",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_string_chars_value",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_string_chars_free",
            params: &["i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_map_keys",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_map_keys_advance",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_map_keys_value",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_map_keys_free",
            params: &["i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_map_values",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_map_values_advance",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_map_values_value",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_map_values_free",
            params: &["i64"],
            ret: "void",
        },
        // -- CLI --
        Builtin {
            name: "kodo_args",
            params: &[],
            ret: "i64",
        },
        Builtin {
            name: "kodo_readln",
            params: &["i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_exit",
            params: &["i64"],
            ret: "void",
        },
        // -- HTTP Server --
        Builtin {
            name: "kodo_http_server_new",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_http_server_recv",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_http_request_method",
            params: &["i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_http_request_path",
            params: &["i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_http_request_body",
            params: &["i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_http_respond",
            params: &["i64", "i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_http_server_free",
            params: &["i64"],
            ret: "void",
        },
        // -- Database --
        Builtin {
            name: "kodo_db_open",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_db_execute",
            params: &["i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_db_query",
            params: &["i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_db_row_next",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_db_row_get_string",
            params: &["i64", "i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_db_row_get_int",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_db_row_advance",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_db_result_free",
            params: &["i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_db_close",
            params: &["i64"],
            ret: "void",
        },
        // -- Test --
        Builtin {
            name: "kodo_assert",
            params: &["i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_assert_true",
            params: &["i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_assert_false",
            params: &["i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_assert_eq_int",
            params: &["i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_assert_eq_string",
            params: &["i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_assert_eq_bool",
            params: &["i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_assert_eq_float",
            params: &["double", "double"],
            ret: "void",
        },
        Builtin {
            name: "kodo_assert_ne_int",
            params: &["i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_assert_ne_string",
            params: &["i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_assert_ne_bool",
            params: &["i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_assert_ne_float",
            params: &["double", "double"],
            ret: "void",
        },
        Builtin {
            name: "kodo_test_start",
            params: &["i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_test_end",
            params: &[],
            ret: "i64",
        },
        Builtin {
            name: "kodo_test_skip",
            params: &[],
            ret: "void",
        },
        Builtin {
            name: "kodo_test_summary",
            params: &["i64", "i64", "i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_test_set_timeout",
            params: &["i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_test_clear_timeout",
            params: &[],
            ret: "void",
        },
        Builtin {
            name: "kodo_test_isolate_start",
            params: &[],
            ret: "void",
        },
        Builtin {
            name: "kodo_test_isolate_end",
            params: &[],
            ret: "void",
        },
        Builtin {
            name: "kodo_prop_start",
            params: &["i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_prop_gen_int",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_prop_gen_bool",
            params: &[],
            ret: "i64",
        },
        Builtin {
            name: "kodo_prop_gen_float",
            params: &["double", "double"],
            ret: "double",
        },
        Builtin {
            name: "kodo_prop_gen_string",
            params: &["i64"],
            ret: "i64",
        },
        // -- Async string helpers --
        Builtin {
            name: "__future_await_string",
            params: &["i64"],
            ret: "{ i64, i64 }",
        },
        Builtin {
            name: "__future_complete_string",
            params: &["i64", "i64", "i64"],
            ret: "void",
        },
        // -- Closure environment helpers --
        Builtin {
            name: "__env_load",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "__env_load_string",
            params: &["i64", "i64", "i64"],
            ret: "void",
        },
        // -- Stdlib expansion (Milestone 8) --
        Builtin {
            name: "kodo_is_alpha",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_is_digit",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_is_alphanumeric",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_is_whitespace",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_char_from_code",
            params: &["i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_string_builder_new",
            params: &[],
            ret: "i64",
        },
        Builtin {
            name: "kodo_string_builder_push",
            params: &["i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_string_builder_push_char",
            params: &["i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_string_builder_to_string",
            params: &["i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_string_builder_len",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_format_int",
            params: &["i64", "i64", "i64", "i64"],
            ret: "void",
        },
        Builtin {
            name: "kodo_timestamp",
            params: &[],
            ret: "i64",
        },
        Builtin {
            name: "kodo_sleep",
            params: &["i64"],
            ret: "void",
        },
        // -- Option/Result synthetic builtins --
        Builtin {
            name: "kodo_option_is_some",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_option_is_none",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_option_unwrap",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_option_unwrap_or",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_result_is_ok",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_result_is_err",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_result_unwrap",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_result_unwrap_err",
            params: &["i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_result_unwrap_or",
            params: &["i64", "i64"],
            ret: "i64",
        },
        // -- List higher-order builtins --
        Builtin {
            name: "kodo_list_map",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_list_filter",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_list_fold",
            params: &["i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_list_reduce",
            params: &["i64", "i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_list_any",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_list_all",
            params: &["i64", "i64"],
            ret: "i64",
        },
        Builtin {
            name: "kodo_list_count",
            params: &["i64", "i64"],
            ret: "i64",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declarations_are_nonempty() {
        let decls = emit_runtime_declarations();
        assert!(!decls.is_empty());
    }

    #[test]
    fn declarations_have_correct_format() {
        let decls = emit_runtime_declarations();
        for decl in &decls {
            assert!(decl.starts_with("declare "), "bad declaration: {decl}");
            assert!(decl.contains('@'), "missing @ in declaration: {decl}");
        }
    }

    #[test]
    fn all_builtins_count() {
        // Sanity check: we have a significant number of builtins.
        let builtins = all_builtins();
        assert!(
            builtins.len() > 100,
            "expected 100+ builtins, got {}",
            builtins.len()
        );
    }
}
