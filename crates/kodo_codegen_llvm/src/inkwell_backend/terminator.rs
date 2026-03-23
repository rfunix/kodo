//! Translation of MIR terminators to inkwell LLVM builder calls.
//!
//! Each basic block in MIR ends with a `Terminator` that transfers control
//! flow. This module translates those terminators to LLVM branch, return,
//! and unreachable instructions via the inkwell builder API.

use std::collections::HashMap;

use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue};

use kodo_mir::{BlockId, LocalId, Terminator};
use kodo_types::Type;

use super::value::{translate_value, unique_name, ValueCtx};

/// Translates a MIR terminator to inkwell builder calls.
///
/// # Arguments
/// * `term` - The terminator to translate.
/// * `context` - The LLVM context.
/// * `module` - The LLVM module.
/// * `builder` - The LLVM IR builder.
/// * `local_allocas` - Mapping from local IDs to alloca stack slots.
/// * `local_types` - Mapping from local IDs to Kodo types.
/// * `fn_map` - Mapping from function names to LLVM function values.
/// * `block_map` - Mapping from MIR block IDs to LLVM basic blocks.
/// * `return_type` - The function's return type.
/// * `struct_defs` - Struct type definitions.
/// * `enum_defs` - Enum type definitions.
/// * `name_counter` - Counter for unique value names.
/// * `ssa_cache` - Per-block SSA store-forwarding cache for avoiding redundant loads.
#[allow(clippy::too_many_arguments)]
pub(crate) fn translate_terminator<'ctx>(
    term: &Terminator,
    context: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    local_allocas: &HashMap<LocalId, PointerValue<'ctx>>,
    local_types: &HashMap<LocalId, Type>,
    fn_map: &HashMap<String, FunctionValue<'ctx>>,
    block_map: &HashMap<BlockId, BasicBlock<'ctx>>,
    return_type: &Type,
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    name_counter: &mut u32,
    ssa_cache: &mut HashMap<LocalId, BasicValueEnum<'ctx>>,
    alloca_block: inkwell::basic_block::BasicBlock<'ctx>,
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
        alloca_block,
    };

    match term {
        Terminator::Return(value) => {
            if super::types::is_void(return_type) {
                builder.build_return(None).unwrap();
            } else if let Some(val) = translate_value(value, &mut vctx) {
                let expected_ty = super::types::to_llvm_type(context, return_type);
                let coerced = coerce_return_value(val, expected_ty, &mut vctx);
                builder.build_return(Some(&coerced)).unwrap();
            } else {
                // Unit value in non-void function — dead block, emit unreachable.
                builder.build_unreachable().unwrap();
            }
        }
        Terminator::Goto(block_id) => {
            if let Some(target_bb) = block_map.get(block_id) {
                builder.build_unconditional_branch(*target_bb).unwrap();
            } else {
                builder.build_unreachable().unwrap();
            }
        }
        Terminator::Branch {
            condition,
            true_block,
            false_block,
        } => {
            if let Some(cond_val) = translate_value(condition, &mut vctx) {
                let cond_int = cond_val.into_int_value();
                // Truncate i64 to i1 by comparing != 0.
                let cmp_name = unique_name(vctx.name_counter, "br_cmp");
                let zero = context.i64_type().const_int(0, false);
                let cond_i1 = builder
                    .build_int_compare(inkwell::IntPredicate::NE, cond_int, zero, &cmp_name)
                    .unwrap();
                let t_bb = block_map.get(true_block);
                let f_bb = block_map.get(false_block);
                if let (Some(t), Some(f)) = (t_bb, f_bb) {
                    builder.build_conditional_branch(cond_i1, *t, *f).unwrap();
                } else {
                    builder.build_unreachable().unwrap();
                }
            } else {
                // Condition is void — should not happen, emit unreachable.
                builder.build_unreachable().unwrap();
            }
        }
        Terminator::Unreachable => {
            builder.build_unreachable().unwrap();
        }
    }
}

/// Coerces a return value to match the expected LLVM return type.
///
/// Handles mismatches between `{ i64, i64 }` struct (enum) and `i64` scalar,
/// and vice versa. This occurs when the MIR return type differs from the
/// actual value type due to enum/Result/Option representation.
fn coerce_return_value<'ctx>(
    val: BasicValueEnum<'ctx>,
    expected: inkwell::types::BasicTypeEnum<'ctx>,
    vctx: &mut ValueCtx<'_, 'ctx>,
) -> BasicValueEnum<'ctx> {
    let val_is_struct = val.is_struct_value();
    let expected_is_struct = expected.is_struct_type();

    if val_is_struct && !expected_is_struct {
        // Returning { i64, i64 } from a function that expects i64.
        // Extract the payload (field 1) — common when returning enum
        // discriminant from a match, or when local holds enum but
        // function returns scalar.
        let sv = val.into_struct_value();
        let name = unique_name(vctx.name_counter, "ret_coerce");
        vctx.builder
            .build_extract_value(sv, 1, &name)
            .unwrap()
            .into()
    } else if !val_is_struct && expected_is_struct {
        // Returning i64 from a function that expects { i64, i64 }.
        // Build a struct with discriminant 0 and the value as payload.
        let i64_val = val.into_int_value();
        let struct_ty = expected.into_struct_type();
        let zero = vctx.context.i64_type().const_int(0, false);
        let s1_name = unique_name(vctx.name_counter, "ret_s1");
        let s1 = vctx
            .builder
            .build_insert_value(struct_ty.const_zero(), zero, 0, &s1_name)
            .unwrap();
        let s2_name = unique_name(vctx.name_counter, "ret_s2");
        let s2 = vctx
            .builder
            .build_insert_value(s1, i64_val, 1, &s2_name)
            .unwrap();
        s2.into_struct_value().into()
    } else {
        val
    }
}
