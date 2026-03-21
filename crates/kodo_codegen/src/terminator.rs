//! Translation of MIR terminators to Cranelift IR.
//!
//! Handles `Return`, `Goto`, `Branch`, and `Unreachable` terminators,
//! including composite (sret) returns and heap cleanup before return.

use std::collections::HashMap;

use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{InstBuilder, MemFlags};
use cranelift_frontend::{FunctionBuilder, Variable};
use cranelift_module::{FuncId, Module};
use cranelift_object::ObjectModule;
use kodo_mir::{BlockId, LocalId, MirFunction, Terminator, Value};
use kodo_types::Type;

use crate::builtins::BuiltinInfo;
use crate::function::{HeapKind, VarMap};
use crate::layout::{
    EnumLayout, StructLayout, STRING_LAYOUT_SIZE, STRING_LEN_OFFSET, STRING_PTR_OFFSET,
};
use crate::module::{cranelift_type, is_composite, is_unit};
use crate::value::{
    create_string_data, expand_string_value_with_builtins, infer_value_type, translate_value,
};
use crate::{CodegenError, Result};

/// Translates a MIR terminator.
#[allow(clippy::too_many_arguments)]
pub(crate) fn translate_terminator(
    term: &Terminator,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    block_map: &HashMap<BlockId, cranelift_codegen::ir::Block>,
    mir_fn: &MirFunction,
    var_map: &VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
    enum_layouts: &HashMap<String, EnumLayout>,
    sret_var: Option<Variable>,
) -> Result<()> {
    match term {
        Terminator::Return(value) => translate_return(
            value,
            builder,
            module,
            func_ids,
            builtins,
            mir_fn,
            var_map,
            struct_layouts,
            enum_layouts,
            sret_var,
        ),
        Terminator::Goto(target) => {
            let cl_block = block_map
                .get(target)
                .ok_or_else(|| CodegenError::Cranelift(format!("undefined block: {target}")))?;
            builder.ins().jump(*cl_block, &[]);
            Ok(())
        }
        Terminator::Branch {
            condition,
            true_block,
            false_block,
        } => {
            let cond = translate_value(
                condition,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            )?;
            let cl_true = block_map
                .get(true_block)
                .ok_or_else(|| CodegenError::Cranelift(format!("undefined block: {true_block}")))?;
            let cl_false = block_map.get(false_block).ok_or_else(|| {
                CodegenError::Cranelift(format!("undefined block: {false_block}"))
            })?;
            builder.ins().brif(cond, *cl_true, &[], *cl_false, &[]);
            Ok(())
        }
        Terminator::Unreachable => {
            builder
                .ins()
                .trap(cranelift_codegen::ir::TrapCode::STACK_OVERFLOW);
            Ok(())
        }
    }
}

/// Translates a Return terminator.
#[allow(clippy::too_many_arguments)]
fn translate_return(
    value: &Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    mir_fn: &MirFunction,
    var_map: &VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
    enum_layouts: &HashMap<String, EnumLayout>,
    sret_var: Option<Variable>,
) -> Result<()> {
    // Identify the local being returned so we skip freeing it.
    let return_local = if let Value::Local(id) = value {
        Some(*id)
    } else {
        None
    };

    if is_composite(&mir_fn.return_type) {
        translate_composite_return(
            value,
            builder,
            module,
            func_ids,
            builtins,
            mir_fn,
            var_map,
            struct_layouts,
            enum_layouts,
            sret_var,
            return_local,
        )
    } else if is_unit(&mir_fn.return_type) {
        let _ = translate_value(
            value,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
        )?;
        // Free heap-allocated locals before returning.
        emit_heap_cleanup(builder, module, builtins, var_map, return_local)?;
        builder.ins().return_(&[]);
        Ok(())
    } else {
        let val = translate_value(
            value,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
        )?;
        let expected = cranelift_type(&mir_fn.return_type);
        let actual = builder.func.dfg.value_type(val);
        let val = if actual != expected && actual.is_int() && expected.is_int() {
            if actual.bits() > expected.bits() {
                builder.ins().ireduce(expected, val)
            } else {
                builder.ins().uextend(expected, val)
            }
        } else {
            val
        };
        // Free heap-allocated locals before returning.
        emit_heap_cleanup(builder, module, builtins, var_map, return_local)?;
        builder.ins().return_(&[val]);
        Ok(())
    }
}

/// Translates a composite (sret) return.
#[allow(clippy::too_many_arguments)]
fn translate_composite_return(
    value: &Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    mir_fn: &MirFunction,
    var_map: &VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
    enum_layouts: &HashMap<String, EnumLayout>,
    sret_var: Option<Variable>,
    return_local: Option<LocalId>,
) -> Result<()> {
    // sret: copy local struct/enum data to the sret pointer, then return void.
    if let Some(sret_v) = sret_var {
        let sret_ptr = builder.use_var(sret_v);

        // For StringConst return value, store ptr+len directly into sret.
        if let Value::StringConst(s) = value {
            let data_id = create_string_data(module, s)?;
            let gv = module.declare_data_in_func(data_id, builder.func);
            let ptr = builder.ins().symbol_value(types::I64, gv);
            #[allow(clippy::cast_possible_wrap)]
            let len = builder.ins().iconst(types::I64, s.len() as i64);
            builder
                .ins()
                .store(MemFlags::new(), ptr, sret_ptr, STRING_PTR_OFFSET);
            builder
                .ins()
                .store(MemFlags::new(), len, sret_ptr, STRING_LEN_OFFSET);
        } else if let Value::BinOp(kodo_ast::BinOp::Add, lhs, rhs) = value {
            // String concatenation returned directly — materialize into sret.
            let lhs_ty = infer_value_type(lhs, var_map);
            let rhs_ty = infer_value_type(rhs, var_map);
            if lhs_ty == Some(Type::String) || rhs_ty == Some(Type::String) {
                let (ptr, len) = expand_string_value_with_builtins(
                    value,
                    builder,
                    module,
                    var_map,
                    Some(builtins),
                )?;
                builder
                    .ins()
                    .store(MemFlags::new(), ptr, sret_ptr, STRING_PTR_OFFSET);
                builder
                    .ins()
                    .store(MemFlags::new(), len, sret_ptr, STRING_LEN_OFFSET);
            } else {
                let src_addr = translate_value(
                    value,
                    builder,
                    module,
                    func_ids,
                    builtins,
                    var_map,
                    struct_layouts,
                )?;
                builder.ins().store(MemFlags::new(), src_addr, sret_ptr, 0);
            }
        } else {
            let src_addr = translate_value(
                value,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            )?;
            let slot_size = match &mir_fn.return_type {
                Type::String => STRING_LAYOUT_SIZE,
                Type::Struct(name) => struct_layouts.get(name).map_or(8, |l| l.total_size),
                Type::Enum(name) => enum_layouts.get(name).map_or(8, |l| l.total_size),
                #[allow(clippy::cast_possible_truncation)]
                Type::Tuple(elems) => 8 + (elems.len() as u32) * 8,
                Type::Generic(base, args) => {
                    let arg_strs: Vec<String> = args.iter().map(ToString::to_string).collect();
                    let mono = format!("{base}__{}", arg_strs.join("_"));
                    enum_layouts.get(&mono).map_or(8, |l| l.total_size)
                }
                _ => 8,
            };
            let num_words = slot_size.div_ceil(8);
            for w in 0..num_words {
                #[allow(clippy::cast_possible_wrap)]
                let off = (w * 8) as i32;
                let src_field = builder.ins().iadd_imm(src_addr, i64::from(off));
                let val = builder
                    .ins()
                    .load(types::I64, MemFlags::new(), src_field, 0);
                let dest_field = builder.ins().iadd_imm(sret_ptr, i64::from(off));
                builder.ins().store(MemFlags::new(), val, dest_field, 0);
            }
        }
    }
    // Free heap-allocated locals before returning.
    emit_heap_cleanup(builder, module, builtins, var_map, return_local)?;
    builder.ins().return_(&[]);
    Ok(())
}

/// Emits cleanup calls for all heap-allocated locals before a function returns.
///
/// Iterates over `var_map.heap_locals` and emits the appropriate free function
/// call for each allocation kind. Locals whose value is being returned
/// (identified by `return_local`) are skipped — the caller owns that value.
pub(crate) fn emit_heap_cleanup(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
    return_local: Option<LocalId>,
) -> Result<()> {
    // Collect into a Vec to avoid borrow issues with the builder.
    let mut locals_to_free: Vec<(LocalId, HeapKind)> = var_map
        .heap_locals
        .iter()
        .map(|(id, kind)| (*id, *kind))
        .collect();
    // Sort for deterministic codegen output.
    locals_to_free.sort_by_key(|(id, _)| id.0);

    for (local_id, kind) in locals_to_free {
        // Do not free the value being returned — ownership transfers to caller.
        if return_local == Some(local_id) {
            continue;
        }
        match kind {
            HeapKind::String => {
                emit_string_free(local_id, builder, module, builtins, var_map)?;
            }
            HeapKind::List => {
                emit_handle_free(
                    local_id,
                    "kodo_list_free",
                    builder,
                    module,
                    builtins,
                    var_map,
                )?;
            }
            HeapKind::Map => {
                emit_handle_free(
                    local_id,
                    "kodo_map_free",
                    builder,
                    module,
                    builtins,
                    var_map,
                )?;
            }
            HeapKind::Set => {
                emit_handle_free(
                    local_id,
                    "kodo_set_free",
                    builder,
                    module,
                    builtins,
                    var_map,
                )?;
            }
        }
    }
    Ok(())
}

/// Emits a `kodo_string_free` call for a heap-allocated string local.
fn emit_string_free(
    local_id: LocalId,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
) -> Result<()> {
    // Load ptr and len from the _String stack slot, then call kodo_string_free.
    if let Some((slot, ref slot_name)) = var_map.stack_slots.get(&local_id) {
        if slot_name == "_String" {
            let ptr_addr = builder
                .ins()
                .stack_addr(types::I64, *slot, STRING_PTR_OFFSET);
            let ptr = builder.ins().load(types::I64, MemFlags::new(), ptr_addr, 0);
            let len_addr = builder
                .ins()
                .stack_addr(types::I64, *slot, STRING_LEN_OFFSET);
            let len = builder.ins().load(types::I64, MemFlags::new(), len_addr, 0);
            let free_info = builtins.get("kodo_string_free").ok_or_else(|| {
                CodegenError::Unsupported("kodo_string_free builtin not found".to_string())
            })?;
            let func_ref = module.declare_func_in_func(free_info.func_id, builder.func);
            builder.ins().call(func_ref, &[ptr, len]);
        }
    }
    Ok(())
}

/// Emits a free call for a handle-based heap-allocated local (list or map).
fn emit_handle_free(
    local_id: LocalId,
    free_name: &str,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
) -> Result<()> {
    let var = var_map.get(local_id)?;
    let handle = builder.use_var(var);
    let free_info = builtins
        .get(free_name)
        .ok_or_else(|| CodegenError::Unsupported(format!("{free_name} builtin not found")))?;
    let func_ref = module.declare_func_in_func(free_info.func_id, builder.func);
    builder.ins().call(func_ref, &[handle]);
    Ok(())
}
