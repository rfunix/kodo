//! Declaration of runtime builtin functions as Cranelift imports.
//!
//! Each builtin corresponds to a C-ABI function provided by the Kōdo runtime
//! library (`libkodo_runtime`). This module forward-declares them so codegen
//! can emit `call` instructions that the linker resolves at link time.

use std::collections::HashMap;

use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{AbiParam, Signature};
use cranelift_codegen::isa::CallConv;
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::ObjectModule;

use crate::{CodegenError, Result};

/// Information about a runtime builtin function.
pub(crate) struct BuiltinInfo {
    /// Cranelift function ID.
    pub(crate) func_id: FuncId,
}

/// Helper to declare a single builtin import, reducing repetition.
fn declare_builtin(module: &mut ObjectModule, name: &str, sig: &Signature) -> Result<FuncId> {
    module
        .declare_function(name, Linkage::Import, sig)
        .map_err(|e| CodegenError::ModuleError(e.to_string()))
}

/// Helper to build a signature with only params (void return).
fn sig_void(call_conv: CallConv, params: &[cranelift_codegen::ir::types::Type]) -> Signature {
    let mut sig = Signature::new(call_conv);
    for p in params {
        sig.params.push(AbiParam::new(*p));
    }
    sig
}

/// Helper to build a signature with params and a single return type.
fn sig_ret(
    call_conv: CallConv,
    params: &[cranelift_codegen::ir::types::Type],
    ret: cranelift_codegen::ir::types::Type,
) -> Signature {
    let mut sig = sig_void(call_conv, params);
    sig.returns.push(AbiParam::new(ret));
    sig
}

/// Declares runtime builtin functions as imports in the object module.
pub(crate) fn declare_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
) -> Result<HashMap<String, BuiltinInfo>> {
    let mut builtins = HashMap::new();

    declare_io_builtins(module, call_conv, &mut builtins)?;
    declare_math_builtins(module, call_conv, &mut builtins)?;
    declare_concurrency_builtins(module, call_conv, &mut builtins)?;
    declare_string_builtins(module, call_conv, &mut builtins)?;
    declare_conversion_builtins(module, call_conv, &mut builtins)?;
    declare_file_io_builtins(module, call_conv, &mut builtins)?;
    declare_collection_builtins(module, call_conv, &mut builtins)?;
    declare_network_builtins(module, call_conv, &mut builtins)?;
    declare_actor_builtins(module, call_conv, &mut builtins)?;
    declare_time_builtins(module, call_conv, &mut builtins)?;
    declare_env_builtins(module, call_conv, &mut builtins)?;
    declare_cleanup_builtins(module, call_conv, &mut builtins)?;
    declare_channel_builtins(module, call_conv, &mut builtins)?;
    declare_rc_builtins(module, call_conv, &mut builtins)?;
    declare_async_builtins(module, call_conv, &mut builtins)?;
    declare_iterator_builtins(module, call_conv, &mut builtins)?;
    declare_cli_builtins(module, call_conv, &mut builtins)?;
    declare_file_extended_builtins(module, call_conv, &mut builtins)?;
    declare_json_builder_builtins(module, call_conv, &mut builtins)?;
    declare_math_extended_builtins(module, call_conv, &mut builtins)?;
    declare_http_server_builtins(module, call_conv, &mut builtins)?;
    declare_db_builtins(module, call_conv, &mut builtins)?;
    declare_test_builtins(module, call_conv, &mut builtins)?;

    Ok(builtins)
}

/// Declares I/O builtins (print, println, `contract_fail`).
fn declare_io_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_void {
        ($runtime_name:expr, $key:expr, $($param:expr),*) => {{
            let sig = sig_void(call_conv, &[$($param),*]);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    decl_void!("kodo_println", "println", types::I64, types::I64);
    decl_void!("kodo_print", "print", types::I64, types::I64);
    decl_void!("kodo_print_int", "print_int", types::I64);
    decl_void!("kodo_print_float", "print_float", types::F64);
    decl_void!("kodo_println_float", "println_float", types::F64);
    decl_void!(
        "kodo_contract_fail",
        "kodo_contract_fail",
        types::I64,
        types::I64
    );
    decl_void!(
        "kodo_contract_fail_recoverable",
        "kodo_contract_fail_recoverable",
        types::I64,
        types::I64
    );
    Ok(())
}

/// Declares math builtins (abs, min, max, clamp).
fn declare_math_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_ret {
        ($runtime_name:expr, $key:expr, [$($param:expr),*], $ret:expr) => {{
            let sig = sig_ret(call_conv, &[$($param),*], $ret);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    decl_ret!("kodo_abs", "abs", [types::I64], types::I64);
    decl_ret!("kodo_min", "min", [types::I64, types::I64], types::I64);
    decl_ret!("kodo_max", "max", [types::I64, types::I64], types::I64);
    decl_ret!(
        "kodo_clamp",
        "clamp",
        [types::I64, types::I64, types::I64],
        types::I64
    );
    Ok(())
}

/// Declares concurrency builtins (spawn, parallel).
fn declare_concurrency_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_void {
        ($runtime_name:expr, $key:expr, $($param:expr),*) => {{
            let sig = sig_void(call_conv, &[$($param),*]);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }
    macro_rules! decl_ret {
        ($runtime_name:expr, $key:expr, [$($param:expr),*], $ret:expr) => {{
            let sig = sig_ret(call_conv, &[$($param),*], $ret);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    decl_void!("kodo_spawn_task", "kodo_spawn_task", types::I64);
    decl_void!(
        "kodo_spawn_task_with_env",
        "kodo_spawn_task_with_env",
        types::I64,
        types::I64,
        types::I64
    );
    // Green thread spawn builtins.
    decl_void!("kodo_green_spawn", "kodo_green_spawn", types::I64);
    decl_void!(
        "kodo_green_spawn_with_env",
        "kodo_green_spawn_with_env",
        types::I64,
        types::I64,
        types::I64
    );
    // Cooperative yield: allows the green-thread scheduler to switch coroutines.
    {
        let sig = sig_void(call_conv, &[]);
        let func_id = declare_builtin(module, "kodo_green_maybe_yield", &sig)?;
        builtins.insert(
            "kodo_green_maybe_yield".to_string(),
            BuiltinInfo { func_id },
        );
    }
    // Future builtins for async/await.
    decl_ret!("kodo_future_new", "kodo_future_new", [], types::I64);
    decl_void!(
        "kodo_future_complete",
        "kodo_future_complete",
        types::I64,
        types::I64
    );
    decl_ret!(
        "kodo_future_await",
        "kodo_future_await",
        [types::I64],
        types::I64
    );
    // Byte-buffer variants for composite return types (e.g., String).
    decl_void!(
        "kodo_future_complete_bytes",
        "kodo_future_complete_bytes",
        types::I64,
        types::I64,
        types::I64
    );
    decl_void!(
        "kodo_future_await_bytes",
        "kodo_future_await_bytes",
        types::I64,
        types::I64,
        types::I64
    );
    decl_ret!("kodo_parallel_begin", "kodo_parallel_begin", [], types::I64);
    decl_void!(
        "kodo_parallel_spawn",
        "kodo_parallel_spawn",
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    decl_void!("kodo_parallel_join", "kodo_parallel_join", types::I64);
    Ok(())
}

/// Declares string method builtins (length, contains, trim, etc.).
fn declare_string_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    declare_string_query_builtins(module, call_conv, builtins)?;
    declare_string_transform_builtins(module, call_conv, builtins)?;
    Ok(())
}

/// Declares string query builtins (length, contains, starts/ends with, `index_of`, eq).
fn declare_string_query_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_ret {
        ($runtime_name:expr, $key:expr, [$($param:expr),*], $ret:expr) => {{
            let sig = sig_ret(call_conv, &[$($param),*], $ret);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    decl_ret!(
        "kodo_string_length",
        "String_length",
        [types::I64, types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_string_byte_length",
        "String_byte_length",
        [types::I64, types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_string_char_count",
        "String_char_count",
        [types::I64, types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_string_contains",
        "String_contains",
        [types::I64, types::I64, types::I64, types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_string_starts_with",
        "String_starts_with",
        [types::I64, types::I64, types::I64, types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_string_ends_with",
        "String_ends_with",
        [types::I64, types::I64, types::I64, types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_string_index_of",
        "String_index_of",
        [types::I64, types::I64, types::I64, types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_string_eq",
        "String_eq",
        [types::I64, types::I64, types::I64, types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_string_split",
        "String_split",
        [types::I64, types::I64, types::I64, types::I64],
        types::I64
    );
    // string_lines: (ptr, len) -> i64 (list handle)
    decl_ret!(
        "kodo_string_lines",
        "String_lines",
        [types::I64, types::I64],
        types::I64
    );
    // string_parse_int: (ptr, len) -> i64
    decl_ret!(
        "kodo_string_parse_int",
        "String_parse_int",
        [types::I64, types::I64],
        types::I64
    );
    // string_char_at: (ptr, len, index) -> i64
    decl_ret!(
        "kodo_string_char_at",
        "String_char_at",
        [types::I64, types::I64, types::I64],
        types::I64
    );
    Ok(())
}

/// Declares string transform builtins (trim, upper, lower, substring, concat, replace).
fn declare_string_transform_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_void {
        ($runtime_name:expr, $key:expr, $($param:expr),*) => {{
            let sig = sig_void(call_conv, &[$($param),*]);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    decl_void!(
        "kodo_string_trim",
        "String_trim",
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    decl_void!(
        "kodo_string_to_upper",
        "String_to_upper",
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    decl_void!(
        "kodo_string_to_lower",
        "String_to_lower",
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    decl_void!(
        "kodo_string_substring",
        "String_substring",
        types::I64,
        types::I64,
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    decl_void!(
        "kodo_string_concat",
        "String_concat",
        types::I64,
        types::I64,
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    decl_void!(
        "kodo_string_replace",
        "String_replace",
        types::I64,
        types::I64,
        types::I64,
        types::I64,
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    // string_repeat: (ptr, len, count, out_ptr, out_len) -> void
    decl_void!(
        "kodo_string_repeat",
        "String_repeat",
        types::I64,
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    Ok(())
}

/// Declares type conversion builtins (Int/Float64 to string/float/int).
fn declare_conversion_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_void {
        ($runtime_name:expr, $key:expr, $($param:expr),*) => {{
            let sig = sig_void(call_conv, &[$($param),*]);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }
    macro_rules! decl_ret {
        ($runtime_name:expr, $key:expr, [$($param:expr),*], $ret:expr) => {{
            let sig = sig_ret(call_conv, &[$($param),*], $ret);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    decl_void!(
        "kodo_int_to_string",
        "Int_to_string",
        types::I64,
        types::I64,
        types::I64
    );
    decl_ret!(
        "kodo_int_to_float64",
        "Int_to_float64",
        [types::I64],
        types::F64
    );
    decl_void!(
        "kodo_float64_to_string",
        "Float64_to_string",
        types::F64,
        types::I64,
        types::I64
    );
    decl_ret!(
        "kodo_float64_to_int",
        "Float64_to_int",
        [types::F64],
        types::I64
    );
    decl_void!(
        "kodo_bool_to_string",
        "Bool_to_string",
        types::I64,
        types::I64,
        types::I64
    );
    Ok(())
}

/// Declares file I/O builtins (exists, read, write).
fn declare_file_io_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_ret {
        ($runtime_name:expr, $key:expr, [$($param:expr),*], $ret:expr) => {{
            let sig = sig_ret(call_conv, &[$($param),*], $ret);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    decl_ret!(
        "kodo_file_exists",
        "file_exists",
        [types::I64, types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_file_read",
        "file_read",
        [types::I64, types::I64, types::I64, types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_file_write",
        "file_write",
        [
            types::I64,
            types::I64,
            types::I64,
            types::I64,
            types::I64,
            types::I64
        ],
        types::I64
    );
    Ok(())
}

/// Declares list and map collection builtins.
fn declare_collection_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    declare_list_builtins(module, call_conv, builtins)?;
    declare_map_builtins_impl(module, call_conv, builtins)?;
    Ok(())
}

/// Declares list builtins (new, push, get, length, slice, sort, join, etc.).
fn declare_list_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_void {
        ($runtime_name:expr, $key:expr, $($param:expr),*) => {{
            let sig = sig_void(call_conv, &[$($param),*]);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }
    macro_rules! decl_ret {
        ($runtime_name:expr, $key:expr, [$($param:expr),*], $ret:expr) => {{
            let sig = sig_ret(call_conv, &[$($param),*], $ret);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    decl_ret!("kodo_list_new", "list_new", [], types::I64);
    decl_void!("kodo_list_push", "list_push", types::I64, types::I64);
    decl_void!(
        "kodo_list_get",
        "list_get",
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    decl_ret!("kodo_list_length", "list_length", [types::I64], types::I64);
    decl_ret!(
        "kodo_list_contains",
        "list_contains",
        [types::I64, types::I64],
        types::I64
    );
    decl_ret!("kodo_list_pop_simple", "list_pop", [types::I64], types::I64);
    decl_ret!(
        "kodo_list_remove",
        "list_remove",
        [types::I64, types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_list_set",
        "list_set",
        [types::I64, types::I64, types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_list_is_empty",
        "list_is_empty",
        [types::I64],
        types::I64
    );
    decl_void!("kodo_list_reverse", "list_reverse", types::I64);
    decl_ret!(
        "kodo_list_slice",
        "list_slice",
        [types::I64, types::I64, types::I64],
        types::I64
    );
    decl_void!("kodo_list_sort", "list_sort", types::I64);
    decl_void!(
        "kodo_list_join",
        "list_join",
        types::I64,
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    Ok(())
}

/// Declares map builtins (new, insert, get, etc.).
#[allow(clippy::too_many_lines)]
fn declare_map_builtins_impl(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_void {
        ($runtime_name:expr, $key:expr, $($param:expr),*) => {{
            let sig = sig_void(call_conv, &[$($param),*]);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }
    macro_rules! decl_ret {
        ($runtime_name:expr, $key:expr, [$($param:expr),*], $ret:expr) => {{
            let sig = sig_ret(call_conv, &[$($param),*], $ret);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    decl_ret!("kodo_map_new", "map_new", [], types::I64);
    decl_void!(
        "kodo_map_insert",
        "map_insert",
        types::I64,
        types::I64,
        types::I64
    );
    decl_void!(
        "kodo_map_get",
        "map_get",
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    decl_ret!(
        "kodo_map_contains_key",
        "map_contains_key",
        [types::I64, types::I64],
        types::I64
    );
    decl_ret!("kodo_map_length", "map_length", [types::I64], types::I64);
    decl_ret!(
        "kodo_map_remove",
        "map_remove",
        [types::I64, types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_map_is_empty",
        "map_is_empty",
        [types::I64],
        types::I64
    );

    // -- String Key variants (Map<String, Int> and Map<String, String>) --
    decl_void!(
        "kodo_map_insert_sk",
        "map_insert_sk",
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    decl_void!(
        "kodo_map_get_sk",
        "map_get_sk",
        types::I64,
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    decl_ret!(
        "kodo_map_contains_key_sk",
        "map_contains_key_sk",
        [types::I64, types::I64, types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_map_remove_sk",
        "map_remove_sk",
        [types::I64, types::I64, types::I64],
        types::I64
    );
    decl_void!("kodo_map_free_sk", "map_free_sk", types::I64);

    // -- String Value variants (Map<Int, String>) --
    decl_void!(
        "kodo_map_insert_sv",
        "map_insert_sv",
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    decl_void!(
        "kodo_map_get_sv",
        "map_get_sv",
        types::I64,
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    decl_void!("kodo_map_free_sv", "map_free_sv", types::I64);

    // -- Both String variants (Map<String, String>) --
    decl_void!(
        "kodo_map_insert_ss",
        "map_insert_ss",
        types::I64,
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    decl_void!(
        "kodo_map_get_ss",
        "map_get_ss",
        types::I64,
        types::I64,
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    decl_void!("kodo_map_free_ss", "map_free_ss", types::I64);

    Ok(())
}

/// Declares HTTP client and JSON parsing builtins.
fn declare_network_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_void {
        ($runtime_name:expr, $key:expr, $($param:expr),*) => {{
            let sig = sig_void(call_conv, &[$($param),*]);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }
    macro_rules! decl_ret {
        ($runtime_name:expr, $key:expr, [$($param:expr),*], $ret:expr) => {{
            let sig = sig_ret(call_conv, &[$($param),*], $ret);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    // HTTP client
    decl_ret!(
        "kodo_http_get",
        "http_get",
        [types::I64, types::I64, types::I64, types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_http_post",
        "http_post",
        [
            types::I64,
            types::I64,
            types::I64,
            types::I64,
            types::I64,
            types::I64
        ],
        types::I64
    );

    // JSON parsing
    decl_ret!(
        "kodo_json_parse",
        "json_parse",
        [types::I64, types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_json_get_string",
        "json_get_string",
        [types::I64, types::I64, types::I64, types::I64, types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_json_get_int",
        "json_get_int",
        [types::I64, types::I64, types::I64],
        types::I64
    );
    decl_void!("kodo_json_free", "json_free", types::I64);
    // json_stringify: (handle, out_ptr, out_len) -> void (string-returning via out-params)
    decl_void!(
        "kodo_json_stringify",
        "json_stringify",
        types::I64,
        types::I64,
        types::I64
    );
    // json_get_bool: (handle, key_ptr, key_len) -> i64
    decl_ret!(
        "kodo_json_get_bool",
        "json_get_bool",
        [types::I64, types::I64, types::I64],
        types::I64
    );
    // json_get_float: (handle, key_ptr, key_len) -> f64
    decl_ret!(
        "kodo_json_get_float",
        "json_get_float",
        [types::I64, types::I64, types::I64],
        types::F64
    );
    // json_get_array: (handle, key_ptr, key_len) -> i64
    decl_ret!(
        "kodo_json_get_array",
        "json_get_array",
        [types::I64, types::I64, types::I64],
        types::I64
    );
    // json_get_object: (handle, key_ptr, key_len) -> i64
    decl_ret!(
        "kodo_json_get_object",
        "json_get_object",
        [types::I64, types::I64, types::I64],
        types::I64
    );
    Ok(())
}

/// Declares actor runtime builtins (new, get/set field, send, free).
fn declare_actor_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_void {
        ($runtime_name:expr, $key:expr, $($param:expr),*) => {{
            let sig = sig_void(call_conv, &[$($param),*]);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }
    macro_rules! decl_ret {
        ($runtime_name:expr, $key:expr, [$($param:expr),*], $ret:expr) => {{
            let sig = sig_ret(call_conv, &[$($param),*], $ret);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    decl_ret!("kodo_actor_new", "kodo_actor_new", [types::I64], types::I64);
    decl_ret!(
        "kodo_actor_get_field",
        "kodo_actor_get_field",
        [types::I64, types::I64],
        types::I64
    );
    decl_void!(
        "kodo_actor_set_field",
        "kodo_actor_set_field",
        types::I64,
        types::I64,
        types::I64
    );
    decl_void!(
        "kodo_actor_send",
        "kodo_actor_send",
        types::I64,
        types::I64,
        types::I64
    );
    decl_void!("kodo_actor_free", "kodo_actor_free", types::I64, types::I64);
    Ok(())
}

/// Declares time builtins (now, format, elapsed).
fn declare_time_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_void {
        ($runtime_name:expr, $key:expr, $($param:expr),*) => {{
            let sig = sig_void(call_conv, &[$($param),*]);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }
    macro_rules! decl_ret {
        ($runtime_name:expr, $key:expr, [$($param:expr),*], $ret:expr) => {{
            let sig = sig_ret(call_conv, &[$($param),*], $ret);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    decl_ret!("kodo_time_now", "time_now", [], types::I64);
    decl_ret!("kodo_time_now_ms", "time_now_ms", [], types::I64);
    decl_void!(
        "kodo_time_format",
        "time_format",
        types::I64,
        types::I64,
        types::I64
    );
    decl_ret!(
        "kodo_time_elapsed_ms",
        "time_elapsed_ms",
        [types::I64],
        types::I64
    );
    Ok(())
}

/// Declares environment variable builtins (get, set).
fn declare_env_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_void {
        ($runtime_name:expr, $key:expr, $($param:expr),*) => {{
            let sig = sig_void(call_conv, &[$($param),*]);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    decl_void!(
        "kodo_env_get",
        "env_get",
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    decl_void!(
        "kodo_env_set",
        "env_set",
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    Ok(())
}

/// Declares cleanup builtins for heap-allocated values (string, list, map free).
fn declare_cleanup_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_void {
        ($runtime_name:expr, $key:expr, $($param:expr),*) => {{
            let sig = sig_void(call_conv, &[$($param),*]);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    decl_void!(
        "kodo_string_free",
        "kodo_string_free",
        types::I64,
        types::I64
    );
    decl_void!("kodo_list_free", "kodo_list_free", types::I64);
    decl_void!("kodo_map_free", "kodo_map_free", types::I64);
    Ok(())
}

/// Declares channel builtins for inter-thread communication.
fn declare_channel_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_void {
        ($runtime_name:expr, $key:expr, $($param:expr),*) => {{
            let sig = sig_void(call_conv, &[$($param),*]);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }
    macro_rules! decl_ret {
        ($runtime_name:expr, $key:expr, [$($param:expr),*], $ret:expr) => {{
            let sig = sig_ret(call_conv, &[$($param),*], $ret);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    decl_ret!("kodo_channel_new", "channel_new", [], types::I64);
    // channel_new_bool and channel_new_string map to the same runtime function
    // (channels are type-erased at runtime — only the type checker differentiates).
    decl_ret!("kodo_channel_new", "channel_new_bool", [], types::I64);
    decl_ret!("kodo_channel_new", "channel_new_string", [], types::I64);
    decl_void!("kodo_channel_send", "channel_send", types::I64, types::I64);
    decl_ret!(
        "kodo_channel_recv",
        "channel_recv",
        [types::I64],
        types::I64
    );
    decl_void!(
        "kodo_channel_send_bool",
        "channel_send_bool",
        types::I64,
        types::I64
    );
    decl_ret!(
        "kodo_channel_recv_bool",
        "channel_recv_bool",
        [types::I64],
        types::I64
    );
    decl_void!(
        "kodo_channel_send_string",
        "channel_send_string",
        types::I64,
        types::I64,
        types::I64
    );
    decl_void!(
        "kodo_channel_recv_string",
        "channel_recv_string",
        types::I64,
        types::I64,
        types::I64
    );
    decl_void!("kodo_channel_free", "channel_free", types::I64);

    // Channel select — wait on multiple channels, return index of first ready.
    decl_ret!(
        "kodo_channel_select_2",
        "channel_select_2",
        [types::I64, types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_channel_select_3",
        "channel_select_3",
        [types::I64, types::I64, types::I64],
        types::I64
    );

    // Generic type-erased channel builtins.
    decl_ret!(
        "kodo_channel_generic_new",
        "channel_generic_new",
        [],
        types::I64
    );
    decl_void!(
        "kodo_channel_generic_send",
        "channel_generic_send",
        types::I64,
        types::I64,
        types::I64
    );
    decl_ret!(
        "kodo_channel_generic_recv",
        "channel_generic_recv",
        [types::I64, types::I64, types::I64],
        types::I64
    );
    decl_void!(
        "kodo_channel_generic_free",
        "channel_generic_free",
        types::I64
    );
    Ok(())
}

/// Declares reference counting builtins (Phase 39: alloc, free, inc, dec,
/// count for handles and inc/dec for strings).
fn declare_rc_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_void {
        ($runtime_name:expr, $key:expr, $($param:expr),*) => {{
            let sig = sig_void(call_conv, &[$($param),*]);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    // kodo_alloc(size: i64) -> i64 (returns user-data pointer)
    {
        let sig = sig_ret(call_conv, &[types::I64], types::I64);
        let func_id = declare_builtin(module, "kodo_alloc", &sig)?;
        builtins.insert("kodo_alloc".to_string(), BuiltinInfo { func_id });
    }

    // kodo_free(handle: i64) -> void
    decl_void!("kodo_free", "kodo_free", types::I64);

    decl_void!("kodo_rc_inc", "kodo_rc_inc", types::I64);
    decl_void!("kodo_rc_dec", "kodo_rc_dec", types::I64);

    // kodo_rc_count(handle: i64) -> i64
    {
        let sig = sig_ret(call_conv, &[types::I64], types::I64);
        let func_id = declare_builtin(module, "kodo_rc_count", &sig)?;
        builtins.insert("kodo_rc_count".to_string(), BuiltinInfo { func_id });
    }

    decl_void!(
        "kodo_rc_inc_string",
        "kodo_rc_inc_string",
        types::I64,
        types::I64
    );
    decl_void!(
        "kodo_rc_dec_string",
        "kodo_rc_dec_string",
        types::I64,
        types::I64
    );

    // Closure handle functions
    {
        let sig = sig_ret(call_conv, &[types::I64, types::I64], types::I64);
        let func_id = declare_builtin(module, "kodo_closure_new", &sig)?;
        builtins.insert("kodo_closure_new".to_string(), BuiltinInfo { func_id });
    }
    {
        let sig = sig_ret(call_conv, &[types::I64], types::I64);
        let func_id = declare_builtin(module, "kodo_closure_func", &sig)?;
        builtins.insert("kodo_closure_func".to_string(), BuiltinInfo { func_id });
    }
    {
        let sig = sig_ret(call_conv, &[types::I64], types::I64);
        let func_id = declare_builtin(module, "kodo_closure_env", &sig)?;
        builtins.insert("kodo_closure_env".to_string(), BuiltinInfo { func_id });
    }
    Ok(())
}

/// Declares async builtins (spawn, await).
fn declare_async_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_ret {
        ($runtime_name:expr, $key:expr, [$($param:expr),*], $ret:expr) => {{
            let sig = sig_ret(call_conv, &[$($param),*], $ret);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    decl_ret!(
        "kodo_spawn_async",
        "kodo_spawn_async",
        [types::I64, types::I64, types::I64],
        types::I64
    );
    decl_ret!("kodo_await", "kodo_await", [types::I64], types::I64);
    Ok(())
}

/// Declares iterator builtins for List, String chars, and Map keys/values.
fn declare_iterator_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_void {
        ($runtime_name:expr, $key:expr, $($param:expr),*) => {{
            let sig = sig_void(call_conv, &[$($param),*]);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }
    macro_rules! decl_ret {
        ($runtime_name:expr, $key:expr, [$($param:expr),*], $ret:expr) => {{
            let sig = sig_ret(call_conv, &[$($param),*], $ret);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    // List iterator
    decl_ret!("kodo_list_iter", "list_iter", [types::I64], types::I64);
    decl_ret!(
        "kodo_list_iterator_advance",
        "list_iterator_advance",
        [types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_list_iterator_value",
        "list_iterator_value",
        [types::I64],
        types::I64
    );
    decl_void!("kodo_list_iterator_free", "list_iterator_free", types::I64);

    // String chars iterator: takes (ptr, len) as i64 args
    decl_ret!(
        "kodo_string_chars",
        "String_chars",
        [types::I64, types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_string_chars_advance",
        "string_chars_advance",
        [types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_string_chars_value",
        "string_chars_value",
        [types::I64],
        types::I64
    );
    decl_void!("kodo_string_chars_free", "string_chars_free", types::I64);

    // Map keys iterator
    decl_ret!("kodo_map_keys", "Map_keys", [types::I64], types::I64);
    decl_ret!(
        "kodo_map_keys_advance",
        "map_keys_advance",
        [types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_map_keys_value",
        "map_keys_value",
        [types::I64],
        types::I64
    );
    decl_void!("kodo_map_keys_free", "map_keys_free", types::I64);

    // Map values iterator
    decl_ret!("kodo_map_values", "Map_values", [types::I64], types::I64);
    decl_ret!(
        "kodo_map_values_advance",
        "map_values_advance",
        [types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_map_values_value",
        "map_values_value",
        [types::I64],
        types::I64
    );
    decl_void!("kodo_map_values_free", "map_values_free", types::I64);

    Ok(())
}

/// Declares CLI builtins (args, readln, exit).
fn declare_cli_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_void {
        ($runtime_name:expr, $key:expr, $($param:expr),*) => {{
            let sig = sig_void(call_conv, &[$($param),*]);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }
    macro_rules! decl_ret {
        ($runtime_name:expr, $key:expr, [$($param:expr),*], $ret:expr) => {{
            let sig = sig_ret(call_conv, &[$($param),*], $ret);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    // args() -> i64 (list handle)
    decl_ret!("kodo_args", "args", [], types::I64);
    // readln(out_ptr, out_len) -> void (string-returning)
    decl_void!("kodo_readln", "readln", types::I64, types::I64);
    // exit(code) -> void
    decl_void!("kodo_exit", "exit", types::I64);
    Ok(())
}

/// Declares extended file I/O builtins (append, delete, `dir_list`, `dir_exists`).
fn declare_file_extended_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_ret {
        ($runtime_name:expr, $key:expr, [$($param:expr),*], $ret:expr) => {{
            let sig = sig_ret(call_conv, &[$($param),*], $ret);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    // file_append: (path_ptr, path_len, content_ptr, content_len, out_ptr, out_len) -> i64
    decl_ret!(
        "kodo_file_append",
        "file_append",
        [
            types::I64,
            types::I64,
            types::I64,
            types::I64,
            types::I64,
            types::I64
        ],
        types::I64
    );
    // file_delete: (path_ptr, path_len) -> i64
    decl_ret!(
        "kodo_file_delete",
        "file_delete",
        [types::I64, types::I64],
        types::I64
    );
    // dir_list: (path_ptr, path_len) -> i64 (list handle)
    decl_ret!(
        "kodo_dir_list",
        "dir_list",
        [types::I64, types::I64],
        types::I64
    );
    // dir_exists: (path_ptr, path_len) -> i64
    decl_ret!(
        "kodo_dir_exists",
        "dir_exists",
        [types::I64, types::I64],
        types::I64
    );
    Ok(())
}

/// Declares JSON builder builtins (`new_object`, `set_string`, `set_int`, `set_bool`).
fn declare_json_builder_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_void {
        ($runtime_name:expr, $key:expr, $($param:expr),*) => {{
            let sig = sig_void(call_conv, &[$($param),*]);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }
    macro_rules! decl_ret {
        ($runtime_name:expr, $key:expr, [$($param:expr),*], $ret:expr) => {{
            let sig = sig_ret(call_conv, &[$($param),*], $ret);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    // json_new_object() -> i64
    decl_ret!("kodo_json_new_object", "json_new_object", [], types::I64);
    // json_set_string(handle, key_ptr, key_len, val_ptr, val_len) -> void
    decl_void!(
        "kodo_json_set_string",
        "json_set_string",
        types::I64,
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    // json_set_int(handle, key_ptr, key_len, value) -> void
    decl_void!(
        "kodo_json_set_int",
        "json_set_int",
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    // json_set_bool(handle, key_ptr, key_len, value) -> void
    decl_void!(
        "kodo_json_set_bool",
        "json_set_bool",
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    // json_set_float(handle, key_ptr, key_len, float_value) -> void
    decl_void!(
        "kodo_json_set_float",
        "json_set_float",
        types::I64,
        types::I64,
        types::I64,
        types::F64
    );
    Ok(())
}

/// Declares extended math builtins (`sqrt`, `pow`, trig, `floor`, `ceil`, `round`, `rand_int`).
fn declare_math_extended_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_ret {
        ($runtime_name:expr, $key:expr, [$($param:expr),*], $ret:expr) => {{
            let sig = sig_ret(call_conv, &[$($param),*], $ret);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    decl_ret!("kodo_sqrt", "sqrt", [types::F64], types::F64);
    decl_ret!("kodo_pow", "pow", [types::F64, types::F64], types::F64);
    decl_ret!("kodo_sin", "sin", [types::F64], types::F64);
    decl_ret!("kodo_cos", "cos", [types::F64], types::F64);
    decl_ret!("kodo_log", "log", [types::F64], types::F64);
    decl_ret!("kodo_floor", "floor", [types::F64], types::F64);
    decl_ret!("kodo_ceil", "ceil", [types::F64], types::F64);
    decl_ret!("kodo_round", "round", [types::F64], types::F64);
    decl_ret!(
        "kodo_rand_int",
        "rand_int",
        [types::I64, types::I64],
        types::I64
    );
    Ok(())
}

/// Declares `SQLite` database builtins (open, execute, query, row access, close).
fn declare_db_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_void {
        ($runtime_name:expr, $key:expr, $($param:expr),*) => {{
            let sig = sig_void(call_conv, &[$($param),*]);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }
    macro_rules! decl_ret {
        ($runtime_name:expr, $key:expr, [$($param:expr),*], $ret:expr) => {{
            let sig = sig_ret(call_conv, &[$($param),*], $ret);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    // db_open: (path_ptr, path_len) -> i64
    decl_ret!(
        "kodo_db_open",
        "db_open",
        [types::I64, types::I64],
        types::I64
    );
    // db_execute: (db, sql_ptr, sql_len) -> i64
    decl_ret!(
        "kodo_db_execute",
        "db_execute",
        [types::I64, types::I64, types::I64],
        types::I64
    );
    // db_query: (db, sql_ptr, sql_len) -> i64
    decl_ret!(
        "kodo_db_query",
        "db_query",
        [types::I64, types::I64, types::I64],
        types::I64
    );
    // db_row_next: (result) -> i64
    decl_ret!("kodo_db_row_next", "db_row_next", [types::I64], types::I64);
    // db_row_get_string: (result, col, out_ptr, out_len) -> void
    decl_void!(
        "kodo_db_row_get_string",
        "db_row_get_string",
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    // db_row_get_int: (result, col) -> i64
    decl_ret!(
        "kodo_db_row_get_int",
        "db_row_get_int",
        [types::I64, types::I64],
        types::I64
    );
    // db_row_advance: (result) -> i64
    decl_ret!(
        "kodo_db_row_advance",
        "db_row_advance",
        [types::I64],
        types::I64
    );
    // db_result_free: (result) -> void
    decl_void!("kodo_db_result_free", "db_result_free", types::I64);
    // db_close: (db) -> void
    decl_void!("kodo_db_close", "db_close", types::I64);
    Ok(())
}

/// Declares HTTP server builtins (`server_new`, recv, request methods, respond, free).
fn declare_http_server_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_void {
        ($runtime_name:expr, $key:expr, $($param:expr),*) => {{
            let sig = sig_void(call_conv, &[$($param),*]);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }
    macro_rules! decl_ret {
        ($runtime_name:expr, $key:expr, [$($param:expr),*], $ret:expr) => {{
            let sig = sig_ret(call_conv, &[$($param),*], $ret);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    decl_ret!(
        "kodo_http_server_new",
        "http_server_new",
        [types::I64],
        types::I64
    );
    decl_ret!(
        "kodo_http_server_recv",
        "http_server_recv",
        [types::I64],
        types::I64
    );
    // http_request_method: (req, out_ptr, out_len) -> void (string-returning)
    decl_void!(
        "kodo_http_request_method",
        "http_request_method",
        types::I64,
        types::I64,
        types::I64
    );
    // http_request_path: (req, out_ptr, out_len) -> void (string-returning)
    decl_void!(
        "kodo_http_request_path",
        "http_request_path",
        types::I64,
        types::I64,
        types::I64
    );
    // http_request_body: (req, out_ptr, out_len) -> void (string-returning)
    decl_void!(
        "kodo_http_request_body",
        "http_request_body",
        types::I64,
        types::I64,
        types::I64
    );
    // http_respond: (req, status, body_ptr, body_len) -> void
    decl_void!(
        "kodo_http_respond",
        "http_respond",
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    decl_void!("kodo_http_server_free", "http_server_free", types::I64);
    Ok(())
}

/// Declares test framework builtins (assertions, test lifecycle, and property testing).
#[allow(clippy::too_many_lines)]
fn declare_test_builtins(
    module: &mut ObjectModule,
    call_conv: CallConv,
    builtins: &mut HashMap<String, BuiltinInfo>,
) -> Result<()> {
    macro_rules! decl_void {
        ($runtime_name:expr, $key:expr) => {{
            let sig = sig_void(call_conv, &[]);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
        ($runtime_name:expr, $key:expr, $($param:expr),+) => {{
            let sig = sig_void(call_conv, &[$($param),+]);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }
    macro_rules! decl_ret {
        ($runtime_name:expr, $key:expr, [$($param:expr),*], $ret:expr) => {{
            let sig = sig_ret(call_conv, &[$($param),*], $ret);
            let func_id = declare_builtin(module, $runtime_name, &sig)?;
            builtins.insert($key.to_string(), BuiltinInfo { func_id });
        }};
    }

    // Simple assertion builtins — Bool may be I8 (direct) or I64 (after uextend
    // from comparison results), so we declare as I64 and truncate in the runtime.
    decl_void!("kodo_assert", "assert", types::I64);
    decl_void!("kodo_assert_true", "assert_true", types::I64);
    decl_void!("kodo_assert_false", "assert_false", types::I64);

    // assert_eq monomorphized variants.
    decl_void!(
        "kodo_assert_eq_int",
        "kodo_assert_eq_int",
        types::I64,
        types::I64
    );
    // Strings are composite types passed as pointers to 16-byte (ptr+len) slots.
    decl_void!(
        "kodo_assert_eq_string",
        "kodo_assert_eq_string",
        types::I64,
        types::I64
    );
    decl_void!(
        "kodo_assert_eq_bool",
        "kodo_assert_eq_bool",
        types::I64,
        types::I64
    );
    decl_void!(
        "kodo_assert_eq_float",
        "kodo_assert_eq_float",
        types::F64,
        types::F64
    );

    // assert_ne monomorphized variants.
    decl_void!(
        "kodo_assert_ne_int",
        "kodo_assert_ne_int",
        types::I64,
        types::I64
    );
    decl_void!(
        "kodo_assert_ne_string",
        "kodo_assert_ne_string",
        types::I64,
        types::I64
    );
    decl_void!(
        "kodo_assert_ne_bool",
        "kodo_assert_ne_bool",
        types::I64,
        types::I64
    );
    decl_void!(
        "kodo_assert_ne_float",
        "kodo_assert_ne_float",
        types::F64,
        types::F64
    );

    // Test lifecycle builtins.
    // kodo_test_start takes a String (composite → pointer to 16-byte slot).
    decl_void!("kodo_test_start", "kodo_test_start", types::I64);
    decl_ret!("kodo_test_end", "kodo_test_end", [], types::I64);
    decl_void!("kodo_test_skip", "kodo_test_skip");
    decl_void!(
        "kodo_test_summary",
        "kodo_test_summary",
        types::I64,
        types::I64,
        types::I64,
        types::I64,
        types::I64
    );
    // Timeout and isolation builtins.
    decl_void!("kodo_test_set_timeout", "kodo_test_set_timeout", types::I64);
    decl_void!("kodo_test_clear_timeout", "kodo_test_clear_timeout");
    decl_void!("kodo_test_isolate_start", "kodo_test_isolate_start");
    decl_void!("kodo_test_isolate_end", "kodo_test_isolate_end");

    // Property testing builtins.
    decl_void!("kodo_prop_start", "kodo_prop_start", types::I64, types::I64);
    decl_ret!(
        "kodo_prop_gen_int",
        "kodo_prop_gen_int",
        [types::I64, types::I64],
        types::I64
    );
    decl_ret!("kodo_prop_gen_bool", "kodo_prop_gen_bool", [], types::I64);
    decl_ret!(
        "kodo_prop_gen_float",
        "kodo_prop_gen_float",
        [types::F64, types::F64],
        types::F64
    );
    decl_ret!(
        "kodo_prop_gen_string",
        "kodo_prop_gen_string",
        [types::I64],
        types::I64
    );
    Ok(())
}
