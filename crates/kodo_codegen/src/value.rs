//! Translation of MIR values to Cranelift IR values.
//!
//! Handles constants, locals, binary/unary operations, struct/enum access,
//! function references, and string comparison dispatch.

use std::collections::HashMap;

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{InstBuilder, MemFlags};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{DataDescription, FuncId, Linkage, Module};
use cranelift_object::ObjectModule;
use kodo_ast::BinOp;
use kodo_mir::Value;
use kodo_types::Type;

use crate::builtins::BuiltinInfo;
use crate::function::VarMap;
use crate::layout::{StructLayout, STRING_LEN_OFFSET, STRING_PTR_OFFSET};
use crate::{CodegenError, Result};

/// Creates a read-only data section for a string literal.
pub(crate) fn create_string_data(
    module: &mut ObjectModule,
    s: &str,
) -> Result<cranelift_module::DataId> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let name = format!(".str.{id}");

    let data_id = module
        .declare_data(&name, Linkage::Local, false, false)
        .map_err(|e| CodegenError::ModuleError(e.to_string()))?;

    let mut desc = DataDescription::new();
    desc.define(s.as_bytes().to_vec().into_boxed_slice());

    module
        .define_data(data_id, &desc)
        .map_err(|e| CodegenError::ModuleError(e.to_string()))?;

    Ok(data_id)
}

/// Infers the Kōdo source type of a MIR [`Value`], if possible.
///
/// Used to detect String operands in binary operations so we can
/// dispatch to `kodo_string_eq` instead of a raw pointer comparison.
pub(crate) fn infer_value_type(value: &Value, var_map: &VarMap) -> Option<Type> {
    match value {
        Value::StringConst(_) => Some(Type::String),
        Value::IntConst(_) => Some(Type::Int),
        Value::FloatConst(_) => Some(Type::Float64),
        Value::BoolConst(_) => Some(Type::Bool),
        Value::Local(id) => {
            if let Some((_, ref tag)) = var_map.stack_slots.get(id) {
                if tag == "_String" {
                    return Some(Type::String);
                }
            }
            if let Some(&cl_ty) = var_map.var_types.get(id) {
                if cl_ty == types::F64 {
                    return Some(Type::Float64);
                }
            }
            None
        }
        _ => None,
    }
}

/// Expands a MIR [`Value`] known to be a String into a `(ptr, len)` pair of Cranelift values.
pub(crate) fn expand_string_value(
    value: &Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    var_map: &VarMap,
) -> Result<(cranelift_codegen::ir::Value, cranelift_codegen::ir::Value)> {
    match value {
        Value::StringConst(s) => {
            let data_id = create_string_data(module, s)?;
            let gv = module.declare_data_in_func(data_id, builder.func);
            let ptr = builder.ins().symbol_value(types::I64, gv);
            #[allow(clippy::cast_possible_wrap)]
            let len = builder.ins().iconst(types::I64, s.len() as i64);
            Ok((ptr, len))
        }
        Value::Local(id) => {
            if let Some((slot, ref tag)) = var_map.stack_slots.get(id) {
                if tag == "_String" {
                    let ptr = builder
                        .ins()
                        .stack_load(types::I64, *slot, STRING_PTR_OFFSET);
                    let len = builder
                        .ins()
                        .stack_load(types::I64, *slot, STRING_LEN_OFFSET);
                    return Ok((ptr, len));
                }
            }
            // Fallback: treat the value as a pointer with unknown length
            let var = var_map.get(*id)?;
            let ptr = builder.use_var(var);
            let len = builder.ins().iconst(types::I64, 0);
            Ok((ptr, len))
        }
        _ => Err(CodegenError::Unsupported(
            "cannot expand non-string value as string".to_string(),
        )),
    }
}

/// Widens or narrows boolean operands so they share the same Cranelift type.
pub(crate) fn normalize_bool_operands(
    left: cranelift_codegen::ir::Value,
    right: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    let lt = builder.func.dfg.value_type(left);
    let rt = builder.func.dfg.value_type(right);
    if lt == rt {
        (left, right)
    } else if lt.bits() < rt.bits() {
        (builder.ins().uextend(rt, left), right)
    } else {
        (left, builder.ins().uextend(lt, right))
    }
}

/// Translates a MIR [`Value`] to a Cranelift IR value.
///
/// The `func_ids` and `builtins` parameters are passed through for recursive
/// calls on compound values (`BinOp`, `Not`, `Neg`).
#[allow(clippy::only_used_in_recursion, clippy::too_many_arguments)]
pub(crate) fn translate_value(
    value: &Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
) -> Result<cranelift_codegen::ir::Value> {
    match value {
        Value::IntConst(n) => Ok(builder.ins().iconst(types::I64, *n)),
        Value::FloatConst(f) => Ok(builder.ins().f64const(*f)),
        Value::BoolConst(b) => Ok(builder.ins().iconst(types::I8, i64::from(*b))),
        Value::StringConst(s) => {
            let data_id = create_string_data(module, s)?;
            let gv = module.declare_data_in_func(data_id, builder.func);
            Ok(builder.ins().symbol_value(types::I64, gv))
        }
        Value::Local(local_id) => {
            let var = var_map.get(*local_id)?;
            Ok(builder.use_var(var))
        }
        Value::BinOp(op, lhs, rhs) => translate_binop_value(
            *op,
            lhs,
            rhs,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
        ),
        Value::Not(inner) => translate_not_value(
            inner,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
        ),
        Value::Neg(inner) => translate_neg_value(
            inner,
            builder,
            module,
            func_ids,
            builtins,
            var_map,
            struct_layouts,
        ),
        Value::StructLit { .. } | Value::FieldGet { .. } | Value::EnumVariant { .. } => {
            // Struct/enum construction handled at the instruction level.
            Ok(builder.ins().iconst(types::I64, 0))
        }
        Value::EnumDiscriminant(inner) => {
            let addr = resolve_enum_addr(
                inner,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            )?;
            Ok(builder.ins().load(types::I64, MemFlags::new(), addr, 0))
        }
        Value::EnumPayload {
            value: inner,
            field_index,
        } => {
            let addr = resolve_enum_addr(
                inner,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            )?;
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            let field_offset = (8 + (*field_index as usize) * 8) as i32;
            let field_addr = builder.ins().iadd_imm(addr, i64::from(field_offset));
            Ok(builder
                .ins()
                .load(types::I64, MemFlags::new(), field_addr, 0))
        }
        Value::Unit => Ok(builder.ins().iconst(types::I64, 0)),
        Value::FuncRef(name) => {
            // Resolve function pointer: look up in user functions, then builtins.
            if let Some(&fid) = func_ids.get(name.as_str()) {
                let fref = module.declare_func_in_func(fid, builder.func);
                Ok(builder.ins().func_addr(types::I64, fref))
            } else if let Some(bi) = builtins.get(name.as_str()) {
                let fref = module.declare_func_in_func(bi.func_id, builder.func);
                Ok(builder.ins().func_addr(types::I64, fref))
            } else {
                Err(CodegenError::Unsupported(format!(
                    "function reference to unknown function: {name}"
                )))
            }
        }
        Value::MakeDynTrait { .. } => {
            // MakeDynTrait is handled at the instruction level (Assign)
            // where we have access to the stack slot for the fat pointer.
            // If we get here, return a placeholder zero.
            Ok(builder.ins().iconst(types::I64, 0))
        }
    }
}

/// Resolves the base address for an enum value (discriminant or payload extraction).
fn resolve_enum_addr(
    inner: &Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
) -> Result<cranelift_codegen::ir::Value> {
    match inner {
        Value::Local(obj_id) => {
            if let Some((slot, _)) = var_map.stack_slots.get(obj_id) {
                Ok(builder.ins().stack_addr(types::I64, *slot, 0))
            } else {
                let var = var_map.get(*obj_id)?;
                Ok(builder.use_var(var))
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
        ),
    }
}

/// Translates a logical NOT value by XOR-ing with 1.
#[allow(clippy::too_many_arguments)]
fn translate_not_value(
    inner: &Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
) -> Result<cranelift_codegen::ir::Value> {
    let val = translate_value(
        inner,
        builder,
        module,
        func_ids,
        builtins,
        var_map,
        struct_layouts,
    )?;
    let one = builder.ins().iconst(types::I8, 1);
    Ok(builder.ins().bxor(val, one))
}

/// Translates an arithmetic negation value (integer or float).
#[allow(clippy::too_many_arguments)]
fn translate_neg_value(
    inner: &Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
) -> Result<cranelift_codegen::ir::Value> {
    let val = translate_value(
        inner,
        builder,
        module,
        func_ids,
        builtins,
        var_map,
        struct_layouts,
    )?;
    let val_ty = builder.func.dfg.value_type(val);
    if val_ty == types::F64 || val_ty == types::F32 {
        Ok(builder.ins().fneg(val))
    } else {
        Ok(builder.ins().ineg(val))
    }
}

/// Translates a binary operation value, dispatching to string/float/int as needed.
#[allow(clippy::too_many_arguments)]
fn translate_binop_value(
    op: BinOp,
    lhs: &Value,
    rhs: &Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
) -> Result<cranelift_codegen::ir::Value> {
    // Check if both operands are strings for Eq/Ne comparison.
    if matches!(op, BinOp::Eq | BinOp::Ne) {
        let lhs_ty = infer_value_type(lhs, var_map);
        let rhs_ty = infer_value_type(rhs, var_map);
        if lhs_ty == Some(Type::String) || rhs_ty == Some(Type::String) {
            return translate_string_comparison(op, lhs, rhs, builder, module, builtins, var_map);
        }
    }
    // Check if operands are Float64 — use floating-point instructions.
    {
        let lhs_ty = infer_value_type(lhs, var_map);
        let rhs_ty = infer_value_type(rhs, var_map);
        if lhs_ty == Some(Type::Float64) || rhs_ty == Some(Type::Float64) {
            let left = translate_value(
                lhs,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            )?;
            let right = translate_value(
                rhs,
                builder,
                module,
                func_ids,
                builtins,
                var_map,
                struct_layouts,
            )?;
            return Ok(translate_float_binop(op, left, right, builder));
        }
    }
    let left = translate_value(
        lhs,
        builder,
        module,
        func_ids,
        builtins,
        var_map,
        struct_layouts,
    )?;
    let right = translate_value(
        rhs,
        builder,
        module,
        func_ids,
        builtins,
        var_map,
        struct_layouts,
    )?;
    Ok(translate_int_binop(op, left, right, builder))
}

/// Emits a string comparison by calling `kodo_string_eq` with (ptr, len) pairs.
///
/// For `BinOp::Eq`, returns the result directly.
/// For `BinOp::Ne`, inverts the result.
fn translate_string_comparison(
    op: BinOp,
    lhs: &Value,
    rhs: &Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    builtins: &HashMap<String, BuiltinInfo>,
    var_map: &VarMap,
) -> Result<cranelift_codegen::ir::Value> {
    // Expand LHS to (ptr, len)
    let (lhs_ptr, lhs_len) = expand_string_value(lhs, builder, module, var_map)?;
    // Expand RHS to (ptr, len)
    let (rhs_ptr, rhs_len) = expand_string_value(rhs, builder, module, var_map)?;

    // Call kodo_string_eq(ptr1, len1, ptr2, len2) -> i64
    let eq_info = builtins
        .get("String_eq")
        .ok_or_else(|| CodegenError::Unsupported("String_eq builtin not found".to_string()))?;
    let func_ref = module.declare_func_in_func(eq_info.func_id, builder.func);
    let call = builder
        .ins()
        .call(func_ref, &[lhs_ptr, lhs_len, rhs_ptr, rhs_len]);
    let result = builder.inst_results(call)[0];

    match op {
        BinOp::Ne => {
            // Invert: result XOR 1
            let one = builder.ins().iconst(types::I64, 1);
            Ok(builder.ins().bxor(result, one))
        }
        _ => Ok(result),
    }
}

/// Translates an integer binary operation to Cranelift IR.
fn translate_int_binop(
    op: BinOp,
    left: cranelift_codegen::ir::Value,
    right: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
) -> cranelift_codegen::ir::Value {
    match op {
        BinOp::Add => builder.ins().iadd(left, right),
        BinOp::Sub => builder.ins().isub(left, right),
        BinOp::Mul => builder.ins().imul(left, right),
        BinOp::Div => builder.ins().sdiv(left, right),
        BinOp::Mod => builder.ins().srem(left, right),
        BinOp::Eq => {
            let cmp = builder.ins().icmp(IntCC::Equal, left, right);
            builder.ins().uextend(types::I64, cmp)
        }
        BinOp::Ne => {
            let cmp = builder.ins().icmp(IntCC::NotEqual, left, right);
            builder.ins().uextend(types::I64, cmp)
        }
        BinOp::Lt => {
            let cmp = builder.ins().icmp(IntCC::SignedLessThan, left, right);
            builder.ins().uextend(types::I64, cmp)
        }
        BinOp::Gt => {
            let cmp = builder.ins().icmp(IntCC::SignedGreaterThan, left, right);
            builder.ins().uextend(types::I64, cmp)
        }
        BinOp::Le => {
            let cmp = builder
                .ins()
                .icmp(IntCC::SignedLessThanOrEqual, left, right);
            builder.ins().uextend(types::I64, cmp)
        }
        BinOp::Ge => {
            let cmp = builder
                .ins()
                .icmp(IntCC::SignedGreaterThanOrEqual, left, right);
            builder.ins().uextend(types::I64, cmp)
        }
        BinOp::And => {
            let (l, r) = normalize_bool_operands(left, right, builder);
            builder.ins().band(l, r)
        }
        BinOp::Or => {
            let (l, r) = normalize_bool_operands(left, right, builder);
            builder.ins().bor(l, r)
        }
    }
}

/// Translates a floating-point binary operation to Cranelift IR.
fn translate_float_binop(
    op: BinOp,
    left: cranelift_codegen::ir::Value,
    right: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
) -> cranelift_codegen::ir::Value {
    match op {
        BinOp::Add => builder.ins().fadd(left, right),
        BinOp::Sub => builder.ins().fsub(left, right),
        BinOp::Mul => builder.ins().fmul(left, right),
        BinOp::Div => builder.ins().fdiv(left, right),
        BinOp::Mod => {
            let div = builder.ins().fdiv(left, right);
            let floored = builder.ins().floor(div);
            let product = builder.ins().fmul(floored, right);
            builder.ins().fsub(left, product)
        }
        BinOp::Eq => {
            let cmp = builder.ins().fcmp(FloatCC::Equal, left, right);
            builder.ins().uextend(types::I64, cmp)
        }
        BinOp::Ne => {
            let cmp = builder.ins().fcmp(FloatCC::NotEqual, left, right);
            builder.ins().uextend(types::I64, cmp)
        }
        BinOp::Lt => {
            let cmp = builder.ins().fcmp(FloatCC::LessThan, left, right);
            builder.ins().uextend(types::I64, cmp)
        }
        BinOp::Gt => {
            let cmp = builder.ins().fcmp(FloatCC::GreaterThan, left, right);
            builder.ins().uextend(types::I64, cmp)
        }
        BinOp::Le => {
            let cmp = builder.ins().fcmp(FloatCC::LessThanOrEqual, left, right);
            builder.ins().uextend(types::I64, cmp)
        }
        BinOp::Ge => {
            let cmp = builder.ins().fcmp(FloatCC::GreaterThanOrEqual, left, right);
            builder.ins().uextend(types::I64, cmp)
        }
        BinOp::And | BinOp::Or => builder.ins().f64const(0.0),
    }
}
