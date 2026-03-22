//! Translation of MIR terminators to inkwell LLVM builder calls.
//!
//! Each basic block in MIR ends with a `Terminator` that transfers control
//! flow. This module translates those terminators to LLVM branch, return,
//! and unreachable instructions via the inkwell builder API.

#[cfg(feature = "inkwell")]
use std::collections::HashMap;

#[cfg(feature = "inkwell")]
use inkwell::basic_block::BasicBlock;
#[cfg(feature = "inkwell")]
use inkwell::builder::Builder;
#[cfg(feature = "inkwell")]
use inkwell::context::Context;
#[cfg(feature = "inkwell")]
use inkwell::module::Module;
#[cfg(feature = "inkwell")]
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue};

#[cfg(feature = "inkwell")]
use kodo_mir::{BlockId, LocalId, Terminator};
#[cfg(feature = "inkwell")]
use kodo_types::Type;

#[cfg(feature = "inkwell")]
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
#[cfg(feature = "inkwell")]
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

    match term {
        Terminator::Return(value) => {
            if super::types::is_void(return_type) {
                builder.build_return(None).unwrap();
            } else if let Some(val) = translate_value(value, &mut vctx) {
                builder.build_return(Some(&val)).unwrap();
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
