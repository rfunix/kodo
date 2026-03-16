//! Function-level translation from MIR to Cranelift IR.
//!
//! Contains the `VarMap` struct that tracks the mapping between MIR locals
//! and Cranelift variables/stack slots, and the top-level `translate_function`
//! driver that sets up blocks, variables, and dispatches to instruction/terminator
//! translation.

use std::collections::HashMap;

use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{InstBuilder, MemFlags, StackSlot};
use cranelift_frontend::{FunctionBuilder, Variable};
use cranelift_module::FuncId;
use cranelift_object::ObjectModule;
use kodo_mir::{BlockId, LocalId, MirFunction};
use kodo_types::Type;

use crate::builtins::BuiltinInfo;
use crate::instruction::translate_instruction;
use crate::layout::{EnumLayout, StructLayout, STRING_LAYOUT_SIZE};
use crate::module::{cranelift_type, is_composite};
use crate::terminator::translate_terminator;
use crate::{CodegenError, Result};

/// Classifies heap-allocated locals so the correct free function is called.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeapKind {
    /// A dynamically allocated `String` (ptr + len in a `_String` stack slot).
    String,
    /// A heap-allocated `List` (opaque i64 handle).
    List,
    /// A heap-allocated `Map` (opaque i64 handle).
    Map,
}

/// Holds the mapping from MIR locals to Cranelift variables during translation.
pub(crate) struct VarMap {
    /// Variables for scalar values.
    pub(crate) vars: HashMap<LocalId, Variable>,
    /// Cranelift type for each scalar variable.
    pub(crate) var_types: HashMap<LocalId, types::Type>,
    /// Stack slots for struct values.
    pub(crate) stack_slots: HashMap<LocalId, (StackSlot, String)>,
    /// Locals that hold heap-allocated values and must be freed before return.
    pub(crate) heap_locals: HashMap<LocalId, HeapKind>,
}

impl VarMap {
    /// Creates an empty variable map.
    pub(crate) fn new() -> Self {
        Self {
            vars: HashMap::new(),
            var_types: HashMap::new(),
            stack_slots: HashMap::new(),
            heap_locals: HashMap::new(),
        }
    }

    /// Looks up the Cranelift variable for a MIR local.
    pub(crate) fn get(&self, id: LocalId) -> Result<Variable> {
        self.vars
            .get(&id)
            .copied()
            .ok_or_else(|| CodegenError::Cranelift(format!("undefined local: {id}")))
    }

    /// Defines a variable value with automatic type narrowing/widening when needed.
    pub(crate) fn def_var_with_cast(
        &self,
        id: LocalId,
        val: cranelift_codegen::ir::Value,
        builder: &mut FunctionBuilder,
    ) -> Result<()> {
        let var = self.get(id)?;
        let declared = self.var_types.get(&id).copied().unwrap_or_else(|| {
            eprintln!("warning: codegen: variable {id:?} has no declared type, defaulting to I64");
            types::I64
        });
        let actual = builder.func.dfg.value_type(val);
        let final_val = if declared == actual {
            val
        } else if declared.is_float() || actual.is_float() {
            // Float types cannot use ireduce/uextend — assign directly.
            val
        } else if declared.bits() < actual.bits() {
            builder.ins().ireduce(declared, val)
        } else {
            builder.ins().uextend(declared, val)
        };
        builder.def_var(var, final_val);
        Ok(())
    }
}

/// Translates a single MIR function into Cranelift IR using the given builder.
#[allow(clippy::too_many_arguments)]
pub(crate) fn translate_function(
    mir_fn: &MirFunction,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    func_ids: &HashMap<String, FuncId>,
    builtins: &HashMap<String, BuiltinInfo>,
    struct_layouts: &HashMap<String, StructLayout>,
    enum_layouts: &HashMap<String, EnumLayout>,
) -> Result<()> {
    // Create Cranelift blocks for each MIR basic block.
    let mut block_map: HashMap<BlockId, cranelift_codegen::ir::Block> = HashMap::new();
    for bb in &mir_fn.blocks {
        let cl_block = builder.create_block();
        block_map.insert(bb.id, cl_block);
    }

    let entry_block = block_map[&mir_fn.entry];

    // Determine if this function uses sret (composite return type).
    let has_sret = is_composite(&mir_fn.return_type);
    // Declare a variable to hold the sret pointer.
    let sret_var = if has_sret {
        let var = builder.declare_var(types::I64);
        Some(var)
    } else {
        None
    };

    // Declare Cranelift variables for each MIR local.
    let mut var_map = VarMap::new();
    declare_locals(mir_fn, builder, &mut var_map, struct_layouts, enum_layouts);

    // Append params to the entry block and define param variables.
    let param_count = mir_fn.param_count();
    builder.append_block_params_for_function_params(entry_block);
    builder.switch_to_block(entry_block);

    // If sret, the first block param is the sret pointer.
    let sret_offset: usize = usize::from(has_sret);

    if let Some(sret_v) = sret_var {
        let sret_param = builder.block_params(entry_block)[0];
        builder.def_var(sret_v, sret_param);
    }

    define_params(
        mir_fn,
        builder,
        &var_map,
        struct_layouts,
        enum_layouts,
        entry_block,
        sret_offset,
        param_count,
    )?;

    // Initialize non-param variables to zero to avoid "variable not defined" errors.
    initialize_non_param_locals(mir_fn, builder, &var_map, param_count)?;

    // Translate each basic block.
    // We defer sealing to after all blocks are translated, because loops
    // create back-edges that mean a block's predecessors are not all known
    // when it is first visited.
    for (idx, bb) in mir_fn.blocks.iter().enumerate() {
        let cl_block = block_map[&bb.id];

        if idx > 0 {
            builder.switch_to_block(cl_block);
        }

        for instr in &bb.instructions {
            translate_instruction(
                instr,
                builder,
                module,
                func_ids,
                builtins,
                &mut var_map,
                struct_layouts,
                enum_layouts,
            )?;
        }

        translate_terminator(
            &bb.terminator,
            builder,
            module,
            func_ids,
            builtins,
            &block_map,
            mir_fn,
            &var_map,
            struct_layouts,
            enum_layouts,
            sret_var,
        )?;
    }

    // Seal all blocks now that all predecessors are known.
    for bb in &mir_fn.blocks {
        let cl_block = block_map[&bb.id];
        builder.seal_block(cl_block);
    }

    Ok(())
}

/// Declares Cranelift variables and stack slots for each MIR local.
fn declare_locals(
    mir_fn: &MirFunction,
    builder: &mut FunctionBuilder,
    var_map: &mut VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
    enum_layouts: &HashMap<String, EnumLayout>,
) {
    for local in &mir_fn.locals {
        match &local.ty {
            Type::String => {
                // Allocate a 16-byte stack slot for String: (ptr: i64, len: i64).
                let slot =
                    builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
                        cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                        STRING_LAYOUT_SIZE,
                        0,
                    ));
                var_map
                    .stack_slots
                    .insert(local.id, (slot, "_String".to_string()));
                let var = builder.declare_var(types::I64);
                var_map.vars.insert(local.id, var);
                var_map.var_types.insert(local.id, types::I64);
            }
            Type::Struct(ref name) => {
                // Allocate a stack slot for struct types.
                if let Some(layout) = struct_layouts.get(name) {
                    let slot =
                        builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
                            cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                            layout.total_size,
                            0,
                        ));
                    var_map.stack_slots.insert(local.id, (slot, name.clone()));
                }
                let var = builder.declare_var(types::I64);
                var_map.vars.insert(local.id, var);
                var_map.var_types.insert(local.id, types::I64);
            }
            Type::Enum(ref name) => {
                // Allocate a stack slot for enum types.
                if let Some(layout) = enum_layouts.get(name) {
                    let slot =
                        builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
                            cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                            layout.total_size,
                            0,
                        ));
                    var_map.stack_slots.insert(local.id, (slot, name.clone()));
                }
                let var = builder.declare_var(types::I64);
                var_map.vars.insert(local.id, var);
                var_map.var_types.insert(local.id, types::I64);
            }
            Type::DynTrait(ref _trait_name) => {
                // Allocate a 16-byte stack slot for dyn Trait fat pointer:
                // (data_ptr: i64, vtable_ptr: i64).
                let slot =
                    builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
                        cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                        crate::layout::DYN_TRAIT_LAYOUT_SIZE,
                        0,
                    ));
                var_map
                    .stack_slots
                    .insert(local.id, (slot, "_DynTrait".to_string()));
                let var = builder.declare_var(types::I64);
                var_map.vars.insert(local.id, var);
                var_map.var_types.insert(local.id, types::I64);
            }
            Type::Tuple(ref elems) => {
                // Allocate a stack slot for tuple: discriminant (8) + N fields (8 each).
                #[allow(clippy::cast_possible_truncation)]
                let total_size = 8 + (elems.len() as u32) * 8;
                let slot =
                    builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
                        cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                        total_size,
                        0,
                    ));
                var_map
                    .stack_slots
                    .insert(local.id, (slot, "__Tuple".to_string()));
                let var = builder.declare_var(types::I64);
                var_map.vars.insert(local.id, var);
                var_map.var_types.insert(local.id, types::I64);
            }
            _ => {
                let cl_ty = cranelift_type(&local.ty);
                let var = builder.declare_var(cl_ty);
                var_map.vars.insert(local.id, var);
                var_map.var_types.insert(local.id, cl_ty);
            }
        }
    }
}

/// Defines parameter variables from block params.
#[allow(clippy::too_many_arguments)]
fn define_params(
    mir_fn: &MirFunction,
    builder: &mut FunctionBuilder,
    var_map: &VarMap,
    struct_layouts: &HashMap<String, StructLayout>,
    enum_layouts: &HashMap<String, EnumLayout>,
    entry_block: cranelift_codegen::ir::Block,
    sret_offset: usize,
    param_count: usize,
) -> Result<()> {
    for i in 0..param_count {
        let param_val = builder.block_params(entry_block)[i + sret_offset];
        #[allow(clippy::cast_possible_truncation)]
        let local_id = LocalId(i as u32);
        let local_ty = &mir_fn.locals[i].ty;

        if is_composite(local_ty) {
            // Composite param: the value is a pointer to the caller's data.
            // Copy it into our local stack slot so mutations don't affect caller.
            if let Some((slot, _)) = var_map.stack_slots.get(&local_id) {
                let slot_size = match local_ty {
                    Type::String => STRING_LAYOUT_SIZE,
                    Type::Struct(name) => struct_layouts.get(name).map_or(8, |l| l.total_size),
                    Type::Enum(name) => enum_layouts.get(name).map_or(8, |l| l.total_size),
                    _ => 8,
                };
                let num_words = slot_size.div_ceil(8);
                let dest_addr = builder.ins().stack_addr(types::I64, *slot, 0);
                for w in 0..num_words {
                    #[allow(clippy::cast_possible_wrap)]
                    let off = (w * 8) as i32;
                    let src_field = builder.ins().iadd_imm(param_val, i64::from(off));
                    let val = builder
                        .ins()
                        .load(types::I64, MemFlags::new(), src_field, 0);
                    let dest_field = builder.ins().iadd_imm(dest_addr, i64::from(off));
                    builder.ins().store(MemFlags::new(), val, dest_field, 0);
                }
                let var = var_map.get(local_id)?;
                builder.def_var(var, dest_addr);
            } else {
                let var = var_map.get(local_id)?;
                builder.def_var(var, param_val);
            }
        } else {
            let var = var_map.get(local_id)?;
            builder.def_var(var, param_val);
        }
    }
    Ok(())
}

/// Initializes non-parameter variables to zero to avoid "variable not defined" errors.
fn initialize_non_param_locals(
    mir_fn: &MirFunction,
    builder: &mut FunctionBuilder,
    var_map: &VarMap,
    param_count: usize,
) -> Result<()> {
    for local in mir_fn.locals.iter().skip(param_count) {
        if var_map.stack_slots.contains_key(&local.id) {
            // Initialize struct variable to stack slot address (will be set later).
            let var = var_map.get(local.id)?;
            let zero = builder.ins().iconst(types::I64, 0);
            builder.def_var(var, zero);
            continue;
        }
        let var = var_map.get(local.id)?;
        let ty = cranelift_type(&local.ty);
        let zero = if ty.is_float() {
            builder.ins().f64const(0.0)
        } else {
            builder.ins().iconst(ty, 0)
        };
        builder.def_var(var, zero);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cranelift_codegen::ir::types;

    // ---------------------------------------------------------------
    // VarMap::new tests
    // ---------------------------------------------------------------

    #[test]
    fn var_map_new_is_empty() {
        let vm = VarMap::new();
        assert!(vm.vars.is_empty());
        assert!(vm.var_types.is_empty());
        assert!(vm.stack_slots.is_empty());
        assert!(vm.heap_locals.is_empty());
    }

    // ---------------------------------------------------------------
    // VarMap::get tests
    // ---------------------------------------------------------------

    #[test]
    fn var_map_get_undefined_returns_error() {
        let vm = VarMap::new();
        let result = vm.get(LocalId(42));
        assert!(result.is_err());
    }

    #[test]
    fn var_map_get_defined_returns_variable() {
        let mut vm = VarMap::new();
        let var = Variable::from_u32(0);
        vm.vars.insert(LocalId(0), var);
        let result = vm.get(LocalId(0));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), var);
    }

    #[test]
    fn var_map_get_wrong_id_returns_error() {
        let mut vm = VarMap::new();
        vm.vars.insert(LocalId(0), Variable::from_u32(0));
        assert!(vm.get(LocalId(1)).is_err());
    }

    // ---------------------------------------------------------------
    // VarMap insertion and lookup
    // ---------------------------------------------------------------

    #[test]
    fn var_map_insert_and_retrieve_multiple() {
        let mut vm = VarMap::new();
        for i in 0..5 {
            vm.vars.insert(LocalId(i), Variable::from_u32(i));
            vm.var_types.insert(LocalId(i), types::I64);
        }
        assert_eq!(vm.vars.len(), 5);
        assert_eq!(vm.var_types.len(), 5);
        for i in 0..5 {
            assert!(vm.get(LocalId(i)).is_ok());
        }
    }

    #[test]
    fn var_map_var_types_stores_correct_types() {
        let mut vm = VarMap::new();
        vm.var_types.insert(LocalId(0), types::I64);
        vm.var_types.insert(LocalId(1), types::I8);
        vm.var_types.insert(LocalId(2), types::F64);
        assert_eq!(vm.var_types[&LocalId(0)], types::I64);
        assert_eq!(vm.var_types[&LocalId(1)], types::I8);
        assert_eq!(vm.var_types[&LocalId(2)], types::F64);
    }

    // ---------------------------------------------------------------
    // HeapKind tests
    // ---------------------------------------------------------------

    #[test]
    fn heap_kind_equality() {
        assert_eq!(HeapKind::String, HeapKind::String);
        assert_eq!(HeapKind::List, HeapKind::List);
        assert_eq!(HeapKind::Map, HeapKind::Map);
        assert_ne!(HeapKind::String, HeapKind::List);
        assert_ne!(HeapKind::List, HeapKind::Map);
    }

    #[test]
    fn heap_kind_clone() {
        let kind = HeapKind::String;
        let cloned = kind;
        assert_eq!(kind, cloned);
    }

    #[test]
    fn var_map_heap_locals_tracking() {
        let mut vm = VarMap::new();
        vm.heap_locals.insert(LocalId(0), HeapKind::String);
        vm.heap_locals.insert(LocalId(1), HeapKind::List);
        vm.heap_locals.insert(LocalId(2), HeapKind::Map);
        assert_eq!(vm.heap_locals.len(), 3);
        assert_eq!(vm.heap_locals[&LocalId(0)], HeapKind::String);
        assert_eq!(vm.heap_locals[&LocalId(1)], HeapKind::List);
        assert_eq!(vm.heap_locals[&LocalId(2)], HeapKind::Map);
    }
}
