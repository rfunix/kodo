//! Runtime builtin function declarations for the inkwell LLVM backend.
//!
//! Declares all Kodo runtime functions so they can be called from generated
//! code. These correspond to C-ABI functions provided by `libkodo_runtime`.

use inkwell::context::Context;
use inkwell::module::Module;

/// A single runtime builtin declaration for the inkwell backend.
struct InkwellBuiltin {
    /// The C-ABI function name.
    name: &'static str,
    /// Parameter types: `'i'` for i64, `'d'` for double.
    params: &'static [char],
    /// Return type: `'v'` for void, `'i'` for i64, `'d'` for double.
    ret: char,
}

/// Declares all runtime builtins in the given LLVM module.
///
/// This function declares every C-ABI function that compiled Kodo code may call,
/// matching the full list from the textual backend's `builtins.rs`.
#[allow(clippy::too_many_lines)]
pub(crate) fn declare_all_runtime_builtins<'a>(context: &'a Context, module: &Module<'a>) {
    let i64_ty = context.i64_type();
    let f64_ty = context.f64_type();
    let void_ty = context.void_type();

    let builtins = all_builtins();
    for b in &builtins {
        // Skip if already declared.
        if module.get_function(b.name).is_some() {
            continue;
        }

        let param_types: Vec<inkwell::types::BasicMetadataTypeEnum> = b
            .params
            .iter()
            .map(|c| match c {
                'd' => f64_ty.into(),
                _ => i64_ty.into(),
            })
            .collect();

        let fn_type = match b.ret {
            'v' => void_ty.fn_type(&param_types, false),
            'd' => f64_ty.fn_type(&param_types, false),
            _ => i64_ty.fn_type(&param_types, false),
        };

        module.add_function(b.name, fn_type, None);
    }

    // __env_pack_N: fixed-arity variants for closure environment packing (0..=8).
    for n in 0u32..=8 {
        let name = format!("__env_pack_{n}");
        if module.get_function(&name).is_some() {
            continue;
        }
        let params: Vec<inkwell::types::BasicMetadataTypeEnum> =
            (0..n).map(|_| i64_ty.into()).collect();
        let fn_type = i64_ty.fn_type(&params, false);
        module.add_function(&name, fn_type, None);
    }
}

/// Returns the full list of runtime builtins.
#[allow(clippy::too_many_lines)]
fn all_builtins() -> Vec<InkwellBuiltin> {
    vec![
        // -- I/O --
        InkwellBuiltin {
            name: "kodo_println",
            params: &['i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_print",
            params: &['i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_print_int",
            params: &['i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_print_float",
            params: &['d'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_println_float",
            params: &['d'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_contract_fail",
            params: &['i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_contract_fail_recoverable",
            params: &['i', 'i'],
            ret: 'v',
        },
        // -- Math --
        InkwellBuiltin {
            name: "kodo_abs",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_min",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_max",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_clamp",
            params: &['i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_sqrt",
            params: &['d'],
            ret: 'd',
        },
        InkwellBuiltin {
            name: "kodo_pow",
            params: &['d', 'd'],
            ret: 'd',
        },
        InkwellBuiltin {
            name: "kodo_sin",
            params: &['d'],
            ret: 'd',
        },
        InkwellBuiltin {
            name: "kodo_cos",
            params: &['d'],
            ret: 'd',
        },
        InkwellBuiltin {
            name: "kodo_log",
            params: &['d'],
            ret: 'd',
        },
        InkwellBuiltin {
            name: "kodo_floor",
            params: &['d'],
            ret: 'd',
        },
        InkwellBuiltin {
            name: "kodo_ceil",
            params: &['d'],
            ret: 'd',
        },
        InkwellBuiltin {
            name: "kodo_round",
            params: &['d'],
            ret: 'd',
        },
        InkwellBuiltin {
            name: "kodo_rand_int",
            params: &['i', 'i'],
            ret: 'i',
        },
        // -- Concurrency --
        InkwellBuiltin {
            name: "kodo_spawn_task",
            params: &['i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_spawn_task_with_env",
            params: &['i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_green_spawn",
            params: &['i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_green_spawn_with_env",
            params: &['i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_green_maybe_yield",
            params: &[],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_future_new",
            params: &[],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_future_complete",
            params: &['i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_future_await",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_future_complete_bytes",
            params: &['i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_future_await_bytes",
            params: &['i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_parallel_begin",
            params: &[],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_parallel_spawn",
            params: &['i', 'i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_parallel_join",
            params: &['i'],
            ret: 'v',
        },
        // -- String query --
        InkwellBuiltin {
            name: "kodo_string_length",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_string_byte_length",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_string_char_count",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_string_contains",
            params: &['i', 'i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_string_starts_with",
            params: &['i', 'i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_string_ends_with",
            params: &['i', 'i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_string_index_of",
            params: &['i', 'i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_string_eq",
            params: &['i', 'i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_string_split",
            params: &['i', 'i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_string_lines",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_string_parse_int",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_string_char_at",
            params: &['i', 'i', 'i'],
            ret: 'i',
        },
        // -- String transform --
        InkwellBuiltin {
            name: "kodo_string_trim",
            params: &['i', 'i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_string_to_upper",
            params: &['i', 'i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_string_to_lower",
            params: &['i', 'i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_string_substring",
            params: &['i', 'i', 'i', 'i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_string_concat",
            params: &['i', 'i', 'i', 'i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_string_replace",
            params: &['i', 'i', 'i', 'i', 'i', 'i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_string_repeat",
            params: &['i', 'i', 'i', 'i', 'i'],
            ret: 'v',
        },
        // -- Conversions --
        InkwellBuiltin {
            name: "kodo_int_to_string",
            params: &['i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_int_to_float64",
            params: &['i'],
            ret: 'd',
        },
        InkwellBuiltin {
            name: "kodo_float64_to_string",
            params: &['d', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_float64_to_int",
            params: &['d'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_bool_to_string",
            params: &['i', 'i', 'i'],
            ret: 'v',
        },
        // -- File I/O --
        InkwellBuiltin {
            name: "kodo_file_exists",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_file_read",
            params: &['i', 'i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_file_write",
            params: &['i', 'i', 'i', 'i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_file_append",
            params: &['i', 'i', 'i', 'i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_file_delete",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_dir_list",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_dir_exists",
            params: &['i', 'i'],
            ret: 'i',
        },
        // -- Collections: List --
        InkwellBuiltin {
            name: "kodo_list_new",
            params: &[],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_list_push",
            params: &['i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_list_get",
            params: &['i', 'i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_list_length",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_list_contains",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_list_pop_simple",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_list_remove",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_list_set",
            params: &['i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_list_is_empty",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_list_reverse",
            params: &['i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_list_slice",
            params: &['i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_list_sort",
            params: &['i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_list_sort_by",
            params: &['i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_list_join",
            params: &['i', 'i', 'i', 'i', 'i'],
            ret: 'v',
        },
        // -- Collections: Map --
        InkwellBuiltin {
            name: "kodo_map_new",
            params: &[],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_map_insert",
            params: &['i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_map_get",
            params: &['i', 'i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_map_contains_key",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_map_length",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_map_remove",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_map_is_empty",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_map_merge",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_map_filter",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_map_insert_sk",
            params: &['i', 'i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_map_get_sk",
            params: &['i', 'i', 'i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_map_contains_key_sk",
            params: &['i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_map_remove_sk",
            params: &['i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_map_free_sk",
            params: &['i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_map_insert_sv",
            params: &['i', 'i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_map_get_sv",
            params: &['i', 'i', 'i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_map_free_sv",
            params: &['i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_map_insert_ss",
            params: &['i', 'i', 'i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_map_get_ss",
            params: &['i', 'i', 'i', 'i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_map_free_ss",
            params: &['i'],
            ret: 'v',
        },
        // -- Collections: Set --
        InkwellBuiltin {
            name: "kodo_set_new",
            params: &[],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_set_add",
            params: &['i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_set_contains",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_set_remove",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_set_length",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_set_is_empty",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_set_union",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_set_intersection",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_set_difference",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_set_free",
            params: &['i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_set_to_list",
            params: &['i'],
            ret: 'i',
        },
        // -- Network / JSON --
        InkwellBuiltin {
            name: "kodo_http_get",
            params: &['i', 'i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_http_post",
            params: &['i', 'i', 'i', 'i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_json_parse",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_json_get_string",
            params: &['i', 'i', 'i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_json_get_int",
            params: &['i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_json_free",
            params: &['i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_json_stringify",
            params: &['i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_json_get_bool",
            params: &['i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_json_get_float",
            params: &['i', 'i', 'i'],
            ret: 'd',
        },
        InkwellBuiltin {
            name: "kodo_json_get_array",
            params: &['i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_json_get_object",
            params: &['i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_json_new_object",
            params: &[],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_json_set_string",
            params: &['i', 'i', 'i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_json_set_int",
            params: &['i', 'i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_json_set_bool",
            params: &['i', 'i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_json_set_float",
            params: &['i', 'i', 'i', 'd'],
            ret: 'v',
        },
        // -- Actor --
        InkwellBuiltin {
            name: "kodo_actor_new",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_actor_get_field",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_actor_set_field",
            params: &['i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_actor_send",
            params: &['i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_actor_free",
            params: &['i', 'i'],
            ret: 'v',
        },
        // -- Time --
        InkwellBuiltin {
            name: "kodo_time_now",
            params: &[],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_time_now_ms",
            params: &[],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_time_format",
            params: &['i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_time_elapsed_ms",
            params: &['i'],
            ret: 'i',
        },
        // -- Env --
        InkwellBuiltin {
            name: "kodo_env_get",
            params: &['i', 'i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_env_set",
            params: &['i', 'i', 'i', 'i'],
            ret: 'v',
        },
        // -- Cleanup --
        InkwellBuiltin {
            name: "kodo_string_free",
            params: &['i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_list_free",
            params: &['i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_map_free",
            params: &['i'],
            ret: 'v',
        },
        // -- Channels --
        InkwellBuiltin {
            name: "kodo_channel_new",
            params: &[],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_channel_send",
            params: &['i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_channel_recv",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_channel_send_bool",
            params: &['i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_channel_recv_bool",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_channel_send_string",
            params: &['i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_channel_recv_string",
            params: &['i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_channel_free",
            params: &['i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_channel_select_2",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_channel_select_3",
            params: &['i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_channel_generic_new",
            params: &[],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_channel_generic_send",
            params: &['i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_channel_generic_recv",
            params: &['i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_channel_generic_free",
            params: &['i'],
            ret: 'v',
        },
        // -- RC --
        InkwellBuiltin {
            name: "kodo_alloc",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_free",
            params: &['i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_rc_inc",
            params: &['i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_rc_dec",
            params: &['i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_rc_count",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_rc_inc_string",
            params: &['i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_rc_dec_string",
            params: &['i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_closure_new",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_closure_func",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_closure_env",
            params: &['i'],
            ret: 'i',
        },
        // -- Async --
        InkwellBuiltin {
            name: "kodo_spawn_async",
            params: &['i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_await",
            params: &['i'],
            ret: 'i',
        },
        // -- Iterators --
        InkwellBuiltin {
            name: "kodo_list_iter",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_list_iterator_advance",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_list_iterator_value",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_list_iterator_free",
            params: &['i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_string_chars",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_string_chars_advance",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_string_chars_value",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_string_chars_free",
            params: &['i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_map_keys",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_map_keys_advance",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_map_keys_value",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_map_keys_free",
            params: &['i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_map_values",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_map_values_advance",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_map_values_value",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_map_values_free",
            params: &['i'],
            ret: 'v',
        },
        // -- CLI --
        InkwellBuiltin {
            name: "kodo_args",
            params: &[],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_readln",
            params: &['i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_exit",
            params: &['i'],
            ret: 'v',
        },
        // -- HTTP Server --
        InkwellBuiltin {
            name: "kodo_http_server_new",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_http_server_recv",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_http_request_method",
            params: &['i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_http_request_path",
            params: &['i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_http_request_body",
            params: &['i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_http_respond",
            params: &['i', 'i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_http_server_free",
            params: &['i'],
            ret: 'v',
        },
        // -- Database --
        InkwellBuiltin {
            name: "kodo_db_open",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_db_execute",
            params: &['i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_db_query",
            params: &['i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_db_row_next",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_db_row_get_string",
            params: &['i', 'i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_db_row_get_int",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_db_row_advance",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_db_result_free",
            params: &['i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_db_close",
            params: &['i'],
            ret: 'v',
        },
        // -- Test --
        InkwellBuiltin {
            name: "kodo_assert",
            params: &['i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_assert_true",
            params: &['i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_assert_false",
            params: &['i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_assert_eq_int",
            params: &['i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_assert_eq_string",
            params: &['i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_assert_eq_bool",
            params: &['i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_assert_eq_float",
            params: &['d', 'd'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_assert_ne_int",
            params: &['i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_assert_ne_string",
            params: &['i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_assert_ne_bool",
            params: &['i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_assert_ne_float",
            params: &['d', 'd'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_test_start",
            params: &['i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_test_end",
            params: &[],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_test_skip",
            params: &[],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_test_summary",
            params: &['i', 'i', 'i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_test_set_timeout",
            params: &['i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_test_clear_timeout",
            params: &[],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_test_isolate_start",
            params: &[],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_test_isolate_end",
            params: &[],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_prop_start",
            params: &['i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_prop_gen_int",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_prop_gen_bool",
            params: &[],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_prop_gen_float",
            params: &['d', 'd'],
            ret: 'd',
        },
        InkwellBuiltin {
            name: "kodo_prop_gen_string",
            params: &['i'],
            ret: 'i',
        },
        // -- Async string helpers --
        // __future_await_string returns { i64, i64 } — handled specially below.
        InkwellBuiltin {
            name: "__future_complete_string",
            params: &['i', 'i', 'i'],
            ret: 'v',
        },
        // -- Closure environment helpers --
        InkwellBuiltin {
            name: "__env_load",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "__env_load_string",
            params: &['i', 'i', 'i'],
            ret: 'v',
        },
        // -- Stdlib expansion --
        InkwellBuiltin {
            name: "kodo_is_alpha",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_is_digit",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_is_alphanumeric",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_is_whitespace",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_char_from_code",
            params: &['i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_string_builder_new",
            params: &[],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_string_builder_push",
            params: &['i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_string_builder_push_char",
            params: &['i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_string_builder_to_string",
            params: &['i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_string_builder_len",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_format_int",
            params: &['i', 'i', 'i', 'i'],
            ret: 'v',
        },
        InkwellBuiltin {
            name: "kodo_timestamp",
            params: &[],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_sleep",
            params: &['i'],
            ret: 'v',
        },
        // -- Option/Result synthetic builtins --
        InkwellBuiltin {
            name: "kodo_option_is_some",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_option_is_none",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_option_unwrap",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_option_unwrap_or",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_result_is_ok",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_result_is_err",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_result_unwrap",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_result_unwrap_err",
            params: &['i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_result_unwrap_or",
            params: &['i', 'i'],
            ret: 'i',
        },
        // -- List higher-order builtins --
        InkwellBuiltin {
            name: "kodo_list_map",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_list_filter",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_list_fold",
            params: &['i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_list_reduce",
            params: &['i', 'i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_list_any",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_list_all",
            params: &['i', 'i'],
            ret: 'i',
        },
        InkwellBuiltin {
            name: "kodo_list_count",
            params: &['i', 'i'],
            ret: 'i',
        },
        // -- Regex --
        // kodo_regex_match(pattern_ptr, pattern_len, text_ptr, text_len) -> i64
        InkwellBuiltin {
            name: "kodo_regex_match",
            params: &['i', 'i', 'i', 'i'],
            ret: 'i',
        },
        // kodo_regex_find(pattern_ptr, pattern_len, text_ptr, text_len, out_ptr, out_len) -> i64
        // Returns 0 = Some, 1 = None; string result written through out_ptr/out_len.
        InkwellBuiltin {
            name: "kodo_regex_find",
            params: &['i', 'i', 'i', 'i', 'i', 'i'],
            ret: 'i',
        },
        // kodo_regex_replace(pattern_ptr, pattern_len, text_ptr, text_len,
        //                    repl_ptr, repl_len, out_ptr, out_len) -> void
        InkwellBuiltin {
            name: "kodo_regex_replace",
            params: &['i', 'i', 'i', 'i', 'i', 'i', 'i', 'i'],
            ret: 'v',
        },
    ]
}
