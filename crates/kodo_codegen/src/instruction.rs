//! Translation of MIR instructions to Cranelift IR.
//!
//! Handles `Assign`, `Call`, `IncRef`, `DecRef`, and `IndirectCall`
//! instructions, including special-cased string builtin expansion.

use std::collections::HashMap;

use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{AbiParam, InstBuilder, MemFlags, Signature};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::ObjectModule;
use kodo_mir::{Instruction, LocalId, Value};
use kodo_types::Type;

use crate::builtins::BuiltinInfo;
use crate::function::{HeapKind, VarMap};
use crate::layout::{
    EnumLayout, StructLayout, STRING_LAYOUT_SIZE, STRING_LEN_OFFSET, STRING_PTR_OFFSET,
};
use crate::module::{cranelift_type, is_composite, is_unit};
use crate::value::{
    create_string_data, expand_string_value_with_builtins, infer_value_type, resolve_enum_addr,
    translate_value,
};
use crate::{CodegenError, Result};

/// Returns true if the callee is a builtin that needs special handling
/// (string arg expansion, out-parameter returns, etc.).
pub(crate) fn is_special_builtin(callee: &str) -> bool {
    matches!(
        callee,
        "println"
            | "print"
            | "kodo_contract_fail"
            | "kodo_contract_fail_recoverable"
            | "String_length"
            | "String_contains"
            | "String_starts_with"
            | "String_ends_with"
            | "String_trim"
            | "String_to_upper"
            | "String_to_lower"
            | "String_substring"
            | "String_split"
            | "String_lines"
            | "String_parse_int"
            | "String_concat"
            | "String_index_of"
            | "String_replace"
            | "String_chars"
            | "String_char_at"
            | "String_repeat"
            | "list_join"
            | "json_get_bool"
            | "json_get_float"
            | "json_get_array"
            | "json_get_object"
            | "json_set_float"
            | "Int_to_string"
            | "Float64_to_string"
            | "Bool_to_string"
            | "file_exists"
            | "file_read"
            | "file_write"
            | "list_get"
            | "map_get"
            | "map_get_sk"
            | "map_get_sv"
            | "map_get_ss"
            | "map_insert_sk"
            | "map_insert_sv"
            | "map_insert_ss"
            | "map_contains_key_sk"
            | "map_remove_sk"
            | "http_get"
            | "http_post"
            | "json_parse"
            | "json_get_string"
            | "json_get_int"
            | "time_format"
            | "env_get"
            | "env_set"
            | "channel_send_string"
            | "channel_recv_string"
            | "readln"
            | "json_stringify"
            | "json_set_string"
            | "json_set_int"
            | "json_set_bool"
            | "file_append"
            | "file_delete"
            | "dir_list"
            | "dir_exists"
            | "http_request_method"
            | "http_request_path"
            | "http_request_body"
            | "http_respond"
            | "db_open"
            | "db_execute"
            | "db_query"
            | "db_row_get_string"
            | "db_row_get_int"
            | "db_result_free"
            | "db_close"
            | "char_at"
            | "char_from_code"
            | "is_alpha"
            | "is_digit"
            | "is_alphanumeric"
            | "is_whitespace"
            | "string_builder_push"
            | "string_builder_push_char"
            | "string_builder_to_string"
            | "string_builder_len"
            | "string_builder_new"
            | "format_int"
            | "timestamp"
            | "sleep"
    )
}

/// Returns true if the builtin returns a borrowed String slice via
/// out-parameters (no heap allocation — must NOT be freed).
///
/// `substring` and `trim` return pointers into the original string data,
/// so calling `kodo_string_free` on them would free memory that was never
/// allocated by `Box::into_raw`, causing a double-free / SIGABRT on exit.
pub(crate) fn is_borrowed_string_builtin(callee: &str) -> bool {
    matches!(callee, "String_trim" | "String_substring")
}

/// Returns true if the builtin returns a String via out-parameters.
pub(crate) fn is_string_returning_builtin(callee: &str) -> bool {
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

/// Returns true if the builtin allocates a new list on the heap.
pub(crate) fn is_list_allocating_builtin(callee: &str) -> bool {
    matches!(
        callee,
        "list_new"
            | "String_split"
            | "String_lines"
            | "list_slice"
            | "args"
            | "dir_list"
            | "List_map"
            | "List_filter"
    )
}

/// Returns true if the builtin allocates a new map on the heap.
pub(crate) fn is_map_allocating_builtin(callee: &str) -> bool {
    matches!(callee, "map_new" | "map_merge" | "map_filter")
}

/// Returns true if the builtin allocates a new set on the heap.
pub(crate) fn is_set_allocating_builtin(callee: &str) -> bool {
    matches!(
        callee,
        "set_new" | "set_union" | "set_intersection" | "set_difference"
    )
}

/// Translates a single MIR instruction.
#[allow(clippy::too_many_arguments)]
pub(crate) fn translate_instruction(
    instr: &Instruction,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &mut VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
    enum_layouts: &HashMap<String, EnumLayout>,
) -> Result<()> {
    match instr {
        Instruction::Assign(local_id, value) => translate_assign(
            *local_id,
            value,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
            enum_layouts,
        ),
        Instruction::Call { dest, callee, args } => translate_call(
            *dest,
            callee,
            args,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
            enum_layouts,
        ),
        Instruction::IncRef(local_id) => {
            // Call kodo_rc_inc to increment the reference count.
            if let Ok(var) = var_map.get(*local_id) {
                let handle_val = builder.use_var(var);
                if let Some(bi) = builtins.get("kodo_rc_inc") {
                    let func_ref = module.declare_func_in_func(bi.func_id, builder.func);
                    builder.ins().call(func_ref, &[handle_val]);
                }
            }
            Ok(())
        }
        Instruction::DecRef(local_id) => {
            if let Ok(var) = var_map.get(*local_id) {
                let handle_val = builder.use_var(var);
                if let Some(bi) = builtins.get("kodo_rc_dec") {
                    let func_ref = module.declare_func_in_func(bi.func_id, builder.func);
                    builder.ins().call(func_ref, &[handle_val]);
                }
            }
            Ok(())
        }
        Instruction::IndirectCall {
            dest,
            callee,
            args,
            return_type,
            param_types,
        } => translate_indirect_call(
            *dest,
            callee,
            args,
            return_type,
            param_types,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
        ),
        Instruction::VirtualCall {
            dest,
            object,
            vtable_index,
            args,
            return_type,
            param_types,
        } => translate_virtual_call(
            *dest,
            *object,
            *vtable_index,
            args,
            return_type,
            param_types,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
        ),
        // Emit a cooperative yield point: call kodo_green_maybe_yield() so the
        // green-thread scheduler can switch to another ready coroutine.
        Instruction::Yield => {
            if let Some(bi) = builtins.get("kodo_green_maybe_yield") {
                let func_ref = module.declare_func_in_func(bi.func_id, builder.func);
                builder.ins().call(func_ref, &[]);
            }
            Ok(())
        }
    }
}

/// Translates an Assign instruction.
#[allow(clippy::too_many_arguments)]
fn translate_assign(
    local_id: LocalId,
    value: &Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &mut VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
    enum_layouts: &HashMap<String, EnumLayout>,
) -> Result<()> {
    // Handle String + String concatenation via the `+` operator.
    if let Value::BinOp(kodo_ast::BinOp::Add, lhs, rhs) = value {
        let lhs_ty = infer_value_type(lhs, var_map);
        let rhs_ty = infer_value_type(rhs, var_map);
        if lhs_ty == Some(Type::String) || rhs_ty == Some(Type::String) {
            return translate_string_concat_assign(
                local_id, lhs, rhs, builder, module, builtins, var_map,
            );
        }
    }

    // Handle StringConst assignment to a String stack slot.
    if let Value::StringConst(s) = value {
        if translate_string_const_assign(local_id, s, builder, module, var_map)? {
            return Ok(());
        }
    }

    // Handle MakeDynTrait: construct a fat pointer in a stack slot.
    if let Value::MakeDynTrait {
        value: inner_value,
        concrete_type,
        trait_name,
    } = value
    {
        return translate_make_dyn_trait(
            local_id,
            inner_value,
            concrete_type,
            trait_name,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
        );
    }

    // Dispatch to specialized handlers based on value type.
    if let Some(result) = try_translate_enum_or_struct_assign(
        local_id,
        value,
        builder,
        module,
        func_ids,
        builtins,
        var_map,
        struct_layouts,
        enum_layouts,
    ) {
        return result;
    }

    let val = translate_value(
        value,
        builder,
        module,
        func_ids,
        builtins,
        var_map,
        struct_layouts,
    )?;
    var_map.def_var_with_cast(local_id, val, builder)
}

/// Handles `StringConst` assignment to a `_String` stack slot.
/// Returns `Ok(true)` if handled, `Ok(false)` if not a string slot.
fn translate_string_const_assign(
    local_id: LocalId,
    s: &str,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    var_map: &VarMap,
) -> Result<bool> {
    if let Some((slot, ref slot_name)) = var_map.stack_slots.get(&local_id) {
        if slot_name == "_String" {
            let data_id = create_string_data(module, s)?;
            let gv = module.declare_data_in_func(data_id, builder.func);
            let ptr = builder.ins().symbol_value(types::I64, gv);
            #[allow(clippy::cast_possible_wrap)]
            let len = builder.ins().iconst(types::I64, s.len() as i64);
            let base = builder
                .ins()
                .stack_addr(types::I64, *slot, STRING_PTR_OFFSET);
            builder.ins().store(MemFlags::new(), ptr, base, 0);
            let len_addr = builder
                .ins()
                .stack_addr(types::I64, *slot, STRING_LEN_OFFSET);
            builder.ins().store(MemFlags::new(), len, len_addr, 0);
            let var = var_map.get(local_id)?;
            let addr = builder.ins().stack_addr(types::I64, *slot, 0);
            builder.def_var(var, addr);
            return Ok(true);
        }
    }
    Ok(false)
}

/// Tries to dispatch assignment to enum/struct/composite-copy specialized handlers.
/// Returns `Ok(Some(Ok(())))` if handled, `Ok(None)` to fall through to default.
#[allow(clippy::too_many_arguments)]
fn try_translate_enum_or_struct_assign(
    local_id: LocalId,
    value: &Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
    enum_layouts: &HashMap<String, EnumLayout>,
) -> Option<Result<()>> {
    // Handle enum variant assignment.
    if let Value::EnumVariant {
        discriminant, args, ..
    } = value
    {
        return Some(translate_enum_variant_assign(
            local_id,
            *discriminant,
            args,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
            enum_layouts,
        ));
    }

    // Handle enum discriminant extraction.
    if let Value::EnumDiscriminant(inner) = value {
        return Some(translate_enum_discriminant_assign(
            local_id,
            inner,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
        ));
    }

    // Handle enum payload extraction.
    if let Value::EnumPayload {
        value: inner,
        field_index,
    } = value
    {
        return Some(translate_enum_payload_assign(
            local_id,
            inner,
            *field_index,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
            enum_layouts,
        ));
    }

    // Handle struct literal assignment.
    if let Value::StructLit { name, fields } = value {
        if var_map.stack_slots.contains_key(&local_id) {
            return Some(translate_struct_lit_assign(
                local_id,
                name,
                fields,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            ));
        }
    }

    // Handle field get assignment.
    if let Value::FieldGet {
        object,
        field,
        struct_name,
    } = value
    {
        return Some(translate_field_get_assign(
            local_id,
            object,
            field,
            struct_name,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
        ));
    }

    // Handle struct/enum copy: Assign(dest, Local(src)) where both have stack slots.
    if let Value::Local(src_id) = value {
        if let (Some((dest_slot, _)), Some((src_slot, _))) = (
            var_map.stack_slots.get(&local_id),
            var_map.stack_slots.get(src_id),
        ) {
            return Some(translate_composite_copy(
                local_id,
                *src_id,
                *dest_slot,
                *src_slot,
                builder,
                var_map,
                struct_layouts,
                enum_layouts,
            ));
        }
    }

    None
}

/// Translates String concatenation assignment.
fn translate_string_concat_assign(
    local_id: LocalId,
    lhs: &Value,
    rhs: &Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &mut VarMap,
) -> Result<()> {
    let (lhs_ptr, lhs_len) =
        expand_string_value_with_builtins(lhs, builder, module, var_map, Some(builtins))?;
    let (rhs_ptr, rhs_len) =
        expand_string_value_with_builtins(rhs, builder, module, var_map, Some(builtins))?;
    let out_slot = builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
        cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
        16,
        0,
    ));
    let out_ptr_addr = builder.ins().stack_addr(types::I64, out_slot, 0);
    let out_len_addr = builder.ins().stack_addr(types::I64, out_slot, 8);
    let concat_info = builtins
        .get("String_concat")
        .ok_or_else(|| CodegenError::Unsupported("String_concat builtin not found".to_string()))?;
    let func_ref = module.declare_func_in_func(concat_info.func_id, builder.func);
    builder.ins().call(
        func_ref,
        &[
            lhs_ptr,
            lhs_len,
            rhs_ptr,
            rhs_len,
            out_ptr_addr,
            out_len_addr,
        ],
    );
    if let Some((dest_slot, ref dest_name)) = var_map.stack_slots.get(&local_id) {
        if dest_name == "_String" {
            let result_ptr = builder
                .ins()
                .load(types::I64, MemFlags::new(), out_ptr_addr, 0);
            let result_len = builder
                .ins()
                .load(types::I64, MemFlags::new(), out_len_addr, 0);
            let dest_ptr_addr = builder
                .ins()
                .stack_addr(types::I64, *dest_slot, STRING_PTR_OFFSET);
            builder
                .ins()
                .store(MemFlags::new(), result_ptr, dest_ptr_addr, 0);
            let dest_len_addr = builder
                .ins()
                .stack_addr(types::I64, *dest_slot, STRING_LEN_OFFSET);
            builder
                .ins()
                .store(MemFlags::new(), result_len, dest_len_addr, 0);
            let var = var_map.get(local_id)?;
            let addr = builder.ins().stack_addr(types::I64, *dest_slot, 0);
            builder.def_var(var, addr);
            // Mark as heap-allocated so it will be freed before return.
            var_map.heap_locals.insert(local_id, HeapKind::String);
            return Ok(());
        }
    }
    let result_ptr = builder
        .ins()
        .load(types::I64, MemFlags::new(), out_ptr_addr, 0);
    let var = var_map.get(local_id)?;
    builder.def_var(var, result_ptr);
    // Mark as heap-allocated so it will be freed before return.
    var_map.heap_locals.insert(local_id, HeapKind::String);
    Ok(())
}

/// Translates enum variant assignment.
#[allow(clippy::too_many_arguments)]
fn translate_enum_variant_assign(
    local_id: LocalId,
    discriminant: u8,
    args: &[Value],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
    enum_layouts: &HashMap<String, EnumLayout>,
) -> Result<()> {
    if let Some((slot, _)) = var_map.stack_slots.get(&local_id) {
        // Store discriminant at offset 0.
        #[allow(clippy::cast_lossless)]
        let disc_val = builder.ins().iconst(types::I64, discriminant as i64);
        let disc_addr = builder.ins().stack_addr(types::I64, *slot, 0);
        builder.ins().store(MemFlags::new(), disc_val, disc_addr, 0);
        // Store payload fields at offsets 8, 16, 24, ...
        // For String payloads, we store a pointer to a KodoString-like
        // stack slot (ptr, len) so that payload extraction can dereference
        // it uniformly — matching the layout used by runtime builtins.
        for (idx, arg) in args.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            let field_offset = (8 + idx * 8) as i32;
            let dest_addr = builder.ins().stack_addr(types::I64, *slot, field_offset);

            // Check if this arg is a String (const or local with _String slot)
            // and store a pointer to a (ptr, len) pair instead of a raw value.
            let handled = match arg {
                Value::StringConst(s) => {
                    let data_id = create_string_data(module, s)?;
                    let gv = module.declare_data_in_func(data_id, builder.func);
                    let ptr = builder.ins().symbol_value(types::I64, gv);
                    #[allow(clippy::cast_possible_wrap)]
                    let len = builder.ins().iconst(types::I64, s.len() as i64);
                    let tmp_slot =
                        builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
                            cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                            STRING_LAYOUT_SIZE,
                            0,
                        ));
                    let tmp_addr = builder.ins().stack_addr(types::I64, tmp_slot, 0);
                    builder
                        .ins()
                        .store(MemFlags::new(), ptr, tmp_addr, STRING_PTR_OFFSET);
                    builder
                        .ins()
                        .store(MemFlags::new(), len, tmp_addr, STRING_LEN_OFFSET);
                    builder.ins().store(MemFlags::new(), tmp_addr, dest_addr, 0);
                    true
                }
                Value::Local(arg_id) => {
                    if let Some((arg_slot, ref sn)) = var_map.stack_slots.get(arg_id) {
                        if sn == "_String" {
                            let str_addr = builder.ins().stack_addr(types::I64, *arg_slot, 0);
                            builder.ins().store(MemFlags::new(), str_addr, dest_addr, 0);
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
                _ => false,
            };

            if !handled {
                let val = translate_value(
                    arg,
                    builder,
                    module,
                    func_ids,
                    builtins,
                    var_map,
                    struct_layouts,
                )?;
                builder.ins().store(MemFlags::new(), val, dest_addr, 0);
            }
        }
        let base_addr = builder.ins().stack_addr(types::I64, *slot, 0);
        let var = var_map.get(local_id)?;
        builder.def_var(var, base_addr);
        return Ok(());
    }
    // Fallback: no stack slot, store discriminant as scalar.
    let _ = enum_layouts;
    #[allow(clippy::cast_lossless)]
    let disc_val = builder.ins().iconst(types::I64, discriminant as i64);
    let var = var_map.get(local_id)?;
    builder.def_var(var, disc_val);
    Ok(())
}

/// Translates enum discriminant extraction assignment.
#[allow(clippy::too_many_arguments)]
fn translate_enum_discriminant_assign(
    local_id: LocalId,
    inner: &Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
) -> Result<()> {
    let addr = match inner {
        Value::Local(obj_id) => {
            if let Some((slot, _)) = var_map.stack_slots.get(obj_id) {
                builder.ins().stack_addr(types::I64, *slot, 0)
            } else {
                let var = var_map.get(*obj_id)?;
                builder.use_var(var)
            }
        }
        _ => translate_value(
            inner,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
        )?,
    };
    let disc = builder.ins().load(types::I64, MemFlags::new(), addr, 0);
    let var = var_map.get(local_id)?;
    builder.def_var(var, disc);
    Ok(())
}

/// Translates enum payload extraction assignment.
#[allow(clippy::too_many_arguments)]
fn translate_enum_payload_assign(
    local_id: LocalId,
    inner: &Value,
    field_index: u32,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
    enum_layouts: &HashMap<String, EnumLayout>,
) -> Result<()> {
    let addr = match inner {
        Value::Local(obj_id) => {
            if let Some((slot, _)) = var_map.stack_slots.get(obj_id) {
                builder.ins().stack_addr(types::I64, *slot, 0)
            } else {
                let var = var_map.get(*obj_id)?;
                builder.use_var(var)
            }
        }
        _ => translate_value(
            inner,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
        )?,
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    let field_offset = (8 + (field_index as usize) * 8) as i32;
    let field_addr = builder.ins().iadd_imm(addr, i64::from(field_offset));
    let loaded = builder
        .ins()
        .load(types::I64, MemFlags::new(), field_addr, 0);

    // If the destination has a composite stack slot, the extracted payload is
    // a pointer to the source data. Copy it into the destination stack slot.
    if let Some((dest_slot, ref dest_name)) = var_map.stack_slots.get(&local_id) {
        if dest_name == "_String" {
            // String: copy (ptr, len) from the source.
            let str_ptr = builder.ins().load(types::I64, MemFlags::new(), loaded, 0);
            let str_len =
                builder
                    .ins()
                    .load(types::I64, MemFlags::new(), loaded, STRING_LEN_OFFSET);
            let dest_ptr_addr = builder
                .ins()
                .stack_addr(types::I64, *dest_slot, STRING_PTR_OFFSET);
            builder
                .ins()
                .store(MemFlags::new(), str_ptr, dest_ptr_addr, 0);
            let dest_len_addr = builder
                .ins()
                .stack_addr(types::I64, *dest_slot, STRING_LEN_OFFSET);
            builder
                .ins()
                .store(MemFlags::new(), str_len, dest_len_addr, 0);
            let var = var_map.get(local_id)?;
            let slot_addr = builder.ins().stack_addr(types::I64, *dest_slot, 0);
            builder.def_var(var, slot_addr);
            return Ok(());
        }
        // Enum or struct destination: the payload is a pointer to the source
        // composite data. Copy it word-by-word into the dest stack slot.
        let slot_size = enum_layouts
            .get(dest_name)
            .map(|l| l.total_size)
            .or_else(|| struct_layouts.get(dest_name).map(|l| l.total_size))
            .unwrap_or(8);
        let num_words = slot_size.div_ceil(8);
        let dest_addr = builder.ins().stack_addr(types::I64, *dest_slot, 0);
        for i in 0..num_words {
            #[allow(clippy::cast_possible_wrap)]
            let off = (i * 8) as i32;
            let src_field = builder.ins().iadd_imm(loaded, i64::from(off));
            let val = builder
                .ins()
                .load(types::I64, MemFlags::new(), src_field, 0);
            let dest_field = builder.ins().iadd_imm(dest_addr, i64::from(off));
            builder.ins().store(MemFlags::new(), val, dest_field, 0);
        }
        let var = var_map.get(local_id)?;
        builder.def_var(var, dest_addr);
        return Ok(());
    }

    let var = var_map.get(local_id)?;
    builder.def_var(var, loaded);
    Ok(())
}

/// Translates struct literal assignment into a stack slot.
#[allow(clippy::too_many_arguments)]
fn translate_struct_lit_assign(
    local_id: LocalId,
    name: &str,
    fields: &[(String, Value)],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
) -> Result<()> {
    let (slot, _) = var_map.stack_slots[&local_id];
    let layout = struct_layouts
        .get(name)
        .ok_or_else(|| CodegenError::Unsupported(format!("unknown struct: {name}")))?;
    for (field_name, field_val) in fields {
        let (_, offset, _cl_ty) = layout
            .field_offsets
            .iter()
            .find(|(n, _, _)| n == field_name)
            .ok_or_else(|| {
                CodegenError::Unsupported(format!("unknown field {field_name} in struct {name}"))
            })?;
        // If the field value is a String (stack slot or const),
        // copy both ptr and len (16 bytes) into the struct.
        if let Value::StringConst(s) = field_val {
            let data_id = create_string_data(module, s)?;
            let gv = module.declare_data_in_func(data_id, builder.func);
            let ptr = builder.ins().symbol_value(types::I64, gv);
            #[allow(clippy::cast_possible_wrap)]
            let len = builder.ins().iconst(types::I64, s.len() as i64);
            #[allow(clippy::cast_possible_wrap)]
            let faddr = builder.ins().stack_addr(types::I64, slot, *offset as i32);
            builder.ins().store(MemFlags::new(), ptr, faddr, 0);
            let faddr_len = builder.ins().iadd_imm(faddr, i64::from(STRING_LEN_OFFSET));
            builder.ins().store(MemFlags::new(), len, faddr_len, 0);
            continue;
        }
        if let Value::Local(src_id) = field_val {
            if let Some((src_slot, ref sn)) = var_map.stack_slots.get(src_id) {
                if sn == "_String" {
                    let sp = builder
                        .ins()
                        .stack_addr(types::I64, *src_slot, STRING_PTR_OFFSET);
                    let ptr = builder.ins().load(types::I64, MemFlags::new(), sp, 0);
                    let sl = builder
                        .ins()
                        .stack_addr(types::I64, *src_slot, STRING_LEN_OFFSET);
                    let len = builder.ins().load(types::I64, MemFlags::new(), sl, 0);
                    #[allow(clippy::cast_possible_wrap)]
                    let faddr = builder.ins().stack_addr(types::I64, slot, *offset as i32);
                    builder.ins().store(MemFlags::new(), ptr, faddr, 0);
                    let faddr_len = builder.ins().iadd_imm(faddr, i64::from(STRING_LEN_OFFSET));
                    builder.ins().store(MemFlags::new(), len, faddr_len, 0);
                    continue;
                }
            }
        }
        let val = translate_value(
            field_val,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
        )?;
        #[allow(clippy::cast_possible_wrap)]
        let addr = builder.ins().stack_addr(types::I64, slot, *offset as i32);
        builder.ins().store(MemFlags::new(), val, addr, 0);
    }
    // Set the variable to the stack slot address.
    let base_addr = builder.ins().stack_addr(types::I64, slot, 0);
    let var = var_map.get(local_id)?;
    builder.def_var(var, base_addr);
    Ok(())
}

/// Translates field get assignment.
#[allow(clippy::too_many_arguments)]
fn translate_field_get_assign(
    local_id: LocalId,
    object: &Value,
    field: &str,
    struct_name: &str,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
) -> Result<()> {
    let layout = struct_layouts
        .get(struct_name)
        .ok_or_else(|| CodegenError::Unsupported(format!("unknown struct: {struct_name}")))?;
    let (_, offset, cl_ty) = layout
        .field_offsets
        .iter()
        .find(|(n, _, _)| n == field)
        .ok_or_else(|| {
            CodegenError::Unsupported(format!("unknown field {field} in struct {struct_name}"))
        })?;
    // Get the object's stack slot address.
    let obj_addr = match object {
        Value::Local(obj_id) => {
            if let Some((slot, _)) = var_map.stack_slots.get(obj_id) {
                builder.ins().stack_addr(types::I64, *slot, 0)
            } else {
                let var = var_map.get(*obj_id)?;
                builder.use_var(var)
            }
        }
        _ => translate_value(
            object,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
        )?,
    };
    let field_addr = builder.ins().iadd_imm(obj_addr, i64::from(*offset));
    // If the dest is a _String stack slot, copy both ptr and len (16 bytes).
    if let Some((dest_slot, ref dest_name)) = var_map.stack_slots.get(&local_id) {
        if dest_name == "_String" {
            let ptr = builder
                .ins()
                .load(types::I64, MemFlags::new(), field_addr, 0);
            let len_addr = builder
                .ins()
                .iadd_imm(field_addr, i64::from(STRING_LEN_OFFSET));
            let len = builder.ins().load(types::I64, MemFlags::new(), len_addr, 0);
            let dp = builder
                .ins()
                .stack_addr(types::I64, *dest_slot, STRING_PTR_OFFSET);
            builder.ins().store(MemFlags::new(), ptr, dp, 0);
            let dl = builder
                .ins()
                .stack_addr(types::I64, *dest_slot, STRING_LEN_OFFSET);
            builder.ins().store(MemFlags::new(), len, dl, 0);
            let var = var_map.get(local_id)?;
            let addr = builder.ins().stack_addr(types::I64, *dest_slot, 0);
            builder.def_var(var, addr);
            return Ok(());
        }
    }
    let loaded = builder.ins().load(*cl_ty, MemFlags::new(), field_addr, 0);
    let var = var_map.get(local_id)?;
    builder.def_var(var, loaded);
    Ok(())
}

/// Translates composite (struct/enum/string) copy between stack slots.
#[allow(clippy::too_many_arguments)]
fn translate_composite_copy(
    local_id: LocalId,
    src_id: LocalId,
    dest_slot: cranelift_codegen::ir::StackSlot,
    src_slot: cranelift_codegen::ir::StackSlot,
    builder: &mut FunctionBuilder,
    var_map: &VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
    enum_layouts: &HashMap<String, EnumLayout>,
) -> Result<()> {
    let src_addr = builder.ins().stack_addr(types::I64, src_slot, 0);
    let dest_addr = builder.ins().stack_addr(types::I64, dest_slot, 0);
    // Copy 8-byte chunks. Find slot size from struct/enum layouts.
    let src_slot_name = &var_map.stack_slots[&src_id].1;
    let slot_size = if src_slot_name == "_String" {
        crate::layout::STRING_LAYOUT_SIZE
    } else {
        struct_layouts
            .get(src_slot_name)
            .map(|l| l.total_size)
            .or_else(|| enum_layouts.get(src_slot_name).map(|l| l.total_size))
            .unwrap_or(8)
    };
    let num_words = slot_size.div_ceil(8);
    for i in 0..num_words {
        #[allow(clippy::cast_possible_wrap)]
        let off = (i * 8) as i32;
        let src_field = builder.ins().iadd_imm(src_addr, i64::from(off));
        let val = builder
            .ins()
            .load(types::I64, MemFlags::new(), src_field, 0);
        let dest_field = builder.ins().iadd_imm(dest_addr, i64::from(off));
        builder.ins().store(MemFlags::new(), val, dest_field, 0);
    }
    let var = var_map.get(local_id)?;
    builder.def_var(var, dest_addr);
    Ok(())
}

/// Translates a Call instruction.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn translate_call(
    dest: LocalId,
    callee: &str,
    args: &[Value],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &mut VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
    enum_layouts: &HashMap<String, EnumLayout>,
) -> Result<()> {
    // Synthetic __env_pack: allocate a stack slot and pack capture
    // values into it, returning a pointer to the slot.
    if callee == "__env_pack" {
        return translate_env_pack(
            dest,
            args,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
        );
    }

    // Synthetic __env_load: load a value from an env pointer at a given byte offset.
    if callee == "__env_load" {
        if let (Some(ptr_arg), Some(Value::IntConst(offset))) = (args.first(), args.get(1)) {
            let ptr_val = translate_value(
                ptr_arg,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            )?;
            #[allow(clippy::cast_possible_truncation)]
            let off = *offset as i32;
            let val = builder
                .ins()
                .load(types::I64, MemFlags::new(), ptr_val, off);
            var_map.def_var_with_cast(dest, val, builder)?;
            return Ok(());
        }
    }

    // Synthetic __env_load_string: load a String (ptr+len) from an env pointer.
    // The env stores a pointer to a _String slot (8 bytes). We load that
    // pointer, then copy the 16 bytes (ptr, len) from it into the dest's
    // _String stack slot.
    if callee == "__env_load_string" {
        if let (Some(ptr_arg), Some(Value::IntConst(offset))) = (args.first(), args.get(1)) {
            let ptr_val = translate_value(
                ptr_arg,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            )?;
            #[allow(clippy::cast_possible_truncation)]
            let off = *offset as i32;
            // Load the pointer to the source _String slot.
            let src_addr = builder
                .ins()
                .load(types::I64, MemFlags::new(), ptr_val, off);
            // Load ptr and len from the source slot.
            let str_ptr =
                builder
                    .ins()
                    .load(types::I64, MemFlags::new(), src_addr, STRING_PTR_OFFSET);
            let str_len =
                builder
                    .ins()
                    .load(types::I64, MemFlags::new(), src_addr, STRING_LEN_OFFSET);
            // Store into the dest's _String stack slot.
            if let Some((slot, ref slot_name)) = var_map.stack_slots.get(&dest) {
                if slot_name == "_String" {
                    let dest_ptr_addr =
                        builder
                            .ins()
                            .stack_addr(types::I64, *slot, STRING_PTR_OFFSET);
                    builder
                        .ins()
                        .store(MemFlags::new(), str_ptr, dest_ptr_addr, 0);
                    let dest_len_addr =
                        builder
                            .ins()
                            .stack_addr(types::I64, *slot, STRING_LEN_OFFSET);
                    builder
                        .ins()
                        .store(MemFlags::new(), str_len, dest_len_addr, 0);
                    let var = var_map.get(dest)?;
                    let addr = builder.ins().stack_addr(types::I64, *slot, 0);
                    builder.def_var(var, addr);
                    return Ok(());
                }
            }
            // Fallback: just store the pointer.
            var_map.def_var_with_cast(dest, src_addr, builder)?;
            return Ok(());
        }
    }

    // Synthetic __future_await_string: await a future carrying a composite
    // String result (ptr + len = 16 bytes) and copy it into the dest's
    // _String stack slot.
    if callee == "__future_await_string" {
        if let Some(handle_arg) = args.first() {
            let handle_val = translate_value(
                handle_arg,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            )?;
            if let Some((slot, ref slot_name)) = var_map.stack_slots.get(&dest) {
                if slot_name == "_String" {
                    let slot_addr = builder.ins().stack_addr(types::I64, *slot, 0);
                    let data_size = builder.ins().iconst(types::I64, 16);
                    // Call kodo_future_await_bytes(handle, out_ptr, data_size)
                    if let Some(bi) = builtins.get("kodo_future_await_bytes") {
                        let func_ref = module.declare_func_in_func(bi.func_id, builder.func);
                        builder
                            .ins()
                            .call(func_ref, &[handle_val, slot_addr, data_size]);
                    }
                    // Point the variable at the stack slot.
                    let var = var_map.get(dest)?;
                    builder.def_var(var, slot_addr);
                    // Mark as heap-allocated for cleanup.
                    var_map.heap_locals.insert(dest, HeapKind::String);
                    return Ok(());
                }
            }
        }
    }

    // Synthetic __future_complete_string: complete a future with a String
    // value (ptr + len = 16 bytes) from a _String stack slot.
    if callee == "__future_complete_string" {
        if let (Some(handle_arg), Some(str_arg)) = (args.first(), args.get(1)) {
            let handle_val = translate_value(
                handle_arg,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            )?;
            let str_val = translate_value(
                str_arg,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            )?;
            let data_size = builder.ins().iconst(types::I64, 16);
            // Call kodo_future_complete_bytes(handle, data_ptr, data_size)
            if let Some(bi) = builtins.get("kodo_future_complete_bytes") {
                let func_ref = module.declare_func_in_func(bi.func_id, builder.func);
                builder
                    .ins()
                    .call(func_ref, &[handle_val, str_val, data_size]);
            }
            return Ok(());
        }
    }

    // Enum discriminant check methods — inline, no FFI needed.
    // Layout: [discriminant i64 @ offset 0][payload @ offset 8+].
    // is_ok/is_some = discriminant == 0; is_err/is_none = discriminant != 0.
    if matches!(
        callee,
        "Result_is_ok" | "Result_is_err" | "Option_is_some" | "Option_is_none"
    ) {
        if let Some(arg) = args.first() {
            let addr = resolve_enum_addr(
                arg,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            )?;
            let disc = builder.ins().load(types::I64, MemFlags::new(), addr, 0);
            let cmp = match callee {
                "Result_is_ok" | "Option_is_some" => {
                    builder
                        .ins()
                        .icmp_imm(cranelift_codegen::ir::condcodes::IntCC::Equal, disc, 0)
                }
                _ => builder.ins().icmp_imm(
                    cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                    disc,
                    0,
                ),
            };
            var_map.def_var_with_cast(dest, cmp, builder)?;
            return Ok(());
        }
    }

    // Result/Option unwrap methods — inline payload extraction with trap on wrong variant.
    // Layout: [discriminant i64 @ offset 0][payload i64 @ offset 8].
    // unwrap() traps if Err/None; unwrap_err() traps if Ok.
    if matches!(
        callee,
        "Result_unwrap" | "Result_unwrap_err" | "Option_unwrap"
    ) {
        if let Some(arg) = args.first() {
            let addr = resolve_enum_addr(
                arg,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            )?;
            let disc = builder.ins().load(types::I64, MemFlags::new(), addr, 0);

            // Trap if the enum holds the wrong variant.
            match callee {
                "Result_unwrap" | "Option_unwrap" => {
                    // Err/None has discriminant != 0 → trap.
                    builder
                        .ins()
                        .trapnz(disc, cranelift_codegen::ir::TrapCode::unwrap_user(1));
                }
                _ => {
                    // unwrap_err: Ok has discriminant == 0 → trap.
                    builder
                        .ins()
                        .trapz(disc, cranelift_codegen::ir::TrapCode::unwrap_user(1));
                }
            }

            // Extract payload from offset 8.
            let payload = builder.ins().load(types::I64, MemFlags::new(), addr, 8);

            // For composite destinations (String, enum, struct), the payload
            // is a pointer to the source data. Copy it into the dest stack slot.
            if let Some((dest_slot, ref slot_name)) = var_map.stack_slots.get(&dest) {
                if slot_name == "_String" {
                    // String: copy (ptr, len) from the source.
                    let ptr = builder.ins().load(types::I64, MemFlags::new(), payload, 0);
                    let len = builder.ins().load(types::I64, MemFlags::new(), payload, 8);
                    let dest_addr = builder.ins().stack_addr(types::I64, *dest_slot, 0);
                    builder.ins().store(MemFlags::new(), ptr, dest_addr, 0);
                    builder.ins().store(MemFlags::new(), len, dest_addr, 8);
                    let var = var_map.get(dest)?;
                    builder.def_var(var, dest_addr);
                    return Ok(());
                }
                // Enum or struct destination: the payload is a pointer to the
                // source composite data. Copy it word-by-word into the dest slot.
                let slot_size = enum_layouts
                    .get(slot_name)
                    .map(|l| l.total_size)
                    .or_else(|| struct_layouts.get(slot_name).map(|l| l.total_size))
                    .unwrap_or(8);
                let num_words = slot_size.div_ceil(8);
                let dest_addr = builder.ins().stack_addr(types::I64, *dest_slot, 0);
                for i in 0..num_words {
                    #[allow(clippy::cast_possible_wrap)]
                    let off = (i * 8) as i32;
                    let src_field = builder.ins().iadd_imm(payload, i64::from(off));
                    let val = builder
                        .ins()
                        .load(types::I64, MemFlags::new(), src_field, 0);
                    let dest_field = builder.ins().iadd_imm(dest_addr, i64::from(off));
                    builder.ins().store(MemFlags::new(), val, dest_field, 0);
                }
                let var = var_map.get(dest)?;
                builder.def_var(var, dest_addr);
                return Ok(());
            }

            // For non-String destinations (Int, Bool, etc.), use the raw value.
            var_map.def_var_with_cast(dest, payload, builder)?;
            return Ok(());
        }
    }

    // Check if this is a builtin that needs special arg/return handling.
    if is_special_builtin(callee) {
        let handled = emit_string_builtin_call(
            callee,
            args,
            dest,
            builder,
            module,
            builtins,
            var_map,
            func_ids,
            struct_layouts,
        )?;
        if handled {
            // Mark heap-allocated string locals for cleanup.
            // Borrowed builtins (substring, trim) return pointers into
            // existing data and must NOT be freed.
            if is_string_returning_builtin(callee) && !is_borrowed_string_builtin(callee) {
                var_map.heap_locals.insert(dest, HeapKind::String);
            }
            return Ok(());
        }
    }

    // Track list/map/set allocating builtins for cleanup before return.
    if is_list_allocating_builtin(callee) {
        var_map.heap_locals.insert(dest, HeapKind::List);
    } else if is_map_allocating_builtin(callee) {
        var_map.heap_locals.insert(dest, HeapKind::Map);
    } else if is_set_allocating_builtin(callee) {
        var_map.heap_locals.insert(dest, HeapKind::Set);
    }

    // Check if the dest has a composite type (sret return from callee).
    let dest_is_composite = var_map.stack_slots.contains_key(&dest);

    let mut arg_vals = Vec::with_capacity(args.len() + 1);

    // If the callee returns a composite type, pass sret pointer as first arg.
    if dest_is_composite {
        if let Some((slot, _)) = var_map.stack_slots.get(&dest) {
            let sret_addr = builder.ins().stack_addr(types::I64, *slot, 0);
            arg_vals.push(sret_addr);
        }
    }

    for arg in args {
        // Check if this arg is a composite type (struct/enum) — pass its address.
        if let Value::Local(arg_local_id) = arg {
            if var_map.stack_slots.contains_key(arg_local_id) {
                // Pass the stack slot address as a pointer.
                let var = var_map.get(*arg_local_id)?;
                let addr = builder.use_var(var);
                arg_vals.push(addr);
                continue;
            }
        }
        // StringConst passed to a user function expecting composite String:
        // create a temp 16-byte stack slot with (ptr, len) and pass its address.
        if let Value::StringConst(s) = arg {
            let data_id = create_string_data(module, s)?;
            let gv = module.declare_data_in_func(data_id, builder.func);
            let ptr = builder.ins().symbol_value(types::I64, gv);
            #[allow(clippy::cast_possible_wrap)]
            let len = builder.ins().iconst(types::I64, s.len() as i64);
            let tmp_slot =
                builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
                    cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                    STRING_LAYOUT_SIZE,
                    0,
                ));
            let tmp_addr = builder.ins().stack_addr(types::I64, tmp_slot, 0);
            builder
                .ins()
                .store(MemFlags::new(), ptr, tmp_addr, STRING_PTR_OFFSET);
            builder
                .ins()
                .store(MemFlags::new(), len, tmp_addr, STRING_LEN_OFFSET);
            arg_vals.push(tmp_addr);
            continue;
        }
        arg_vals.push(translate_value(
            arg,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
        )?);
    }

    // Widen i8 (Bool) arguments to i64 when the callee signature expects i64.
    // This handles the mismatch between Bool values (represented as i8 in
    // Cranelift) and builtin functions declared with i64 parameters.
    let resolve_func_ref = builtins
        .get(callee)
        .map(|b| b.func_id)
        .or_else(|| func_ids.get(callee).copied());
    if let Some(fid) = resolve_func_ref {
        let sig = &module.declarations().get_function_decl(fid).signature;
        for (i, val) in arg_vals.iter_mut().enumerate() {
            if let Some(param) = sig.params.get(i) {
                let actual = builder.func.dfg.value_type(*val);
                if actual != param.value_type && !actual.is_float() && !param.value_type.is_float()
                {
                    if actual.bits() < param.value_type.bits() {
                        *val = builder.ins().uextend(param.value_type, *val);
                    } else if actual.bits() > param.value_type.bits() {
                        *val = builder.ins().ireduce(param.value_type, *val);
                    }
                }
            }
        }
    }

    if let Some(builtin) = builtins.get(callee) {
        let func_ref = module.declare_func_in_func(builtin.func_id, builder.func);
        let call = builder.ins().call(func_ref, &arg_vals);
        define_call_result(dest, dest_is_composite, call, builder, var_map)?;
    } else if let Some(&user_func_id) = func_ids.get(callee) {
        let func_ref = module.declare_func_in_func(user_func_id, builder.func);
        let call = builder.ins().call(func_ref, &arg_vals);
        define_call_result(dest, dest_is_composite, call, builder, var_map)?;
    } else {
        return Err(CodegenError::Unsupported(format!(
            "unknown function: {callee}"
        )));
    }
    Ok(())
}

/// Defines the result of a call instruction in the variable map.
fn define_call_result(
    dest: LocalId,
    dest_is_composite: bool,
    call: cranelift_codegen::ir::Inst,
    builder: &mut FunctionBuilder,
    var_map: &VarMap,
) -> Result<()> {
    if dest_is_composite {
        // The result was written into the stack slot via sret; set var to addr.
        if let Some((slot, _)) = var_map.stack_slots.get(&dest) {
            let var = var_map.get(dest)?;
            let addr = builder.ins().stack_addr(types::I64, *slot, 0);
            builder.def_var(var, addr);
        }
    } else {
        let results = builder.inst_results(call);
        if results.is_empty() {
            let zero = builder.ins().iconst(types::I64, 0);
            var_map.def_var_with_cast(dest, zero, builder)?;
        } else {
            var_map.def_var_with_cast(dest, results[0], builder)?;
        }
    }
    Ok(())
}

/// Translates the synthetic `__env_pack` call.
#[allow(clippy::too_many_arguments)]
fn translate_env_pack(
    dest: LocalId,
    args: &[Value],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
) -> Result<()> {
    let num_captures = args.len();
    #[allow(clippy::cast_possible_wrap)]
    let alloc_size = (num_captures as i64) * 8;

    // Heap-allocate via kodo_alloc so the buffer outlives the current frame.
    let alloc_builtin = builtins
        .get("kodo_alloc")
        .ok_or_else(|| CodegenError::Unsupported("kodo_alloc not declared".to_string()))?;
    let alloc_ref = module.declare_func_in_func(alloc_builtin.func_id, builder.func);
    let size_val = builder.ins().iconst(types::I64, alloc_size);
    let alloc_call = builder.ins().call(alloc_ref, &[size_val]);
    let env_ptr = builder.inst_results(alloc_call)[0];

    for (idx, arg) in args.iter().enumerate() {
        let val = translate_value(
            arg,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
        )?;
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let offset = (idx * 8) as i32;
        builder.ins().store(MemFlags::new(), val, env_ptr, offset);
    }
    let var = var_map.get(dest)?;
    builder.def_var(var, env_ptr);
    Ok(())
}

/// Translates an `IndirectCall` instruction.
#[allow(clippy::too_many_arguments)]
fn translate_indirect_call(
    dest: LocalId,
    callee: &Value,
    args: &[Value],
    return_type: &Type,
    param_types: &[Type],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
) -> Result<()> {
    // The closure handle is an i64 -- extract func_ptr and env_ptr.
    let handle_val = translate_value(
        callee,
        builder,
        module,
        func_ids,
        builtins,
        var_map,
        struct_layouts,
    )?;

    let closure_func_bi = builtins
        .get("kodo_closure_func")
        .ok_or_else(|| CodegenError::Unsupported("kodo_closure_func not declared".to_string()))?;
    let closure_func_ref = module.declare_func_in_func(closure_func_bi.func_id, builder.func);
    let func_call = builder.ins().call(closure_func_ref, &[handle_val]);
    let func_ptr = builder.inst_results(func_call)[0];

    let closure_env_bi = builtins
        .get("kodo_closure_env")
        .ok_or_else(|| CodegenError::Unsupported("kodo_closure_env not declared".to_string()))?;
    let closure_env_ref = module.declare_func_in_func(closure_env_bi.func_id, builder.func);
    let env_call = builder.ins().call(closure_env_ref, &[handle_val]);
    let env_ptr = builder.inst_results(env_call)[0];

    // Build the signature: env_ptr (i64) + original params.
    let mut sig = Signature::new(CallConv::SystemV);
    sig.params.push(AbiParam::new(types::I64)); // env_ptr
    for pt in param_types {
        sig.params.push(AbiParam::new(cranelift_type(pt)));
    }
    if !is_unit(return_type) {
        sig.returns.push(AbiParam::new(cranelift_type(return_type)));
    }
    let sig_ref = builder.import_signature(sig);

    // Build args: env_ptr first, then the user-visible arguments.
    let mut arg_vals = Vec::with_capacity(args.len() + 1);
    arg_vals.push(env_ptr);
    for arg in args {
        arg_vals.push(translate_value(
            arg,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
        )?);
    }

    let call = builder.ins().call_indirect(sig_ref, func_ptr, &arg_vals);
    let var = var_map.get(dest)?;
    if is_unit(return_type) {
        let zero = builder.ins().iconst(types::I64, 0);
        builder.def_var(var, zero);
    } else {
        let results = builder.inst_results(call);
        if results.is_empty() {
            let zero = builder.ins().iconst(types::I64, 0);
            builder.def_var(var, zero);
        } else {
            builder.def_var(var, results[0]);
        }
    }
    Ok(())
}

/// Emits a call to a string builtin, expanding `StringConst` args into (ptr, len) pairs.
///
/// Returns `Ok(true)` if the call was handled, `Ok(false)` if not.
#[allow(clippy::too_many_arguments)]
fn emit_string_builtin_call(
    callee: &str,
    args: &[Value],
    dest: LocalId,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
    func_ids: &HashMap<String, FuncId>,
    struct_layouts: &HashMap<String, StructLayout>,
) -> Result<bool> {
    let mut arg_vals = Vec::new();

    // Expand each argument: StringConst -> (ptr, len),
    // String local (stack slot) -> load (ptr, len), others -> single value.
    for arg in args {
        expand_builtin_arg(
            arg,
            &mut arg_vals,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
        )?;
    }

    // list_get and map_get use out-parameters: (out_value, out_is_some).
    if callee == "list_get" || callee == "map_get" || callee == "map_get_sk" {
        return emit_list_map_get(callee, &arg_vals, dest, builder, module, builtins, var_map);
    }

    // map_get_sv and map_get_ss return String via (out_ptr, out_len, out_is_some).
    if callee == "map_get_sv" || callee == "map_get_ss" {
        return emit_map_get_string_value(
            callee, &arg_vals, dest, builder, module, builtins, var_map,
        );
    }

    // For builtins that return a String via out-parameters.
    if is_string_returning_builtin(callee) {
        return emit_string_returning_call(
            callee, &arg_vals, dest, builder, module, builtins, var_map,
        );
    }

    // file_read, file_write, http_get, http_post, and file_append return
    // Result<String, String> or Result<Unit, String> via out-parameters.
    if callee == "file_read"
        || callee == "file_write"
        || callee == "http_get"
        || callee == "http_post"
        || callee == "file_append"
    {
        return emit_file_io_call(callee, &arg_vals, dest, builder, module, builtins, var_map);
    }

    // Widen I8 args (Bool) to I64 to match runtime function signatures.
    for val in &mut arg_vals {
        if builder.func.dfg.value_type(*val) == types::I8 {
            *val = builder.ins().uextend(types::I64, *val);
        }
    }

    let builtin = builtins
        .get(callee)
        .ok_or_else(|| CodegenError::Unsupported(format!("builtin {callee}")))?;
    let func_ref = module.declare_func_in_func(builtin.func_id, builder.func);
    let call = builder.ins().call(func_ref, &arg_vals);

    let results = builder.inst_results(call);
    if results.is_empty() {
        let zero = builder.ins().iconst(types::I64, 0);
        var_map.def_var_with_cast(dest, zero, builder)?;
    } else {
        var_map.def_var_with_cast(dest, results[0], builder)?;
    }

    Ok(true)
}

/// Expands a single argument for a string builtin call.
///
/// `StringConst` values and `Local` values with `_String` stack slots are
/// expanded to `(ptr, len)` pairs. `BinOp(Add, ...)` with String operands
/// (from f-string interpolation or chained concat) are computed into a temp
/// stack slot and then expanded.
#[allow(clippy::too_many_arguments)]
fn expand_builtin_arg(
    arg: &Value,
    arg_vals: &mut Vec<cranelift_codegen::ir::Value>,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
) -> Result<()> {
    if let Value::StringConst(s) = arg {
        let data_id = create_string_data(module, s)?;
        let gv = module.declare_data_in_func(data_id, builder.func);
        let ptr = builder.ins().symbol_value(types::I64, gv);
        #[allow(clippy::cast_possible_wrap)]
        let len = builder.ins().iconst(types::I64, s.len() as i64);
        arg_vals.push(ptr);
        arg_vals.push(len);
    } else if let Value::BinOp(kodo_ast::BinOp::Add, lhs, rhs) = arg {
        // Check if this is a String concat (e.g. f-string interpolation result).
        let lhs_ty = infer_value_type(lhs, var_map);
        let rhs_ty = infer_value_type(rhs, var_map);
        if lhs_ty == Some(Type::String) || rhs_ty == Some(Type::String) {
            let (ptr, len) =
                expand_string_value_with_builtins(arg, builder, module, var_map, Some(builtins))?;
            arg_vals.push(ptr);
            arg_vals.push(len);
        } else {
            arg_vals.push(translate_value(
                arg,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            )?);
        }
    } else if let Value::Local(local_id) = arg {
        if let Some((slot, ref slot_name)) = var_map.stack_slots.get(local_id) {
            if slot_name == "_String" {
                // Load ptr and len from the String stack slot.
                let ptr_addr = builder
                    .ins()
                    .stack_addr(types::I64, *slot, STRING_PTR_OFFSET);
                let ptr = builder.ins().load(types::I64, MemFlags::new(), ptr_addr, 0);
                let len_addr = builder
                    .ins()
                    .stack_addr(types::I64, *slot, STRING_LEN_OFFSET);
                let len = builder.ins().load(types::I64, MemFlags::new(), len_addr, 0);
                arg_vals.push(ptr);
                arg_vals.push(len);
            } else {
                // Non-String composite: pass as single value.
                arg_vals.push(translate_value(
                    arg,
                    builder,
                    module,
                    func_ids,
                    builtins,
                    var_map,
                    struct_layouts,
                )?);
            }
        } else {
            arg_vals.push(translate_value(
                arg,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            )?);
        }
    } else {
        arg_vals.push(translate_value(
            arg,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
        )?);
    }
    Ok(())
}

/// Emits a `list_get` or `map_get` call with out-parameters.
fn emit_list_map_get(
    callee: &str,
    arg_vals: &[cranelift_codegen::ir::Value],
    dest: LocalId,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
) -> Result<bool> {
    let out_slot = builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
        cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
        16, // 8 bytes for value + 8 bytes for is_some
        0,
    ));
    let out_value_addr = builder.ins().stack_addr(types::I64, out_slot, 0);
    let out_is_some_addr = builder.ins().stack_addr(types::I64, out_slot, 8);
    let mut all_args = arg_vals.to_vec();
    all_args.push(out_value_addr);
    all_args.push(out_is_some_addr);

    let builtin = builtins
        .get(callee)
        .ok_or_else(|| CodegenError::Unsupported(format!("builtin {callee}")))?;
    let func_ref = module.declare_func_in_func(builtin.func_id, builder.func);
    builder.ins().call(func_ref, &all_args);

    // Load the raw value from the out-parameter.
    let result_val = builder
        .ins()
        .load(types::I64, MemFlags::new(), out_value_addr, 0);

    // If the destination has a _String stack slot, the retrieved value is a
    // pointer to a KodoString struct (ptr + len at offsets 0 and 8).
    // We need to copy the string data into the destination stack slot so
    // that downstream consumers (e.g. println) can read (ptr, len) from it.
    if let Some((dest_slot, ref dest_name)) = var_map.stack_slots.get(&dest) {
        if dest_name == "_String" {
            let str_ptr = builder
                .ins()
                .load(types::I64, MemFlags::new(), result_val, 0);
            let str_len =
                builder
                    .ins()
                    .load(types::I64, MemFlags::new(), result_val, STRING_LEN_OFFSET);
            let dest_ptr_addr = builder
                .ins()
                .stack_addr(types::I64, *dest_slot, STRING_PTR_OFFSET);
            builder
                .ins()
                .store(MemFlags::new(), str_ptr, dest_ptr_addr, 0);
            let dest_len_addr = builder
                .ins()
                .stack_addr(types::I64, *dest_slot, STRING_LEN_OFFSET);
            builder
                .ins()
                .store(MemFlags::new(), str_len, dest_len_addr, 0);
            let var = var_map.get(dest)?;
            let addr = builder.ins().stack_addr(types::I64, *dest_slot, 0);
            builder.def_var(var, addr);
            return Ok(true);
        }
    }

    // Default: store the raw i64 value as a scalar.
    let var = var_map.get(dest)?;
    builder.def_var(var, result_val);
    Ok(true)
}

/// Emits a `map_get_sv` or `map_get_ss` call that returns a String via
/// out-parameters `(out_ptr, out_len, out_is_some)`.
fn emit_map_get_string_value(
    callee: &str,
    arg_vals: &[cranelift_codegen::ir::Value],
    dest: LocalId,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
) -> Result<bool> {
    // Allocate 24 bytes: 8 for ptr, 8 for len, 8 for is_some.
    let out_slot = builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
        cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
        24,
        0,
    ));
    let out_ptr_addr = builder.ins().stack_addr(types::I64, out_slot, 0);
    let out_len_addr = builder.ins().stack_addr(types::I64, out_slot, 8);
    let out_is_some_addr = builder.ins().stack_addr(types::I64, out_slot, 16);
    let mut all_args = arg_vals.to_vec();
    all_args.push(out_ptr_addr);
    all_args.push(out_len_addr);
    all_args.push(out_is_some_addr);

    let builtin = builtins
        .get(callee)
        .ok_or_else(|| CodegenError::Unsupported(format!("builtin {callee}")))?;
    let func_ref = module.declare_func_in_func(builtin.func_id, builder.func);
    builder.ins().call(func_ref, &all_args);

    // Store the String result (ptr, len) into the destination stack slot.
    if let Some((dest_slot, ref dest_name)) = var_map.stack_slots.get(&dest) {
        if dest_name == "_String" {
            let result_ptr = builder
                .ins()
                .load(types::I64, MemFlags::new(), out_ptr_addr, 0);
            let result_len = builder
                .ins()
                .load(types::I64, MemFlags::new(), out_len_addr, 0);
            let dest_ptr_addr = builder
                .ins()
                .stack_addr(types::I64, *dest_slot, STRING_PTR_OFFSET);
            builder
                .ins()
                .store(MemFlags::new(), result_ptr, dest_ptr_addr, 0);
            let dest_len_addr = builder
                .ins()
                .stack_addr(types::I64, *dest_slot, STRING_LEN_OFFSET);
            builder
                .ins()
                .store(MemFlags::new(), result_len, dest_len_addr, 0);
            let var = var_map.get(dest)?;
            let addr = builder.ins().stack_addr(types::I64, *dest_slot, 0);
            builder.def_var(var, addr);
            return Ok(true);
        }
    }
    // Fallback: store pointer as scalar.
    let result_ptr = builder
        .ins()
        .load(types::I64, MemFlags::new(), out_ptr_addr, 0);
    let var = var_map.get(dest)?;
    builder.def_var(var, result_ptr);
    Ok(true)
}

/// Emits a string-returning builtin call with out-parameters.
fn emit_string_returning_call(
    callee: &str,
    arg_vals: &[cranelift_codegen::ir::Value],
    dest: LocalId,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
) -> Result<bool> {
    let out_slot = builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
        cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
        16, // 8 bytes for ptr + 8 bytes for len
        0,
    ));
    let out_ptr_addr = builder.ins().stack_addr(types::I64, out_slot, 0);
    let out_len_addr = builder.ins().stack_addr(types::I64, out_slot, 8);
    // Widen I8 args (Bool) to I64 to match runtime function signatures.
    let mut all_args: Vec<cranelift_codegen::ir::Value> = arg_vals
        .iter()
        .map(|val| {
            if builder.func.dfg.value_type(*val) == types::I8 {
                builder.ins().uextend(types::I64, *val)
            } else {
                *val
            }
        })
        .collect();
    all_args.push(out_ptr_addr);
    all_args.push(out_len_addr);

    let builtin = builtins
        .get(callee)
        .ok_or_else(|| CodegenError::Unsupported(format!("builtin {callee}")))?;
    let func_ref = module.declare_func_in_func(builtin.func_id, builder.func);
    builder.ins().call(func_ref, &all_args);

    // If the dest has a String stack slot, store both ptr and len into it.
    if let Some((dest_slot, ref dest_name)) = var_map.stack_slots.get(&dest) {
        if dest_name == "_String" {
            let result_ptr = builder
                .ins()
                .load(types::I64, MemFlags::new(), out_ptr_addr, 0);
            let result_len = builder
                .ins()
                .load(types::I64, MemFlags::new(), out_len_addr, 0);
            let dest_ptr_addr = builder
                .ins()
                .stack_addr(types::I64, *dest_slot, STRING_PTR_OFFSET);
            builder
                .ins()
                .store(MemFlags::new(), result_ptr, dest_ptr_addr, 0);
            let dest_len_addr = builder
                .ins()
                .stack_addr(types::I64, *dest_slot, STRING_LEN_OFFSET);
            builder
                .ins()
                .store(MemFlags::new(), result_len, dest_len_addr, 0);
            let var = var_map.get(dest)?;
            let addr = builder.ins().stack_addr(types::I64, *dest_slot, 0);
            builder.def_var(var, addr);
            return Ok(true);
        }
    }
    // Fallback: store only the pointer as scalar.
    let result_ptr = builder
        .ins()
        .load(types::I64, MemFlags::new(), out_ptr_addr, 0);
    let var = var_map.get(dest)?;
    builder.def_var(var, result_ptr);
    Ok(true)
}

/// Emits a `file_read` or `file_write` call with Result enum out-parameters.
fn emit_file_io_call(
    callee: &str,
    arg_vals: &[cranelift_codegen::ir::Value],
    dest: LocalId,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
) -> Result<bool> {
    // Allocate out-parameters for the result string (ptr, len).
    let out_slot = builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
        cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
        16, // 8 bytes for ptr + 8 bytes for len
        0,
    ));
    let out_ptr_addr = builder.ins().stack_addr(types::I64, out_slot, 0);
    let out_len_addr = builder.ins().stack_addr(types::I64, out_slot, 8);
    let mut all_args = arg_vals.to_vec();
    all_args.push(out_ptr_addr);
    all_args.push(out_len_addr);

    let builtin = builtins
        .get(callee)
        .ok_or_else(|| CodegenError::Unsupported(format!("builtin {callee}")))?;
    let func_ref = module.declare_func_in_func(builtin.func_id, builder.func);
    let call = builder.ins().call(func_ref, &all_args);
    let discriminant = builder.inst_results(call)[0]; // 0=Ok, 1=Err

    // Store the Result enum into the destination stack slot.
    // Layout: [discriminant: i64] [payload: i64 (pointer to KodoString-like (ptr, len))]
    // The payload stores a pointer to the out_slot which contains (ptr, len),
    // so that enum payload extraction can uniformly dereference it.
    if let Some((dest_slot, _)) = var_map.stack_slots.get(&dest) {
        let dest_addr = builder.ins().stack_addr(types::I64, *dest_slot, 0);
        // Store discriminant.
        builder
            .ins()
            .store(MemFlags::new(), discriminant, dest_addr, 0);
        // Store pointer to the (ptr, len) pair as payload.
        let kodo_str_addr = builder.ins().stack_addr(types::I64, out_slot, 0);
        builder
            .ins()
            .store(MemFlags::new(), kodo_str_addr, dest_addr, 8);
        let var = var_map.get(dest)?;
        builder.def_var(var, dest_addr);
    } else {
        // Fallback: store discriminant as scalar.
        var_map.def_var_with_cast(dest, discriminant, builder)?;
    }
    Ok(true)
}

/// Translates `MakeDynTrait`: constructs a fat pointer `(data_ptr, vtable_ptr)` in a stack slot.
///
/// The data pointer is the address of the concrete value (a pointer to its stack slot),
/// and the vtable pointer is the address of the vtable for the `(concrete_type, trait)` pair.
#[allow(clippy::too_many_arguments)]
fn translate_make_dyn_trait(
    dest: LocalId,
    inner_value: &Value,
    concrete_type: &str,
    trait_name: &str,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &mut VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
) -> Result<()> {
    use crate::layout::{DYN_TRAIT_DATA_OFFSET, DYN_TRAIT_LAYOUT_SIZE, DYN_TRAIT_VTABLE_OFFSET};

    // Get the data pointer: if the inner value is a local with a stack slot,
    // use the stack slot address. Otherwise, translate as a scalar.
    let data_ptr = match inner_value {
        Value::Local(id) => {
            if let Some((slot, _)) = var_map.stack_slots.get(id) {
                builder.ins().stack_addr(types::I64, *slot, 0)
            } else {
                let var = var_map.get(*id)?;
                builder.use_var(var)
            }
        }
        _ => translate_value(
            inner_value,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
        )?,
    };

    // Look up or create the vtable data symbol for (concrete_type, trait_name).
    let vtable_name = format!("__vtable_{concrete_type}_{trait_name}");
    let vtable_ptr = if let Ok(vtable_data_id) =
        module.declare_data(&vtable_name, Linkage::Local, false, false)
    {
        let gv = module.declare_data_in_func(vtable_data_id, builder.func);
        builder.ins().symbol_value(types::I64, gv)
    } else {
        // Vtable not found — use a null pointer (graceful fallback).
        builder.ins().iconst(types::I64, 0)
    };

    // Create or reuse the stack slot for the fat pointer.
    if let Some((slot, _)) = var_map.stack_slots.get(&dest) {
        // Store data_ptr and vtable_ptr into the existing stack slot.
        builder
            .ins()
            .stack_store(data_ptr, *slot, DYN_TRAIT_DATA_OFFSET);
        builder
            .ins()
            .stack_store(vtable_ptr, *slot, DYN_TRAIT_VTABLE_OFFSET);
        let addr = builder.ins().stack_addr(types::I64, *slot, 0);
        let var = var_map.get(dest)?;
        builder.def_var(var, addr);
    } else {
        // Create a new stack slot for the fat pointer.
        let slot = builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
            cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
            DYN_TRAIT_LAYOUT_SIZE,
            0,
        ));
        builder
            .ins()
            .stack_store(data_ptr, slot, DYN_TRAIT_DATA_OFFSET);
        builder
            .ins()
            .stack_store(vtable_ptr, slot, DYN_TRAIT_VTABLE_OFFSET);
        var_map
            .stack_slots
            .insert(dest, (slot, "_DynTrait".to_string()));
        let addr = builder.ins().stack_addr(types::I64, slot, 0);
        let var = var_map.get(dest)?;
        builder.def_var(var, addr);
    }
    Ok(())
}

/// Translates a `VirtualCall` instruction (dynamic dispatch through vtable).
///
/// The object is a fat pointer stored in a stack slot: `[data_ptr, vtable_ptr]`.
/// We load the vtable pointer, index into it to get the function pointer,
/// then perform an indirect call with the data pointer as the first argument (self).
#[allow(clippy::too_many_arguments)]
fn translate_virtual_call(
    dest: LocalId,
    object: LocalId,
    vtable_index: u32,
    args: &[Value],
    return_type: &Type,
    param_types: &[Type],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
) -> Result<()> {
    use crate::layout::{DYN_TRAIT_DATA_OFFSET, DYN_TRAIT_VTABLE_OFFSET};

    // Load data_ptr and vtable_ptr from the fat pointer stack slot.
    let (data_ptr, vtable_ptr) = if let Some((slot, _)) = var_map.stack_slots.get(&object) {
        let data = builder
            .ins()
            .stack_load(types::I64, *slot, DYN_TRAIT_DATA_OFFSET);
        let vtable = builder
            .ins()
            .stack_load(types::I64, *slot, DYN_TRAIT_VTABLE_OFFSET);
        (data, vtable)
    } else {
        // Fallback: if the object is a scalar variable, treat it as the data pointer
        // and assume vtable is zero (this shouldn't happen in well-formed MIR).
        let var = var_map.get(object)?;
        let val = builder.use_var(var);
        let zero = builder.ins().iconst(types::I64, 0);
        (val, zero)
    };

    // Index into the vtable to get the function pointer.
    // vtable layout: [fn_ptr_0, fn_ptr_1, ...]
    // Each vtable slot is 8 bytes (one function pointer).
    // vtable_index is bounded by the number of trait methods, so this won't overflow.
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    let fn_ptr_offset = (i64::from(vtable_index) * 8) as i32;
    let fn_ptr = builder
        .ins()
        .load(types::I64, MemFlags::trusted(), vtable_ptr, fn_ptr_offset);

    // Build the signature, replicating sret logic from `build_signature`
    // in `module.rs`. When the return type is composite (struct, String,
    // enum, etc.), the callee expects an sret pointer as its first param
    // and writes the result there instead of returning it in a register.
    let has_sret = is_composite(return_type);
    let dest_is_composite = var_map.stack_slots.contains_key(&dest);
    let mut sig = Signature::new(CallConv::SystemV);

    if has_sret {
        sig.params.push(AbiParam::new(types::I64)); // sret pointer
    }

    sig.params.push(AbiParam::new(types::I64)); // self (data_ptr)
    for pt in param_types {
        if is_composite(pt) {
            sig.params.push(AbiParam::new(types::I64)); // pointer
        } else {
            sig.params.push(AbiParam::new(cranelift_type(pt)));
        }
    }
    // Only add a scalar return if the return type is not composite and not unit.
    if !has_sret && !is_unit(return_type) {
        sig.returns.push(AbiParam::new(cranelift_type(return_type)));
    }
    let sig_ref = builder.import_signature(sig);

    // Build argument list.
    let mut arg_vals = Vec::with_capacity(2 + args.len());
    // sret pointer first if needed — use the pre-allocated stack slot of dest.
    if dest_is_composite {
        if let Some((slot, _)) = var_map.stack_slots.get(&dest) {
            let sret_addr = builder.ins().stack_addr(types::I64, *slot, 0);
            arg_vals.push(sret_addr);
        }
    }
    arg_vals.push(data_ptr);
    for arg in args {
        arg_vals.push(translate_value(
            arg,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
        )?);
    }

    let call = builder.ins().call_indirect(sig_ref, fn_ptr, &arg_vals);
    define_call_result(dest, dest_is_composite, call, builder, var_map)?;
    Ok(())
}
