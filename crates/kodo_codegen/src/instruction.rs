//! Translation of MIR instructions to Cranelift IR.
//!
//! Handles `Assign`, `Call`, `IncRef`, `DecRef`, and `IndirectCall`
//! instructions, including special-cased string builtin expansion.

use std::collections::HashMap;

use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{AbiParam, InstBuilder, MemFlags, Signature};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{FuncId, Module};
use cranelift_object::ObjectModule;
use kodo_mir::{Instruction, LocalId, Value};
use kodo_types::Type;

use crate::builtins::BuiltinInfo;
use crate::function::{HeapKind, VarMap};
use crate::layout::{EnumLayout, StructLayout, STRING_LEN_OFFSET, STRING_PTR_OFFSET};
use crate::module::{cranelift_type, is_unit};
use crate::value::{create_string_data, expand_string_value, infer_value_type, translate_value};
use crate::{CodegenError, Result};

/// Returns true if the callee is a builtin that needs special handling
/// (string arg expansion, out-parameter returns, etc.).
pub(crate) fn is_special_builtin(callee: &str) -> bool {
    matches!(
        callee,
        "println"
            | "print"
            | "kodo_contract_fail"
            | "String_length"
            | "String_contains"
            | "String_starts_with"
            | "String_ends_with"
            | "String_trim"
            | "String_to_upper"
            | "String_to_lower"
            | "String_substring"
            | "String_split"
            | "String_concat"
            | "String_index_of"
            | "String_replace"
            | "Int_to_string"
            | "Float64_to_string"
            | "file_exists"
            | "file_read"
            | "file_write"
            | "list_get"
            | "map_get"
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
    )
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
            | "Int_to_string"
            | "Float64_to_string"
            | "http_get"
            | "http_post"
            | "json_get_string"
            | "time_format"
            | "env_get"
            | "channel_recv_string"
    )
}

/// Returns true if the builtin allocates a new list on the heap.
pub(crate) fn is_list_allocating_builtin(callee: &str) -> bool {
    matches!(callee, "list_new" | "String_split")
}

/// Returns true if the builtin allocates a new map on the heap.
pub(crate) fn is_map_allocating_builtin(callee: &str) -> bool {
    matches!(callee, "map_new")
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
        ),
        Instruction::IncRef(local_id) => {
            // Phase 1: call kodo_rc_inc with the handle value (no-op stub).
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
    let (lhs_ptr, lhs_len) = expand_string_value(lhs, builder, module, var_map)?;
    let (rhs_ptr, rhs_len) = expand_string_value(rhs, builder, module, var_map)?;
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
            let field_offset = (8 + idx * 8) as i32;
            let addr = builder.ins().stack_addr(types::I64, *slot, field_offset);
            builder.ins().store(MemFlags::new(), val, addr, 0);
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
#[allow(clippy::too_many_arguments)]
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
            if is_string_returning_builtin(callee) {
                var_map.heap_locals.insert(dest, HeapKind::String);
            }
            return Ok(());
        }
    }

    // Track list/map allocating builtins for cleanup before return.
    if is_list_allocating_builtin(callee) {
        var_map.heap_locals.insert(dest, HeapKind::List);
    } else if is_map_allocating_builtin(callee) {
        var_map.heap_locals.insert(dest, HeapKind::Map);
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
    #[allow(clippy::cast_possible_truncation)]
    let slot_size = (num_captures * 8) as u32;
    let slot = builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
        cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
        slot_size,
        0,
    ));
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
        let addr = builder.ins().stack_addr(types::I64, slot, offset);
        builder.ins().store(MemFlags::new(), val, addr, 0);
    }
    let base_addr = builder.ins().stack_addr(types::I64, slot, 0);
    let var = var_map.get(dest)?;
    builder.def_var(var, base_addr);
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
    // Build the signature for the indirect call.
    let mut sig = Signature::new(CallConv::SystemV);
    for pt in param_types {
        sig.params.push(AbiParam::new(cranelift_type(pt)));
    }
    if !is_unit(return_type) {
        sig.returns.push(AbiParam::new(cranelift_type(return_type)));
    }
    let sig_ref = builder.import_signature(sig);

    // Translate the function pointer value.
    let callee_val = translate_value(
        callee,
        builder,
        module,
        func_ids,
        builtins,
        var_map,
        struct_layouts,
    )?;

    // Translate arguments.
    let mut arg_vals = Vec::with_capacity(args.len());
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

    let call = builder.ins().call_indirect(sig_ref, callee_val, &arg_vals);
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
    if callee == "list_get" || callee == "map_get" {
        return emit_list_map_get(callee, &arg_vals, dest, builder, module, builtins, var_map);
    }

    // For builtins that return a String via out-parameters.
    if is_string_returning_builtin(callee) {
        return emit_string_returning_call(
            callee, &arg_vals, dest, builder, module, builtins, var_map,
        );
    }

    // file_read and file_write return Result<String, String> via out-parameters.
    if callee == "file_read" || callee == "file_write" {
        return emit_file_io_call(callee, &arg_vals, dest, builder, module, builtins, var_map);
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

    // Load the value as the result.
    let result_val = builder
        .ins()
        .load(types::I64, MemFlags::new(), out_value_addr, 0);
    let var = var_map.get(dest)?;
    builder.def_var(var, result_val);
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
    let mut all_args = arg_vals.to_vec();
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
    // Layout: [discriminant: i64] [payload: i64 (string ptr)]
    if let Some((dest_slot, _)) = var_map.stack_slots.get(&dest) {
        let dest_addr = builder.ins().stack_addr(types::I64, *dest_slot, 0);
        // Store discriminant.
        builder
            .ins()
            .store(MemFlags::new(), discriminant, dest_addr, 0);
        // Store string pointer (the out_ptr value) as payload.
        let result_ptr = builder
            .ins()
            .load(types::I64, MemFlags::new(), out_ptr_addr, 0);
        builder
            .ins()
            .store(MemFlags::new(), result_ptr, dest_addr, 8);
        let var = var_map.get(dest)?;
        builder.def_var(var, dest_addr);
    } else {
        // Fallback: store discriminant as scalar.
        var_map.def_var_with_cast(dest, discriminant, builder)?;
    }
    Ok(true)
}
