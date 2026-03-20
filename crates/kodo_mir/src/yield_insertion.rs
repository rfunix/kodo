//! Yield point insertion pass for green thread scheduling.
//!
//! This pass inserts [`Instruction::Yield`] at strategic points in the MIR
//! to enable cooperative multitasking. Green threads voluntarily yield control
//! at these points, allowing the scheduler to context-switch to other threads.
//!
//! Yield points are inserted:
//! 1. **Before user function calls** — but NOT before internal runtime builtins.
//! 2. **At loop back-edges** — when a `Goto` terminator jumps to a block with
//!    a lower ID (a back-edge in the CFG), a yield is inserted at the start
//!    of the target block.
//!
//! ## Design Rationale
//!
//! Cooperative scheduling requires explicit yield points. Without them, a
//! CPU-bound green thread could starve all others. By inserting yields before
//! function calls and at loop headers, we guarantee that long-running
//! computations periodically give other threads a chance to run.
//!
//! Builtins are excluded because they are short, non-blocking operations
//! where the overhead of a yield check would be wasteful.

use std::collections::HashSet;

use crate::{BlockId, Instruction, MirFunction, Terminator};

/// Prefixes and exact names of builtin functions that should NOT receive
/// yield points. These are internal runtime functions, short builtins,
/// or green thread primitives (yielding before a yield would be redundant).
const BUILTIN_PREFIXES: &[&str] = &[
    "print_int",
    "print_float",
    "print_bool",
    "print",
    "println",
    "assert",
    "kodo_test_",
    "kodo_prop_",
    "kodo_green_",
    "kodo_future_",
    "kodo_rc_",
    "kodo_contract_",
    "kodo_alloc",
    "kodo_closure_",
    "string_",
    "String_",
    "list_",
    "map_",
    "json_",
    "abs",
    "min",
    "max",
    "sqrt",
    "pow",
    "sin",
    "cos",
    "tan",
    "log",
    "ceil",
    "floor",
    "round",
    "to_upper",
    "to_lower",
    "Int_to_string",
    "Float64_to_string",
    "Bool_to_string",
    "time_now",
    "time_format",
    "env_get",
    "env_set",
    "args",
    "readln",
    "Result_",
    "Option_",
    "__env_pack",
    "__env_load",
    "db_",
    "dir_",
];

/// Returns `true` if the callee is a builtin that should NOT have a yield
/// point inserted before it.
fn is_builtin(callee: &str) -> bool {
    BUILTIN_PREFIXES
        .iter()
        .any(|prefix| callee.starts_with(prefix))
}

/// Inserts yield points for green thread scheduling.
///
/// Yield points are inserted before user function calls and at loop
/// back-edges. This enables cooperative scheduling — green threads
/// voluntarily yield control at these points.
///
/// Builtins and internal runtime functions do NOT get yield points
/// to avoid unnecessary overhead.
pub fn insert_yield_points(functions: &mut [MirFunction]) {
    for func in functions {
        insert_yields_in_function(func);
    }
}

/// Inserts yield points into a single MIR function.
///
/// Skips test harness functions (names starting with `__test_`) since
/// test functions do not participate in green thread scheduling.
fn insert_yields_in_function(func: &mut MirFunction) {
    // Skip test harness functions.
    if func.name.starts_with("__test_") {
        return;
    }

    // Phase 1: Insert yields before user function calls.
    insert_yields_before_calls(func);

    // Phase 2: Insert yields at loop back-edge targets.
    insert_yields_at_back_edges(func);
}

/// Inserts `Instruction::Yield` before every non-builtin `Call` instruction.
fn insert_yields_before_calls(func: &mut MirFunction) {
    for block in &mut func.blocks {
        let mut new_instructions = Vec::with_capacity(block.instructions.len());
        for instr in &block.instructions {
            if let Instruction::Call { callee, .. } = instr {
                if !is_builtin(callee) {
                    new_instructions.push(Instruction::Yield);
                }
            }
            new_instructions.push(instr.clone());
        }
        block.instructions = new_instructions;
    }
}

/// Inserts `Instruction::Yield` at the start of blocks targeted by back-edges.
///
/// A back-edge is a `Goto` terminator that jumps to a block with a lower
/// (or equal) ID — indicating a loop. We insert a yield at the start of
/// the target block so that each loop iteration has a yield point.
///
/// Each target block gets at most one yield insertion, even if multiple
/// back-edges target it.
fn insert_yields_at_back_edges(func: &mut MirFunction) {
    // Collect back-edge target block IDs.
    let mut back_edge_targets: HashSet<BlockId> = HashSet::new();
    for block in &func.blocks {
        match &block.terminator {
            Terminator::Goto(target) if target.0 <= block.id.0 => {
                back_edge_targets.insert(*target);
            }
            Terminator::Branch {
                true_block,
                false_block,
                ..
            } => {
                if true_block.0 <= block.id.0 {
                    back_edge_targets.insert(*true_block);
                }
                if false_block.0 <= block.id.0 {
                    back_edge_targets.insert(*false_block);
                }
            }
            _ => {}
        }
    }

    // Insert Yield at the start of each back-edge target block.
    for block in &mut func.blocks {
        if back_edge_targets.contains(&block.id) {
            // Only insert if there isn't already a Yield at the start.
            let already_has_yield = block
                .instructions
                .first()
                .is_some_and(|i| matches!(i, Instruction::Yield));
            if !already_has_yield {
                block.instructions.insert(0, Instruction::Yield);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BasicBlock, BlockId, Instruction, Local, LocalId, MirFunction, Terminator, Value};
    use kodo_types::Type;

    /// Helper to create a simple MIR function with the given blocks.
    fn make_function_with_blocks(name: &str, blocks: Vec<BasicBlock>) -> MirFunction {
        MirFunction {
            name: name.to_string(),
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
            ],
            blocks,
            entry: BlockId(0),
        }
    }

    #[test]
    fn yield_inserted_before_user_call() {
        let mut func = make_function_with_blocks(
            "caller",
            vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Call {
                    dest: LocalId(0),
                    callee: "user_func".to_string(),
                    args: vec![Value::IntConst(42)],
                }],
                terminator: Terminator::Return(Value::Unit),
            }],
        );

        let mut functions = [func];
        insert_yield_points(&mut functions);
        func = functions
            .into_iter()
            .next()
            .unwrap_or_else(|| unreachable!());

        assert_eq!(func.blocks[0].instructions.len(), 2);
        assert_eq!(func.blocks[0].instructions[0], Instruction::Yield);
        assert!(matches!(
            func.blocks[0].instructions[1],
            Instruction::Call { .. }
        ));
    }

    #[test]
    fn yield_not_inserted_before_builtin() {
        let builtins = [
            "print_int",
            "println",
            "assert",
            "list_new",
            "map_get",
            "String_length",
            "kodo_green_maybe_yield",
            "sqrt",
        ];
        for builtin_name in builtins {
            let mut func = make_function_with_blocks(
                "caller",
                vec![BasicBlock {
                    id: BlockId(0),
                    instructions: vec![Instruction::Call {
                        dest: LocalId(0),
                        callee: builtin_name.to_string(),
                        args: vec![],
                    }],
                    terminator: Terminator::Return(Value::Unit),
                }],
            );

            let mut functions = [func];
            insert_yield_points(&mut functions);
            func = functions
                .into_iter()
                .next()
                .unwrap_or_else(|| unreachable!());

            assert_eq!(
                func.blocks[0].instructions.len(),
                1,
                "Yield should NOT be inserted before builtin `{builtin_name}`"
            );
        }
    }

    #[test]
    fn yield_inserted_at_loop_back_edge() {
        // bb0: instructions, Goto(bb1)
        // bb1: instructions, Goto(bb0) <-- back-edge to bb0
        let mut func = make_function_with_blocks(
            "looper",
            vec![
                BasicBlock {
                    id: BlockId(0),
                    instructions: vec![Instruction::Assign(LocalId(0), Value::IntConst(0))],
                    terminator: Terminator::Goto(BlockId(1)),
                },
                BasicBlock {
                    id: BlockId(1),
                    instructions: vec![Instruction::Assign(LocalId(1), Value::IntConst(1))],
                    terminator: Terminator::Goto(BlockId(0)), // back-edge
                },
            ],
        );

        let mut functions = [func];
        insert_yield_points(&mut functions);
        func = functions
            .into_iter()
            .next()
            .unwrap_or_else(|| unreachable!());

        // bb0 should have a Yield inserted at the start (back-edge target).
        assert_eq!(func.blocks[0].instructions[0], Instruction::Yield);
        assert_eq!(func.blocks[0].instructions.len(), 2);
    }

    #[test]
    fn yield_not_inserted_in_test_function() {
        let mut func = make_function_with_blocks(
            "__test_0",
            vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Call {
                    dest: LocalId(0),
                    callee: "user_func".to_string(),
                    args: vec![],
                }],
                terminator: Terminator::Return(Value::Unit),
            }],
        );

        let mut functions = [func];
        insert_yield_points(&mut functions);
        func = functions
            .into_iter()
            .next()
            .unwrap_or_else(|| unreachable!());

        // No Yield should be inserted in test functions.
        assert_eq!(func.blocks[0].instructions.len(), 1);
        assert!(matches!(
            func.blocks[0].instructions[0],
            Instruction::Call { .. }
        ));
    }

    #[test]
    fn yield_at_back_edge_only_inserted_once() {
        // Two back-edges targeting the same block.
        // bb0: Branch { true: bb1, false: bb0 } <-- back-edge to bb0
        // bb1: Goto(bb0) <-- another back-edge to bb0
        let mut func = make_function_with_blocks(
            "multi_back",
            vec![
                BasicBlock {
                    id: BlockId(0),
                    instructions: vec![Instruction::Assign(LocalId(0), Value::IntConst(0))],
                    terminator: Terminator::Branch {
                        condition: Value::BoolConst(true),
                        true_block: BlockId(1),
                        false_block: BlockId(0),
                    },
                },
                BasicBlock {
                    id: BlockId(1),
                    instructions: vec![],
                    terminator: Terminator::Goto(BlockId(0)),
                },
            ],
        );

        let mut functions = [func];
        insert_yield_points(&mut functions);
        func = functions
            .into_iter()
            .next()
            .unwrap_or_else(|| unreachable!());

        // bb0 should have exactly one Yield at the start.
        let yield_count = func.blocks[0]
            .instructions
            .iter()
            .filter(|i| matches!(i, Instruction::Yield))
            .count();
        assert_eq!(
            yield_count, 1,
            "Back-edge yield should only be inserted once"
        );
    }

    #[test]
    fn yield_display() {
        let instr = Instruction::Yield;
        assert_eq!(instr.to_string(), "yield");
    }

    #[test]
    fn is_builtin_checks() {
        assert!(is_builtin("print_int"));
        assert!(is_builtin("println"));
        assert!(is_builtin("kodo_green_maybe_yield"));
        assert!(is_builtin("list_new"));
        assert!(is_builtin("map_get"));
        assert!(is_builtin("String_length"));
        assert!(is_builtin("sqrt"));
        assert!(is_builtin("assert"));
        assert!(!is_builtin("user_func"));
        assert!(!is_builtin("my_module_function"));
    }

    #[test]
    fn yield_before_io_calls() {
        // I/O calls like http_get and file_read should get yield points.
        let io_calls = [
            "http_get",
            "http_post",
            "file_read",
            "file_write",
            "channel_recv",
            "channel_select_2",
            "channel_select_3",
        ];
        for callee_name in io_calls {
            assert!(
                !is_builtin(callee_name),
                "{callee_name} should NOT be classified as a builtin (should get yield)"
            );
        }
    }
}
