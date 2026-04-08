//! Tests for MIR terminator translation — Return, Goto, Branch, Unreachable.
//!
//! Each test exercises a different terminator variant or CFG shape to ensure
//! the Cranelift backend correctly handles all control-flow exits.

use kodo_codegen::{compile_module, CodegenOptions};
use kodo_mir::{BasicBlock, BlockId, Instruction, Local, LocalId, MirFunction, Terminator, Value};
use kodo_types::Type;

fn opts() -> CodegenOptions {
    CodegenOptions::default()
}

// ---------------------------------------------------------------------------
// Return variants
// ---------------------------------------------------------------------------

#[test]
fn terminator_return_int_const() {
    let func = MirFunction {
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
    };
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

#[test]
fn terminator_return_bool_const() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Bool,
        param_count: 0,
        locals: vec![],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: Terminator::Return(Value::BoolConst(false)),
        }],
        entry: BlockId(0),
    };
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

#[test]
fn terminator_return_unit() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Unit,
        param_count: 0,
        locals: vec![],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: Terminator::Return(Value::Unit),
        }],
        entry: BlockId(0),
    };
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

#[test]
fn terminator_return_local() {
    let func = MirFunction {
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
            instructions: vec![Instruction::Assign(LocalId(0), Value::IntConst(7))],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

#[test]
fn terminator_return_binop() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: Terminator::Return(Value::BinOp(
                kodo_ast::BinOp::Add,
                Box::new(Value::IntConst(3)),
                Box::new(Value::IntConst(4)),
            )),
        }],
        entry: BlockId(0),
    };
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

#[test]
fn terminator_return_param() {
    // Identity function: fn id(x: Int) -> Int { return x }
    let func = MirFunction {
        name: "id".to_string(),
        return_type: Type::Int,
        param_count: 1,
        locals: vec![Local {
            id: LocalId(0),
            ty: Type::Int,
            mutable: false,
        }],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

// ---------------------------------------------------------------------------
// Goto
// ---------------------------------------------------------------------------

#[test]
fn terminator_goto_single_hop() {
    // bb0: goto bb1
    // bb1: return 0
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![],
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Goto(BlockId(1)),
            },
            BasicBlock {
                id: BlockId(1),
                instructions: vec![],
                terminator: Terminator::Return(Value::IntConst(0)),
            },
        ],
        entry: BlockId(0),
    };
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

#[test]
fn terminator_goto_chain() {
    // bb0 → bb1 → bb2 → return
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![Local {
            id: LocalId(0),
            ty: Type::Int,
            mutable: true,
        }],
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Assign(LocalId(0), Value::IntConst(1))],
                terminator: Terminator::Goto(BlockId(1)),
            },
            BasicBlock {
                id: BlockId(1),
                instructions: vec![Instruction::Assign(
                    LocalId(0),
                    Value::BinOp(
                        kodo_ast::BinOp::Mul,
                        Box::new(Value::Local(LocalId(0))),
                        Box::new(Value::IntConst(2)),
                    ),
                )],
                terminator: Terminator::Goto(BlockId(2)),
            },
            BasicBlock {
                id: BlockId(2),
                instructions: vec![],
                terminator: Terminator::Return(Value::Local(LocalId(0))),
            },
        ],
        entry: BlockId(0),
    };
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

// ---------------------------------------------------------------------------
// Branch
// ---------------------------------------------------------------------------

#[test]
fn terminator_branch_true_path() {
    // if true { return 1 } else { return 2 }
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![Local {
            id: LocalId(0),
            ty: Type::Int,
            mutable: true,
        }],
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Assign(LocalId(0), Value::IntConst(0))],
                terminator: Terminator::Branch {
                    condition: Value::BoolConst(true),
                    true_block: BlockId(1),
                    false_block: BlockId(2),
                },
            },
            BasicBlock {
                id: BlockId(1),
                instructions: vec![Instruction::Assign(LocalId(0), Value::IntConst(1))],
                terminator: Terminator::Goto(BlockId(3)),
            },
            BasicBlock {
                id: BlockId(2),
                instructions: vec![Instruction::Assign(LocalId(0), Value::IntConst(2))],
                terminator: Terminator::Goto(BlockId(3)),
            },
            BasicBlock {
                id: BlockId(3),
                instructions: vec![],
                terminator: Terminator::Return(Value::Local(LocalId(0))),
            },
        ],
        entry: BlockId(0),
    };
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

#[test]
fn terminator_branch_on_comparison() {
    // fn abs(x: Int) -> Int { if x < 0 { return -x } else { return x } }
    let func = MirFunction {
        name: "abs".to_string(),
        return_type: Type::Int,
        param_count: 1,
        locals: vec![
            Local {
                id: LocalId(0),
                ty: Type::Int,
                mutable: false,
            },
            Local {
                id: LocalId(1),
                ty: Type::Bool,
                mutable: false,
            },
            Local {
                id: LocalId(2),
                ty: Type::Int,
                mutable: true,
            },
        ],
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Assign(
                    LocalId(1),
                    Value::BinOp(
                        kodo_ast::BinOp::Lt,
                        Box::new(Value::Local(LocalId(0))),
                        Box::new(Value::IntConst(0)),
                    ),
                )],
                terminator: Terminator::Branch {
                    condition: Value::Local(LocalId(1)),
                    true_block: BlockId(1),
                    false_block: BlockId(2),
                },
            },
            BasicBlock {
                id: BlockId(1),
                instructions: vec![Instruction::Assign(
                    LocalId(2),
                    Value::Neg(Box::new(Value::Local(LocalId(0)))),
                )],
                terminator: Terminator::Goto(BlockId(3)),
            },
            BasicBlock {
                id: BlockId(2),
                instructions: vec![Instruction::Assign(LocalId(2), Value::Local(LocalId(0)))],
                terminator: Terminator::Goto(BlockId(3)),
            },
            BasicBlock {
                id: BlockId(3),
                instructions: vec![],
                terminator: Terminator::Return(Value::Local(LocalId(2))),
            },
        ],
        entry: BlockId(0),
    };
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

#[test]
fn terminator_branch_nested() {
    // Nested if: if a { if b { 1 } else { 2 } } else { 3 }
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 2,
        locals: vec![
            Local {
                id: LocalId(0),
                ty: Type::Bool,
                mutable: false,
            },
            Local {
                id: LocalId(1),
                ty: Type::Bool,
                mutable: false,
            },
            Local {
                id: LocalId(2),
                ty: Type::Int,
                mutable: true,
            },
        ],
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Branch {
                    condition: Value::Local(LocalId(0)),
                    true_block: BlockId(1),
                    false_block: BlockId(4),
                },
            },
            BasicBlock {
                id: BlockId(1),
                instructions: vec![],
                terminator: Terminator::Branch {
                    condition: Value::Local(LocalId(1)),
                    true_block: BlockId(2),
                    false_block: BlockId(3),
                },
            },
            BasicBlock {
                id: BlockId(2),
                instructions: vec![Instruction::Assign(LocalId(2), Value::IntConst(1))],
                terminator: Terminator::Goto(BlockId(5)),
            },
            BasicBlock {
                id: BlockId(3),
                instructions: vec![Instruction::Assign(LocalId(2), Value::IntConst(2))],
                terminator: Terminator::Goto(BlockId(5)),
            },
            BasicBlock {
                id: BlockId(4),
                instructions: vec![Instruction::Assign(LocalId(2), Value::IntConst(3))],
                terminator: Terminator::Goto(BlockId(5)),
            },
            BasicBlock {
                id: BlockId(5),
                instructions: vec![],
                terminator: Terminator::Return(Value::Local(LocalId(2))),
            },
        ],
        entry: BlockId(0),
    };
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

#[test]
fn terminator_while_loop() {
    // while i < 5 { i = i + 1 }; return i
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![
            Local {
                id: LocalId(0),
                ty: Type::Int,
                mutable: true,
            },
            Local {
                id: LocalId(1),
                ty: Type::Bool,
                mutable: false,
            },
        ],
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Assign(LocalId(0), Value::IntConst(0))],
                terminator: Terminator::Goto(BlockId(1)),
            },
            BasicBlock {
                id: BlockId(1),
                instructions: vec![Instruction::Assign(
                    LocalId(1),
                    Value::BinOp(
                        kodo_ast::BinOp::Lt,
                        Box::new(Value::Local(LocalId(0))),
                        Box::new(Value::IntConst(5)),
                    ),
                )],
                terminator: Terminator::Branch {
                    condition: Value::Local(LocalId(1)),
                    true_block: BlockId(2),
                    false_block: BlockId(3),
                },
            },
            BasicBlock {
                id: BlockId(2),
                instructions: vec![Instruction::Assign(
                    LocalId(0),
                    Value::BinOp(
                        kodo_ast::BinOp::Add,
                        Box::new(Value::Local(LocalId(0))),
                        Box::new(Value::IntConst(1)),
                    ),
                )],
                terminator: Terminator::Goto(BlockId(1)),
            },
            BasicBlock {
                id: BlockId(3),
                instructions: vec![],
                terminator: Terminator::Return(Value::Local(LocalId(0))),
            },
        ],
        entry: BlockId(0),
    };
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

// ---------------------------------------------------------------------------
// Unreachable
// ---------------------------------------------------------------------------

#[test]
fn terminator_unreachable_in_dead_block() {
    // bb0 jumps to bb1; bb2 is unreachable but present in the function.
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![],
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                instructions: vec![],
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
                terminator: Terminator::Unreachable,
            },
        ],
        entry: BlockId(0),
    };
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

// ---------------------------------------------------------------------------
// Multi-return (multiple paths that return)
// ---------------------------------------------------------------------------

#[test]
fn terminator_multiple_return_paths() {
    // fn sign(x: Int) -> Int { if x > 0 { return 1 } else if x < 0 { return -1 } else { return 0 } }
    let func = MirFunction {
        name: "sign".to_string(),
        return_type: Type::Int,
        param_count: 1,
        locals: vec![
            Local {
                id: LocalId(0),
                ty: Type::Int,
                mutable: false,
            },
            Local {
                id: LocalId(1),
                ty: Type::Bool,
                mutable: false,
            },
        ],
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Assign(
                    LocalId(1),
                    Value::BinOp(
                        kodo_ast::BinOp::Gt,
                        Box::new(Value::Local(LocalId(0))),
                        Box::new(Value::IntConst(0)),
                    ),
                )],
                terminator: Terminator::Branch {
                    condition: Value::Local(LocalId(1)),
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
                instructions: vec![Instruction::Assign(
                    LocalId(1),
                    Value::BinOp(
                        kodo_ast::BinOp::Lt,
                        Box::new(Value::Local(LocalId(0))),
                        Box::new(Value::IntConst(0)),
                    ),
                )],
                terminator: Terminator::Branch {
                    condition: Value::Local(LocalId(1)),
                    true_block: BlockId(3),
                    false_block: BlockId(4),
                },
            },
            BasicBlock {
                id: BlockId(3),
                instructions: vec![],
                terminator: Terminator::Return(Value::Neg(Box::new(Value::IntConst(1)))),
            },
            BasicBlock {
                id: BlockId(4),
                instructions: vec![],
                terminator: Terminator::Return(Value::IntConst(0)),
            },
        ],
        entry: BlockId(0),
    };
    assert!(compile_module(&[func], &opts(), None).is_ok());
}
