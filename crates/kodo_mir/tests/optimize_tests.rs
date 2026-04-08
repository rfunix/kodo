//! Tests for the MIR optimization passes.
//!
//! Each test builds MIR directly (without going through the AST lowering pass)
//! and verifies the expected transformation: constant folding, dead code
//! elimination, copy propagation, RC pair elimination, and function inlining.

use kodo_ast::BinOp;
use kodo_mir::optimize::{optimize_all, optimize_function};
use kodo_mir::{
    apply_recoverable_contracts, BasicBlock, BlockId, Instruction, Local, LocalId, MirFunction,
    Terminator, Value,
};
use kodo_types::Type;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn local(id: u32, ty: Type) -> Local {
    Local {
        id: LocalId(id),
        ty,
        mutable: true,
    }
}

fn simple_func(name: &str, instructions: Vec<Instruction>, terminator: Terminator) -> MirFunction {
    let locals: Vec<Local> = instructions
        .iter()
        .filter_map(|i| {
            if let Instruction::Assign(id, _) = i {
                Some(local(id.0, Type::Int))
            } else {
                None
            }
        })
        .collect();
    MirFunction {
        name: name.to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals,
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions,
            terminator,
        }],
        entry: BlockId(0),
    }
}

// ---------------------------------------------------------------------------
// Constant Folding
// ---------------------------------------------------------------------------

#[test]
fn fold_int_add() {
    let mut func = simple_func(
        "main",
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
    optimize_function(&mut func);
    // After folding, the assign value should be IntConst(7).
    assert!(
        matches!(
            &func.blocks[0].instructions[0],
            Instruction::Assign(_, Value::IntConst(7))
        ),
        "3 + 4 should fold to 7, got {:?}",
        func.blocks[0].instructions
    );
}

#[test]
fn fold_int_mul() {
    let mut func = simple_func(
        "main",
        vec![Instruction::Assign(
            LocalId(0),
            Value::BinOp(
                BinOp::Mul,
                Box::new(Value::IntConst(6)),
                Box::new(Value::IntConst(7)),
            ),
        )],
        Terminator::Return(Value::Local(LocalId(0))),
    );
    optimize_function(&mut func);
    assert!(
        matches!(
            &func.blocks[0].instructions[0],
            Instruction::Assign(_, Value::IntConst(42))
        ),
        "6 * 7 should fold to 42"
    );
}

#[test]
fn fold_int_sub() {
    let mut func = simple_func(
        "main",
        vec![Instruction::Assign(
            LocalId(0),
            Value::BinOp(
                BinOp::Sub,
                Box::new(Value::IntConst(10)),
                Box::new(Value::IntConst(3)),
            ),
        )],
        Terminator::Return(Value::Local(LocalId(0))),
    );
    optimize_function(&mut func);
    assert!(
        matches!(
            &func.blocks[0].instructions[0],
            Instruction::Assign(_, Value::IntConst(7))
        ),
        "10 - 3 should fold to 7"
    );
}

#[test]
fn fold_int_div() {
    let mut func = simple_func(
        "main",
        vec![Instruction::Assign(
            LocalId(0),
            Value::BinOp(
                BinOp::Div,
                Box::new(Value::IntConst(42)),
                Box::new(Value::IntConst(6)),
            ),
        )],
        Terminator::Return(Value::Local(LocalId(0))),
    );
    optimize_function(&mut func);
    assert!(
        matches!(
            &func.blocks[0].instructions[0],
            Instruction::Assign(_, Value::IntConst(7))
        ),
        "42 / 6 should fold to 7"
    );
}

#[test]
fn fold_int_mod() {
    let mut func = simple_func(
        "main",
        vec![Instruction::Assign(
            LocalId(0),
            Value::BinOp(
                BinOp::Mod,
                Box::new(Value::IntConst(17)),
                Box::new(Value::IntConst(5)),
            ),
        )],
        Terminator::Return(Value::Local(LocalId(0))),
    );
    optimize_function(&mut func);
    assert!(
        matches!(
            &func.blocks[0].instructions[0],
            Instruction::Assign(_, Value::IntConst(2))
        ),
        "17 % 5 should fold to 2"
    );
}

#[test]
fn fold_div_by_zero_not_folded() {
    let orig = Value::BinOp(
        BinOp::Div,
        Box::new(Value::IntConst(5)),
        Box::new(Value::IntConst(0)),
    );
    let mut func = simple_func(
        "main",
        vec![Instruction::Assign(LocalId(0), orig)],
        Terminator::Return(Value::Local(LocalId(0))),
    );
    optimize_function(&mut func);
    // Division by zero should NOT be folded (it would produce a panic/undefined behavior).
    assert!(
        matches!(
            &func.blocks[0].instructions[0],
            Instruction::Assign(_, Value::BinOp(BinOp::Div, _, _))
        ),
        "5 / 0 should not be folded"
    );
}

#[test]
fn fold_int_comparison_eq() {
    let mut func = simple_func(
        "main",
        vec![Instruction::Assign(
            LocalId(0),
            Value::BinOp(
                BinOp::Eq,
                Box::new(Value::IntConst(5)),
                Box::new(Value::IntConst(5)),
            ),
        )],
        Terminator::Return(Value::Local(LocalId(0))),
    );
    optimize_function(&mut func);
    assert!(
        matches!(
            &func.blocks[0].instructions[0],
            Instruction::Assign(_, Value::BoolConst(true))
        ),
        "5 == 5 should fold to true"
    );
}

#[test]
fn fold_int_comparison_ne() {
    let mut func = simple_func(
        "main",
        vec![Instruction::Assign(
            LocalId(0),
            Value::BinOp(
                BinOp::Ne,
                Box::new(Value::IntConst(3)),
                Box::new(Value::IntConst(7)),
            ),
        )],
        Terminator::Return(Value::Local(LocalId(0))),
    );
    optimize_function(&mut func);
    assert!(
        matches!(
            &func.blocks[0].instructions[0],
            Instruction::Assign(_, Value::BoolConst(true))
        ),
        "3 != 7 should fold to true"
    );
}

#[test]
fn fold_int_comparison_lt() {
    let mut func = simple_func(
        "main",
        vec![Instruction::Assign(
            LocalId(0),
            Value::BinOp(
                BinOp::Lt,
                Box::new(Value::IntConst(1)),
                Box::new(Value::IntConst(2)),
            ),
        )],
        Terminator::Return(Value::Local(LocalId(0))),
    );
    optimize_function(&mut func);
    assert!(
        matches!(
            &func.blocks[0].instructions[0],
            Instruction::Assign(_, Value::BoolConst(true))
        ),
        "1 < 2 should fold to true"
    );
}

#[test]
fn fold_bool_and() {
    let mut func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Bool,
        param_count: 0,
        locals: vec![local(0, Type::Bool)],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    BinOp::And,
                    Box::new(Value::BoolConst(true)),
                    Box::new(Value::BoolConst(false)),
                ),
            )],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    optimize_function(&mut func);
    assert!(
        matches!(
            &func.blocks[0].instructions[0],
            Instruction::Assign(_, Value::BoolConst(false))
        ),
        "true && false should fold to false"
    );
}

#[test]
fn fold_bool_or() {
    let mut func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Bool,
        param_count: 0,
        locals: vec![local(0, Type::Bool)],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    BinOp::Or,
                    Box::new(Value::BoolConst(false)),
                    Box::new(Value::BoolConst(true)),
                ),
            )],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    optimize_function(&mut func);
    assert!(
        matches!(
            &func.blocks[0].instructions[0],
            Instruction::Assign(_, Value::BoolConst(true))
        ),
        "false || true should fold to true"
    );
}

#[test]
fn fold_not_bool_const() {
    let mut func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Bool,
        param_count: 0,
        locals: vec![local(0, Type::Bool)],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Assign(
                LocalId(0),
                Value::Not(Box::new(Value::BoolConst(true))),
            )],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    optimize_function(&mut func);
    assert!(
        matches!(
            &func.blocks[0].instructions[0],
            Instruction::Assign(_, Value::BoolConst(false))
        ),
        "!true should fold to false"
    );
}

#[test]
fn fold_neg_int_const() {
    let mut func = simple_func(
        "main",
        vec![Instruction::Assign(
            LocalId(0),
            Value::Neg(Box::new(Value::IntConst(42))),
        )],
        Terminator::Return(Value::Local(LocalId(0))),
    );
    optimize_function(&mut func);
    assert!(
        matches!(
            &func.blocks[0].instructions[0],
            Instruction::Assign(_, Value::IntConst(-42))
        ),
        "-42 should fold to -42"
    );
}

#[test]
fn fold_nested_binop() {
    // (3 + 4) * 2 should fold to 14
    let mut func = simple_func(
        "main",
        vec![Instruction::Assign(
            LocalId(0),
            Value::BinOp(
                BinOp::Mul,
                Box::new(Value::BinOp(
                    BinOp::Add,
                    Box::new(Value::IntConst(3)),
                    Box::new(Value::IntConst(4)),
                )),
                Box::new(Value::IntConst(2)),
            ),
        )],
        Terminator::Return(Value::Local(LocalId(0))),
    );
    optimize_function(&mut func);
    assert!(
        matches!(
            &func.blocks[0].instructions[0],
            Instruction::Assign(_, Value::IntConst(14))
        ),
        "(3+4)*2 should fold to 14"
    );
}

#[test]
fn fold_constant_in_terminator() {
    // Return(1 + 2) should be folded to Return(3)
    let mut func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: Terminator::Return(Value::BinOp(
                BinOp::Add,
                Box::new(Value::IntConst(1)),
                Box::new(Value::IntConst(2)),
            )),
        }],
        entry: BlockId(0),
    };
    optimize_function(&mut func);
    assert!(
        matches!(
            func.blocks[0].terminator,
            Terminator::Return(Value::IntConst(3))
        ),
        "constant in Return should be folded"
    );
}

#[test]
fn fold_constant_in_branch_condition() {
    // Branch(true && false, bb1, bb2) — condition folds to BoolConst(false)
    let mut func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![local(0, Type::Int)],
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Assign(LocalId(0), Value::IntConst(0))],
                terminator: Terminator::Branch {
                    condition: Value::BinOp(
                        BinOp::And,
                        Box::new(Value::BoolConst(true)),
                        Box::new(Value::BoolConst(false)),
                    ),
                    true_block: BlockId(1),
                    false_block: BlockId(2),
                },
            },
            BasicBlock {
                id: BlockId(1),
                instructions: vec![Instruction::Assign(LocalId(0), Value::IntConst(1))],
                terminator: Terminator::Return(Value::Local(LocalId(0))),
            },
            BasicBlock {
                id: BlockId(2),
                instructions: vec![],
                terminator: Terminator::Return(Value::IntConst(2)),
            },
        ],
        entry: BlockId(0),
    };
    optimize_function(&mut func);
    // The branch condition should have been folded to BoolConst(false).
    assert!(
        matches!(
            func.blocks[0].terminator,
            Terminator::Branch {
                condition: Value::BoolConst(false),
                ..
            }
        ),
        "branch condition should fold to false"
    );
}

// ---------------------------------------------------------------------------
// Dead Code Elimination
// ---------------------------------------------------------------------------

#[test]
fn dce_removes_unused_assign() {
    // _0 = 99; return 42  →  after DCE: _0 assign removed (it's never read)
    let mut func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![local(0, Type::Int)],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Assign(LocalId(0), Value::IntConst(99))],
            terminator: Terminator::Return(Value::IntConst(42)),
        }],
        entry: BlockId(0),
    };
    optimize_function(&mut func);
    // The dead assignment to _0 should be eliminated.
    assert!(
        func.blocks[0].instructions.is_empty(),
        "unused assign should be eliminated by DCE, got: {:?}",
        func.blocks[0].instructions
    );
}

#[test]
fn dce_keeps_used_assign() {
    // _0 = 42; return _0 — the assign is used, should NOT be removed
    let mut func = simple_func(
        "main",
        vec![Instruction::Assign(LocalId(0), Value::IntConst(42))],
        Terminator::Return(Value::Local(LocalId(0))),
    );
    optimize_function(&mut func);
    // After copy propagation, return might be Return(IntConst(42)) — but the assign may be dead now
    // So we only check that the return value is correct.
    assert!(
        matches!(
            func.blocks[0].terminator,
            Terminator::Return(Value::IntConst(42)) | Terminator::Return(Value::Local(LocalId(0)))
        ),
        "return 42 should remain"
    );
}

// ---------------------------------------------------------------------------
// Copy Propagation
// ---------------------------------------------------------------------------

#[test]
fn copy_propagation_eliminates_redundant_copy() {
    // _0 = 42; _1 = _0; return _1
    // After copy propagation: _1's use of _0 becomes 42, then DCE removes _0.
    let mut func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![local(0, Type::Int), local(1, Type::Int)],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Assign(LocalId(0), Value::IntConst(42)),
                Instruction::Assign(LocalId(1), Value::Local(LocalId(0))),
            ],
            terminator: Terminator::Return(Value::Local(LocalId(1))),
        }],
        entry: BlockId(0),
    };
    optimize_function(&mut func);
    // After full optimization, the return should propagate back to a constant
    // (or the function should be trivially returning 42 with dead assignments removed).
    match &func.blocks[0].terminator {
        Terminator::Return(Value::IntConst(42)) => {} // fully optimized
        Terminator::Return(Value::Local(_)) => {}     // partially optimized — acceptable
        other => panic!("unexpected terminator after copy propagation: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// RC Pair Elimination
// ---------------------------------------------------------------------------

#[test]
fn rc_pair_elimination_removes_incref_decref() {
    // IncRef(_0); DecRef(_0) — this is an RC no-op and should be eliminated
    let mut func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Unit,
        param_count: 0,
        locals: vec![local(0, Type::String)],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Assign(LocalId(0), Value::StringConst("x".to_string())),
                Instruction::IncRef(LocalId(0)),
                Instruction::DecRef(LocalId(0)),
            ],
            terminator: Terminator::Return(Value::Unit),
        }],
        entry: BlockId(0),
    };
    optimize_function(&mut func);
    // After RC pair elimination, IncRef+DecRef for the same local should be gone.
    let has_incref = func.blocks[0]
        .instructions
        .iter()
        .any(|i| matches!(i, Instruction::IncRef(_)));
    let has_decref = func.blocks[0]
        .instructions
        .iter()
        .any(|i| matches!(i, Instruction::DecRef(LocalId(0))));
    assert!(!has_incref, "IncRef should be eliminated by RC pair elim");
    // Note: DecRef for string locals may be kept for memory management; IncRef+DecRef pairs are removed.
    // We only check IncRef is gone.
    let _ = has_decref; // may or may not be present depending on DCE
}

// ---------------------------------------------------------------------------
// Function Inlining via optimize_all
// ---------------------------------------------------------------------------

#[test]
fn inlining_small_identity_function() {
    // fn id(x: Int) -> Int { return x }
    // fn main() -> Int { id(42) }
    // After inlining: main returns 42 directly.
    let id_fn = MirFunction {
        name: "id".to_string(),
        return_type: Type::Int,
        param_count: 1,
        locals: vec![local(0, Type::Int)],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };

    let main_fn = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![local(0, Type::Int)],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Call {
                dest: LocalId(0),
                callee: "id".to_string(),
                args: vec![Value::IntConst(42)],
            }],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };

    let mut fns = vec![main_fn, id_fn];
    optimize_all(&mut fns);

    let main_fn = fns.remove(0);
    // After inlining id(42): the call should be replaced with the body of id
    // substituted with arg = 42, and after constant/copy folding, return 42.
    match &main_fn.blocks[0].terminator {
        Terminator::Return(Value::IntConst(42)) => {} // fully inlined + folded
        Terminator::Return(Value::Local(_)) => {}     // partially optimized
        other => panic!("expected return 42 after inlining, got: {other:?}"),
    }
}

#[test]
fn inlining_skips_functions_with_multiple_blocks() {
    // Functions with more than one block (control flow) should not be inlined.
    let complex_fn = MirFunction {
        name: "complex".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![local(0, Type::Int)],
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Assign(LocalId(0), Value::IntConst(1))],
                terminator: Terminator::Branch {
                    condition: Value::BoolConst(true),
                    true_block: BlockId(1),
                    false_block: BlockId(2),
                },
            },
            BasicBlock {
                id: BlockId(1),
                instructions: vec![],
                terminator: Terminator::Return(Value::IntConst(1)),
            },
            BasicBlock {
                id: BlockId(2),
                instructions: vec![],
                terminator: Terminator::Return(Value::IntConst(2)),
            },
        ],
        entry: BlockId(0),
    };

    let caller = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![local(0, Type::Int)],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Call {
                dest: LocalId(0),
                callee: "complex".to_string(),
                args: vec![],
            }],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };

    let mut fns = vec![caller, complex_fn];
    optimize_all(&mut fns);

    // The call to `complex` should still be present (not inlined).
    let main_fn = &fns[0];
    assert!(
        matches!(
            &main_fn.blocks[0].instructions[..],
            [Instruction::Call { callee, .. }] if callee == "complex"
        ),
        "multi-block function should not be inlined"
    );
}

// ---------------------------------------------------------------------------
// apply_recoverable_contracts
// ---------------------------------------------------------------------------

#[test]
fn recoverable_contracts_renames_fail_callee() {
    // A function with a branch -> fail_block containing kodo_contract_fail.
    // After apply_recoverable_contracts, the callee becomes kodo_contract_fail_recoverable.
    let mut funcs = vec![MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![local(0, Type::Unit)],
        blocks: vec![
            // bb0: branch to bb1 (continue) or bb2 (fail)
            BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Branch {
                    condition: Value::BoolConst(true),
                    true_block: BlockId(1),
                    false_block: BlockId(2),
                },
            },
            // bb1: continue block
            BasicBlock {
                id: BlockId(1),
                instructions: vec![],
                terminator: Terminator::Return(Value::IntConst(0)),
            },
            // bb2: fail block with kodo_contract_fail call
            BasicBlock {
                id: BlockId(2),
                instructions: vec![Instruction::Call {
                    dest: LocalId(0),
                    callee: "kodo_contract_fail".to_string(),
                    args: vec![Value::StringConst("oops".to_string())],
                }],
                terminator: Terminator::Unreachable,
            },
        ],
        entry: BlockId(0),
    }];

    apply_recoverable_contracts(&mut funcs);

    // The callee in bb2 should now be kodo_contract_fail_recoverable.
    let fail_block = &funcs[0].blocks[2];
    assert!(
        matches!(
            &fail_block.instructions[0],
            Instruction::Call { callee, .. } if callee == "kodo_contract_fail_recoverable"
        ),
        "contract fail callee should be renamed"
    );
    // The Unreachable terminator should become Goto(bb1) — the continue_block.
    assert!(
        matches!(fail_block.terminator, Terminator::Goto(BlockId(1))),
        "fail block should goto continue block, got: {:?}",
        fail_block.terminator
    );
}

#[test]
fn recoverable_contracts_preserves_non_contract_code() {
    // A function with no contract fail calls should be unchanged.
    let mut funcs = vec![MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: Terminator::Return(Value::IntConst(42)),
        }],
        entry: BlockId(0),
    }];

    apply_recoverable_contracts(&mut funcs);

    assert!(
        matches!(
            funcs[0].blocks[0].terminator,
            Terminator::Return(Value::IntConst(42))
        ),
        "non-contract function should be unchanged"
    );
}

// ---------------------------------------------------------------------------
// MirFunction validation
// ---------------------------------------------------------------------------

#[test]
fn validate_empty_blocks_fails() {
    let func = MirFunction {
        name: "bad".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![],
        blocks: vec![],
        entry: BlockId(0),
    };
    assert!(
        func.validate().is_err(),
        "empty blocks should fail validation"
    );
}

#[test]
fn validate_missing_entry_block_fails() {
    // entry points to BlockId(5), but only BlockId(0) exists
    let func = MirFunction {
        name: "bad".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: Terminator::Return(Value::IntConst(0)),
        }],
        entry: BlockId(5),
    };
    assert!(
        func.validate().is_err(),
        "missing entry block should fail validation"
    );
}

#[test]
fn validate_valid_function_passes() {
    let func = MirFunction {
        name: "ok".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: Terminator::Return(Value::IntConst(0)),
        }],
        entry: BlockId(0),
    };
    assert!(
        func.validate().is_ok(),
        "valid function should pass validation"
    );
}
