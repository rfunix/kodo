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
//! ## Concurrency-Aware Optimization
//!
//! Functions that do not participate in concurrency (no `spawn`, `channel_*`,
//! or `async` operations) skip yield insertion entirely. This avoids massive
//! overhead in pure recursive functions — e.g., `fib(35)` would otherwise
//! generate ~58 million unnecessary yield calls.
//!
//! The analysis is inter-procedural: a function needs yields if it directly
//! uses concurrency primitives OR calls any function that does (transitively).
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

/// Concurrency primitives that mark a function as needing yield points.
const CONCURRENCY_PRIMITIVES: &[&str] = &[
    "kodo_green_spawn",
    "kodo_green_spawn_with_env",
    "kodo_spawn_async",
    "channel_new",
    "channel_send",
    "channel_recv",
    "channel_select",
];

/// Returns `true` if the callee is a concurrency primitive.
fn is_concurrency_primitive(callee: &str) -> bool {
    CONCURRENCY_PRIMITIVES
        .iter()
        .any(|prim| callee.starts_with(prim))
}

/// Returns `true` if the function directly uses concurrency primitives.
fn uses_concurrency_directly(func: &MirFunction) -> bool {
    func.blocks.iter().any(|block| {
        block.instructions.iter().any(|instr| {
            matches!(instr, Instruction::Call { callee, .. } if is_concurrency_primitive(callee))
        })
    })
}

/// Collects the set of non-builtin callees from a function.
fn collect_callees(func: &MirFunction) -> HashSet<String> {
    let mut callees = HashSet::new();
    for block in &func.blocks {
        for instr in &block.instructions {
            if let Instruction::Call { callee, .. } = instr {
                if !is_builtin(callee) {
                    callees.insert(callee.clone());
                }
            }
        }
    }
    callees
}

/// Computes the set of function names that need yield points via
/// inter-procedural analysis. A function needs yields if it directly
/// uses concurrency primitives or transitively calls one that does.
fn compute_functions_needing_yields(functions: &[MirFunction]) -> HashSet<String> {
    // Phase 1: Mark functions that directly use concurrency.
    let mut needs_yield: HashSet<String> = functions
        .iter()
        .filter(|f| uses_concurrency_directly(f))
        .map(|f| f.name.clone())
        .collect();

    // The main function always needs yields because it may be the
    // entry point for green thread scheduling.
    for func in functions {
        if func.name == "main" {
            needs_yield.insert(func.name.clone());
        }
    }

    // Phase 2: Fixed-point propagation — if a function calls any
    // function in needs_yield, it also needs yields.
    let callees_map: std::collections::HashMap<String, HashSet<String>> = functions
        .iter()
        .map(|f| (f.name.clone(), collect_callees(f)))
        .collect();

    loop {
        let mut changed = false;
        for func in functions {
            if needs_yield.contains(&func.name) {
                continue;
            }
            if let Some(callees) = callees_map.get(&func.name) {
                if callees.iter().any(|c| needs_yield.contains(c)) {
                    needs_yield.insert(func.name.clone());
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }

    needs_yield
}

/// Inserts yield points for green thread scheduling.
///
/// Only functions that participate in concurrency (directly or
/// transitively) receive yield points. Pure computational functions
/// like recursive fibonacci are left untouched for maximum performance.
pub fn insert_yield_points(functions: &mut [MirFunction]) {
    let needs_yield = compute_functions_needing_yields(functions);

    for func in functions {
        if needs_yield.contains(&func.name) {
            insert_yields_in_function(func);
        }
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

    /// Helper: creates a function that calls `kodo_green_spawn` (concurrent).
    fn make_concurrent_function(name: &str) -> MirFunction {
        make_function_with_blocks(
            name,
            vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Call {
                    dest: LocalId(0),
                    callee: "kodo_green_spawn".to_string(),
                    args: vec![],
                }],
                terminator: Terminator::Return(Value::Unit),
            }],
        )
    }

    #[test]
    fn yield_skipped_for_pure_function() {
        // A function that only calls user_func (no concurrency) should NOT
        // receive yields — this is the key optimization.
        let func = make_function_with_blocks(
            "pure_caller",
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

        assert_eq!(
            functions[0].blocks[0].instructions.len(),
            1,
            "Pure function should NOT get yield points"
        );
    }

    #[test]
    fn yield_inserted_in_concurrent_function() {
        // A function that calls kodo_green_spawn should get yields
        // before its other user calls.
        let func = make_function_with_blocks(
            "spawner",
            vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction::Call {
                        dest: LocalId(0),
                        callee: "user_func".to_string(),
                        args: vec![],
                    },
                    Instruction::Call {
                        dest: LocalId(1),
                        callee: "kodo_green_spawn".to_string(),
                        args: vec![],
                    },
                ],
                terminator: Terminator::Return(Value::Unit),
            }],
        );

        let mut functions = [func];
        insert_yield_points(&mut functions);

        // user_func gets a yield before it; kodo_green_spawn is a builtin, no yield.
        assert_eq!(functions[0].blocks[0].instructions.len(), 3);
        assert_eq!(functions[0].blocks[0].instructions[0], Instruction::Yield);
    }

    #[test]
    fn yield_inserted_in_main() {
        // main() always gets yields because it's the green thread entry point.
        let func = make_function_with_blocks(
            "main",
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

        assert_eq!(functions[0].blocks[0].instructions.len(), 2);
        assert_eq!(functions[0].blocks[0].instructions[0], Instruction::Yield);
    }

    #[test]
    fn yield_propagated_transitively() {
        // func_a calls kodo_green_spawn (concurrent)
        // func_b calls func_a (transitively concurrent)
        // func_c calls func_b (transitively concurrent)
        // func_d calls only builtins (pure — no yields)
        let func_a = make_concurrent_function("func_a");

        let func_b = make_function_with_blocks(
            "func_b",
            vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Call {
                    dest: LocalId(0),
                    callee: "func_a".to_string(),
                    args: vec![],
                }],
                terminator: Terminator::Return(Value::Unit),
            }],
        );

        let func_c = make_function_with_blocks(
            "func_c",
            vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Call {
                    dest: LocalId(0),
                    callee: "func_b".to_string(),
                    args: vec![],
                }],
                terminator: Terminator::Return(Value::Unit),
            }],
        );

        let func_d = make_function_with_blocks(
            "func_d",
            vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Call {
                    dest: LocalId(0),
                    callee: "print_int".to_string(),
                    args: vec![Value::IntConst(42)],
                }],
                terminator: Terminator::Return(Value::Unit),
            }],
        );

        let mut functions = [func_a, func_b, func_c, func_d];
        insert_yield_points(&mut functions);

        // func_b calls func_a (concurrent) → gets yields.
        let func_b_yields = functions[1].blocks[0]
            .instructions
            .iter()
            .filter(|i| matches!(i, Instruction::Yield))
            .count();
        assert!(
            func_b_yields > 0,
            "func_b should get yields (calls concurrent func_a)"
        );

        // func_c calls func_b (transitively concurrent) → gets yields.
        let func_c_yields = functions[2].blocks[0]
            .instructions
            .iter()
            .filter(|i| matches!(i, Instruction::Yield))
            .count();
        assert!(
            func_c_yields > 0,
            "func_c should get yields (transitively concurrent)"
        );

        // func_d only calls builtins → no yields.
        let func_d_yields = functions[3].blocks[0]
            .instructions
            .iter()
            .filter(|i| matches!(i, Instruction::Yield))
            .count();
        assert_eq!(func_d_yields, 0, "func_d should NOT get yields (pure)");
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
            // Even in a concurrent function (main), builtins don't get yields.
            let func = make_function_with_blocks(
                "main",
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

            assert_eq!(
                functions[0].blocks[0].instructions.len(),
                1,
                "Yield should NOT be inserted before builtin `{builtin_name}`"
            );
        }
    }

    #[test]
    fn yield_inserted_at_loop_back_edge_in_main() {
        // main() with a loop should get yields at back-edges.
        let func = make_function_with_blocks(
            "main",
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

        assert_eq!(functions[0].blocks[0].instructions[0], Instruction::Yield);
        assert_eq!(functions[0].blocks[0].instructions.len(), 2);
    }

    #[test]
    fn yield_skipped_at_loop_back_edge_in_pure_function() {
        // A pure function with a loop should NOT get yields.
        let func = make_function_with_blocks(
            "pure_looper",
            vec![
                BasicBlock {
                    id: BlockId(0),
                    instructions: vec![Instruction::Assign(LocalId(0), Value::IntConst(0))],
                    terminator: Terminator::Goto(BlockId(1)),
                },
                BasicBlock {
                    id: BlockId(1),
                    instructions: vec![Instruction::Assign(LocalId(1), Value::IntConst(1))],
                    terminator: Terminator::Goto(BlockId(0)),
                },
            ],
        );

        let mut functions = [func];
        insert_yield_points(&mut functions);

        assert_eq!(
            functions[0].blocks[0].instructions.len(),
            1,
            "Pure looper should NOT get yield at back-edge"
        );
    }

    #[test]
    fn yield_not_inserted_in_test_function() {
        let func = make_function_with_blocks(
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

        assert_eq!(functions[0].blocks[0].instructions.len(), 1);
        assert!(matches!(
            functions[0].blocks[0].instructions[0],
            Instruction::Call { .. }
        ));
    }

    #[test]
    fn yield_at_back_edge_only_inserted_once() {
        // In main (concurrent), back-edge targets get exactly one yield.
        let func = make_function_with_blocks(
            "main",
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

        let yield_count = functions[0].blocks[0]
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
    fn is_concurrency_primitive_checks() {
        assert!(is_concurrency_primitive("kodo_green_spawn"));
        assert!(is_concurrency_primitive("kodo_green_spawn_with_env"));
        assert!(is_concurrency_primitive("kodo_spawn_async"));
        assert!(is_concurrency_primitive("channel_new"));
        assert!(is_concurrency_primitive("channel_send"));
        assert!(is_concurrency_primitive("channel_recv"));
        assert!(is_concurrency_primitive("channel_select_2"));
        assert!(!is_concurrency_primitive("user_func"));
        assert!(!is_concurrency_primitive("print_int"));
        assert!(!is_concurrency_primitive("fib"));
    }

    #[test]
    fn yield_before_io_calls_in_concurrent_fn() {
        // I/O calls like http_get and file_read should get yield points
        // only when the function participates in concurrency.
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
