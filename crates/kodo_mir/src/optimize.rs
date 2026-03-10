//! Basic MIR optimization passes.
//!
//! This module implements three optimization passes that run after MIR lowering:
//!
//! 1. **Constant folding** — Evaluates compile-time constant expressions.
//! 2. **Dead code elimination** — Removes unused local assignments.
//! 3. **Copy propagation** — Replaces copies with their source values.
//!
//! ## Academic References
//!
//! - **\[EC\]** *Engineering a Compiler* Ch. 8â10 â Data-flow analysis,
//!   optimization frameworks, and local value numbering.
//! - **\[Tiger\]** *Modern Compiler Implementation in ML* Ch. 10 â
//!   Liveness analysis and register allocation.

use std::collections::{HashMap, HashSet};

use kodo_ast::BinOp;

use crate::{BasicBlock, Instruction, LocalId, MirFunction, Terminator, Value};

/// Applies all optimization passes to a MIR function in sequence.
///
/// The passes run in the following order:
/// 1. Constant folding
/// 2. Dead code elimination
/// 3. Copy propagation
pub fn optimize_function(func: &mut MirFunction) {
    constant_fold(func);
    dead_code_eliminate(func);
    copy_propagate(func);
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
            Instruction::Call { args, .. } => {
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
            Instruction::Call { .. } | Instruction::IndirectCall { .. } => true,
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
        | Value::EnumPayload { value: inner, .. } => {
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
                Instruction::Call { args, .. } => {
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
        | Value::EnumPayload { value: inner, .. } => {
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
}
