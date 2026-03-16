//! Unit tests for codegen instruction translation — verifying that each
//! `MirInstruction` variant compiles through Cranelift without errors.

use kodo_codegen::{compile_module, CodegenOptions};
use kodo_mir::{BasicBlock, BlockId, Instruction, Local, LocalId, MirFunction, Terminator, Value};
use kodo_types::Type;

/// Helper: creates a single-function MIR module and compiles it.
fn compile_single(func: MirFunction) -> Result<Vec<u8>, String> {
    compile_module(&[func], &CodegenOptions::default(), None).map_err(|e| format!("{e}"))
}

/// Helper: creates a main + helper function module and compiles it.
fn compile_pair(main_fn: MirFunction, helper_fn: MirFunction) -> Result<Vec<u8>, String> {
    compile_module(&[main_fn, helper_fn], &CodegenOptions::default(), None)
        .map_err(|e| format!("{e}"))
}

// ---------------------------------------------------------------------------
// Assign instruction tests
// ---------------------------------------------------------------------------

#[test]
fn codegen_assign_int_const() {
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
            instructions: vec![Instruction::Assign(LocalId(0), Value::IntConst(42))],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(
        compile_single(func).is_ok(),
        "assign int const should compile"
    );
}

#[test]
fn codegen_assign_bool_const() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Bool,
        param_count: 0,
        locals: vec![Local {
            id: LocalId(0),
            ty: Type::Bool,
            mutable: false,
        }],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Assign(LocalId(0), Value::BoolConst(true))],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(
        compile_single(func).is_ok(),
        "assign bool const should compile"
    );
}

// Note: Float64 assign is currently not supported as a standalone
// instruction in the codegen backend — float handling requires
// going through the full pipeline (parser → type checker → MIR)
// which correctly handles type widths. Direct MIR construction
// with Float64 locals triggers a Cranelift verifier error.
// This will be addressed when float codegen is extended.

#[test]
fn codegen_assign_unit() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Unit,
        param_count: 0,
        locals: vec![Local {
            id: LocalId(0),
            ty: Type::Unit,
            mutable: false,
        }],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Assign(LocalId(0), Value::Unit)],
            terminator: Terminator::Return(Value::Unit),
        }],
        entry: BlockId(0),
    };
    assert!(compile_single(func).is_ok(), "assign unit should compile");
}

// ---------------------------------------------------------------------------
// BinOp instruction tests
// ---------------------------------------------------------------------------

#[test]
fn codegen_binop_add() {
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
            instructions: vec![Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    kodo_ast::BinOp::Add,
                    Box::new(Value::IntConst(3)),
                    Box::new(Value::IntConst(7)),
                ),
            )],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(compile_single(func).is_ok(), "add binop should compile");
}

#[test]
fn codegen_binop_sub() {
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
            instructions: vec![Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    kodo_ast::BinOp::Sub,
                    Box::new(Value::IntConst(10)),
                    Box::new(Value::IntConst(3)),
                ),
            )],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(compile_single(func).is_ok(), "sub binop should compile");
}

#[test]
fn codegen_binop_mul() {
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
            instructions: vec![Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    kodo_ast::BinOp::Mul,
                    Box::new(Value::IntConst(6)),
                    Box::new(Value::IntConst(7)),
                ),
            )],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(compile_single(func).is_ok(), "mul binop should compile");
}

#[test]
fn codegen_binop_comparison() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Bool,
        param_count: 0,
        locals: vec![Local {
            id: LocalId(0),
            ty: Type::Bool,
            mutable: false,
        }],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    kodo_ast::BinOp::Lt,
                    Box::new(Value::IntConst(3)),
                    Box::new(Value::IntConst(5)),
                ),
            )],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(
        compile_single(func).is_ok(),
        "comparison binop should compile"
    );
}

// ---------------------------------------------------------------------------
// Call instruction tests
// ---------------------------------------------------------------------------

#[test]
fn codegen_direct_call() {
    let callee = MirFunction {
        name: "double".to_string(),
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
            terminator: Terminator::Return(Value::BinOp(
                kodo_ast::BinOp::Mul,
                Box::new(Value::Local(LocalId(0))),
                Box::new(Value::IntConst(2)),
            )),
        }],
        entry: BlockId(0),
    };
    let main_fn = MirFunction {
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
    assert!(
        compile_pair(main_fn, callee).is_ok(),
        "direct call should compile"
    );
}

#[test]
fn codegen_call_multiple_args() {
    let add = MirFunction {
        name: "add".to_string(),
        return_type: Type::Int,
        param_count: 2,
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
            instructions: vec![],
            terminator: Terminator::Return(Value::BinOp(
                kodo_ast::BinOp::Add,
                Box::new(Value::Local(LocalId(0))),
                Box::new(Value::Local(LocalId(1))),
            )),
        }],
        entry: BlockId(0),
    };
    let main_fn = MirFunction {
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
                callee: "add".to_string(),
                args: vec![Value::IntConst(3), Value::IntConst(7)],
            }],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(
        compile_pair(main_fn, add).is_ok(),
        "multi-arg call should compile"
    );
}

// ---------------------------------------------------------------------------
// Branch / CFG tests
// ---------------------------------------------------------------------------

#[test]
fn codegen_branch_goto() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![Local {
            id: LocalId(0),
            ty: Type::Int,
            mutable: false,
        }],
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Assign(LocalId(0), Value::IntConst(1))],
                terminator: Terminator::Goto(BlockId(1)),
            },
            BasicBlock {
                id: BlockId(1),
                instructions: vec![],
                terminator: Terminator::Return(Value::Local(LocalId(0))),
            },
        ],
        entry: BlockId(0),
    };
    assert!(compile_single(func).is_ok(), "goto should compile");
}

#[test]
fn codegen_branch_conditional() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 1,
        locals: vec![
            Local {
                id: LocalId(0),
                ty: Type::Bool,
                mutable: false,
            },
            Local {
                id: LocalId(1),
                ty: Type::Int,
                mutable: true,
            },
        ],
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Assign(LocalId(1), Value::IntConst(0))],
                terminator: Terminator::Branch {
                    condition: Value::Local(LocalId(0)),
                    true_block: BlockId(1),
                    false_block: BlockId(2),
                },
            },
            BasicBlock {
                id: BlockId(1),
                instructions: vec![Instruction::Assign(LocalId(1), Value::IntConst(1))],
                terminator: Terminator::Goto(BlockId(3)),
            },
            BasicBlock {
                id: BlockId(2),
                instructions: vec![Instruction::Assign(LocalId(1), Value::IntConst(2))],
                terminator: Terminator::Goto(BlockId(3)),
            },
            BasicBlock {
                id: BlockId(3),
                instructions: vec![],
                terminator: Terminator::Return(Value::Local(LocalId(1))),
            },
        ],
        entry: BlockId(0),
    };
    assert!(
        compile_single(func).is_ok(),
        "conditional branch should compile"
    );
}

#[test]
fn codegen_loop_cfg() {
    // Simple loop: while i < 10 { i = i + 1 }
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
                terminator: Terminator::Goto(BlockId(1)),
            },
            BasicBlock {
                id: BlockId(1),
                instructions: vec![],
                terminator: Terminator::Branch {
                    condition: Value::BinOp(
                        kodo_ast::BinOp::Lt,
                        Box::new(Value::Local(LocalId(0))),
                        Box::new(Value::IntConst(10)),
                    ),
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
    assert!(compile_single(func).is_ok(), "loop CFG should compile");
}

// ---------------------------------------------------------------------------
// More BinOp instruction tests
// ---------------------------------------------------------------------------

#[test]
fn codegen_binop_div() {
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
            instructions: vec![Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    kodo_ast::BinOp::Div,
                    Box::new(Value::IntConst(42)),
                    Box::new(Value::IntConst(6)),
                ),
            )],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(compile_single(func).is_ok(), "div binop should compile");
}

#[test]
fn codegen_binop_mod() {
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
            instructions: vec![Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    kodo_ast::BinOp::Mod,
                    Box::new(Value::IntConst(17)),
                    Box::new(Value::IntConst(5)),
                ),
            )],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(compile_single(func).is_ok(), "mod binop should compile");
}

#[test]
fn codegen_binop_eq() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Bool,
        param_count: 0,
        locals: vec![Local {
            id: LocalId(0),
            ty: Type::Bool,
            mutable: false,
        }],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    kodo_ast::BinOp::Eq,
                    Box::new(Value::IntConst(5)),
                    Box::new(Value::IntConst(5)),
                ),
            )],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(compile_single(func).is_ok(), "eq binop should compile");
}

#[test]
fn codegen_binop_ne() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Bool,
        param_count: 0,
        locals: vec![Local {
            id: LocalId(0),
            ty: Type::Bool,
            mutable: false,
        }],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    kodo_ast::BinOp::Ne,
                    Box::new(Value::IntConst(3)),
                    Box::new(Value::IntConst(7)),
                ),
            )],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(compile_single(func).is_ok(), "ne binop should compile");
}

#[test]
fn codegen_binop_gt() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Bool,
        param_count: 0,
        locals: vec![Local {
            id: LocalId(0),
            ty: Type::Bool,
            mutable: false,
        }],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    kodo_ast::BinOp::Gt,
                    Box::new(Value::IntConst(10)),
                    Box::new(Value::IntConst(3)),
                ),
            )],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(compile_single(func).is_ok(), "gt binop should compile");
}

#[test]
fn codegen_binop_le() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Bool,
        param_count: 0,
        locals: vec![Local {
            id: LocalId(0),
            ty: Type::Bool,
            mutable: false,
        }],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    kodo_ast::BinOp::Le,
                    Box::new(Value::IntConst(3)),
                    Box::new(Value::IntConst(5)),
                ),
            )],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(compile_single(func).is_ok(), "le binop should compile");
}

#[test]
fn codegen_binop_ge() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Bool,
        param_count: 0,
        locals: vec![Local {
            id: LocalId(0),
            ty: Type::Bool,
            mutable: false,
        }],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    kodo_ast::BinOp::Ge,
                    Box::new(Value::IntConst(7)),
                    Box::new(Value::IntConst(3)),
                ),
            )],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(compile_single(func).is_ok(), "ge binop should compile");
}

#[test]
fn codegen_binop_and() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Bool,
        param_count: 0,
        locals: vec![Local {
            id: LocalId(0),
            ty: Type::Bool,
            mutable: false,
        }],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    kodo_ast::BinOp::And,
                    Box::new(Value::BoolConst(true)),
                    Box::new(Value::BoolConst(false)),
                ),
            )],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(compile_single(func).is_ok(), "and binop should compile");
}

#[test]
fn codegen_binop_or() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Bool,
        param_count: 0,
        locals: vec![Local {
            id: LocalId(0),
            ty: Type::Bool,
            mutable: false,
        }],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    kodo_ast::BinOp::Or,
                    Box::new(Value::BoolConst(false)),
                    Box::new(Value::BoolConst(true)),
                ),
            )],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(compile_single(func).is_ok(), "or binop should compile");
}

// ---------------------------------------------------------------------------
// Assign with StringConst
// ---------------------------------------------------------------------------

#[test]
fn codegen_assign_string_const() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![Local {
            id: LocalId(0),
            ty: Type::String,
            mutable: false,
        }],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Assign(
                LocalId(0),
                Value::StringConst("hello, world".to_string()),
            )],
            terminator: Terminator::Return(Value::IntConst(0)),
        }],
        entry: BlockId(0),
    };
    assert!(
        compile_single(func).is_ok(),
        "assign string const should compile"
    );
}

#[test]
fn codegen_assign_empty_string() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![Local {
            id: LocalId(0),
            ty: Type::String,
            mutable: false,
        }],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Assign(
                LocalId(0),
                Value::StringConst(String::new()),
            )],
            terminator: Terminator::Return(Value::IntConst(0)),
        }],
        entry: BlockId(0),
    };
    assert!(
        compile_single(func).is_ok(),
        "assign empty string should compile"
    );
}

// ---------------------------------------------------------------------------
// Builtin call tests (print_int)
// ---------------------------------------------------------------------------

#[test]
fn codegen_call_builtin_print_int() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Unit,
        param_count: 0,
        locals: vec![Local {
            id: LocalId(0),
            ty: Type::Unit,
            mutable: false,
        }],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Call {
                dest: LocalId(0),
                callee: "print_int".to_string(),
                args: vec![Value::IntConst(42)],
            }],
            terminator: Terminator::Return(Value::Unit),
        }],
        entry: BlockId(0),
    };
    assert!(
        compile_single(func).is_ok(),
        "builtin print_int call should compile"
    );
}

#[test]
fn codegen_call_builtin_print_int_with_expression() {
    let func = MirFunction {
        name: "main".to_string(),
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
                ty: Type::Unit,
                mutable: false,
            },
        ],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Assign(
                    LocalId(0),
                    Value::BinOp(
                        kodo_ast::BinOp::Mul,
                        Box::new(Value::IntConst(6)),
                        Box::new(Value::IntConst(7)),
                    ),
                ),
                Instruction::Call {
                    dest: LocalId(1),
                    callee: "print_int".to_string(),
                    args: vec![Value::Local(LocalId(0))],
                },
            ],
            terminator: Terminator::Return(Value::Unit),
        }],
        entry: BlockId(0),
    };
    assert!(
        compile_single(func).is_ok(),
        "builtin print_int with computed arg should compile"
    );
}

// ---------------------------------------------------------------------------
// Compound arithmetic expressions
// ---------------------------------------------------------------------------

#[test]
fn codegen_nested_binop() {
    // (3 + 7) * 2
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
            instructions: vec![Instruction::Assign(
                LocalId(0),
                Value::BinOp(
                    kodo_ast::BinOp::Mul,
                    Box::new(Value::BinOp(
                        kodo_ast::BinOp::Add,
                        Box::new(Value::IntConst(3)),
                        Box::new(Value::IntConst(7)),
                    )),
                    Box::new(Value::IntConst(2)),
                ),
            )],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(compile_single(func).is_ok(), "nested binop should compile");
}

#[test]
fn codegen_binop_with_locals() {
    // a = 10; b = 3; c = a - b
    let func = MirFunction {
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
            Local {
                id: LocalId(2),
                ty: Type::Int,
                mutable: false,
            },
        ],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Assign(LocalId(0), Value::IntConst(10)),
                Instruction::Assign(LocalId(1), Value::IntConst(3)),
                Instruction::Assign(
                    LocalId(2),
                    Value::BinOp(
                        kodo_ast::BinOp::Sub,
                        Box::new(Value::Local(LocalId(0))),
                        Box::new(Value::Local(LocalId(1))),
                    ),
                ),
            ],
            terminator: Terminator::Return(Value::Local(LocalId(2))),
        }],
        entry: BlockId(0),
    };
    assert!(
        compile_single(func).is_ok(),
        "binop with locals should compile"
    );
}

// ---------------------------------------------------------------------------
// Unary operators with locals
// ---------------------------------------------------------------------------

#[test]
fn codegen_not_local() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Bool,
        param_count: 0,
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
        ],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Assign(LocalId(0), Value::BoolConst(true)),
                Instruction::Assign(LocalId(1), Value::Not(Box::new(Value::Local(LocalId(0))))),
            ],
            terminator: Terminator::Return(Value::Local(LocalId(1))),
        }],
        entry: BlockId(0),
    };
    assert!(compile_single(func).is_ok(), "not of local should compile");
}

#[test]
fn codegen_neg_local() {
    let func = MirFunction {
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
                Instruction::Assign(LocalId(0), Value::IntConst(99)),
                Instruction::Assign(LocalId(1), Value::Neg(Box::new(Value::Local(LocalId(0))))),
            ],
            terminator: Terminator::Return(Value::Local(LocalId(1))),
        }],
        entry: BlockId(0),
    };
    assert!(compile_single(func).is_ok(), "neg of local should compile");
}

// ---------------------------------------------------------------------------
// Advanced control flow tests
// ---------------------------------------------------------------------------

#[test]
fn codegen_diamond_cfg() {
    // Diamond pattern: entry -> {then, else} -> merge -> return
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![
            Local {
                id: LocalId(0),
                ty: Type::Bool,
                mutable: false,
            },
            Local {
                id: LocalId(1),
                ty: Type::Int,
                mutable: true,
            },
        ],
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction::Assign(LocalId(0), Value::BoolConst(true)),
                    Instruction::Assign(LocalId(1), Value::IntConst(0)),
                ],
                terminator: Terminator::Branch {
                    condition: Value::Local(LocalId(0)),
                    true_block: BlockId(1),
                    false_block: BlockId(2),
                },
            },
            BasicBlock {
                id: BlockId(1),
                instructions: vec![Instruction::Assign(LocalId(1), Value::IntConst(10))],
                terminator: Terminator::Goto(BlockId(3)),
            },
            BasicBlock {
                id: BlockId(2),
                instructions: vec![Instruction::Assign(LocalId(1), Value::IntConst(20))],
                terminator: Terminator::Goto(BlockId(3)),
            },
            BasicBlock {
                id: BlockId(3),
                instructions: vec![],
                terminator: Terminator::Return(Value::Local(LocalId(1))),
            },
        ],
        entry: BlockId(0),
    };
    assert!(compile_single(func).is_ok(), "diamond CFG should compile");
}

#[test]
fn codegen_nested_branches() {
    // Nested if-else: entry -> branch1 -> {branch2 -> {bb3, bb4}, bb5} -> merge
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
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
            // bb0: entry
            BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction::Assign(LocalId(0), Value::BoolConst(true)),
                    Instruction::Assign(LocalId(1), Value::BoolConst(false)),
                    Instruction::Assign(LocalId(2), Value::IntConst(0)),
                ],
                terminator: Terminator::Branch {
                    condition: Value::Local(LocalId(0)),
                    true_block: BlockId(1),
                    false_block: BlockId(4),
                },
            },
            // bb1: first true branch — nested check
            BasicBlock {
                id: BlockId(1),
                instructions: vec![],
                terminator: Terminator::Branch {
                    condition: Value::Local(LocalId(1)),
                    true_block: BlockId(2),
                    false_block: BlockId(3),
                },
            },
            // bb2: inner true
            BasicBlock {
                id: BlockId(2),
                instructions: vec![Instruction::Assign(LocalId(2), Value::IntConst(1))],
                terminator: Terminator::Goto(BlockId(5)),
            },
            // bb3: inner false
            BasicBlock {
                id: BlockId(3),
                instructions: vec![Instruction::Assign(LocalId(2), Value::IntConst(2))],
                terminator: Terminator::Goto(BlockId(5)),
            },
            // bb4: outer false
            BasicBlock {
                id: BlockId(4),
                instructions: vec![Instruction::Assign(LocalId(2), Value::IntConst(3))],
                terminator: Terminator::Goto(BlockId(5)),
            },
            // bb5: merge
            BasicBlock {
                id: BlockId(5),
                instructions: vec![],
                terminator: Terminator::Return(Value::Local(LocalId(2))),
            },
        ],
        entry: BlockId(0),
    };
    assert!(
        compile_single(func).is_ok(),
        "nested branches should compile"
    );
}

#[test]
fn codegen_chain_of_gotos() {
    // Linear chain: bb0 -> bb1 -> bb2 -> bb3 (return)
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
                        kodo_ast::BinOp::Add,
                        Box::new(Value::Local(LocalId(0))),
                        Box::new(Value::IntConst(10)),
                    ),
                )],
                terminator: Terminator::Goto(BlockId(2)),
            },
            BasicBlock {
                id: BlockId(2),
                instructions: vec![Instruction::Assign(
                    LocalId(0),
                    Value::BinOp(
                        kodo_ast::BinOp::Mul,
                        Box::new(Value::Local(LocalId(0))),
                        Box::new(Value::IntConst(2)),
                    ),
                )],
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
    assert!(
        compile_single(func).is_ok(),
        "chain of gotos should compile"
    );
}

#[test]
fn codegen_branch_on_comparison_result() {
    // Compute comparison, then branch on it
    let func = MirFunction {
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
                mutable: true,
            },
        ],
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction::Assign(LocalId(0), Value::IntConst(5)),
                    Instruction::Assign(LocalId(1), Value::IntConst(0)),
                ],
                terminator: Terminator::Branch {
                    condition: Value::BinOp(
                        kodo_ast::BinOp::Gt,
                        Box::new(Value::Local(LocalId(0))),
                        Box::new(Value::IntConst(3)),
                    ),
                    true_block: BlockId(1),
                    false_block: BlockId(2),
                },
            },
            BasicBlock {
                id: BlockId(1),
                instructions: vec![Instruction::Assign(LocalId(1), Value::IntConst(100))],
                terminator: Terminator::Goto(BlockId(3)),
            },
            BasicBlock {
                id: BlockId(2),
                instructions: vec![Instruction::Assign(LocalId(1), Value::IntConst(200))],
                terminator: Terminator::Goto(BlockId(3)),
            },
            BasicBlock {
                id: BlockId(3),
                instructions: vec![],
                terminator: Terminator::Return(Value::Local(LocalId(1))),
            },
        ],
        entry: BlockId(0),
    };
    assert!(
        compile_single(func).is_ok(),
        "branch on comparison result should compile"
    );
}

// ---------------------------------------------------------------------------
// Return from expression
// ---------------------------------------------------------------------------

#[test]
fn codegen_return_binop_directly() {
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
                Box::new(Value::IntConst(20)),
                Box::new(Value::IntConst(22)),
            )),
        }],
        entry: BlockId(0),
    };
    assert!(
        compile_single(func).is_ok(),
        "return binop directly should compile"
    );
}

// ---------------------------------------------------------------------------
// Multiple assignments in a single block
// ---------------------------------------------------------------------------

#[test]
fn codegen_multiple_assigns_single_block() {
    let func = MirFunction {
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
                ty: Type::Bool,
                mutable: false,
            },
            Local {
                id: LocalId(2),
                ty: Type::Int,
                mutable: false,
            },
        ],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Assign(LocalId(0), Value::IntConst(42)),
                Instruction::Assign(LocalId(1), Value::BoolConst(true)),
                Instruction::Assign(
                    LocalId(2),
                    Value::BinOp(
                        kodo_ast::BinOp::Add,
                        Box::new(Value::Local(LocalId(0))),
                        Box::new(Value::IntConst(8)),
                    ),
                ),
            ],
            terminator: Terminator::Return(Value::Local(LocalId(2))),
        }],
        entry: BlockId(0),
    };
    assert!(
        compile_single(func).is_ok(),
        "multiple assigns in single block should compile"
    );
}

// ---------------------------------------------------------------------------
// Mutable variable re-assignment
// ---------------------------------------------------------------------------

#[test]
fn codegen_mutable_reassignment() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![Local {
            id: LocalId(0),
            ty: Type::Int,
            mutable: true,
        }],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Assign(LocalId(0), Value::IntConst(1)),
                Instruction::Assign(LocalId(0), Value::IntConst(2)),
                Instruction::Assign(LocalId(0), Value::IntConst(3)),
            ],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(
        compile_single(func).is_ok(),
        "mutable reassignment should compile"
    );
}

// ---------------------------------------------------------------------------
// Call with zero args
// ---------------------------------------------------------------------------

#[test]
fn codegen_call_zero_args() {
    let callee = MirFunction {
        name: "get_value".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: Terminator::Return(Value::IntConst(99)),
        }],
        entry: BlockId(0),
    };
    let main_fn = MirFunction {
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
                callee: "get_value".to_string(),
                args: vec![],
            }],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(
        compile_pair(main_fn, callee).is_ok(),
        "call with zero args should compile"
    );
}

// ---------------------------------------------------------------------------
// Negation and Not tests
// ---------------------------------------------------------------------------

#[test]
fn codegen_not_value() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Bool,
        param_count: 0,
        locals: vec![Local {
            id: LocalId(0),
            ty: Type::Bool,
            mutable: false,
        }],
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
    assert!(compile_single(func).is_ok(), "not value should compile");
}

#[test]
fn codegen_neg_value() {
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
            instructions: vec![Instruction::Assign(
                LocalId(0),
                Value::Neg(Box::new(Value::IntConst(42))),
            )],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(compile_single(func).is_ok(), "neg value should compile");
}

// ---------------------------------------------------------------------------
// IncRef / DecRef tests
// ---------------------------------------------------------------------------

#[test]
fn codegen_incref_decref() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Unit,
        param_count: 0,
        locals: vec![Local {
            id: LocalId(0),
            ty: Type::Int,
            mutable: false,
        }],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Assign(LocalId(0), Value::IntConst(0)),
                Instruction::IncRef(LocalId(0)),
                Instruction::DecRef(LocalId(0)),
            ],
            terminator: Terminator::Return(Value::Unit),
        }],
        entry: BlockId(0),
    };
    assert!(compile_single(func).is_ok(), "incref/decref should compile");
}

// ---------------------------------------------------------------------------
// FuncRef tests
// ---------------------------------------------------------------------------

#[test]
fn codegen_func_ref() {
    let target = MirFunction {
        name: "target".to_string(),
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
    let main_fn = MirFunction {
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
            instructions: vec![Instruction::Assign(
                LocalId(0),
                Value::FuncRef("target".to_string()),
            )],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(
        compile_pair(main_fn, target).is_ok(),
        "func ref should compile"
    );
}

// ---------------------------------------------------------------------------
// Multiple functions compilation
// ---------------------------------------------------------------------------

#[test]
fn codegen_multiple_functions() {
    let f1 = MirFunction {
        name: "helper".to_string(),
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
            terminator: Terminator::Return(Value::BinOp(
                kodo_ast::BinOp::Add,
                Box::new(Value::Local(LocalId(0))),
                Box::new(Value::IntConst(1)),
            )),
        }],
        entry: BlockId(0),
    };
    let f2 = MirFunction {
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
                    callee: "helper".to_string(),
                    args: vec![Value::IntConst(10)],
                },
                Instruction::Call {
                    dest: LocalId(1),
                    callee: "helper".to_string(),
                    args: vec![Value::Local(LocalId(0))],
                },
            ],
            terminator: Terminator::Return(Value::Local(LocalId(1))),
        }],
        entry: BlockId(0),
    };
    let result = compile_module(&[f2, f1], &CodegenOptions::default(), None);
    assert!(
        result.is_ok(),
        "multiple functions with calls should compile: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// Return value types
// ---------------------------------------------------------------------------

#[test]
fn codegen_return_unit() {
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
    assert!(compile_single(func).is_ok(), "return unit should compile");
}

#[test]
fn codegen_return_bool() {
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
    assert!(compile_single(func).is_ok(), "return bool should compile");
}

// ---------------------------------------------------------------------------
// Unreachable terminator
// ---------------------------------------------------------------------------

#[test]
fn codegen_unreachable() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Unit,
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
                terminator: Terminator::Return(Value::Unit),
            },
            BasicBlock {
                id: BlockId(2),
                instructions: vec![],
                terminator: Terminator::Unreachable,
            },
        ],
        entry: BlockId(0),
    };
    assert!(
        compile_single(func).is_ok(),
        "unreachable block should compile"
    );
}
