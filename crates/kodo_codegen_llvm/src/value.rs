//! Translation of MIR `Value` to LLVM IR expressions.
//!
//! Each MIR `Value` variant is emitted as one or more LLVM IR instructions,
//! producing either an SSA register reference or an inline constant.

use std::collections::HashMap;

use kodo_ast::BinOp;
use kodo_mir::{LocalId, Value};
use kodo_types::Type;

use crate::emitter::LLVMEmitter;
use crate::function::StackLocals;
use crate::instruction::fresh_reg;
use crate::types::{enum_payload_bytes, llvm_type};

/// The result of emitting a value: either a register name, a constant literal,
/// a float constant, or void (for unit values).
pub(crate) enum ValueResult {
    /// An SSA register name (e.g., `"%3"`).
    Register(String),
    /// An integer constant literal (e.g., `"42"`).
    Constant(String),
    /// A floating-point constant literal (e.g., `"3.14"`).
    FloatConstant(String),
    /// The void/unit value — no result.
    Void,
}

/// Emits LLVM IR for a MIR `Value`, returning how the result is represented.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn emit_value(
    value: &Value,
    emitter: &mut LLVMEmitter,
    local_regs: &HashMap<LocalId, String>,
    local_types: &HashMap<LocalId, Type>,
    next_reg: &mut u32,
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    string_constants: &mut Vec<(String, String)>,
    stack_locals: &StackLocals,
) -> ValueResult {
    match value {
        Value::IntConst(n) => ValueResult::Constant(n.to_string()),
        Value::FloatConst(f) => {
            // LLVM requires hex representation for exact floats.
            ValueResult::FloatConstant(format_float_llvm(*f))
        }
        Value::BoolConst(b) => ValueResult::Constant(if *b { "1" } else { "0" }.to_string()),
        Value::StringConst(s) => emit_string_constant(s, emitter, next_reg, string_constants),
        Value::Local(id) => {
            // If this local has a stack slot, load from it.
            if let Some(alloca_reg) = stack_locals.get(id) {
                let ty = local_types.get(id).unwrap_or(&Type::Int);
                let ty_str = llvm_type(ty, struct_defs, enum_defs);
                if ty_str == "void" {
                    return ValueResult::Void;
                }
                let reg = fresh_reg(next_reg);
                emitter.indent(&format!("{reg} = load {ty_str}, ptr {alloca_reg}"));
                return ValueResult::Register(reg);
            }
            if let Some(reg) = local_regs.get(id) {
                ValueResult::Register(reg.clone())
            } else {
                // Uninitialized local — return zero.
                let ty = local_types.get(id).unwrap_or(&Type::Int);
                let ty_str = llvm_type(ty, struct_defs, enum_defs);
                if ty_str == "void" {
                    return ValueResult::Void;
                }
                let reg = fresh_reg(next_reg);
                if ty_str == "double" {
                    emitter.indent(&format!("{reg} = fadd double 0.0, 0.0"));
                } else if ty_str == "{ i64, i64 }" {
                    // Uninitialized string — zero struct.
                    let r1 = fresh_reg(next_reg);
                    emitter.indent(&format!(
                        "{r1} = insertvalue {{ i64, i64 }} zeroinitializer, i64 0, 0"
                    ));
                    emitter.indent(&format!(
                        "{reg} = insertvalue {{ i64, i64 }} {r1}, i64 0, 1"
                    ));
                } else {
                    emitter.indent(&format!("{reg} = add {ty_str} 0, 0"));
                }
                ValueResult::Register(reg)
            }
        }
        Value::BinOp(op, lhs, rhs) => emit_binop(
            *op,
            lhs,
            rhs,
            emitter,
            local_regs,
            local_types,
            next_reg,
            struct_defs,
            enum_defs,
            string_constants,
            stack_locals,
        ),
        Value::Not(inner) => {
            let vr = emit_value(
                inner,
                emitter,
                local_regs,
                local_types,
                next_reg,
                struct_defs,
                enum_defs,
                string_constants,
                stack_locals,
            );
            let inner_reg = value_result_to_reg(&vr, emitter, next_reg, "i64");
            let reg = fresh_reg(next_reg);
            emitter.indent(&format!("{reg} = xor i64 {inner_reg}, 1"));
            ValueResult::Register(reg)
        }
        Value::Neg(inner) => {
            let inner_ty = infer_value_type_simple(inner, local_types);
            let vr = emit_value(
                inner,
                emitter,
                local_regs,
                local_types,
                next_reg,
                struct_defs,
                enum_defs,
                string_constants,
                stack_locals,
            );
            if matches!(inner_ty, Type::Float64 | Type::Float32) {
                let inner_reg = value_result_to_reg(&vr, emitter, next_reg, "double");
                let reg = fresh_reg(next_reg);
                emitter.indent(&format!("{reg} = fneg double {inner_reg}"));
                ValueResult::Register(reg)
            } else {
                let inner_reg = value_result_to_reg(&vr, emitter, next_reg, "i64");
                let reg = fresh_reg(next_reg);
                emitter.indent(&format!("{reg} = sub i64 0, {inner_reg}"));
                ValueResult::Register(reg)
            }
        }
        Value::Unit => ValueResult::Void,
        Value::FuncRef(name) => {
            let reg = fresh_reg(next_reg);
            emitter.indent(&format!("{reg} = ptrtoint ptr @{name} to i64"));
            ValueResult::Register(reg)
        }
        Value::StructLit { name, fields } => emit_struct_lit(
            name,
            fields,
            emitter,
            local_regs,
            local_types,
            next_reg,
            struct_defs,
            enum_defs,
            string_constants,
            stack_locals,
        ),
        Value::FieldGet {
            object,
            field,
            struct_name,
        } => emit_field_get(
            object,
            field,
            struct_name,
            emitter,
            local_regs,
            local_types,
            next_reg,
            struct_defs,
            enum_defs,
            string_constants,
            stack_locals,
        ),
        Value::EnumVariant {
            discriminant,
            args,
            enum_name,
            ..
        } => emit_enum_variant(
            *discriminant,
            args,
            enum_name,
            emitter,
            local_regs,
            local_types,
            next_reg,
            struct_defs,
            enum_defs,
            string_constants,
            stack_locals,
        ),
        Value::EnumDiscriminant(inner) => {
            let inner_ty = infer_value_type_simple(inner, local_types);
            let enum_ty_str = llvm_type(&inner_ty, struct_defs, enum_defs);
            let vr = emit_value(
                inner,
                emitter,
                local_regs,
                local_types,
                next_reg,
                struct_defs,
                enum_defs,
                string_constants,
                stack_locals,
            );
            let inner_reg = value_result_to_reg(&vr, emitter, next_reg, &enum_ty_str);
            let reg = fresh_reg(next_reg);
            emitter.indent(&format!(
                "{reg} = extractvalue {enum_ty_str} {inner_reg}, 0"
            ));
            ValueResult::Register(reg)
        }
        Value::EnumPayload {
            value: inner,
            field_index,
        } => {
            let inner_ty = infer_value_type_simple(inner, local_types);
            let enum_ty_str = llvm_type(&inner_ty, struct_defs, enum_defs);
            let payload_size = enum_payload_bytes(&inner_ty, struct_defs, enum_defs);
            // Extract payload from enum.
            let vr = emit_value(
                inner,
                emitter,
                local_regs,
                local_types,
                next_reg,
                struct_defs,
                enum_defs,
                string_constants,
                stack_locals,
            );
            let inner_reg = value_result_to_reg(&vr, emitter, next_reg, &enum_ty_str);
            // Payload is in [N x i8] at index 1. Extract and bitcast.
            let payload_reg = fresh_reg(next_reg);
            emitter.indent(&format!(
                "{payload_reg} = extractvalue {enum_ty_str} {inner_reg}, 1"
            ));
            // For simplicity, bitcast the first 8 bytes to i64.
            let _ = field_index; // We just grab the first i64 of the payload.
            // Use alloca + store + load to reinterpret bytes as i64.
            // Alloca must be allocated before the load register to maintain SSA order.
            let alloca_reg = fresh_reg(next_reg);
            emitter.indent(&format!("{alloca_reg} = alloca [{payload_size} x i8]"));
            emitter.indent(&format!(
                "store [{payload_size} x i8] {payload_reg}, ptr {alloca_reg}"
            ));
            let result_reg = fresh_reg(next_reg);
            emitter.indent(&format!("{result_reg} = load i64, ptr {alloca_reg}"));
            ValueResult::Register(result_reg)
        }
        Value::MakeDynTrait { value: inner, .. } => {
            // Simplified: just pass through the inner value as i64.
            emit_value(
                inner,
                emitter,
                local_regs,
                local_types,
                next_reg,
                struct_defs,
                enum_defs,
                string_constants,
                stack_locals,
            )
        }
    }
}

/// Emits a string constant, adding it to the module-level constant pool.
fn emit_string_constant(
    s: &str,
    emitter: &mut LLVMEmitter,
    next_reg: &mut u32,
    string_constants: &mut Vec<(String, String)>,
) -> ValueResult {
    let idx = string_constants.len();
    let global_name = format!(".str.{idx}");
    string_constants.push((global_name.clone(), s.to_string()));

    let len = s.len();
    let ptr_reg = fresh_reg(next_reg);
    emitter.indent(&format!(
        "{ptr_reg} = getelementptr [{len} x i8], [{len} x i8]* @{global_name}, i32 0, i32 0"
    ));

    // Build a { i64, i64 } struct from ptr and len.
    let ptr_int_reg = fresh_reg(next_reg);
    emitter.indent(&format!("{ptr_int_reg} = ptrtoint ptr {ptr_reg} to i64"));

    let s1 = fresh_reg(next_reg);
    let s2 = fresh_reg(next_reg);
    emitter.indent(&format!(
        "{s1} = insertvalue {{ i64, i64 }} undef, i64 {ptr_int_reg}, 0"
    ));
    emitter.indent(&format!(
        "{s2} = insertvalue {{ i64, i64 }} {s1}, i64 {len}, 1"
    ));

    ValueResult::Register(s2)
}

/// Converts a `ValueResult` to a register, materializing constants as needed.
fn value_result_to_reg(
    vr: &ValueResult,
    emitter: &mut LLVMEmitter,
    next_reg: &mut u32,
    ty: &str,
) -> String {
    match vr {
        ValueResult::Register(r) => r.clone(),
        ValueResult::Constant(v) => {
            let reg = fresh_reg(next_reg);
            emitter.indent(&format!("{reg} = add {ty} {v}, 0"));
            reg
        }
        ValueResult::FloatConstant(v) => {
            let reg = fresh_reg(next_reg);
            emitter.indent(&format!("{reg} = fadd double {v}, 0.0"));
            reg
        }
        ValueResult::Void => {
            let reg = fresh_reg(next_reg);
            emitter.indent(&format!("{reg} = add i64 0, 0"));
            reg
        }
    }
}

/// Emits a binary operation.
#[allow(clippy::too_many_arguments)]
fn emit_binop(
    op: BinOp,
    lhs: &Value,
    rhs: &Value,
    emitter: &mut LLVMEmitter,
    local_regs: &HashMap<LocalId, String>,
    local_types: &HashMap<LocalId, Type>,
    next_reg: &mut u32,
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    string_constants: &mut Vec<(String, String)>,
    stack_locals: &StackLocals,
) -> ValueResult {
    let lhs_ty = infer_value_type_simple(lhs, local_types);
    let is_float = matches!(lhs_ty, Type::Float64 | Type::Float32);
    let is_string = matches!(lhs_ty, Type::String);

    let lhs_vr = emit_value(
        lhs,
        emitter,
        local_regs,
        local_types,
        next_reg,
        struct_defs,
        enum_defs,
        string_constants,
        stack_locals,
    );
    let rhs_vr = emit_value(
        rhs,
        emitter,
        local_regs,
        local_types,
        next_reg,
        struct_defs,
        enum_defs,
        string_constants,
        stack_locals,
    );

    // String concatenation: emit runtime call.
    if is_string && matches!(op, BinOp::Add) {
        let str_ty = "{ i64, i64 }";
        let l = value_result_to_reg(&lhs_vr, emitter, next_reg, str_ty);
        let r = value_result_to_reg(&rhs_vr, emitter, next_reg, str_ty);
        // Extract ptr and len from both strings.
        let l_ptr = fresh_reg(next_reg);
        let l_len = fresh_reg(next_reg);
        let r_ptr = fresh_reg(next_reg);
        let r_len = fresh_reg(next_reg);
        emitter.indent(&format!("{l_ptr} = extractvalue {{ i64, i64 }} {l}, 0"));
        emitter.indent(&format!("{l_len} = extractvalue {{ i64, i64 }} {l}, 1"));
        emitter.indent(&format!("{r_ptr} = extractvalue {{ i64, i64 }} {r}, 0"));
        emitter.indent(&format!("{r_len} = extractvalue {{ i64, i64 }} {r}, 1"));
        // Allocate output space for the result string (ptr + len via out params).
        let out_ptr = fresh_reg(next_reg);
        let out_len = fresh_reg(next_reg);
        emitter.indent(&format!("{out_ptr} = alloca i64"));
        emitter.indent(&format!("{out_len} = alloca i64"));
        // Convert alloca ptrs to i64 for the runtime call.
        let out_ptr_i64 = fresh_reg(next_reg);
        let out_len_i64 = fresh_reg(next_reg);
        emitter.indent(&format!("{out_ptr_i64} = ptrtoint ptr {out_ptr} to i64"));
        emitter.indent(&format!("{out_len_i64} = ptrtoint ptr {out_len} to i64"));
        emitter.indent(&format!(
            "call void @kodo_string_concat(i64 {l_ptr}, i64 {l_len}, i64 {r_ptr}, i64 {r_len}, i64 {out_ptr_i64}, i64 {out_len_i64})"
        ));
        // Load the result string.
        let res_ptr = fresh_reg(next_reg);
        let res_len = fresh_reg(next_reg);
        emitter.indent(&format!("{res_ptr} = load i64, ptr {out_ptr}"));
        emitter.indent(&format!("{res_len} = load i64, ptr {out_len}"));
        let s1 = fresh_reg(next_reg);
        let s2 = fresh_reg(next_reg);
        emitter.indent(&format!(
            "{s1} = insertvalue {{ i64, i64 }} undef, i64 {res_ptr}, 0"
        ));
        emitter.indent(&format!(
            "{s2} = insertvalue {{ i64, i64 }} {s1}, i64 {res_len}, 1"
        ));
        return ValueResult::Register(s2);
    }

    // String comparison: emit runtime call instead of icmp.
    if is_string && matches!(op, BinOp::Eq | BinOp::Ne) {
        let str_ty = "{ i64, i64 }";
        let l = value_result_to_reg(&lhs_vr, emitter, next_reg, str_ty);
        let r = value_result_to_reg(&rhs_vr, emitter, next_reg, str_ty);
        // Extract ptr and len from both strings.
        let l_ptr = fresh_reg(next_reg);
        let l_len = fresh_reg(next_reg);
        let r_ptr = fresh_reg(next_reg);
        let r_len = fresh_reg(next_reg);
        emitter.indent(&format!("{l_ptr} = extractvalue {{ i64, i64 }} {l}, 0"));
        emitter.indent(&format!("{l_len} = extractvalue {{ i64, i64 }} {l}, 1"));
        emitter.indent(&format!("{r_ptr} = extractvalue {{ i64, i64 }} {r}, 0"));
        emitter.indent(&format!("{r_len} = extractvalue {{ i64, i64 }} {r}, 1"));
        let result = fresh_reg(next_reg);
        emitter.indent(&format!(
            "{result} = call i64 @kodo_string_eq(i64 {l_ptr}, i64 {l_len}, i64 {r_ptr}, i64 {r_len})"
        ));
        if matches!(op, BinOp::Ne) {
            let neg = fresh_reg(next_reg);
            emitter.indent(&format!("{neg} = xor i64 {result}, 1"));
            return ValueResult::Register(neg);
        }
        return ValueResult::Register(result);
    }

    let ty_str = if is_float { "double" } else { "i64" };
    let l = value_result_to_reg(&lhs_vr, emitter, next_reg, ty_str);
    let r = value_result_to_reg(&rhs_vr, emitter, next_reg, ty_str);

    if is_float {
        match op {
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                let cond = match op {
                    BinOp::Ne => "one",
                    BinOp::Lt => "olt",
                    BinOp::Gt => "ogt",
                    BinOp::Le => "ole",
                    BinOp::Ge => "oge",
                    _ => "oeq",
                };
                // Allocate cmp_reg first, then result_reg, to maintain SSA order.
                let cmp_reg = fresh_reg(next_reg);
                let result_reg = fresh_reg(next_reg);
                emitter.indent(&format!("{cmp_reg} = fcmp {cond} double {l}, {r}"));
                emitter.indent(&format!("{result_reg} = zext i1 {cmp_reg} to i64"));
                return ValueResult::Register(result_reg);
            }
            BinOp::And | BinOp::Or => {
                // Logical ops on floats: treat as truthy (nonzero).
                let llvm_op = if matches!(op, BinOp::And) {
                    "and"
                } else {
                    "or"
                };
                let reg = fresh_reg(next_reg);
                emitter.indent(&format!(
                    "{reg} = {llvm_op} i64 0, 0 ; float logical op stub"
                ));
                return ValueResult::Register(reg);
            }
            _ => {}
        }
        let llvm_op = match op {
            BinOp::Add => "fadd",
            BinOp::Sub => "fsub",
            BinOp::Mul => "fmul",
            BinOp::Div => "fdiv",
            BinOp::Mod => "frem",
            _ => unreachable!("comparison and logical ops handled above"),
        };
        let reg = fresh_reg(next_reg);
        emitter.indent(&format!("{reg} = {llvm_op} double {l}, {r}"));
        ValueResult::Register(reg)
    } else {
        match op {
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                let cond = match op {
                    BinOp::Ne => "ne",
                    BinOp::Lt => "slt",
                    BinOp::Gt => "sgt",
                    BinOp::Le => "sle",
                    BinOp::Ge => "sge",
                    _ => "eq",
                };
                // Allocate cmp_reg first, then result_reg, to maintain SSA order.
                let cmp_reg = fresh_reg(next_reg);
                let result_reg = fresh_reg(next_reg);
                emitter.indent(&format!("{cmp_reg} = icmp {cond} i64 {l}, {r}"));
                emitter.indent(&format!("{result_reg} = zext i1 {cmp_reg} to i64"));
                return ValueResult::Register(result_reg);
            }
            _ => {}
        }
        let llvm_op = match op {
            BinOp::Add => "add",
            BinOp::Sub => "sub",
            BinOp::Mul => "mul",
            BinOp::Div => "sdiv",
            BinOp::Mod => "srem",
            BinOp::And => "and",
            BinOp::Or => "or",
            _ => unreachable!("comparison ops handled above"),
        };
        let reg = fresh_reg(next_reg);
        emitter.indent(&format!("{reg} = {llvm_op} i64 {l}, {r}"));
        ValueResult::Register(reg)
    }
}

/// Emits a struct literal value.
#[allow(clippy::too_many_arguments)]
fn emit_struct_lit(
    name: &str,
    fields: &[(String, Value)],
    emitter: &mut LLVMEmitter,
    local_regs: &HashMap<LocalId, String>,
    local_types: &HashMap<LocalId, Type>,
    next_reg: &mut u32,
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    string_constants: &mut Vec<(String, String)>,
    stack_locals: &StackLocals,
) -> ValueResult {
    let struct_ty = format!("%{name}");

    // Start with undef.
    let mut current = "undef".to_string();
    let field_order: Vec<String> = struct_defs
        .get(name)
        .map(|fs| fs.iter().map(|(n, _)| n.clone()).collect())
        .unwrap_or_default();

    for (idx, field_name) in field_order.iter().enumerate() {
        if let Some((_, val)) = fields.iter().find(|(n, _)| n == field_name) {
            let vr = emit_value(
                val,
                emitter,
                local_regs,
                local_types,
                next_reg,
                struct_defs,
                enum_defs,
                string_constants,
                stack_locals,
            );
            let field_ty = struct_defs
                .get(name)
                .and_then(|fs| fs.get(idx))
                .map_or(Type::Int, |(_, t)| t.clone());
            let field_ty_str = llvm_type(&field_ty, struct_defs, enum_defs);
            let val_reg = value_result_to_reg(&vr, emitter, next_reg, &field_ty_str);
            let new_reg = fresh_reg(next_reg);
            emitter.indent(&format!(
                "{new_reg} = insertvalue {struct_ty} {current}, {field_ty_str} {val_reg}, {idx}"
            ));
            current = new_reg;
        }
    }

    if current == "undef" {
        ValueResult::Void
    } else {
        ValueResult::Register(current)
    }
}

/// Emits a field get from a struct value.
#[allow(clippy::too_many_arguments)]
fn emit_field_get(
    object: &Value,
    field: &str,
    struct_name: &str,
    emitter: &mut LLVMEmitter,
    local_regs: &HashMap<LocalId, String>,
    local_types: &HashMap<LocalId, Type>,
    next_reg: &mut u32,
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    string_constants: &mut Vec<(String, String)>,
    stack_locals: &StackLocals,
) -> ValueResult {
    let vr = emit_value(
        object,
        emitter,
        local_regs,
        local_types,
        next_reg,
        struct_defs,
        enum_defs,
        string_constants,
        stack_locals,
    );
    let struct_ty = format!("%{struct_name}");
    let obj_reg = value_result_to_reg(&vr, emitter, next_reg, &struct_ty);

    let field_idx = struct_defs
        .get(struct_name)
        .and_then(|fs| fs.iter().position(|(n, _)| n == field))
        .unwrap_or(0);

    let reg = fresh_reg(next_reg);
    emitter.indent(&format!(
        "{reg} = extractvalue {struct_ty} {obj_reg}, {field_idx}"
    ));
    ValueResult::Register(reg)
}

/// Emits an enum variant construction.
#[allow(clippy::too_many_arguments)]
fn emit_enum_variant(
    discriminant: u8,
    args: &[Value],
    enum_name: &str,
    emitter: &mut LLVMEmitter,
    local_regs: &HashMap<LocalId, String>,
    local_types: &HashMap<LocalId, Type>,
    next_reg: &mut u32,
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    string_constants: &mut Vec<(String, String)>,
    stack_locals: &StackLocals,
) -> ValueResult {
    let enum_ty = llvm_type(&Type::Enum(enum_name.to_string()), struct_defs, enum_defs);

    // Start with zeroinitializer and set discriminant.
    let d_reg = fresh_reg(next_reg);
    emitter.indent(&format!(
        "{d_reg} = insertvalue {enum_ty} zeroinitializer, i64 {discriminant}, 0"
    ));

    if args.is_empty() {
        return ValueResult::Register(d_reg);
    }

    // Determine the payload byte size for this enum.
    let payload_size = enum_payload_bytes(
        &Type::Enum(enum_name.to_string()),
        struct_defs,
        enum_defs,
    );

    // For the first payload arg, store into the payload bytes.
    let arg_ty = infer_value_type_simple(&args[0], local_types);
    let arg_ty_str = llvm_type(&arg_ty, struct_defs, enum_defs);
    let vr = emit_value(
        &args[0],
        emitter,
        local_regs,
        local_types,
        next_reg,
        struct_defs,
        enum_defs,
        string_constants,
        stack_locals,
    );
    let val_reg = value_result_to_reg(&vr, emitter, next_reg, &arg_ty_str);

    // Use alloca to store the payload value as bytes, then insertvalue.
    let alloca_reg = fresh_reg(next_reg);
    emitter.indent(&format!("{alloca_reg} = alloca [{payload_size} x i8]"));
    emitter.indent(&format!("store {arg_ty_str} {val_reg}, ptr {alloca_reg}"));
    let bytes_reg = fresh_reg(next_reg);
    emitter.indent(&format!(
        "{bytes_reg} = load [{payload_size} x i8], ptr {alloca_reg}"
    ));
    let result = fresh_reg(next_reg);
    emitter.indent(&format!(
        "{result} = insertvalue {enum_ty} {d_reg}, [{payload_size} x i8] {bytes_reg}, 1"
    ));

    ValueResult::Register(result)
}

/// Simple type inference for deciding between integer and float operations.
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

/// Extended type inference that uses struct definitions to resolve `FieldGet` types.
pub(crate) fn infer_value_type_ext(
    value: &Value,
    local_types: &HashMap<LocalId, Type>,
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
) -> Type {
    match value {
        Value::FieldGet {
            object,
            field,
            struct_name,
        } => {
            let _ = object; // struct_name already tells us the struct type.
            struct_defs
                .get(struct_name)
                .and_then(|fields| fields.iter().find(|(n, _)| n == field))
                .map_or(Type::Int, |(_, t)| t.clone())
        }
        _ => infer_value_type_simple(value, local_types),
    }
}

/// Formats a float for LLVM IR (uses hex representation for exactness).
fn format_float_llvm(f: f64) -> String {
    // LLVM accepts decimal notation for common values and hex for exact representation.
    // Use hex format for exact bit representation.
    let bits = f.to_bits();
    format!("0x{bits:016X}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_float_zero() {
        assert_eq!(format_float_llvm(0.0), "0x0000000000000000");
    }

    #[test]
    fn format_float_pi() {
        let s = format_float_llvm(std::f64::consts::PI);
        assert!(s.starts_with("0x"));
    }

    /// Verifies that comparison operations allocate SSA registers in sequential
    /// order: icmp gets the lower register, zext gets the next one.
    #[test]
    fn comparison_ssa_register_ordering() {
        let mut emitter = crate::emitter::LLVMEmitter::new();
        let local_regs = HashMap::new();
        let local_types = HashMap::new();
        let struct_defs = HashMap::new();
        let enum_defs = HashMap::new();
        let stack_locals = HashMap::new();
        let mut string_constants = Vec::new();
        let mut next_reg: u32 = 0;

        let value = Value::BinOp(
            BinOp::Eq,
            Box::new(Value::IntConst(1)),
            Box::new(Value::IntConst(2)),
        );
        let _ = emit_value(
            &value,
            &mut emitter,
            &local_regs,
            &local_types,
            &mut next_reg,
            &struct_defs,
            &enum_defs,
            &mut string_constants,
            &stack_locals,
        );
        let output = emitter.finish();

        // icmp must get register N and zext must get register N+1.
        assert!(
            output.contains("%2 = icmp eq i64"),
            "icmp should use %2, got: {output}"
        );
        assert!(
            output.contains("%3 = zext i1 %2 to i64"),
            "zext should use %3 and reference %2, got: {output}"
        );
    }

    /// Verifies SSA ordering for float comparison operations.
    #[test]
    fn float_comparison_ssa_register_ordering() {
        let mut emitter = crate::emitter::LLVMEmitter::new();
        let local_regs = HashMap::new();
        let local_types = HashMap::new();
        let struct_defs = HashMap::new();
        let enum_defs = HashMap::new();
        let stack_locals = HashMap::new();
        let mut string_constants = Vec::new();
        let mut next_reg: u32 = 0;

        let value = Value::BinOp(
            BinOp::Lt,
            Box::new(Value::FloatConst(1.0)),
            Box::new(Value::FloatConst(2.0)),
        );
        let _ = emit_value(
            &value,
            &mut emitter,
            &local_regs,
            &local_types,
            &mut next_reg,
            &struct_defs,
            &enum_defs,
            &mut string_constants,
            &stack_locals,
        );
        let output = emitter.finish();

        // fcmp must get register N and zext must get register N+1.
        assert!(
            output.contains("fcmp olt double"),
            "should contain fcmp olt, got: {output}"
        );
        // Verify sequential numbering.
        let icmp_line = output.lines().find(|l| l.contains("fcmp")).unwrap_or("");
        let zext_line = output.lines().find(|l| l.contains("zext")).unwrap_or("");
        let icmp_reg: u32 = icmp_line
            .trim()
            .strip_prefix('%')
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse().ok())
            .unwrap_or(999);
        let zext_reg: u32 = zext_line
            .trim()
            .strip_prefix('%')
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        assert_eq!(
            zext_reg,
            icmp_reg + 1,
            "zext reg ({zext_reg}) should be icmp reg ({icmp_reg}) + 1"
        );
    }

    #[test]
    fn infer_types() {
        let types = HashMap::new();
        assert_eq!(
            infer_value_type_simple(&Value::IntConst(42), &types),
            Type::Int
        );
        assert_eq!(
            infer_value_type_simple(&Value::FloatConst(1.0), &types),
            Type::Float64
        );
        assert_eq!(
            infer_value_type_simple(&Value::BoolConst(true), &types),
            Type::Bool
        );
    }
}
