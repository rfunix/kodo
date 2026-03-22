//! Translation of MIR `Value` to inkwell LLVM values.
//!
//! Each MIR `Value` variant is translated to one or more inkwell builder
//! calls, producing a `BasicValueEnum` that can be used in instructions
//! and terminators.

#[cfg(feature = "inkwell")]
use std::collections::HashMap;

#[cfg(feature = "inkwell")]
use inkwell::builder::Builder;
#[cfg(feature = "inkwell")]
use inkwell::context::Context;
#[cfg(feature = "inkwell")]
use inkwell::module::Module;
#[cfg(feature = "inkwell")]
use inkwell::values::{BasicValueEnum, FunctionValue, IntValue, PointerValue};
#[cfg(feature = "inkwell")]
use inkwell::IntPredicate;

#[cfg(feature = "inkwell")]
use kodo_ast::BinOp;
#[cfg(feature = "inkwell")]
use kodo_mir::{LocalId, Value};
#[cfg(feature = "inkwell")]
use kodo_types::Type;

#[cfg(feature = "inkwell")]
use super::types::to_llvm_type;

/// Context passed to value translation functions to avoid excessive parameters.
#[cfg(feature = "inkwell")]
pub(crate) struct ValueCtx<'a, 'ctx> {
    /// The LLVM context.
    pub context: &'ctx Context,
    /// The LLVM module.
    pub module: &'a Module<'ctx>,
    /// The LLVM IR builder.
    pub builder: &'a Builder<'ctx>,
    /// Mapping from local IDs to their alloca stack slots.
    pub local_allocas: &'a HashMap<LocalId, PointerValue<'ctx>>,
    /// Mapping from local IDs to their Kodo types.
    pub local_types: &'a HashMap<LocalId, Type>,
    /// Mapping from function names to their LLVM function values.
    pub fn_map: &'a HashMap<String, FunctionValue<'ctx>>,
    /// Struct type definitions.
    pub struct_defs: &'a HashMap<String, Vec<(String, Type)>>,
    /// Enum type definitions (reserved for future use).
    #[allow(dead_code)]
    pub enum_defs: &'a HashMap<String, Vec<(String, Vec<Type>)>>,
    /// Counter for generating unique names.
    pub name_counter: &'a mut u32,
}

/// Generates a unique name for an LLVM value.
#[cfg(feature = "inkwell")]
pub(crate) fn unique_name(counter: &mut u32, prefix: &str) -> String {
    let name = format!("{prefix}{counter}");
    *counter += 1;
    name
}

/// Translates a MIR `Value` to an inkwell `BasicValueEnum`.
///
/// Returns `None` for void/unit values that have no LLVM representation.
#[cfg(feature = "inkwell")]
#[allow(clippy::too_many_lines)]
pub(crate) fn translate_value<'ctx>(
    value: &Value,
    ctx: &mut ValueCtx<'_, 'ctx>,
) -> Option<BasicValueEnum<'ctx>> {
    match value {
        Value::IntConst(n) => {
            #[allow(clippy::cast_sign_loss)]
            let val = ctx.context.i64_type().const_int(*n as u64, true);
            Some(val.into())
        }
        Value::FloatConst(f) => {
            let val = ctx.context.f64_type().const_float(*f);
            Some(val.into())
        }
        Value::BoolConst(b) => {
            let val = ctx.context.i64_type().const_int(u64::from(*b), false);
            Some(val.into())
        }
        Value::StringConst(s) => Some(translate_string_const(s, ctx)),
        Value::Local(id) => {
            let alloca = ctx.local_allocas.get(id)?;
            let ty = ctx.local_types.get(id).unwrap_or(&Type::Int);
            if super::types::is_void(ty) {
                return None;
            }
            let llvm_ty = to_llvm_type(ctx.context, ty);
            let name = unique_name(ctx.name_counter, "load");
            let val = ctx.builder.build_load(llvm_ty, *alloca, &name).unwrap();
            Some(val)
        }
        Value::BinOp(op, lhs, rhs) => translate_binop(*op, lhs, rhs, ctx),
        Value::Not(inner) => {
            let inner_val = translate_value(inner, ctx)?;
            let int_val = inner_val.into_int_value();
            let one = ctx.context.i64_type().const_int(1, false);
            let name = unique_name(ctx.name_counter, "not");
            let result = ctx.builder.build_xor(int_val, one, &name).unwrap();
            Some(result.into())
        }
        Value::Neg(inner) => {
            let inner_ty = infer_value_type_simple(inner, ctx.local_types);
            let inner_val = translate_value(inner, ctx)?;
            if matches!(inner_ty, Type::Float64 | Type::Float32) {
                let fval = inner_val.into_float_value();
                let name = unique_name(ctx.name_counter, "fneg");
                let result = ctx.builder.build_float_neg(fval, &name).unwrap();
                Some(result.into())
            } else {
                let ival = inner_val.into_int_value();
                let name = unique_name(ctx.name_counter, "neg");
                let result = ctx.builder.build_int_neg(ival, &name).unwrap();
                Some(result.into())
            }
        }
        Value::Unit => None,
        Value::FuncRef(name) => {
            if let Some(fn_val) = ctx.fn_map.get(name.as_str()).or_else(|| {
                ctx.module
                    .get_function(name)
                    .as_ref()
                    .and_then(|_| ctx.fn_map.get(name.as_str()))
            }) {
                let ptr = fn_val.as_global_value().as_pointer_value();
                let uname = unique_name(ctx.name_counter, "funcref");
                let result = ctx
                    .builder
                    .build_ptr_to_int(ptr, ctx.context.i64_type(), &uname)
                    .unwrap();
                Some(result.into())
            } else if let Some(fn_val) = ctx.module.get_function(name) {
                let ptr = fn_val.as_global_value().as_pointer_value();
                let uname = unique_name(ctx.name_counter, "funcref");
                let result = ctx
                    .builder
                    .build_ptr_to_int(ptr, ctx.context.i64_type(), &uname)
                    .unwrap();
                Some(result.into())
            } else {
                // Unknown function — return 0.
                Some(ctx.context.i64_type().const_int(0, false).into())
            }
        }
        Value::StructLit { name, fields } => translate_struct_lit(name, fields, ctx),
        Value::FieldGet {
            object,
            field,
            struct_name,
        } => translate_field_get(object, field, struct_name, ctx),
        Value::EnumVariant {
            discriminant, args, ..
        } => {
            // Build { i64 discriminant, i64 payload }.
            let enum_ty = ctx.context.struct_type(
                &[ctx.context.i64_type().into(), ctx.context.i64_type().into()],
                false,
            );
            let disc_val = ctx
                .context
                .i64_type()
                .const_int(u64::from(*discriminant), false);
            let uname = unique_name(ctx.name_counter, "enum");
            let mut agg = ctx
                .builder
                .build_insert_value(enum_ty.get_undef(), disc_val, 0, &uname)
                .unwrap();
            if let Some(first_arg) = args.first() {
                if let Some(arg_val) = translate_value(first_arg, ctx) {
                    let payload = to_i64_value(arg_val, ctx);
                    let uname2 = unique_name(ctx.name_counter, "enum_p");
                    agg = ctx
                        .builder
                        .build_insert_value(agg, payload, 1, &uname2)
                        .unwrap();
                }
            }
            Some(agg.into_struct_value().into())
        }
        Value::EnumDiscriminant(inner) => {
            let inner_val = translate_value(inner, ctx)?;
            let struct_val = inner_val.into_struct_value();
            let uname = unique_name(ctx.name_counter, "disc");
            let disc = ctx
                .builder
                .build_extract_value(struct_val, 0, &uname)
                .unwrap();
            Some(disc)
        }
        Value::EnumPayload { value: inner, .. } => {
            let inner_val = translate_value(inner, ctx)?;
            let struct_val = inner_val.into_struct_value();
            let uname = unique_name(ctx.name_counter, "payload");
            let payload = ctx
                .builder
                .build_extract_value(struct_val, 1, &uname)
                .unwrap();
            Some(payload)
        }
        Value::MakeDynTrait { value: inner, .. } => {
            // Simplified: pass through the inner value.
            translate_value(inner, ctx)
        }
    }
}

/// Converts a `BasicValueEnum` to an i64 `IntValue`, bitcasting if needed.
#[cfg(feature = "inkwell")]
fn to_i64_value<'ctx>(val: BasicValueEnum<'ctx>, ctx: &mut ValueCtx<'_, 'ctx>) -> IntValue<'ctx> {
    match val {
        BasicValueEnum::IntValue(iv) => iv,
        BasicValueEnum::FloatValue(fv) => {
            let uname = unique_name(ctx.name_counter, "f2i");
            ctx.builder
                .build_bit_cast(fv, ctx.context.i64_type(), &uname)
                .unwrap()
                .into_int_value()
        }
        BasicValueEnum::StructValue(sv) => {
            // Store struct to alloca, load as i64.
            let uname = unique_name(ctx.name_counter, "s2i_a");
            let alloca = ctx.builder.build_alloca(sv.get_type(), &uname).unwrap();
            ctx.builder.build_store(alloca, sv).unwrap();
            let uname2 = unique_name(ctx.name_counter, "s2i_l");
            ctx.builder
                .build_load(ctx.context.i64_type(), alloca, &uname2)
                .unwrap()
                .into_int_value()
        }
        _ => {
            // Fallback: return 0.
            ctx.context.i64_type().const_int(0, false)
        }
    }
}

/// Translates a string constant to a `{ i64, i64 }` struct (ptr, len).
#[cfg(feature = "inkwell")]
fn translate_string_const<'ctx>(s: &str, ctx: &mut ValueCtx<'_, 'ctx>) -> BasicValueEnum<'ctx> {
    let str_val = ctx.context.const_string(s.as_bytes(), false);
    let uname = unique_name(ctx.name_counter, ".str.");
    let global = ctx.module.add_global(str_val.get_type(), None, &uname);
    global.set_initializer(&str_val);
    global.set_constant(true);

    let ptr = global.as_pointer_value();
    let ptr_name = unique_name(ctx.name_counter, "str_ptr");
    let ptr_int = ctx
        .builder
        .build_ptr_to_int(ptr, ctx.context.i64_type(), &ptr_name)
        .unwrap();
    let len = ctx.context.i64_type().const_int(s.len() as u64, false);

    // Build { i64, i64 } struct.
    let str_struct_ty = ctx.context.struct_type(
        &[ctx.context.i64_type().into(), ctx.context.i64_type().into()],
        false,
    );
    let s1_name = unique_name(ctx.name_counter, "str_s1");
    let s1 = ctx
        .builder
        .build_insert_value(str_struct_ty.get_undef(), ptr_int, 0, &s1_name)
        .unwrap();
    let s2_name = unique_name(ctx.name_counter, "str_s2");
    let s2 = ctx
        .builder
        .build_insert_value(s1, len, 1, &s2_name)
        .unwrap();
    s2.into_struct_value().into()
}

/// Translates a binary operation.
#[cfg(feature = "inkwell")]
#[allow(clippy::too_many_lines)]
fn translate_binop<'ctx>(
    op: BinOp,
    lhs: &Value,
    rhs: &Value,
    ctx: &mut ValueCtx<'_, 'ctx>,
) -> Option<BasicValueEnum<'ctx>> {
    let lhs_ty = infer_value_type_simple(lhs, ctx.local_types);
    let is_float = matches!(lhs_ty, Type::Float64 | Type::Float32);
    let is_string = matches!(lhs_ty, Type::String);

    let lhs_val = translate_value(lhs, ctx)?;
    let rhs_val = translate_value(rhs, ctx)?;

    // String concatenation via runtime call.
    if is_string && matches!(op, BinOp::Add) {
        return translate_string_concat(lhs_val, rhs_val, ctx);
    }

    // String comparison via runtime call.
    if is_string && matches!(op, BinOp::Eq | BinOp::Ne) {
        return translate_string_compare(op, lhs_val, rhs_val, ctx);
    }

    if is_float {
        let l = lhs_val.into_float_value();
        let r = rhs_val.into_float_value();
        match op {
            BinOp::Add => {
                let name = unique_name(ctx.name_counter, "fadd");
                Some(ctx.builder.build_float_add(l, r, &name).unwrap().into())
            }
            BinOp::Sub => {
                let name = unique_name(ctx.name_counter, "fsub");
                Some(ctx.builder.build_float_sub(l, r, &name).unwrap().into())
            }
            BinOp::Mul => {
                let name = unique_name(ctx.name_counter, "fmul");
                Some(ctx.builder.build_float_mul(l, r, &name).unwrap().into())
            }
            BinOp::Div => {
                let name = unique_name(ctx.name_counter, "fdiv");
                Some(ctx.builder.build_float_div(l, r, &name).unwrap().into())
            }
            BinOp::Mod => {
                let name = unique_name(ctx.name_counter, "frem");
                Some(ctx.builder.build_float_rem(l, r, &name).unwrap().into())
            }
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                let pred = match op {
                    BinOp::Ne => inkwell::FloatPredicate::ONE,
                    BinOp::Lt => inkwell::FloatPredicate::OLT,
                    BinOp::Gt => inkwell::FloatPredicate::OGT,
                    BinOp::Le => inkwell::FloatPredicate::OLE,
                    BinOp::Ge => inkwell::FloatPredicate::OGE,
                    _ => inkwell::FloatPredicate::OEQ,
                };
                let cmp_name = unique_name(ctx.name_counter, "fcmp");
                let cmp = ctx
                    .builder
                    .build_float_compare(pred, l, r, &cmp_name)
                    .unwrap();
                let ext_name = unique_name(ctx.name_counter, "fext");
                let result = ctx
                    .builder
                    .build_int_z_extend(cmp, ctx.context.i64_type(), &ext_name)
                    .unwrap();
                Some(result.into())
            }
            BinOp::And | BinOp::Or => {
                // Logical ops on floats — stub as 0.
                Some(ctx.context.i64_type().const_int(0, false).into())
            }
        }
    } else {
        let l = lhs_val.into_int_value();
        let r = rhs_val.into_int_value();
        match op {
            BinOp::Add => {
                let name = unique_name(ctx.name_counter, "add");
                Some(ctx.builder.build_int_add(l, r, &name).unwrap().into())
            }
            BinOp::Sub => {
                let name = unique_name(ctx.name_counter, "sub");
                Some(ctx.builder.build_int_sub(l, r, &name).unwrap().into())
            }
            BinOp::Mul => {
                let name = unique_name(ctx.name_counter, "mul");
                Some(ctx.builder.build_int_mul(l, r, &name).unwrap().into())
            }
            BinOp::Div => {
                let name = unique_name(ctx.name_counter, "sdiv");
                Some(
                    ctx.builder
                        .build_int_signed_div(l, r, &name)
                        .unwrap()
                        .into(),
                )
            }
            BinOp::Mod => {
                let name = unique_name(ctx.name_counter, "srem");
                Some(
                    ctx.builder
                        .build_int_signed_rem(l, r, &name)
                        .unwrap()
                        .into(),
                )
            }
            BinOp::And => {
                let name = unique_name(ctx.name_counter, "and");
                Some(ctx.builder.build_and(l, r, &name).unwrap().into())
            }
            BinOp::Or => {
                let name = unique_name(ctx.name_counter, "or");
                Some(ctx.builder.build_or(l, r, &name).unwrap().into())
            }
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                let pred = match op {
                    BinOp::Ne => IntPredicate::NE,
                    BinOp::Lt => IntPredicate::SLT,
                    BinOp::Gt => IntPredicate::SGT,
                    BinOp::Le => IntPredicate::SLE,
                    BinOp::Ge => IntPredicate::SGE,
                    _ => IntPredicate::EQ,
                };
                let cmp_name = unique_name(ctx.name_counter, "icmp");
                let cmp = ctx
                    .builder
                    .build_int_compare(pred, l, r, &cmp_name)
                    .unwrap();
                let ext_name = unique_name(ctx.name_counter, "zext");
                let result = ctx
                    .builder
                    .build_int_z_extend(cmp, ctx.context.i64_type(), &ext_name)
                    .unwrap();
                Some(result.into())
            }
        }
    }
}

/// Translates a struct literal to an LLVM struct value.
#[cfg(feature = "inkwell")]
#[allow(clippy::cast_possible_truncation)]
fn translate_struct_lit<'ctx>(
    name: &str,
    fields: &[(String, Value)],
    ctx: &mut ValueCtx<'_, 'ctx>,
) -> Option<BasicValueEnum<'ctx>> {
    let field_order: Vec<String> = ctx
        .struct_defs
        .get(name)
        .map(|fs| fs.iter().map(|(n, _)| n.clone()).collect())
        .unwrap_or_default();

    if field_order.is_empty() {
        return None;
    }

    // Build the LLVM struct type from fields.
    let field_types: Vec<inkwell::types::BasicTypeEnum<'ctx>> = ctx
        .struct_defs
        .get(name)
        .map(|fs| {
            fs.iter()
                .map(|(_, t)| to_llvm_type(ctx.context, t))
                .collect()
        })
        .unwrap_or_default();
    let struct_ty = ctx.context.struct_type(&field_types, false);
    let mut current: BasicValueEnum<'ctx> = struct_ty.get_undef().into();

    for (idx, field_name) in field_order.iter().enumerate() {
        if let Some((_, val)) = fields.iter().find(|(n, _)| n == field_name) {
            if let Some(field_val) = translate_value(val, ctx) {
                let uname = unique_name(ctx.name_counter, "sf");
                let agg = ctx
                    .builder
                    .build_insert_value(current.into_struct_value(), field_val, idx as u32, &uname)
                    .unwrap();
                current = BasicValueEnum::StructValue(agg.into_struct_value());
            }
        }
    }

    Some(current)
}

/// Translates a field access on a struct value.
#[cfg(feature = "inkwell")]
#[allow(clippy::cast_possible_truncation)]
fn translate_field_get<'ctx>(
    object: &Value,
    field: &str,
    struct_name: &str,
    ctx: &mut ValueCtx<'_, 'ctx>,
) -> Option<BasicValueEnum<'ctx>> {
    let obj_val = translate_value(object, ctx)?;
    let struct_val = obj_val.into_struct_value();

    let field_idx = ctx
        .struct_defs
        .get(struct_name)
        .and_then(|fs| fs.iter().position(|(n, _)| n == field))
        .unwrap_or(0);

    let uname = unique_name(ctx.name_counter, "field");
    let result = ctx
        .builder
        .build_extract_value(struct_val, field_idx as u32, &uname)
        .unwrap();
    Some(result)
}

/// Translates string concatenation via `kodo_string_concat` runtime call.
#[cfg(feature = "inkwell")]
#[allow(clippy::unnecessary_wraps)]
fn translate_string_concat<'ctx>(
    lhs: BasicValueEnum<'ctx>,
    rhs: BasicValueEnum<'ctx>,
    ctx: &mut ValueCtx<'_, 'ctx>,
) -> Option<BasicValueEnum<'ctx>> {
    let l_struct = lhs.into_struct_value();
    let r_struct = rhs.into_struct_value();

    let l_ptr_name = unique_name(ctx.name_counter, "lp");
    let l_len_name = unique_name(ctx.name_counter, "ll");
    let r_ptr_name = unique_name(ctx.name_counter, "rp");
    let r_len_name = unique_name(ctx.name_counter, "rl");

    let l_ptr = ctx
        .builder
        .build_extract_value(l_struct, 0, &l_ptr_name)
        .unwrap();
    let l_len = ctx
        .builder
        .build_extract_value(l_struct, 1, &l_len_name)
        .unwrap();
    let r_ptr = ctx
        .builder
        .build_extract_value(r_struct, 0, &r_ptr_name)
        .unwrap();
    let r_len = ctx
        .builder
        .build_extract_value(r_struct, 1, &r_len_name)
        .unwrap();

    // Allocate out-parameter slots.
    let out_ptr_name = unique_name(ctx.name_counter, "out_p");
    let out_len_name = unique_name(ctx.name_counter, "out_l");
    let out_ptr = ctx
        .builder
        .build_alloca(ctx.context.i64_type(), &out_ptr_name)
        .unwrap();
    let out_len = ctx
        .builder
        .build_alloca(ctx.context.i64_type(), &out_len_name)
        .unwrap();

    let out_ptr_i64_name = unique_name(ctx.name_counter, "opi");
    let out_len_i64_name = unique_name(ctx.name_counter, "oli");
    let out_ptr_i64 = ctx
        .builder
        .build_ptr_to_int(out_ptr, ctx.context.i64_type(), &out_ptr_i64_name)
        .unwrap();
    let out_len_i64 = ctx
        .builder
        .build_ptr_to_int(out_len, ctx.context.i64_type(), &out_len_i64_name)
        .unwrap();

    if let Some(concat_fn) = ctx.module.get_function("kodo_string_concat") {
        ctx.builder
            .build_call(
                concat_fn,
                &[
                    l_ptr.into(),
                    l_len.into(),
                    r_ptr.into(),
                    r_len.into(),
                    out_ptr_i64.into(),
                    out_len_i64.into(),
                ],
                "concat_call",
            )
            .unwrap();
    }

    // Load results and build string struct.
    let res_ptr_name = unique_name(ctx.name_counter, "rp");
    let res_len_name = unique_name(ctx.name_counter, "rl");
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
    let s1_name = unique_name(ctx.name_counter, "cs1");
    let s1 = ctx
        .builder
        .build_insert_value(str_struct_ty.get_undef(), res_ptr, 0, &s1_name)
        .unwrap();
    let s2_name = unique_name(ctx.name_counter, "cs2");
    let s2 = ctx
        .builder
        .build_insert_value(s1, res_len, 1, &s2_name)
        .unwrap();
    Some(s2.into_struct_value().into())
}

/// Translates string comparison via `kodo_string_eq` runtime call.
#[cfg(feature = "inkwell")]
#[allow(clippy::unnecessary_wraps)]
fn translate_string_compare<'ctx>(
    op: BinOp,
    lhs: BasicValueEnum<'ctx>,
    rhs: BasicValueEnum<'ctx>,
    ctx: &mut ValueCtx<'_, 'ctx>,
) -> Option<BasicValueEnum<'ctx>> {
    let l_struct = lhs.into_struct_value();
    let r_struct = rhs.into_struct_value();

    let l_ptr_name = unique_name(ctx.name_counter, "slp");
    let l_len_name = unique_name(ctx.name_counter, "sll");
    let r_ptr_name = unique_name(ctx.name_counter, "srp");
    let r_len_name = unique_name(ctx.name_counter, "srl");

    let l_ptr = ctx
        .builder
        .build_extract_value(l_struct, 0, &l_ptr_name)
        .unwrap();
    let l_len = ctx
        .builder
        .build_extract_value(l_struct, 1, &l_len_name)
        .unwrap();
    let r_ptr = ctx
        .builder
        .build_extract_value(r_struct, 0, &r_ptr_name)
        .unwrap();
    let r_len = ctx
        .builder
        .build_extract_value(r_struct, 1, &r_len_name)
        .unwrap();

    if let Some(eq_fn) = ctx.module.get_function("kodo_string_eq") {
        let call_name = unique_name(ctx.name_counter, "seq");
        let call_site = ctx
            .builder
            .build_call(
                eq_fn,
                &[l_ptr.into(), l_len.into(), r_ptr.into(), r_len.into()],
                &call_name,
            )
            .unwrap();
        if let Some(result_val) = call_site.try_as_basic_value().basic() {
            if matches!(op, BinOp::Ne) {
                let one = ctx.context.i64_type().const_int(1, false);
                let neg_name = unique_name(ctx.name_counter, "sne");
                let int_val: inkwell::values::IntValue<'ctx> = result_val.into_int_value();
                let negated = ctx.builder.build_xor(int_val, one, &neg_name).unwrap();
                return Some(negated.into());
            }
            return Some(result_val);
        }
    }
    Some(ctx.context.i64_type().const_int(0, false).into())
}

/// Simple type inference for deciding between integer and float operations.
#[cfg(feature = "inkwell")]
pub(crate) fn infer_value_type_simple(value: &Value, local_types: &HashMap<LocalId, Type>) -> Type {
    match value {
        Value::FloatConst(_) => Type::Float64,
        Value::BoolConst(_) | Value::Not(_) => Type::Bool,
        Value::StringConst(_) => Type::String,
        Value::Local(id) => local_types.get(id).cloned().unwrap_or(Type::Int),
        Value::BinOp(_, lhs, _) => infer_value_type_simple(lhs, local_types),
        Value::Neg(inner) => infer_value_type_simple(inner, local_types),
        Value::Unit => Type::Unit,
        Value::StructLit { name, .. } => Type::Struct(name.clone()),
        Value::EnumVariant { enum_name, .. } => Type::Enum(enum_name.clone()),
        Value::IntConst(_)
        | Value::EnumDiscriminant(_)
        | Value::EnumPayload { .. }
        | Value::FuncRef(_)
        | Value::MakeDynTrait { .. }
        | Value::FieldGet { .. } => Type::Int,
    }
}
