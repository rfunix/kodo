//! Translation of MIR instructions to LLVM IR lines.
//!
//! Each MIR `Instruction` variant maps to one or more lines of LLVM IR.
//! The translation uses SSA registers (`%N`) where N corresponds to a
//! per-function counter managed by the function translator.

use std::collections::HashMap;

use kodo_mir::{Instruction, LocalId, Value};
use kodo_types::Type;

use crate::emitter::LLVMEmitter;
use crate::function::StackLocals;
use crate::types::{is_composite, llvm_type};
use crate::value::{emit_value, ValueResult};

/// Emits LLVM IR for a single MIR instruction.
///
/// # Arguments
/// * `instr` - The MIR instruction to translate.
/// * `emitter` - The LLVM IR string builder.
/// * `local_regs` - Mapping from `LocalId` to LLVM SSA register names.
/// * `local_types` - Mapping from `LocalId` to Kodo types.
/// * `next_reg` - Counter for generating fresh SSA register names.
/// * `struct_defs` - Struct type definitions.
/// * `enum_defs` - Enum type definitions.
/// * `string_constants` - Accumulated string constants to emit at module level.
/// * `user_functions` - Set of user-defined function names (for dispatch).
/// * `stack_locals` - Locals with alloca stack slots (for multi-block functions).
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn emit_instruction(
    instr: &Instruction,
    emitter: &mut LLVMEmitter,
    local_regs: &mut HashMap<LocalId, String>,
    local_types: &HashMap<LocalId, Type>,
    next_reg: &mut u32,
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    string_constants: &mut Vec<(String, String)>,
    user_functions: &[String],
    stack_locals: &StackLocals,
) {
    match instr {
        Instruction::Assign(dest, value) => {
            emit_assign(
                *dest,
                value,
                emitter,
                local_regs,
                local_types,
                next_reg,
                struct_defs,
                enum_defs,
                string_constants,
                stack_locals,
            );
        }
        Instruction::Call { dest, callee, args } => {
            emit_call(
                *dest,
                callee,
                args,
                emitter,
                local_regs,
                local_types,
                next_reg,
                struct_defs,
                enum_defs,
                string_constants,
                user_functions,
                stack_locals,
            );
        }
        Instruction::IndirectCall {
            dest,
            callee,
            args,
            return_type,
            param_types,
        } => {
            emit_indirect_call(
                *dest,
                callee,
                args,
                return_type,
                param_types,
                emitter,
                local_regs,
                local_types,
                next_reg,
                struct_defs,
                enum_defs,
                string_constants,
                stack_locals,
            );
        }
        Instruction::VirtualCall { dest, .. } => {
            // Virtual calls are complex; for now emit a placeholder.
            let reg = fresh_reg(next_reg);
            store_to_stack_or_alias(
                *dest,
                &reg,
                emitter,
                local_regs,
                local_types,
                next_reg,
                struct_defs,
                enum_defs,
                stack_locals,
            );
            emitter.indent(&format!("{reg} = add i64 0, 0 ; TODO: virtual call"));
        }
        Instruction::IncRef(local) => {
            let local_ty = local_types.get(local).cloned().unwrap_or(Type::Int);
            // Skip IncRef for composite types that aren't simple pointers.
            if !is_composite(&local_ty) {
                let reg = load_local_reg(
                    *local,
                    emitter,
                    local_regs,
                    local_types,
                    next_reg,
                    struct_defs,
                    enum_defs,
                    stack_locals,
                );
                if let Some(r) = reg {
                    emitter.indent(&format!("call void @kodo_rc_inc(i64 {r})"));
                }
            } else if local_ty == Type::String {
                let reg = load_local_reg(
                    *local,
                    emitter,
                    local_regs,
                    local_types,
                    next_reg,
                    struct_defs,
                    enum_defs,
                    stack_locals,
                );
                if let Some(r) = reg {
                    let ptr_reg = fresh_reg(next_reg);
                    let len_reg = fresh_reg(next_reg);
                    emitter.indent(&format!("{ptr_reg} = extractvalue {{ i64, i64 }} {r}, 0"));
                    emitter.indent(&format!("{len_reg} = extractvalue {{ i64, i64 }} {r}, 1"));
                    emitter.indent(&format!(
                        "call void @kodo_rc_inc_string(i64 {ptr_reg}, i64 {len_reg})"
                    ));
                }
            }
            // For enums, structs, Option, Result — skip IncRef (no heap alloc to refcount).
        }
        Instruction::DecRef(local) => {
            let local_ty = local_types.get(local).cloned().unwrap_or(Type::Int);
            // Skip DecRef for composite types that aren't simple pointers.
            if !is_composite(&local_ty) {
                let reg = load_local_reg(
                    *local,
                    emitter,
                    local_regs,
                    local_types,
                    next_reg,
                    struct_defs,
                    enum_defs,
                    stack_locals,
                );
                if let Some(r) = reg {
                    emitter.indent(&format!("call void @kodo_rc_dec(i64 {r})"));
                }
            } else if local_ty == Type::String {
                let reg = load_local_reg(
                    *local,
                    emitter,
                    local_regs,
                    local_types,
                    next_reg,
                    struct_defs,
                    enum_defs,
                    stack_locals,
                );
                if let Some(r) = reg {
                    let ptr_reg = fresh_reg(next_reg);
                    let len_reg = fresh_reg(next_reg);
                    emitter.indent(&format!("{ptr_reg} = extractvalue {{ i64, i64 }} {r}, 0"));
                    emitter.indent(&format!("{len_reg} = extractvalue {{ i64, i64 }} {r}, 1"));
                    emitter.indent(&format!(
                        "call void @kodo_rc_dec_string(i64 {ptr_reg}, i64 {len_reg})"
                    ));
                }
            }
            // For enums, structs, Option, Result — skip DecRef (no heap alloc to refcount).
        }
        Instruction::Yield => {
            emitter.indent("call void @kodo_green_maybe_yield()");
        }
    }
}

/// Loads a local from its stack slot (if it has one) or returns the register alias.
/// Returns `None` if the local has never been assigned.
#[allow(clippy::too_many_arguments)]
fn load_local_reg(
    local: LocalId,
    emitter: &mut LLVMEmitter,
    local_regs: &HashMap<LocalId, String>,
    local_types: &HashMap<LocalId, Type>,
    next_reg: &mut u32,
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    stack_locals: &StackLocals,
) -> Option<String> {
    if let Some(alloca_reg) = stack_locals.get(&local) {
        let ty = local_types.get(&local).cloned().unwrap_or(Type::Int);
        let ty_str = llvm_type(&ty, struct_defs, enum_defs);
        if ty_str == "void" {
            return None;
        }
        let reg = fresh_reg(next_reg);
        emitter.indent(&format!("{reg} = load {ty_str}, ptr {alloca_reg}"));
        Some(reg)
    } else {
        local_regs.get(&local).cloned()
    }
}

/// Stores a value to the stack slot for a local, or aliases it in the register map.
///
/// When the value type may not match the declared local type (e.g., an `i64`
/// extracted from an enum payload being stored into a `{ i64, i64 }` String
/// slot), we store through `i64` type to avoid LLVM type mismatches.
/// The `alloca` slot is always at least 8 bytes, so this is safe.
#[allow(clippy::too_many_arguments)]
fn store_to_stack_or_alias(
    dest: LocalId,
    reg: &str,
    emitter: &mut LLVMEmitter,
    local_regs: &mut HashMap<LocalId, String>,
    local_types: &HashMap<LocalId, Type>,
    _next_reg: &mut u32,
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    stack_locals: &StackLocals,
) {
    if let Some(alloca_reg) = stack_locals.get(&dest) {
        let ty = local_types.get(&dest).cloned().unwrap_or(Type::Int);
        let ty_str = llvm_type(&ty, struct_defs, enum_defs);
        if ty_str != "void" {
            emitter.indent(&format!("store {ty_str} {reg}, ptr {alloca_reg}"));
        }
    } else {
        local_regs.insert(dest, reg.to_string());
    }
}

/// Stores a value to the stack slot using a known value type, or aliases in the register map.
///
/// Use this variant when the value's LLVM type may differ from the local's declared type.
#[allow(clippy::too_many_arguments)]
fn store_typed_to_stack_or_alias(
    dest: LocalId,
    reg: &str,
    value_ty_str: &str,
    emitter: &mut LLVMEmitter,
    local_regs: &mut HashMap<LocalId, String>,
    stack_locals: &StackLocals,
) {
    if let Some(alloca_reg) = stack_locals.get(&dest) {
        if value_ty_str != "void" {
            emitter.indent(&format!("store {value_ty_str} {reg}, ptr {alloca_reg}"));
        }
    } else {
        local_regs.insert(dest, reg.to_string());
    }
}

/// Generates a fresh SSA register name and increments the counter.
pub(crate) fn fresh_reg(next_reg: &mut u32) -> String {
    let reg = format!("%{next_reg}");
    *next_reg += 1;
    reg
}

/// Emits LLVM IR for an assignment instruction.
#[allow(clippy::too_many_arguments)]
fn emit_assign(
    dest: LocalId,
    value: &Value,
    emitter: &mut LLVMEmitter,
    local_regs: &mut HashMap<LocalId, String>,
    local_types: &HashMap<LocalId, Type>,
    next_reg: &mut u32,
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    string_constants: &mut Vec<(String, String)>,
    stack_locals: &StackLocals,
) {
    let dest_ty = local_types.get(&dest).cloned().unwrap_or(Type::Int);
    // Infer the value's type to handle type mismatches between value and dest.
    let value_ty = crate::value::infer_value_type_ext(value, local_types, struct_defs);
    let vr = emit_value(
        value,
        emitter,
        local_regs,
        local_types,
        next_reg,
        struct_defs,
        enum_defs,
        string_constants,
        stack_locals,
    );

    match vr {
        ValueResult::Register(reg) => {
            // Use the value's inferred type for the store when we have a stack slot.
            // This handles type mismatches (e.g., i64 from EnumPayload into a String slot,
            // or { i64, i64 } from FieldGet into what infer_value_type_simple calls Int).
            if stack_locals.contains_key(&dest) {
                let value_ty_str = llvm_type(&value_ty, struct_defs, enum_defs);
                store_typed_to_stack_or_alias(
                    dest,
                    &reg,
                    &value_ty_str,
                    emitter,
                    local_regs,
                    stack_locals,
                );
            } else {
                local_regs.insert(dest, reg);
            }
        }
        ValueResult::Constant(val) => {
            let ty_str = llvm_type(&dest_ty, struct_defs, enum_defs);
            if ty_str == "void" {
                // Unit assignment - no register needed.
                return;
            }
            // For integer constants, use i64 for the store (which is the actual
            // type of the constant, regardless of dest_ty).
            let reg = fresh_reg(next_reg);
            emitter.indent(&format!("{reg} = add i64 {val}, 0"));
            store_typed_to_stack_or_alias(dest, &reg, "i64", emitter, local_regs, stack_locals);
        }
        ValueResult::FloatConstant(val) => {
            let reg = fresh_reg(next_reg);
            emitter.indent(&format!("{reg} = fadd double {val}, 0.0"));
            store_typed_to_stack_or_alias(dest, &reg, "double", emitter, local_regs, stack_locals);
        }
        ValueResult::Void => {
            // Unit type, nothing to assign.
        }
    }
}

/// Mapping from Kodo builtin keys to runtime C-ABI names.
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
        // String methods
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
        // List iterators
        "list_iter" => "kodo_list_iter",
        "list_iterator_advance" => "kodo_list_iterator_advance",
        "list_iterator_value" => "kodo_list_iterator_value",
        "list_iterator_free" => "kodo_list_iterator_free",
        // String chars iterators
        "string_chars_advance" => "kodo_string_chars_advance",
        "string_chars_value" => "kodo_string_chars_value",
        "string_chars_free" => "kodo_string_chars_free",
        // Map keys iterators
        "map_keys_advance" => "kodo_map_keys_advance",
        "map_keys_value" => "kodo_map_keys_value",
        "map_keys_free" => "kodo_map_keys_free",
        // Map values iterators
        "map_values_advance" => "kodo_map_values_advance",
        "map_values_value" => "kodo_map_values_value",
        "map_values_free" => "kodo_map_values_free",
        // Option/Result synthetic builtins — mapped to runtime helpers.
        "Option_is_some" => "kodo_option_is_some",
        "Option_is_none" => "kodo_option_is_none",
        "Option_unwrap" => "kodo_option_unwrap",
        "Option_unwrap_or" => "kodo_option_unwrap_or",
        "Result_is_ok" => "kodo_result_is_ok",
        "Result_is_err" => "kodo_result_is_err",
        "Result_unwrap" => "kodo_result_unwrap",
        "Result_unwrap_err" => "kodo_result_unwrap_err",
        "Result_unwrap_or" => "kodo_result_unwrap_or",
        // List higher-order methods — mapped to runtime helpers.
        "List_map" => "kodo_list_map",
        "List_filter" => "kodo_list_filter",
        "List_fold" => "kodo_list_fold",
        "List_reduce" => "kodo_list_reduce",
        "List_any" => "kodo_list_any",
        "List_all" => "kodo_list_all",
        "List_count" => "kodo_list_count",
        // Pass-through for already-qualified names.
        other => other,
    }
}

/// Emits LLVM IR for a function call instruction.
#[allow(clippy::too_many_arguments)]
fn emit_call(
    dest: LocalId,
    callee: &str,
    args: &[Value],
    emitter: &mut LLVMEmitter,
    local_regs: &mut HashMap<LocalId, String>,
    local_types: &HashMap<LocalId, Type>,
    next_reg: &mut u32,
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    string_constants: &mut Vec<(String, String)>,
    user_functions: &[String],
    stack_locals: &StackLocals,
) {
    let dest_ty = local_types.get(&dest).cloned().unwrap_or(Type::Unit);

    // Resolve the runtime name for the callee.
    let is_user_fn = user_functions.contains(&callee.to_string());
    let runtime_name = if is_user_fn {
        callee
    } else {
        resolve_runtime_name(callee)
    };

    // Emit argument values.
    let mut arg_strs = Vec::new();
    for arg in args {
        let vr = emit_value(
            arg,
            emitter,
            local_regs,
            local_types,
            next_reg,
            struct_defs,
            enum_defs,
            string_constants,
            stack_locals,
        );
        match vr {
            ValueResult::Register(r) => {
                // Determine arg type from the value.
                let arg_ty = infer_value_type(arg, local_types);
                if is_composite(&arg_ty) && arg_ty == Type::String {
                    // String args are passed as (ptr, len) - extract both fields.
                    let ptr_reg = fresh_reg(next_reg);
                    let len_reg = fresh_reg(next_reg);
                    emitter.indent(&format!("{ptr_reg} = extractvalue {{ i64, i64 }} {r}, 0"));
                    emitter.indent(&format!("{len_reg} = extractvalue {{ i64, i64 }} {r}, 1"));
                    arg_strs.push(format!("i64 {ptr_reg}"));
                    arg_strs.push(format!("i64 {len_reg}"));
                } else {
                    let ty_str = llvm_type(&arg_ty, struct_defs, enum_defs);
                    arg_strs.push(format!("{ty_str} {r}"));
                }
            }
            ValueResult::Constant(val) => {
                arg_strs.push(format!("i64 {val}"));
            }
            ValueResult::FloatConstant(val) => {
                arg_strs.push(format!("double {val}"));
            }
            ValueResult::Void => {}
        }
    }

    let args_str = arg_strs.join(", ");
    let ret_ty = llvm_type(&dest_ty, struct_defs, enum_defs);

    // Rewrite variadic __env_pack to fixed-arity __env_pack_N.
    let final_name = if runtime_name == "__env_pack" {
        format!("__env_pack_{}", args.len())
    } else {
        runtime_name.to_string()
    };

    if ret_ty == "void" {
        emitter.indent(&format!("call void @{final_name}({args_str})"));
    } else {
        let reg = fresh_reg(next_reg);
        emitter.indent(&format!("{reg} = call {ret_ty} @{final_name}({args_str})"));
        // Use the actual return type for store (may differ from local's declared type).
        store_typed_to_stack_or_alias(dest, &reg, &ret_ty, emitter, local_regs, stack_locals);
    }
}

/// Emits LLVM IR for an indirect call instruction.
#[allow(clippy::too_many_arguments)]
fn emit_indirect_call(
    dest: LocalId,
    callee: &Value,
    args: &[Value],
    return_type: &Type,
    param_types: &[Type],
    emitter: &mut LLVMEmitter,
    local_regs: &mut HashMap<LocalId, String>,
    local_types: &HashMap<LocalId, Type>,
    next_reg: &mut u32,
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    string_constants: &mut Vec<(String, String)>,
    stack_locals: &StackLocals,
) {
    // Emit the callee value (a function pointer).
    let callee_vr = emit_value(
        callee,
        emitter,
        local_regs,
        local_types,
        next_reg,
        struct_defs,
        enum_defs,
        string_constants,
        stack_locals,
    );
    let callee_reg = match callee_vr {
        ValueResult::Register(r) => r,
        ValueResult::Constant(v) => {
            let reg = fresh_reg(next_reg);
            emitter.indent(&format!("{reg} = add i64 {v}, 0"));
            reg
        }
        _ => {
            let reg = fresh_reg(next_reg);
            emitter.indent(&format!("{reg} = add i64 0, 0"));
            reg
        }
    };

    // Build function type signature.
    let ret_str = llvm_type(return_type, struct_defs, enum_defs);
    let param_strs: Vec<String> = param_types
        .iter()
        .map(|t| llvm_type(t, struct_defs, enum_defs))
        .collect();
    let fn_ty = format!("{ret_str} ({})", param_strs.join(", "));

    // Emit argument values.
    let mut arg_strs = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        let vr = emit_value(
            arg,
            emitter,
            local_regs,
            local_types,
            next_reg,
            struct_defs,
            enum_defs,
            string_constants,
            stack_locals,
        );
        let ty_str = param_types
            .get(i)
            .map_or("i64".to_string(), |t| llvm_type(t, struct_defs, enum_defs));
        match vr {
            ValueResult::Register(r) => arg_strs.push(format!("{ty_str} {r}")),
            ValueResult::Constant(v) | ValueResult::FloatConstant(v) => {
                arg_strs.push(format!("{ty_str} {v}"));
            }
            ValueResult::Void => {}
        }
    }

    // Cast i64 to function pointer and call.
    let fn_ptr_reg = fresh_reg(next_reg);
    emitter.indent(&format!("{fn_ptr_reg} = inttoptr i64 {callee_reg} to ptr"));

    let args_str = arg_strs.join(", ");
    if ret_str == "void" {
        emitter.indent(&format!("call void {fn_ptr_reg}({args_str})"));
    } else {
        let reg = fresh_reg(next_reg);
        emitter.indent(&format!("{reg} = call {fn_ty} {fn_ptr_reg}({args_str})"));
        store_typed_to_stack_or_alias(dest, &reg, &ret_str, emitter, local_regs, stack_locals);
    }
}

/// Infers the Kodo type of a MIR `Value` from context.
///
/// Delegates to `infer_value_type_simple` which recursively handles
/// `BinOp` and `Neg` to correctly propagate String/Float types.
fn infer_value_type(value: &Value, local_types: &HashMap<LocalId, Type>) -> Type {
    crate::value::infer_value_type_simple(value, local_types)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_reg_increments() {
        let mut counter = 0;
        assert_eq!(fresh_reg(&mut counter), "%0");
        assert_eq!(fresh_reg(&mut counter), "%1");
        assert_eq!(fresh_reg(&mut counter), "%2");
    }

    #[test]
    fn resolve_runtime_name_builtins() {
        assert_eq!(resolve_runtime_name("println"), "kodo_println");
        assert_eq!(resolve_runtime_name("list_new"), "kodo_list_new");
        assert_eq!(resolve_runtime_name("String_length"), "kodo_string_length");
    }

    #[test]
    fn resolve_runtime_name_passthrough() {
        assert_eq!(
            resolve_runtime_name("kodo_contract_fail"),
            "kodo_contract_fail"
        );
        assert_eq!(resolve_runtime_name("my_function"), "my_function");
    }

    /// Verifies that all iterator function names are correctly mapped.
    #[test]
    fn resolve_runtime_name_iterators() {
        assert_eq!(resolve_runtime_name("list_iter"), "kodo_list_iter");
        assert_eq!(
            resolve_runtime_name("list_iterator_advance"),
            "kodo_list_iterator_advance"
        );
        assert_eq!(
            resolve_runtime_name("list_iterator_value"),
            "kodo_list_iterator_value"
        );
        assert_eq!(
            resolve_runtime_name("list_iterator_free"),
            "kodo_list_iterator_free"
        );
        assert_eq!(
            resolve_runtime_name("string_chars_advance"),
            "kodo_string_chars_advance"
        );
        assert_eq!(
            resolve_runtime_name("map_keys_advance"),
            "kodo_map_keys_advance"
        );
        assert_eq!(
            resolve_runtime_name("map_values_advance"),
            "kodo_map_values_advance"
        );
    }

    /// Verifies that Option/Result/List synthetic builtins are correctly mapped.
    #[test]
    fn resolve_runtime_name_synthetic_builtins() {
        assert_eq!(
            resolve_runtime_name("Option_is_some"),
            "kodo_option_is_some"
        );
        assert_eq!(
            resolve_runtime_name("Option_is_none"),
            "kodo_option_is_none"
        );
        assert_eq!(resolve_runtime_name("Result_is_ok"), "kodo_result_is_ok");
        assert_eq!(resolve_runtime_name("Result_is_err"), "kodo_result_is_err");
        assert_eq!(resolve_runtime_name("List_map"), "kodo_list_map");
        assert_eq!(resolve_runtime_name("List_filter"), "kodo_list_filter");
    }
}
