//! Translation of MIR instructions to inkwell LLVM builder calls.
//!
//! Each MIR `Instruction` variant maps to one or more inkwell builder
//! operations. The translation maintains a mapping from `LocalId` to
//! alloca stack slots, storing and loading values as needed.

#[cfg(feature = "inkwell")]
use std::collections::HashMap;

#[cfg(feature = "inkwell")]
use inkwell::builder::Builder;
#[cfg(feature = "inkwell")]
use inkwell::context::Context;
#[cfg(feature = "inkwell")]
use inkwell::module::Module;
#[cfg(feature = "inkwell")]
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue};

#[cfg(feature = "inkwell")]
use kodo_mir::{Instruction, LocalId, Value};
#[cfg(feature = "inkwell")]
use kodo_types::Type;

#[cfg(feature = "inkwell")]
use super::types::to_llvm_type;
#[cfg(feature = "inkwell")]
use super::value::{translate_value, unique_name, ValueCtx};

/// Translates a single MIR instruction to inkwell builder calls.
///
/// # Arguments
/// * `instr` - The MIR instruction to translate.
/// * `context` - The LLVM context.
/// * `module` - The LLVM module.
/// * `builder` - The LLVM IR builder.
/// * `local_allocas` - Mapping from local IDs to alloca stack slots.
/// * `local_types` - Mapping from local IDs to Kodo types.
/// * `fn_map` - Mapping from function names to LLVM function values.
/// * `user_functions` - List of user-defined function names.
/// * `struct_defs` - Struct type definitions.
/// * `enum_defs` - Enum type definitions.
/// * `name_counter` - Counter for unique value names.
/// * `ssa_cache` - Per-block SSA store-forwarding cache to avoid redundant loads.
#[cfg(feature = "inkwell")]
#[allow(clippy::too_many_arguments)]
pub(crate) fn translate_instruction<'ctx>(
    instr: &Instruction,
    context: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    local_allocas: &HashMap<LocalId, PointerValue<'ctx>>,
    local_types: &HashMap<LocalId, Type>,
    fn_map: &HashMap<String, FunctionValue<'ctx>>,
    user_functions: &[String],
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    name_counter: &mut u32,
    ssa_cache: &mut HashMap<LocalId, BasicValueEnum<'ctx>>,
) {
    let mut vctx = ValueCtx {
        context,
        module,
        builder,
        local_allocas,
        local_types,
        fn_map,
        struct_defs,
        enum_defs,
        name_counter,
        ssa_cache,
    };

    match instr {
        Instruction::Assign(dest, value) => {
            translate_assign(*dest, value, &mut vctx);
        }
        Instruction::Call { dest, callee, args } => {
            translate_call(*dest, callee, args, user_functions, &mut vctx);
        }
        Instruction::IndirectCall {
            dest,
            callee,
            args,
            return_type,
            param_types,
        } => {
            translate_indirect_call(*dest, callee, args, return_type, param_types, &mut vctx);
        }
        Instruction::VirtualCall {
            dest,
            vtable_index,
            args,
            ..
        } => {
            // Virtual call stub — store 0 for now.
            let _ = vtable_index;
            let _ = args;
            let zero = vctx.context.i64_type().const_int(0, false);
            if let Some(alloca) = vctx.local_allocas.get(dest) {
                vctx.builder.build_store(*alloca, zero).unwrap();
            }
            vctx.ssa_cache.insert(*dest, zero.into());
        }
        Instruction::IncRef(local) => {
            translate_incref(*local, &mut vctx);
        }
        Instruction::DecRef(local) => {
            translate_decref(*local, &mut vctx);
        }
        Instruction::Yield => {
            if let Some(yield_fn) = vctx.module.get_function("kodo_green_maybe_yield") {
                vctx.builder.build_call(yield_fn, &[], "yield").unwrap();
            }
        }
    }
}

/// Translates an `Assign` instruction.
///
/// Stores the value to the alloca for correctness (other blocks may read it),
/// and also caches it in the SSA cache so subsequent reads in the same block
/// can use the value directly without emitting a redundant load.
#[cfg(feature = "inkwell")]
fn translate_assign(dest: LocalId, value: &Value, ctx: &mut ValueCtx<'_, '_>) {
    if let Some(val) = translate_value(value, ctx) {
        if let Some(alloca) = ctx.local_allocas.get(&dest) {
            ctx.builder.build_store(*alloca, val).unwrap();
        }
        // Cache the assigned value for store-forwarding within the same block.
        ctx.ssa_cache.insert(dest, val);
    }
}

/// Resolves a Kodo callee name to its runtime C-ABI name.
#[cfg(feature = "inkwell")]
#[allow(clippy::too_many_lines)]
fn resolve_runtime_name(callee: &str) -> &str {
    match callee {
        "println" => "kodo_println",
        "print" => "kodo_print",
        "print_int" => "kodo_print_int",
        "print_float" => "kodo_print_float",
        "println_float" => "kodo_println_float",
        "abs" => "kodo_abs",
        "min" => "kodo_min",
        "max" => "kodo_max",
        "clamp" => "kodo_clamp",
        "sqrt" => "kodo_sqrt",
        "pow" => "kodo_pow",
        "sin" => "kodo_sin",
        "cos" => "kodo_cos",
        "log" => "kodo_log",
        "floor" => "kodo_floor",
        "ceil" => "kodo_ceil",
        "round" => "kodo_round",
        "rand_int" => "kodo_rand_int",
        "list_new" => "kodo_list_new",
        "list_push" => "kodo_list_push",
        "list_get" => "kodo_list_get",
        "list_length" => "kodo_list_length",
        "list_contains" => "kodo_list_contains",
        "list_pop" => "kodo_list_pop_simple",
        "list_remove" => "kodo_list_remove",
        "list_set" => "kodo_list_set",
        "list_is_empty" => "kodo_list_is_empty",
        "list_reverse" => "kodo_list_reverse",
        "list_slice" => "kodo_list_slice",
        "list_sort" => "kodo_list_sort",
        "list_join" => "kodo_list_join",
        "map_new" => "kodo_map_new",
        "map_insert" => "kodo_map_insert",
        "map_get" => "kodo_map_get",
        "map_contains_key" => "kodo_map_contains_key",
        "map_length" => "kodo_map_length",
        "map_remove" => "kodo_map_remove",
        "map_is_empty" => "kodo_map_is_empty",
        "map_merge" => "kodo_map_merge",
        "map_filter" => "kodo_map_filter",
        "map_insert_sk" => "kodo_map_insert_sk",
        "map_get_sk" => "kodo_map_get_sk",
        "map_contains_key_sk" => "kodo_map_contains_key_sk",
        "map_remove_sk" => "kodo_map_remove_sk",
        "map_free_sk" => "kodo_map_free_sk",
        "map_insert_sv" => "kodo_map_insert_sv",
        "map_get_sv" => "kodo_map_get_sv",
        "map_free_sv" => "kodo_map_free_sv",
        "map_insert_ss" => "kodo_map_insert_ss",
        "map_get_ss" => "kodo_map_get_ss",
        "map_free_ss" => "kodo_map_free_ss",
        "set_new" => "kodo_set_new",
        "set_add" => "kodo_set_add",
        "set_contains" => "kodo_set_contains",
        "set_remove" => "kodo_set_remove",
        "set_length" => "kodo_set_length",
        "set_is_empty" => "kodo_set_is_empty",
        "set_union" => "kodo_set_union",
        "set_intersection" => "kodo_set_intersection",
        "set_difference" => "kodo_set_difference",
        "set_to_list" => "kodo_set_to_list",
        "file_exists" => "kodo_file_exists",
        "file_read" => "kodo_file_read",
        "file_write" => "kodo_file_write",
        "file_append" => "kodo_file_append",
        "file_delete" => "kodo_file_delete",
        "dir_list" => "kodo_dir_list",
        "dir_exists" => "kodo_dir_exists",
        "http_get" => "kodo_http_get",
        "http_post" => "kodo_http_post",
        "json_parse" => "kodo_json_parse",
        "json_get_string" => "kodo_json_get_string",
        "json_get_int" => "kodo_json_get_int",
        "json_free" => "kodo_json_free",
        "json_stringify" => "kodo_json_stringify",
        "json_get_bool" => "kodo_json_get_bool",
        "json_get_float" => "kodo_json_get_float",
        "json_get_array" => "kodo_json_get_array",
        "json_get_object" => "kodo_json_get_object",
        "json_new_object" => "kodo_json_new_object",
        "json_set_string" => "kodo_json_set_string",
        "json_set_int" => "kodo_json_set_int",
        "json_set_bool" => "kodo_json_set_bool",
        "json_set_float" => "kodo_json_set_float",
        "time_now" => "kodo_time_now",
        "time_now_ms" => "kodo_time_now_ms",
        "time_format" => "kodo_time_format",
        "time_elapsed_ms" => "kodo_time_elapsed_ms",
        "channel_new" | "channel_new_bool" | "channel_new_string" => "kodo_channel_new",
        "channel_send" => "kodo_channel_send",
        "channel_recv" => "kodo_channel_recv",
        "channel_send_bool" => "kodo_channel_send_bool",
        "channel_recv_bool" => "kodo_channel_recv_bool",
        "channel_send_string" => "kodo_channel_send_string",
        "channel_recv_string" => "kodo_channel_recv_string",
        "channel_free" => "kodo_channel_free",
        "channel_select_2" => "kodo_channel_select_2",
        "channel_select_3" => "kodo_channel_select_3",
        "channel_generic_new" => "kodo_channel_generic_new",
        "channel_generic_send" => "kodo_channel_generic_send",
        "channel_generic_recv" => "kodo_channel_generic_recv",
        "channel_generic_free" => "kodo_channel_generic_free",
        "env_get" => "kodo_env_get",
        "env_set" => "kodo_env_set",
        "args" => "kodo_args",
        "readln" => "kodo_readln",
        "exit" => "kodo_exit",
        "db_open" => "kodo_db_open",
        "db_execute" => "kodo_db_execute",
        "db_query" => "kodo_db_query",
        "db_row_next" => "kodo_db_row_next",
        "db_row_get_string" => "kodo_db_row_get_string",
        "db_row_get_int" => "kodo_db_row_get_int",
        "db_row_advance" => "kodo_db_row_advance",
        "db_result_free" => "kodo_db_result_free",
        "db_close" => "kodo_db_close",
        "http_server_new" => "kodo_http_server_new",
        "http_server_recv" => "kodo_http_server_recv",
        "http_request_method" => "kodo_http_request_method",
        "http_request_path" => "kodo_http_request_path",
        "http_request_body" => "kodo_http_request_body",
        "http_respond" => "kodo_http_respond",
        "http_server_free" => "kodo_http_server_free",
        "assert" => "kodo_assert",
        "assert_true" => "kodo_assert_true",
        "assert_false" => "kodo_assert_false",
        "String_length" => "kodo_string_length",
        "String_byte_length" => "kodo_string_byte_length",
        "String_char_count" => "kodo_string_char_count",
        "String_contains" => "kodo_string_contains",
        "String_starts_with" => "kodo_string_starts_with",
        "String_ends_with" => "kodo_string_ends_with",
        "String_index_of" => "kodo_string_index_of",
        "String_eq" => "kodo_string_eq",
        "String_split" => "kodo_string_split",
        "String_lines" => "kodo_string_lines",
        "String_parse_int" => "kodo_string_parse_int",
        "char_at" | "String_char_at" => "kodo_string_char_at",
        "char_from_code" => "kodo_char_from_code",
        "is_alpha" => "kodo_is_alpha",
        "is_digit" => "kodo_is_digit",
        "is_alphanumeric" => "kodo_is_alphanumeric",
        "is_whitespace" => "kodo_is_whitespace",
        "String_trim" => "kodo_string_trim",
        "String_to_upper" => "kodo_string_to_upper",
        "String_to_lower" => "kodo_string_to_lower",
        "String_substring" => "kodo_string_substring",
        "String_concat" => "kodo_string_concat",
        "String_replace" => "kodo_string_replace",
        "String_repeat" => "kodo_string_repeat",
        "String_chars" => "kodo_string_chars",
        "Int_to_string" => "kodo_int_to_string",
        "Int_to_float64" => "kodo_int_to_float64",
        "Float64_to_string" => "kodo_float64_to_string",
        "Float64_to_int" => "kodo_float64_to_int",
        "Bool_to_string" => "kodo_bool_to_string",
        "Map_keys" => "kodo_map_keys",
        "Map_values" => "kodo_map_values",
        "list_iter" => "kodo_list_iter",
        "list_iterator_advance" => "kodo_list_iterator_advance",
        "list_iterator_value" => "kodo_list_iterator_value",
        "list_iterator_free" => "kodo_list_iterator_free",
        "string_chars_advance" => "kodo_string_chars_advance",
        "string_chars_value" => "kodo_string_chars_value",
        "string_chars_free" => "kodo_string_chars_free",
        "map_keys_advance" => "kodo_map_keys_advance",
        "map_keys_value" => "kodo_map_keys_value",
        "map_keys_free" => "kodo_map_keys_free",
        "map_values_advance" => "kodo_map_values_advance",
        "map_values_value" => "kodo_map_values_value",
        "map_values_free" => "kodo_map_values_free",
        "Option_is_some" => "kodo_option_is_some",
        "Option_is_none" => "kodo_option_is_none",
        "Option_unwrap" => "kodo_option_unwrap",
        "Option_unwrap_or" => "kodo_option_unwrap_or",
        "Result_is_ok" => "kodo_result_is_ok",
        "Result_is_err" => "kodo_result_is_err",
        "Result_unwrap" => "kodo_result_unwrap",
        "Result_unwrap_err" => "kodo_result_unwrap_err",
        "Result_unwrap_or" => "kodo_result_unwrap_or",
        "List_map" => "kodo_list_map",
        "List_filter" => "kodo_list_filter",
        "List_fold" => "kodo_list_fold",
        "List_reduce" => "kodo_list_reduce",
        "List_any" => "kodo_list_any",
        "List_all" => "kodo_list_all",
        "List_count" => "kodo_list_count",
        "List_sort_by" => "kodo_list_sort_by",
        other => other,
    }
}

/// Returns true if the builtin returns a String via out-parameters.
#[cfg(feature = "inkwell")]
fn is_string_returning_builtin(callee: &str) -> bool {
    matches!(
        callee,
        "String_trim"
            | "String_to_upper"
            | "String_to_lower"
            | "String_substring"
            | "String_concat"
            | "String_replace"
            | "String_repeat"
            | "Int_to_string"
            | "Float64_to_string"
            | "Bool_to_string"
            | "json_get_string"
            | "json_stringify"
            | "time_format"
            | "env_get"
            | "env_set"
            | "channel_recv_string"
            | "list_join"
            | "readln"
            | "http_request_method"
            | "http_request_path"
            | "http_request_body"
            | "db_row_get_string"
            | "char_from_code"
            | "format_int"
            | "string_builder_to_string"
    )
}

/// Returns true if the builtin uses out-parameters for its return value.
#[cfg(feature = "inkwell")]
fn is_outparam_get_builtin(callee: &str) -> bool {
    matches!(callee, "list_get" | "map_get" | "map_get_sk")
}

/// Translates a function call instruction.
#[cfg(feature = "inkwell")]
fn translate_call<'ctx>(
    dest: LocalId,
    callee: &str,
    args: &[Value],
    user_functions: &[String],
    ctx: &mut ValueCtx<'_, 'ctx>,
) {
    let is_user_fn = user_functions.contains(&callee.to_string());
    let runtime_name = if is_user_fn {
        callee
    } else {
        resolve_runtime_name(callee)
    };

    // Handle string-returning builtins.
    if !is_user_fn && is_string_returning_builtin(callee) {
        translate_string_returning_call(dest, callee, args, ctx);
        return;
    }

    // Handle out-parameter get builtins.
    if !is_user_fn && is_outparam_get_builtin(callee) {
        translate_outparam_get_call(dest, callee, args, ctx);
        return;
    }

    // Rewrite variadic __env_pack to fixed-arity __env_pack_N.
    let final_name = if runtime_name == "__env_pack" {
        format!("__env_pack_{}", args.len())
    } else {
        runtime_name.to_string()
    };

    // Resolve the function.
    let fn_val = ctx
        .fn_map
        .get(final_name.as_str())
        .copied()
        .or_else(|| ctx.module.get_function(&final_name));

    let Some(fn_val) = fn_val else {
        // Unknown function — store 0.
        if let Some(alloca) = ctx.local_allocas.get(&dest) {
            let zero = ctx.context.i64_type().const_int(0, false);
            ctx.builder.build_store(*alloca, zero).unwrap();
        }
        return;
    };

    // Emit argument values, expanding strings to (ptr, len).
    let mut arg_vals: Vec<BasicMetadataValueEnum<'ctx>> = Vec::new();
    for arg in args {
        if let Some(val) = translate_value(arg, ctx) {
            let arg_ty = super::value::infer_value_type_simple(arg, ctx.local_types);
            if arg_ty == Type::String && val.is_struct_value() {
                // Expand string struct {ptr, len} to two args.
                let sv = val.into_struct_value();
                let ptr_name = unique_name(ctx.name_counter, "ap");
                let len_name = unique_name(ctx.name_counter, "al");
                let ptr = ctx.builder.build_extract_value(sv, 0, &ptr_name).unwrap();
                let len = ctx.builder.build_extract_value(sv, 1, &len_name).unwrap();
                arg_vals.push(ptr.into());
                arg_vals.push(len.into());
            } else if arg_ty == Type::String && val.is_int_value() {
                // String handle (i64) — points to a [ptr, len] pair on heap.
                // Load ptr from handle, len from handle+8.
                let handle = val.into_int_value();
                let ptr_type = ctx.context.ptr_type(inkwell::AddressSpace::default());
                let str_ptr = ctx
                    .builder
                    .build_int_to_ptr(handle, ptr_type, &unique_name(ctx.name_counter, "shp"))
                    .unwrap();
                let s_ptr = ctx
                    .builder
                    .build_load(
                        ctx.context.i64_type(),
                        str_ptr,
                        &unique_name(ctx.name_counter, "sp"),
                    )
                    .unwrap();
                let off8 = ctx
                    .builder
                    .build_int_add(
                        handle,
                        ctx.context.i64_type().const_int(8, false),
                        &unique_name(ctx.name_counter, "so"),
                    )
                    .unwrap();
                let len_ptr = ctx
                    .builder
                    .build_int_to_ptr(off8, ptr_type, &unique_name(ctx.name_counter, "slp"))
                    .unwrap();
                let s_len = ctx
                    .builder
                    .build_load(
                        ctx.context.i64_type(),
                        len_ptr,
                        &unique_name(ctx.name_counter, "sl"),
                    )
                    .unwrap();
                arg_vals.push(s_ptr.into());
                arg_vals.push(s_len.into());
            } else {
                arg_vals.push(val.into());
            }
        }
    }

    let call_name = unique_name(ctx.name_counter, "call");
    let call_result = ctx
        .builder
        .build_call(fn_val, &arg_vals, &call_name)
        .unwrap();

    // Store result if not void.
    if let Some(result_val) = call_result.try_as_basic_value().basic() {
        if let Some(alloca) = ctx.local_allocas.get(&dest) {
            ctx.builder.build_store(*alloca, result_val).unwrap();
        }
        // Cache call result for store-forwarding within the same block.
        ctx.ssa_cache.insert(dest, result_val);
    }
}

/// Translates a string-returning builtin call with out-parameters.
#[cfg(feature = "inkwell")]
fn translate_string_returning_call<'ctx>(
    dest: LocalId,
    callee: &str,
    args: &[Value],
    ctx: &mut ValueCtx<'_, 'ctx>,
) {
    let runtime_name = resolve_runtime_name(callee);

    // Emit normal arguments.
    let mut arg_vals: Vec<BasicMetadataValueEnum<'ctx>> = Vec::new();
    for arg in args {
        if let Some(val) = translate_value(arg, ctx) {
            let arg_ty = super::value::infer_value_type_simple(arg, ctx.local_types);
            if arg_ty == Type::String {
                let sv = val.into_struct_value();
                let ptr_name = unique_name(ctx.name_counter, "sp");
                let len_name = unique_name(ctx.name_counter, "sl");
                let ptr = ctx.builder.build_extract_value(sv, 0, &ptr_name).unwrap();
                let len = ctx.builder.build_extract_value(sv, 1, &len_name).unwrap();
                arg_vals.push(ptr.into());
                arg_vals.push(len.into());
            } else {
                arg_vals.push(val.into());
            }
        }
    }

    // Allocate out-parameter slots.
    let out_ptr_name = unique_name(ctx.name_counter, "sro_p");
    let out_len_name = unique_name(ctx.name_counter, "sro_l");
    let out_ptr = ctx
        .builder
        .build_alloca(ctx.context.i64_type(), &out_ptr_name)
        .unwrap();
    let out_len = ctx
        .builder
        .build_alloca(ctx.context.i64_type(), &out_len_name)
        .unwrap();

    let outptr_int_name = unique_name(ctx.name_counter, "sro_ptr_int");
    let outlen_int_name = unique_name(ctx.name_counter, "sro_len_int");
    let out_ptr_i64 = ctx
        .builder
        .build_ptr_to_int(out_ptr, ctx.context.i64_type(), &outptr_int_name)
        .unwrap();
    let out_len_i64 = ctx
        .builder
        .build_ptr_to_int(out_len, ctx.context.i64_type(), &outlen_int_name)
        .unwrap();

    arg_vals.push(out_ptr_i64.into());
    arg_vals.push(out_len_i64.into());

    if let Some(fn_val) = ctx.module.get_function(runtime_name) {
        let call_name = unique_name(ctx.name_counter, "src");
        ctx.builder
            .build_call(fn_val, &arg_vals, &call_name)
            .unwrap();
    }

    // Load results and build string struct.
    let res_ptr_name = unique_name(ctx.name_counter, "sro_res_ptr");
    let res_len_name = unique_name(ctx.name_counter, "sro_res_len");
    let res_ptr = ctx
        .builder
        .build_load(ctx.context.i64_type(), out_ptr, &res_ptr_name)
        .unwrap();
    let res_len = ctx
        .builder
        .build_load(ctx.context.i64_type(), out_len, &res_len_name)
        .unwrap();

    let str_struct_ty = ctx.context.struct_type(
        &[ctx.context.i64_type().into(), ctx.context.i64_type().into()],
        false,
    );
    let s1_name = unique_name(ctx.name_counter, "srs1");
    let s1 = ctx
        .builder
        .build_insert_value(str_struct_ty.get_undef(), res_ptr, 0, &s1_name)
        .unwrap();
    let s2_name = unique_name(ctx.name_counter, "srs2");
    let s2 = ctx
        .builder
        .build_insert_value(s1, res_len, 1, &s2_name)
        .unwrap();

    let string_val: BasicValueEnum<'ctx> = s2.into_struct_value().into();
    if let Some(alloca) = ctx.local_allocas.get(&dest) {
        ctx.builder.build_store(*alloca, string_val).unwrap();
    }
    // Cache string result for store-forwarding within the same block.
    ctx.ssa_cache.insert(dest, string_val);
}

/// Translates an out-parameter get builtin call.
#[cfg(feature = "inkwell")]
fn translate_outparam_get_call<'ctx>(
    dest: LocalId,
    callee: &str,
    args: &[Value],
    ctx: &mut ValueCtx<'_, 'ctx>,
) {
    let runtime_name = resolve_runtime_name(callee);

    let mut arg_vals: Vec<BasicMetadataValueEnum<'ctx>> = Vec::new();
    for arg in args {
        if let Some(val) = translate_value(arg, ctx) {
            let arg_ty = super::value::infer_value_type_simple(arg, ctx.local_types);
            if arg_ty == Type::String {
                let sv = val.into_struct_value();
                let ptr_name = unique_name(ctx.name_counter, "gp");
                let len_name = unique_name(ctx.name_counter, "gl");
                let ptr = ctx.builder.build_extract_value(sv, 0, &ptr_name).unwrap();
                let len = ctx.builder.build_extract_value(sv, 1, &len_name).unwrap();
                arg_vals.push(ptr.into());
                arg_vals.push(len.into());
            } else {
                arg_vals.push(val.into());
            }
        }
    }

    // Allocate out-parameter slots.
    let out_val_name = unique_name(ctx.name_counter, "ov");
    let out_some_name = unique_name(ctx.name_counter, "os");
    let out_value = ctx
        .builder
        .build_alloca(ctx.context.i64_type(), &out_val_name)
        .unwrap();
    let out_is_some = ctx
        .builder
        .build_alloca(ctx.context.i64_type(), &out_some_name)
        .unwrap();

    let outval_int_name = unique_name(ctx.name_counter, "outval_int");
    let outsome_int_name = unique_name(ctx.name_counter, "outsome_int");
    let out_val_i64 = ctx
        .builder
        .build_ptr_to_int(out_value, ctx.context.i64_type(), &outval_int_name)
        .unwrap();
    let out_some_i64 = ctx
        .builder
        .build_ptr_to_int(out_is_some, ctx.context.i64_type(), &outsome_int_name)
        .unwrap();

    arg_vals.push(out_val_i64.into());
    arg_vals.push(out_some_i64.into());

    if let Some(fn_val) = ctx.module.get_function(runtime_name) {
        let call_name = unique_name(ctx.name_counter, "opc");
        ctx.builder
            .build_call(fn_val, &arg_vals, &call_name)
            .unwrap();
    }

    // Load the result.
    let result_name = unique_name(ctx.name_counter, "opr");
    let result = ctx
        .builder
        .build_load(ctx.context.i64_type(), out_value, &result_name)
        .unwrap();

    if let Some(alloca) = ctx.local_allocas.get(&dest) {
        ctx.builder.build_store(*alloca, result).unwrap();
    }
    // Cache result for store-forwarding within the same block.
    ctx.ssa_cache.insert(dest, result);
}

/// Translates an indirect (function pointer) call.
#[cfg(feature = "inkwell")]
fn translate_indirect_call<'ctx>(
    dest: LocalId,
    callee: &Value,
    args: &[Value],
    return_type: &Type,
    param_types: &[Type],
    ctx: &mut ValueCtx<'_, 'ctx>,
) {
    let callee_val = translate_value(callee, ctx);
    let Some(callee_val) = callee_val else {
        if let Some(alloca) = ctx.local_allocas.get(&dest) {
            let zero = ctx.context.i64_type().const_int(0, false);
            ctx.builder.build_store(*alloca, zero).unwrap();
        }
        return;
    };
    let callee_i64 = callee_val.into_int_value();

    // Build LLVM function type.
    let llvm_param_types: Vec<inkwell::types::BasicMetadataTypeEnum<'ctx>> = param_types
        .iter()
        .map(|t| to_llvm_type(ctx.context, t).into())
        .collect();

    let fn_type = if super::types::is_void(return_type) {
        ctx.context.void_type().fn_type(&llvm_param_types, false)
    } else {
        let ret_ty = to_llvm_type(ctx.context, return_type);
        match ret_ty {
            inkwell::types::BasicTypeEnum::IntType(t) => t.fn_type(&llvm_param_types, false),
            inkwell::types::BasicTypeEnum::FloatType(t) => t.fn_type(&llvm_param_types, false),
            inkwell::types::BasicTypeEnum::StructType(t) => t.fn_type(&llvm_param_types, false),
            inkwell::types::BasicTypeEnum::PointerType(t) => t.fn_type(&llvm_param_types, false),
            _ => ctx.context.i64_type().fn_type(&llvm_param_types, false),
        }
    };

    // Convert i64 to function pointer.
    let ptr_name = unique_name(ctx.name_counter, "fptr");
    let fn_ptr = ctx
        .builder
        .build_int_to_ptr(
            callee_i64,
            ctx.context.ptr_type(inkwell::AddressSpace::default()),
            &ptr_name,
        )
        .unwrap();

    // Emit args.
    let mut arg_vals: Vec<BasicMetadataValueEnum<'ctx>> = Vec::new();
    for arg in args {
        if let Some(val) = translate_value(arg, ctx) {
            arg_vals.push(val.into());
        }
    }

    let call_name = unique_name(ctx.name_counter, "icall");
    let result = ctx
        .builder
        .build_indirect_call(fn_type, fn_ptr, &arg_vals, &call_name)
        .unwrap();

    if let Some(result_val) = result.try_as_basic_value().basic() {
        if let Some(alloca) = ctx.local_allocas.get(&dest) {
            ctx.builder.build_store(*alloca, result_val).unwrap();
        }
        // Cache indirect call result for store-forwarding within the same block.
        ctx.ssa_cache.insert(dest, result_val);
    }
}

/// Translates an `IncRef` instruction.
#[cfg(feature = "inkwell")]
fn translate_incref(local: LocalId, ctx: &mut ValueCtx<'_, '_>) {
    let local_ty = ctx.local_types.get(&local).cloned().unwrap_or(Type::Int);
    if local_ty == Type::String {
        // String incref: extract ptr and len, call kodo_rc_inc_string.
        if let Some(alloca) = ctx.local_allocas.get(&local) {
            let str_ty = to_llvm_type(ctx.context, &Type::String);
            let load_name = unique_name(ctx.name_counter, "irc_l");
            let val = ctx.builder.build_load(str_ty, *alloca, &load_name).unwrap();
            let sv = val.into_struct_value();
            let ptr_name = unique_name(ctx.name_counter, "irc_p");
            let len_name = unique_name(ctx.name_counter, "irc_n");
            let ptr = ctx.builder.build_extract_value(sv, 0, &ptr_name).unwrap();
            let len = ctx.builder.build_extract_value(sv, 1, &len_name).unwrap();
            if let Some(fn_val) = ctx.module.get_function("kodo_rc_inc_string") {
                ctx.builder
                    .build_call(fn_val, &[ptr.into(), len.into()], "irc")
                    .unwrap();
            }
        }
    } else if !is_composite(&local_ty) {
        if let Some(alloca) = ctx.local_allocas.get(&local) {
            let load_name = unique_name(ctx.name_counter, "irc_l");
            let val = ctx
                .builder
                .build_load(ctx.context.i64_type(), *alloca, &load_name)
                .unwrap();
            if let Some(fn_val) = ctx.module.get_function("kodo_rc_inc") {
                ctx.builder
                    .build_call(fn_val, &[val.into()], "irc")
                    .unwrap();
            }
        }
    }
}

/// Translates a `DecRef` instruction.
#[cfg(feature = "inkwell")]
fn translate_decref(local: LocalId, ctx: &mut ValueCtx<'_, '_>) {
    let local_ty = ctx.local_types.get(&local).cloned().unwrap_or(Type::Int);
    if local_ty == Type::String {
        if let Some(alloca) = ctx.local_allocas.get(&local) {
            let str_ty = to_llvm_type(ctx.context, &Type::String);
            let load_name = unique_name(ctx.name_counter, "drc_l");
            let val = ctx.builder.build_load(str_ty, *alloca, &load_name).unwrap();
            let sv = val.into_struct_value();
            let ptr_name = unique_name(ctx.name_counter, "drc_p");
            let len_name = unique_name(ctx.name_counter, "drc_n");
            let ptr = ctx.builder.build_extract_value(sv, 0, &ptr_name).unwrap();
            let len = ctx.builder.build_extract_value(sv, 1, &len_name).unwrap();
            if let Some(fn_val) = ctx.module.get_function("kodo_rc_dec_string") {
                ctx.builder
                    .build_call(fn_val, &[ptr.into(), len.into()], "drc")
                    .unwrap();
            }
        }
    } else if !is_composite(&local_ty) {
        if let Some(alloca) = ctx.local_allocas.get(&local) {
            let load_name = unique_name(ctx.name_counter, "drc_l");
            let val = ctx
                .builder
                .build_load(ctx.context.i64_type(), *alloca, &load_name)
                .unwrap();
            if let Some(fn_val) = ctx.module.get_function("kodo_rc_dec") {
                ctx.builder
                    .build_call(fn_val, &[val.into()], "drc")
                    .unwrap();
            }
        }
    }
}

/// Returns true for composite types (structs, enums, etc.) that don't have
/// simple scalar refcounting.
#[cfg(feature = "inkwell")]
fn is_composite(ty: &Type) -> bool {
    matches!(
        ty,
        Type::String | Type::Struct(_) | Type::Enum(_) | Type::Generic(_, _)
    )
}
