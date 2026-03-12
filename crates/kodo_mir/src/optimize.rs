//! Basic MIR optimization passes.
//!
//! This module implements five optimization passes that run after MIR lowering:
//!
//! 1. **Function inlining** — Inlines small, non-recursive function calls.
//! 2. **Constant folding** — Evaluates compile-time constant expressions.
//! 3. **Dead code elimination** — Removes unused local assignments.
//! 4. **Copy propagation** — Replaces copies with their source values.
//! 5. **RC pair elimination** — Removes redundant `IncRef`/`DecRef` pairs.
//!
//! ## Academic References
//!
//! - **\[EC\]** *Engineering a Compiler* Ch. 8–10 — Data-flow analysis,
//!   optimization frameworks, and local value numbering.
//! - **\[EC\]** *Engineering a Compiler* Ch. 8.7 — Inline substitution.
//! - **\[Tiger\]** *Modern Compiler Implementation in ML* Ch. 10 —
//!   Liveness analysis and register allocation.
//! - **\[Tiger\]** *Modern Compiler Implementation* Ch. 15.3 — Inlining heuristics.

use std::collections::{HashMap, HashSet};

use kodo_ast::BinOp;

use crate::{BasicBlock, Instruction, Local, LocalId, MirFunction, Terminator, Value};

/// Applies all optimization passes to a MIR function in sequence.
///
/// The passes run in the following order:
/// 1. Constant folding
/// 2. Dead code elimination
/// 3. Copy propagation
/// 4. RC pair elimination
///
/// This variant does **not** perform function inlining. Use
/// [`optimize_all`] to run the full pipeline including inlining.
pub fn optimize_function(func: &mut MirFunction) {
    constant_fold(func);
    dead_code_eliminate(func);
    copy_propagate(func);
    eliminate_rc_pairs(func);
}

/// Applies all optimization passes — including function inlining — to a
/// list of MIR functions.
///
/// The inlining pass runs first, substituting small function bodies at
/// their call sites. After inlining, the standard per-function passes
/// (constant folding, dead code elimination, copy propagation) run on
/// every function to clean up the inlined code.
///
/// ## Academic References
///
/// - **\[EC\]** *Engineering a Compiler* Ch. 8.7 — Inline substitution
/// - **\[Tiger\]** *Modern Compiler Implementation* Ch. 15.3 — Inlining heuristics
pub fn optimize_all(functions: &mut [MirFunction]) {
    // Phase 1: build a snapshot of eligible inlining candidates.
    // We need a snapshot because we cannot borrow `functions` mutably and
    // immutably at the same time.
    let snapshot: Vec<InlineCandidate> = functions
        .iter()
        .filter_map(InlineCandidate::try_from_function)
        .collect();

    for func in functions.iter_mut() {
        // Phase 1: inline small functions into the caller.
        inline_small_functions(func, &snapshot);
        // Phase 2: standard per-function passes on the (potentially inlined) body.
        constant_fold(func);
        dead_code_eliminate(func);
        copy_propagate(func);
        eliminate_rc_pairs(func);
    }
}

// ---------------------------------------------------------------------------
// Pass 0: Function Inlining
// ---------------------------------------------------------------------------

/// Maximum number of instructions (excluding terminator) in a function body
/// for it to be eligible for inlining.
const MAX_INLINE_INSTRUCTIONS: usize = 3;

/// A snapshot of a function body that is eligible for inlining.
///
/// This is a pre-filtered, cloned view of a [`MirFunction`] so that we can
/// iterate over the caller mutably while reading callee bodies.
struct InlineCandidate {
    /// The function name.
    name: String,
    /// Parameter count.
    param_count: usize,
    /// The single basic block's instructions.
    instructions: Vec<Instruction>,
    /// The single basic block's terminator.
    terminator: Terminator,
    /// All local declarations (params + temporaries).
    locals: Vec<Local>,
}

impl InlineCandidate {
    /// Builds an [`InlineCandidate`] from a function if it passes the
    /// inlining eligibility checks.
    fn try_from_function(func: &MirFunction) -> Option<Self> {
        // Must have exactly one basic block.
        if func.blocks.len() != 1 {
            return None;
        }

        let block = &func.blocks[0];

        // Must have at most MAX_INLINE_INSTRUCTIONS instructions.
        if block.instructions.len() > MAX_INLINE_INSTRUCTIONS {
            return None;
        }

        // Must not contain calls (avoids blow-up from transitive inlining).
        let has_calls = block.instructions.iter().any(|i| {
            matches!(
                i,
                Instruction::Call { .. } | Instruction::IndirectCall { .. }
            )
        });
        if has_calls {
            return None;
        }

        Some(Self {
            name: func.name.clone(),
            param_count: func.param_count,
            instructions: block.instructions.clone(),
            terminator: block.terminator.clone(),
            locals: func.locals.clone(),
        })
    }
}

/// Inlines small function calls within a MIR function.
///
/// A function is eligible for inlining if:
/// - It has a single basic block (no control flow)
/// - Its block has [`MAX_INLINE_INSTRUCTIONS`] or fewer instructions
///   (excluding terminator)
/// - It is not recursive (does not call itself)
/// - It does not itself contain function calls
///
/// Inlining replaces `Call { dest, callee, args }` with the body of
/// the callee, substituting parameters with arguments and remapping
/// non-parameter locals to fresh locals in the caller.
fn inline_small_functions(func: &mut MirFunction, candidates: &[InlineCandidate]) {
    let fn_map: HashMap<&str, &InlineCandidate> =
        candidates.iter().map(|c| (c.name.as_str(), c)).collect();

    for block_idx in 0..func.blocks.len() {
        let mut new_instructions: Vec<Instruction> = Vec::new();

        // Take instructions out temporarily so we can mutably borrow `func.locals`.
        let old_instructions = std::mem::take(&mut func.blocks[block_idx].instructions);

        for instr in &old_instructions {
            if let Instruction::Call { dest, callee, args } = instr {
                // Never inline recursive calls.
                if callee != &func.name {
                    if let Some(candidate) = fn_map.get(callee.as_str()) {
                        if let Some(inlined) = try_inline(candidate, *dest, args, &mut func.locals)
                        {
                            new_instructions.extend(inlined);
                            continue;
                        }
                    }
                }
            }
            new_instructions.push(instr.clone());
        }

        func.blocks[block_idx].instructions = new_instructions;
    }
}

/// Attempts to inline a function call, returning the substituted instructions.
///
/// Returns `None` if inlining is not feasible (e.g., argument count mismatch).
/// New locals needed for the callee's temporaries are appended to `caller_locals`.
fn try_inline(
    callee: &InlineCandidate,
    dest: LocalId,
    args: &[Value],
    caller_locals: &mut Vec<Local>,
) -> Option<Vec<Instruction>> {
    if args.len() != callee.param_count {
        return None;
    }

    // Build parameter -> argument substitution map.
    // Parameters are the first N locals in the callee.
    let mut param_map: HashMap<LocalId, Value> = HashMap::new();
    for (i, arg) in args.iter().enumerate() {
        #[allow(clippy::cast_possible_truncation)]
        let idx = i as u32;
        param_map.insert(LocalId(idx), arg.clone());
    }

    // Allocate new locals in the caller for the callee's non-parameter locals.
    let mut local_remap: HashMap<LocalId, LocalId> = HashMap::new();
    for (i, local) in callee.locals.iter().enumerate() {
        if i < callee.param_count {
            // Parameters are substituted by arguments; no new local needed.
            continue;
        }
        #[allow(clippy::cast_possible_truncation)]
        let old_id = LocalId(i as u32);
        #[allow(clippy::cast_possible_truncation)]
        let new_id = LocalId(caller_locals.len() as u32);
        caller_locals.push(Local {
            id: new_id,
            ty: local.ty.clone(),
            mutable: local.mutable,
        });
        local_remap.insert(old_id, new_id);
    }

    // Substitute all instructions from the callee body.
    let mut result = Vec::new();
    for instr in &callee.instructions {
        result.push(remap_instruction(instr, &param_map, &local_remap));
    }

    // Map the callee's return value to an assignment to `dest`.
    if let Terminator::Return(ret_val) = &callee.terminator {
        let substituted_val = remap_value(ret_val, &param_map, &local_remap);
        result.push(Instruction::Assign(dest, substituted_val));
    }

    Some(result)
}

/// Remaps an instruction's local references using the parameter and local maps.
fn remap_instruction(
    instr: &Instruction,
    param_map: &HashMap<LocalId, Value>,
    local_remap: &HashMap<LocalId, LocalId>,
) -> Instruction {
    match instr {
        Instruction::Assign(local_id, value) => {
            let new_id = remap_local_id(*local_id, local_remap);
            let new_val = remap_value(value, param_map, local_remap);
            Instruction::Assign(new_id, new_val)
        }
        // Call and IndirectCall should not appear in inlineable candidates,
        // but handle them for completeness.
        Instruction::Call { dest, callee, args } => {
            let new_dest = remap_local_id(*dest, local_remap);
            let new_args: Vec<Value> = args
                .iter()
                .map(|a| remap_value(a, param_map, local_remap))
                .collect();
            Instruction::Call {
                dest: new_dest,
                callee: callee.clone(),
                args: new_args,
            }
        }
        Instruction::IndirectCall {
            dest,
            callee,
            args,
            return_type,
            param_types,
        } => {
            let new_dest = remap_local_id(*dest, local_remap);
            let new_callee = remap_value(callee, param_map, local_remap);
            let new_args: Vec<Value> = args
                .iter()
                .map(|a| remap_value(a, param_map, local_remap))
                .collect();
            Instruction::IndirectCall {
                dest: new_dest,
                callee: new_callee,
                args: new_args,
                return_type: return_type.clone(),
                param_types: param_types.clone(),
            }
        }
        Instruction::IncRef(id) => Instruction::IncRef(remap_local_id(*id, local_remap)),
        Instruction::DecRef(id) => Instruction::DecRef(remap_local_id(*id, local_remap)),
        Instruction::VirtualCall {
            dest,
            object,
            vtable_index,
            args,
            return_type,
            param_types,
        } => {
            let new_dest = remap_local_id(*dest, local_remap);
            let new_object = remap_local_id(*object, local_remap);
            let new_args: Vec<Value> = args
                .iter()
                .map(|a| remap_value(a, param_map, local_remap))
                .collect();
            Instruction::VirtualCall {
                dest: new_dest,
                object: new_object,
                vtable_index: *vtable_index,
                args: new_args,
                return_type: return_type.clone(),
                param_types: param_types.clone(),
            }
        }
    }
}

/// Remaps a local id through the local remap table.
fn remap_local_id(id: LocalId, local_remap: &HashMap<LocalId, LocalId>) -> LocalId {
    local_remap.get(&id).copied().unwrap_or(id)
}

/// Recursively remaps local references in a value.
///
/// Parameters (present in `param_map`) are replaced by their argument values.
/// Non-parameter locals are remapped to their fresh caller-local ids.
fn remap_value(
    value: &Value,
    param_map: &HashMap<LocalId, Value>,
    local_remap: &HashMap<LocalId, LocalId>,
) -> Value {
    match value {
        Value::Local(id) => {
            // Check if this is a parameter being replaced by an argument.
            if let Some(arg_val) = param_map.get(id) {
                return arg_val.clone();
            }
            // Otherwise remap to a fresh local in the caller.
            Value::Local(remap_local_id(*id, local_remap))
        }
        Value::BinOp(op, lhs, rhs) => Value::BinOp(
            *op,
            Box::new(remap_value(lhs, param_map, local_remap)),
            Box::new(remap_value(rhs, param_map, local_remap)),
        ),
        Value::Not(inner) => Value::Not(Box::new(remap_value(inner, param_map, local_remap))),
        Value::Neg(inner) => Value::Neg(Box::new(remap_value(inner, param_map, local_remap))),
        Value::EnumDiscriminant(inner) => {
            Value::EnumDiscriminant(Box::new(remap_value(inner, param_map, local_remap)))
        }
        Value::EnumPayload {
            value: inner,
            field_index,
        } => Value::EnumPayload {
            value: Box::new(remap_value(inner, param_map, local_remap)),
            field_index: *field_index,
        },
        Value::FieldGet {
            object,
            field,
            struct_name,
        } => Value::FieldGet {
            object: Box::new(remap_value(object, param_map, local_remap)),
            field: field.clone(),
            struct_name: struct_name.clone(),
        },
        Value::StructLit { name, fields } => Value::StructLit {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(n, v)| (n.clone(), remap_value(v, param_map, local_remap)))
                .collect(),
        },
        Value::EnumVariant {
            enum_name,
            variant,
            discriminant,
            args,
        } => Value::EnumVariant {
            enum_name: enum_name.clone(),
            variant: variant.clone(),
            discriminant: *discriminant,
            args: args
                .iter()
                .map(|a| remap_value(a, param_map, local_remap))
                .collect(),
        },
        Value::IntConst(_)
        | Value::FloatConst(_)
        | Value::BoolConst(_)
        | Value::StringConst(_)
        | Value::Unit
        | Value::FuncRef(_) => value.clone(),
        Value::MakeDynTrait {
            value: inner,
            concrete_type,
            trait_name,
        } => Value::MakeDynTrait {
            value: Box::new(remap_value(inner, param_map, local_remap)),
            concrete_type: concrete_type.clone(),
            trait_name: trait_name.clone(),
        },
    }
}

// ---------------------------------------------------------------------------
// Pass 1: Constant Folding
// ---------------------------------------------------------------------------

/// Folds compile-time constant expressions throughout a function.
///
/// Recursively evaluates `BinOp`, `Not`, and `Neg` nodes whose operands
/// are all constants, replacing them with the computed constant value.
fn constant_fold(func: &mut MirFunction) {
    for block in &mut func.blocks {
        fold_block(block);
    }
}

/// Folds constant expressions within a single basic block.
fn fold_block(block: &mut BasicBlock) {
    for instr in &mut block.instructions {
        match instr {
            Instruction::Assign(_, value) => {
                *value = fold_value(value.clone());
            }
            Instruction::Call { args, .. } | Instruction::VirtualCall { args, .. } => {
                for arg in args.iter_mut() {
                    *arg = fold_value(arg.clone());
                }
            }
            Instruction::IndirectCall { args, callee, .. } => {
                *callee = fold_value(callee.clone());
                for arg in args.iter_mut() {
                    *arg = fold_value(arg.clone());
                }
            }
            Instruction::IncRef(_) | Instruction::DecRef(_) => {}
        }
    }
    // Fold terminators too.
    match &mut block.terminator {
        Terminator::Return(val) => {
            *val = fold_value(val.clone());
        }
        Terminator::Branch { condition, .. } => {
            *condition = fold_value(condition.clone());
        }
        Terminator::Goto(_) | Terminator::Unreachable => {}
    }
}

/// Recursively folds a value, evaluating constant sub-expressions.
fn fold_value(value: Value) -> Value {
    match value {
        Value::BinOp(op, lhs, rhs) => {
            let lhs_folded = fold_value(*lhs);
            let rhs_folded = fold_value(*rhs);
            fold_binop(op, lhs_folded, rhs_folded)
        }
        Value::Not(inner) => {
            let inner_folded = fold_value(*inner);
            if let Value::BoolConst(b) = inner_folded {
                Value::BoolConst(!b)
            } else {
                Value::Not(Box::new(inner_folded))
            }
        }
        Value::Neg(inner) => {
            let inner_folded = fold_value(*inner);
            match inner_folded {
                Value::IntConst(n) => Value::IntConst(-n),
                Value::FloatConst(f) => Value::FloatConst(-f),
                other => Value::Neg(Box::new(other)),
            }
        }
        other => other,
    }
}

/// Attempts to fold a binary operation on two (possibly constant) values.
fn fold_binop(op: BinOp, lhs: Value, rhs: Value) -> Value {
    if let (Value::IntConst(a), Value::IntConst(b)) = (&lhs, &rhs) {
        let a = *a;
        let b = *b;
        match op {
            BinOp::Add => return Value::IntConst(a.wrapping_add(b)),
            BinOp::Sub => return Value::IntConst(a.wrapping_sub(b)),
            BinOp::Mul => return Value::IntConst(a.wrapping_mul(b)),
            BinOp::Div => {
                if b != 0 {
                    return Value::IntConst(a.wrapping_div(b));
                }
            }
            BinOp::Mod => {
                if b != 0 {
                    return Value::IntConst(a.wrapping_rem(b));
                }
            }
            BinOp::Eq => return Value::BoolConst(a == b),
            BinOp::Ne => return Value::BoolConst(a != b),
            BinOp::Lt => return Value::BoolConst(a < b),
            BinOp::Gt => return Value::BoolConst(a > b),
            BinOp::Le => return Value::BoolConst(a <= b),
            BinOp::Ge => return Value::BoolConst(a >= b),
            BinOp::And | BinOp::Or => {}
        }
    }

    if let (Value::FloatConst(a), Value::FloatConst(b)) = (&lhs, &rhs) {
        let a = *a;
        let b = *b;
        match op {
            BinOp::Add => return Value::FloatConst(a + b),
            BinOp::Sub => return Value::FloatConst(a - b),
            BinOp::Mul => return Value::FloatConst(a * b),
            BinOp::Div => {
                if b != 0.0 {
                    return Value::FloatConst(a / b);
                }
            }
            BinOp::Mod => {
                if b != 0.0 {
                    return Value::FloatConst(a % b);
                }
            }
            BinOp::Lt => return Value::BoolConst(a < b),
            BinOp::Gt => return Value::BoolConst(a > b),
            BinOp::Le => return Value::BoolConst(a <= b),
            BinOp::Ge => return Value::BoolConst(a >= b),
            BinOp::Eq | BinOp::Ne | BinOp::And | BinOp::Or => {}
        }
    }

    if let (Value::BoolConst(a), Value::BoolConst(b)) = (&lhs, &rhs) {
        let a = *a;
        let b = *b;
        match op {
            BinOp::And => return Value::BoolConst(a && b),
            BinOp::Or => return Value::BoolConst(a || b),
            BinOp::Eq => return Value::BoolConst(a == b),
            BinOp::Ne => return Value::BoolConst(a != b),
            _ => {}
        }
    }

    Value::BinOp(op, Box::new(lhs), Box::new(rhs))
}

// ---------------------------------------------------------------------------
// Pass 2: Dead Code Elimination
// ---------------------------------------------------------------------------

/// Eliminates assignments to locals that are never read.
///
/// A local is considered "used" if it appears in any read position
/// (instruction operand, terminator value, call argument). Assignments
/// to unused locals are removed, except for `Call` and `IndirectCall`
/// instructions which may have side effects.
fn dead_code_eliminate(func: &mut MirFunction) {
    let used = collect_used_locals(func);
    for block in &mut func.blocks {
        block.instructions.retain(|instr| match instr {
            Instruction::Assign(local_id, _) => used.contains(local_id),
            Instruction::Call { .. }
            | Instruction::IndirectCall { .. }
            | Instruction::VirtualCall { .. }
            | Instruction::IncRef(_)
            | Instruction::DecRef(_) => true,
        });
    }
}

/// Collects all locals that appear in a read position anywhere in the function.
fn collect_used_locals(func: &MirFunction) -> HashSet<LocalId> {
    let mut used = HashSet::new();
    for block in &func.blocks {
        for instr in &block.instructions {
            match instr {
                Instruction::Assign(_, value) => collect_value_locals(value, &mut used),
                Instruction::Call { args, .. } => {
                    for arg in args {
                        collect_value_locals(arg, &mut used);
                    }
                }
                Instruction::IndirectCall { callee, args, .. } => {
                    collect_value_locals(callee, &mut used);
                    for arg in args {
                        collect_value_locals(arg, &mut used);
                    }
                }
                Instruction::VirtualCall { object, args, .. } => {
                    used.insert(*object);
                    for arg in args {
                        collect_value_locals(arg, &mut used);
                    }
                }
                Instruction::IncRef(local) | Instruction::DecRef(local) => {
                    used.insert(*local);
                }
            }
        }
        match &block.terminator {
            Terminator::Return(val) | Terminator::Branch { condition: val, .. } => {
                collect_value_locals(val, &mut used);
            }
            Terminator::Goto(_) | Terminator::Unreachable => {}
        }
    }
    used
}

/// Recursively collects `Local` references from a value.
fn collect_value_locals(value: &Value, used: &mut HashSet<LocalId>) {
    match value {
        Value::Local(id) => {
            used.insert(*id);
        }
        Value::BinOp(_, lhs, rhs) => {
            collect_value_locals(lhs, used);
            collect_value_locals(rhs, used);
        }
        Value::Not(inner)
        | Value::Neg(inner)
        | Value::EnumDiscriminant(inner)
        | Value::EnumPayload { value: inner, .. }
        | Value::MakeDynTrait { value: inner, .. } => {
            collect_value_locals(inner, used);
        }
        Value::FieldGet { object, .. } => {
            collect_value_locals(object, used);
        }
        Value::StructLit { fields, .. } => {
            for (_, v) in fields {
                collect_value_locals(v, used);
            }
        }
        Value::EnumVariant { args, .. } => {
            for arg in args {
                collect_value_locals(arg, used);
            }
        }
        Value::IntConst(_)
        | Value::FloatConst(_)
        | Value::BoolConst(_)
        | Value::StringConst(_)
        | Value::Unit
        | Value::FuncRef(_) => {}
    }
}

// ---------------------------------------------------------------------------
// Pass 3: Copy Propagation
// ---------------------------------------------------------------------------

/// Propagates simple copies (`_x = _y`) by substituting the source local.
///
/// Only propagates assignments of the form `Assign(dest, Local(src))` where
/// the dest local has exactly one definition. Uses cycle detection to avoid
/// infinite loops in chains.
fn copy_propagate(func: &mut MirFunction) {
    let mut copies: HashMap<LocalId, LocalId> = HashMap::new();
    let mut def_count: HashMap<LocalId, usize> = HashMap::new();

    for block in &func.blocks {
        for instr in &block.instructions {
            if let Instruction::Assign(dest, _) = instr {
                *def_count.entry(*dest).or_insert(0) += 1;
            }
        }
    }

    for block in &func.blocks {
        for instr in &block.instructions {
            if let Instruction::Assign(dest, Value::Local(src)) = instr {
                if def_count.get(dest).copied().unwrap_or(0) == 1 {
                    copies.insert(*dest, *src);
                }
            }
        }
    }

    if copies.is_empty() {
        return;
    }

    let resolved: HashMap<LocalId, LocalId> = copies
        .keys()
        .map(|&dest| {
            let mut current = dest;
            let mut visited = HashSet::new();
            while let Some(&src) = copies.get(&current) {
                if !visited.insert(current) {
                    break;
                }
                current = src;
            }
            (dest, current)
        })
        .collect();

    for block in &mut func.blocks {
        for instr in &mut block.instructions {
            match instr {
                Instruction::Assign(_, value) => substitute_value(value, &resolved),
                Instruction::Call { args, .. } | Instruction::VirtualCall { args, .. } => {
                    for arg in args.iter_mut() {
                        substitute_value(arg, &resolved);
                    }
                }
                Instruction::IndirectCall { callee, args, .. } => {
                    substitute_value(callee, &resolved);
                    for arg in args.iter_mut() {
                        substitute_value(arg, &resolved);
                    }
                }
                Instruction::IncRef(_) | Instruction::DecRef(_) => {}
            }
        }
        match &mut block.terminator {
            Terminator::Return(val) | Terminator::Branch { condition: val, .. } => {
                substitute_value(val, &resolved);
            }
            Terminator::Goto(_) | Terminator::Unreachable => {}
        }
    }
}

/// Recursively substitutes `Local(id)` references using the resolved copy map.
fn substitute_value(value: &mut Value, copies: &HashMap<LocalId, LocalId>) {
    match value {
        Value::Local(id) => {
            if let Some(&replacement) = copies.get(id) {
                *id = replacement;
            }
        }
        Value::BinOp(_, lhs, rhs) => {
            substitute_value(lhs, copies);
            substitute_value(rhs, copies);
        }
        Value::Not(inner)
        | Value::Neg(inner)
        | Value::EnumDiscriminant(inner)
        | Value::EnumPayload { value: inner, .. }
        | Value::MakeDynTrait { value: inner, .. } => {
            substitute_value(inner, copies);
        }
        Value::FieldGet { object, .. } => {
            substitute_value(object, copies);
        }
        Value::StructLit { fields, .. } => {
            for (_, v) in fields.iter_mut() {
                substitute_value(v, copies);
            }
        }
        Value::EnumVariant { args, .. } => {
            for arg in args.iter_mut() {
                substitute_value(arg, copies);
            }
        }
        Value::IntConst(_)
        | Value::FloatConst(_)
        | Value::BoolConst(_)
        | Value::StringConst(_)
        | Value::Unit
        | Value::FuncRef(_) => {}
    }
}

// ---------------------------------------------------------------------------
// Pass 4: RC Pair Elimination
// ---------------------------------------------------------------------------

/// Removes redundant adjacent `IncRef(x)` / `DecRef(x)` pairs (in either
/// order) within a basic block.
///
/// An `IncRef(x)` immediately followed by `DecRef(x)` — or `DecRef(x)`
/// immediately followed by `IncRef(x)` — is a no-op from a reference
/// counting perspective and can be safely eliminated.
///
/// This is a peephole optimization that runs after copy propagation so
/// that copy-resolved locals are already substituted.
pub fn eliminate_rc_pairs(func: &mut MirFunction) {
    for block in &mut func.blocks {
        let mut new_instructions: Vec<Instruction> = Vec::with_capacity(block.instructions.len());
        let mut skip_next = false;

        for i in 0..block.instructions.len() {
            if skip_next {
                skip_next = false;
                continue;
            }

            if i + 1 < block.instructions.len() {
                let is_pair = match (&block.instructions[i], &block.instructions[i + 1]) {
                    (Instruction::IncRef(a), Instruction::DecRef(b)) if a == b => true,
                    (Instruction::DecRef(a), Instruction::IncRef(b)) if a == b => true,
                    _ => false,
                };
                if is_pair {
                    skip_next = true;
                    continue;
                }
            }

            new_instructions.push(block.instructions[i].clone());
        }

        block.instructions = new_instructions;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BasicBlock, BlockId, Instruction, Local, MirFunction, Terminator, Value};
    use kodo_ast::BinOp;
    use kodo_types::Type;

    fn make_function(instructions: Vec<Instruction>, terminator: Terminator) -> MirFunction {
        MirFunction {
            name: "test".to_string(),
            return_type: Type::Unit,
            param_count: 0,
            locals: vec![
                Local {
                    id: LocalId(0),
                    ty: Type::Int,
                    mutable: false,
                },
                Local {
                    id: LocalId(1),
                    ty: Type::Int,
                    mutable: false,
                },
                Local {
                    id: LocalId(2),
                    ty: Type::Int,
                    mutable: false,
                },
                Local {
                    id: LocalId(3),
                    ty: Type::Bool,
                    mutable: false,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions,
                terminator,
            }],
            entry: BlockId(0),
        }
    }

    #[test]
    fn constant_fold_add() {
        let mut func = make_function(
            vec![Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    BinOp::Add,
                    Box::new(Value::IntConst(3)),
                    Box::new(Value::IntConst(4)),
                ),
            )],
            Terminator::Return(Value::Local(LocalId(0))),
        );
        constant_fold(&mut func);
        assert_eq!(
            func.blocks[0].instructions[0],
            Instruction::Assign(LocalId(0), Value::IntConst(7))
        );
    }

    #[test]
    fn constant_fold_nested() {
        let mut func = make_function(
            vec![Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    BinOp::Mul,
                    Box::new(Value::BinOp(
                        BinOp::Add,
                        Box::new(Value::IntConst(2)),
                        Box::new(Value::IntConst(3)),
                    )),
                    Box::new(Value::IntConst(4)),
                ),
            )],
            Terminator::Return(Value::Local(LocalId(0))),
        );
        constant_fold(&mut func);
        assert_eq!(
            func.blocks[0].instructions[0],
            Instruction::Assign(LocalId(0), Value::IntConst(20))
        );
    }

    #[test]
    fn constant_fold_comparison() {
        let mut func = make_function(
            vec![Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    BinOp::Lt,
                    Box::new(Value::IntConst(3)),
                    Box::new(Value::IntConst(5)),
                ),
            )],
            Terminator::Return(Value::Local(LocalId(0))),
        );
        constant_fold(&mut func);
        assert_eq!(
            func.blocks[0].instructions[0],
            Instruction::Assign(LocalId(0), Value::BoolConst(true))
        );
    }

    #[test]
    fn constant_fold_bool() {
        let mut func = make_function(
            vec![Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    BinOp::And,
                    Box::new(Value::BoolConst(true)),
                    Box::new(Value::BoolConst(false)),
                ),
            )],
            Terminator::Return(Value::Local(LocalId(0))),
        );
        constant_fold(&mut func);
        assert_eq!(
            func.blocks[0].instructions[0],
            Instruction::Assign(LocalId(0), Value::BoolConst(false))
        );
    }

    #[test]
    fn constant_fold_negation() {
        let mut func = make_function(
            vec![Instruction::Assign(
                LocalId(0),
                Value::Neg(Box::new(Value::IntConst(42))),
            )],
            Terminator::Return(Value::Local(LocalId(0))),
        );
        constant_fold(&mut func);
        assert_eq!(
            func.blocks[0].instructions[0],
            Instruction::Assign(LocalId(0), Value::IntConst(-42))
        );
    }

    #[test]
    fn constant_fold_div_by_zero_preserved() {
        let mut func = make_function(
            vec![Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    BinOp::Div,
                    Box::new(Value::IntConst(10)),
                    Box::new(Value::IntConst(0)),
                ),
            )],
            Terminator::Return(Value::Local(LocalId(0))),
        );
        constant_fold(&mut func);
        assert_eq!(
            func.blocks[0].instructions[0],
            Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    BinOp::Div,
                    Box::new(Value::IntConst(10)),
                    Box::new(Value::IntConst(0))
                )
            )
        );
    }

    #[test]
    fn dce_unused_variable() {
        let mut func = make_function(
            vec![
                Instruction::Assign(LocalId(0), Value::IntConst(42)),
                Instruction::Assign(LocalId(1), Value::IntConst(10)),
            ],
            Terminator::Return(Value::Local(LocalId(1))),
        );
        dead_code_eliminate(&mut func);
        assert_eq!(func.blocks[0].instructions.len(), 1);
        assert_eq!(
            func.blocks[0].instructions[0],
            Instruction::Assign(LocalId(1), Value::IntConst(10))
        );
    }

    #[test]
    fn copy_propagation_basic() {
        let mut func = make_function(
            vec![
                Instruction::Assign(LocalId(0), Value::IntConst(42)),
                Instruction::Assign(LocalId(1), Value::Local(LocalId(0))),
            ],
            Terminator::Return(Value::Local(LocalId(1))),
        );
        copy_propagate(&mut func);
        assert_eq!(
            func.blocks[0].terminator,
            Terminator::Return(Value::Local(LocalId(0)))
        );
    }

    // -----------------------------------------------------------------------
    // Inlining tests
    // -----------------------------------------------------------------------

    /// Helper: creates a named function with the given param count, locals,
    /// instructions, and terminator.
    fn make_named_function(
        name: &str,
        param_count: usize,
        locals: Vec<Local>,
        instructions: Vec<Instruction>,
        terminator: Terminator,
    ) -> MirFunction {
        MirFunction {
            name: name.to_string(),
            return_type: Type::Int,
            param_count,
            locals,
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions,
                terminator,
            }],
            entry: BlockId(0),
        }
    }

    #[test]
    fn test_is_inlineable_basic() {
        // A simple 1-block, 1-instruction function without calls is inlineable.
        let callee = make_named_function(
            "add_one",
            1,
            vec![Local {
                id: LocalId(0),
                ty: Type::Int,
                mutable: false,
            }],
            vec![Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    BinOp::Add,
                    Box::new(Value::Local(LocalId(0))),
                    Box::new(Value::IntConst(1)),
                ),
            )],
            Terminator::Return(Value::Local(LocalId(0))),
        );
        let candidate = InlineCandidate::try_from_function(&callee);
        assert!(candidate.is_some());
    }

    #[test]
    fn test_inline_simple_function() {
        // Callee: fn double(x: Int) -> Int { return x + x }
        let callee = make_named_function(
            "double",
            1,
            vec![Local {
                id: LocalId(0),
                ty: Type::Int,
                mutable: false,
            }],
            vec![Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    BinOp::Add,
                    Box::new(Value::Local(LocalId(0))),
                    Box::new(Value::Local(LocalId(0))),
                ),
            )],
            Terminator::Return(Value::Local(LocalId(0))),
        );

        // Caller: fn main() { let r = double(21) }
        let mut caller = MirFunction {
            name: "main".to_string(),
            return_type: Type::Int,
            param_count: 0,
            locals: vec![Local {
                id: LocalId(0),
                ty: Type::Int,
                mutable: false,
            }],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Call {
                    dest: LocalId(0),
                    callee: "double".to_string(),
                    args: vec![Value::IntConst(21)],
                }],
                terminator: Terminator::Return(Value::Local(LocalId(0))),
            }],
            entry: BlockId(0),
        };

        let candidates: Vec<InlineCandidate> = [&callee]
            .into_iter()
            .filter_map(InlineCandidate::try_from_function)
            .collect();

        inline_small_functions(&mut caller, &candidates);

        // The Call should be replaced by the inlined body + return assignment.
        // Instruction 0: assign (21 + 21) — substituted from callee body
        // Instruction 1: assign return value to dest
        assert_eq!(caller.blocks[0].instructions.len(), 2);
        assert!(
            !matches!(caller.blocks[0].instructions[0], Instruction::Call { .. }),
            "Call should have been inlined"
        );
    }

    #[test]
    fn test_no_inline_large_function() {
        // A function with >3 instructions should not be eligible.
        let large_fn = make_named_function(
            "big",
            1,
            vec![
                Local {
                    id: LocalId(0),
                    ty: Type::Int,
                    mutable: false,
                },
                Local {
                    id: LocalId(1),
                    ty: Type::Int,
                    mutable: false,
                },
                Local {
                    id: LocalId(2),
                    ty: Type::Int,
                    mutable: false,
                },
                Local {
                    id: LocalId(3),
                    ty: Type::Int,
                    mutable: false,
                },
                Local {
                    id: LocalId(4),
                    ty: Type::Int,
                    mutable: false,
                },
            ],
            vec![
                Instruction::Assign(LocalId(1), Value::IntConst(1)),
                Instruction::Assign(LocalId(2), Value::IntConst(2)),
                Instruction::Assign(LocalId(3), Value::IntConst(3)),
                Instruction::Assign(LocalId(4), Value::IntConst(4)),
            ],
            Terminator::Return(Value::Local(LocalId(4))),
        );

        let candidate = InlineCandidate::try_from_function(&large_fn);
        assert!(
            candidate.is_none(),
            "Function with >3 instructions must not be inlineable"
        );
    }

    #[test]
    fn test_no_inline_recursive() {
        // A function that contains a Call instruction is rejected by the
        // candidate check (it also happens to be recursive, but the calls
        // filter catches it first).
        let fact = make_named_function(
            "fact",
            1,
            vec![
                Local {
                    id: LocalId(0),
                    ty: Type::Int,
                    mutable: false,
                },
                Local {
                    id: LocalId(1),
                    ty: Type::Int,
                    mutable: false,
                },
            ],
            vec![Instruction::Call {
                dest: LocalId(1),
                callee: "fact".to_string(),
                args: vec![Value::Local(LocalId(0))],
            }],
            Terminator::Return(Value::Local(LocalId(1))),
        );

        // The candidate check rejects functions with calls in the body.
        let candidate = InlineCandidate::try_from_function(&fact);
        assert!(
            candidate.is_none(),
            "Function containing calls must not be inlineable"
        );
    }

    #[test]
    fn test_no_inline_multi_block() {
        // A function with 2 blocks (branching) should not be eligible.
        let branching_fn = MirFunction {
            name: "branching".to_string(),
            return_type: Type::Int,
            param_count: 1,
            locals: vec![Local {
                id: LocalId(0),
                ty: Type::Int,
                mutable: false,
            }],
            blocks: vec![
                BasicBlock {
                    id: BlockId(0),
                    instructions: vec![],
                    terminator: Terminator::Branch {
                        condition: Value::BoolConst(true),
                        true_block: BlockId(1),
                        false_block: BlockId(1),
                    },
                },
                BasicBlock {
                    id: BlockId(1),
                    instructions: vec![],
                    terminator: Terminator::Return(Value::Local(LocalId(0))),
                },
            ],
            entry: BlockId(0),
        };

        let candidate = InlineCandidate::try_from_function(&branching_fn);
        assert!(
            candidate.is_none(),
            "Multi-block function must not be inlineable"
        );
    }

    #[test]
    fn test_optimize_all_runs_inlining_and_other_passes() {
        // Callee: fn const_five() -> Int { return 5 }
        let callee = make_named_function(
            "const_five",
            0,
            vec![],
            vec![],
            Terminator::Return(Value::IntConst(5)),
        );

        // Caller: fn main() { let r = const_five(); return r + 10 }
        let caller = MirFunction {
            name: "main".to_string(),
            return_type: Type::Int,
            param_count: 0,
            locals: vec![
                Local {
                    id: LocalId(0),
                    ty: Type::Int,
                    mutable: false,
                },
                Local {
                    id: LocalId(1),
                    ty: Type::Int,
                    mutable: false,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction::Call {
                        dest: LocalId(0),
                        callee: "const_five".to_string(),
                        args: vec![],
                    },
                    Instruction::Assign(
                        LocalId(1),
                        Value::BinOp(
                            BinOp::Add,
                            Box::new(Value::Local(LocalId(0))),
                            Box::new(Value::IntConst(10)),
                        ),
                    ),
                ],
                terminator: Terminator::Return(Value::Local(LocalId(1))),
            }],
            entry: BlockId(0),
        };

        let mut functions = vec![callee, caller];
        optimize_all(&mut functions);

        // After inlining, the Call to const_five should be replaced by
        // Assign(_0, IntConst(5)).  The other passes leave the rest intact.
        let main_fn = &functions[1];

        // No Call instructions should remain (const_five was inlined).
        let has_call = main_fn.blocks[0]
            .instructions
            .iter()
            .any(|i| matches!(i, Instruction::Call { .. }));
        assert!(!has_call, "Call should have been inlined by optimize_all");

        // The inlined constant 5 should appear as an assignment.
        let has_five = main_fn.blocks[0]
            .instructions
            .iter()
            .any(|instr| matches!(instr, Instruction::Assign(_, Value::IntConst(5))));
        assert!(
            has_five,
            "Inlined const_five should produce Assign(_, IntConst(5))"
        );
    }

    // -------------------------------------------------------------------
    // RC pair elimination tests
    // -------------------------------------------------------------------

    fn make_rc_function(instructions: Vec<Instruction>) -> MirFunction {
        MirFunction {
            name: "test_rc".to_string(),
            return_type: Type::Unit,
            param_count: 0,
            locals: vec![
                Local {
                    id: LocalId(0),
                    ty: Type::String,
                    mutable: false,
                },
                Local {
                    id: LocalId(1),
                    ty: Type::String,
                    mutable: false,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions,
                terminator: Terminator::Return(Value::Unit),
            }],
            entry: BlockId(0),
        }
    }

    #[test]
    fn rc_pair_inc_dec_same_local_eliminated() {
        let mut func = make_rc_function(vec![
            Instruction::IncRef(LocalId(0)),
            Instruction::DecRef(LocalId(0)),
        ]);
        eliminate_rc_pairs(&mut func);
        assert!(
            func.blocks[0].instructions.is_empty(),
            "IncRef/DecRef pair on same local should be eliminated"
        );
    }

    #[test]
    fn rc_pair_dec_inc_same_local_eliminated() {
        let mut func = make_rc_function(vec![
            Instruction::DecRef(LocalId(0)),
            Instruction::IncRef(LocalId(0)),
        ]);
        eliminate_rc_pairs(&mut func);
        assert!(
            func.blocks[0].instructions.is_empty(),
            "DecRef/IncRef pair on same local should be eliminated"
        );
    }

    #[test]
    fn rc_pair_different_locals_preserved() {
        let mut func = make_rc_function(vec![
            Instruction::IncRef(LocalId(0)),
            Instruction::DecRef(LocalId(1)),
        ]);
        eliminate_rc_pairs(&mut func);
        assert_eq!(
            func.blocks[0].instructions.len(),
            2,
            "IncRef/DecRef on different locals should NOT be eliminated"
        );
    }

    #[test]
    fn rc_pair_with_intervening_instruction_preserved() {
        let mut func = make_rc_function(vec![
            Instruction::IncRef(LocalId(0)),
            Instruction::Assign(LocalId(1), Value::IntConst(42)),
            Instruction::DecRef(LocalId(0)),
        ]);
        eliminate_rc_pairs(&mut func);
        assert_eq!(
            func.blocks[0].instructions.len(),
            3,
            "Non-adjacent IncRef/DecRef should NOT be eliminated"
        );
    }

    #[test]
    fn rc_pair_multiple_pairs_eliminated() {
        let mut func = make_rc_function(vec![
            Instruction::IncRef(LocalId(0)),
            Instruction::DecRef(LocalId(0)),
            Instruction::IncRef(LocalId(1)),
            Instruction::DecRef(LocalId(1)),
        ]);
        eliminate_rc_pairs(&mut func);
        assert!(
            func.blocks[0].instructions.is_empty(),
            "Multiple redundant RC pairs should all be eliminated"
        );
    }

    #[test]
    fn rc_pair_mixed_with_other_instructions() {
        let mut func = make_rc_function(vec![
            Instruction::Assign(LocalId(0), Value::StringConst("hello".to_string())),
            Instruction::IncRef(LocalId(0)),
            Instruction::DecRef(LocalId(0)),
            Instruction::Assign(LocalId(1), Value::IntConst(10)),
        ]);
        eliminate_rc_pairs(&mut func);
        assert_eq!(func.blocks[0].instructions.len(), 2);
        assert!(matches!(
            func.blocks[0].instructions[0],
            Instruction::Assign(LocalId(0), _)
        ));
        assert!(matches!(
            func.blocks[0].instructions[1],
            Instruction::Assign(LocalId(1), _)
        ));
    }

    #[test]
    fn rc_pair_single_incref_preserved() {
        let mut func = make_rc_function(vec![Instruction::IncRef(LocalId(0))]);
        eliminate_rc_pairs(&mut func);
        assert_eq!(func.blocks[0].instructions.len(), 1);
    }

    #[test]
    fn rc_pair_single_decref_preserved() {
        let mut func = make_rc_function(vec![Instruction::DecRef(LocalId(0))]);
        eliminate_rc_pairs(&mut func);
        assert_eq!(func.blocks[0].instructions.len(), 1);
    }

    #[test]
    fn rc_pair_empty_block_no_crash() {
        let mut func = make_rc_function(vec![]);
        eliminate_rc_pairs(&mut func);
        assert!(func.blocks[0].instructions.is_empty());
    }

    #[test]
    fn rc_optimize_function_includes_rc_elimination() {
        // Verify that optimize_function calls eliminate_rc_pairs.
        let mut func = MirFunction {
            name: "test_opt_rc".to_string(),
            return_type: Type::Unit,
            param_count: 0,
            locals: vec![Local {
                id: LocalId(0),
                ty: Type::String,
                mutable: false,
            }],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction::IncRef(LocalId(0)),
                    Instruction::DecRef(LocalId(0)),
                ],
                terminator: Terminator::Return(Value::Unit),
            }],
            entry: BlockId(0),
        };
        optimize_function(&mut func);
        assert!(
            func.blocks[0].instructions.is_empty(),
            "optimize_function should eliminate redundant RC pairs"
        );
    }
}
