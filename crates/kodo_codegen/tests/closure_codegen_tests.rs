//! Integration tests for closure codegen — verifying that lambda-lifted
//! closures compile through the Cranelift backend as regular functions.

use kodo_codegen::{compile_module, CodegenOptions};
use kodo_mir::{BasicBlock, BlockId, Instruction, Local, LocalId, MirFunction, Terminator, Value};
use kodo_types::Type;

#[test]
fn compile_closure_no_captures() {
    // A lifted closure: __closure_0(x: Int) -> Int { return x + 1 }
    // main calls __closure_0(41) via direct Call.
    let closure_fn = MirFunction {
        name: "__closure_0".to_string(),
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
                callee: "__closure_0".to_string(),
                args: vec![Value::IntConst(41)],
            }],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    let result = compile_module(&[main_fn, closure_fn], &CodegenOptions::default(), None);
    assert!(
        result.is_ok(),
        "closure without captures should compile: {result:?}"
    );
}

#[test]
fn compile_closure_with_captures() {
    // __closure_0(captured_a: Int, x: Int) -> Int { return captured_a + x }
    let closure_fn = MirFunction {
        name: "__closure_0".to_string(),
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
                Instruction::Assign(LocalId(0), Value::IntConst(10)),
                Instruction::Call {
                    dest: LocalId(1),
                    callee: "__closure_0".to_string(),
                    args: vec![Value::Local(LocalId(0)), Value::IntConst(5)],
                },
            ],
            terminator: Terminator::Return(Value::Local(LocalId(1))),
        }],
        entry: BlockId(0),
    };
    let result = compile_module(&[main_fn, closure_fn], &CodegenOptions::default(), None);
    assert!(
        result.is_ok(),
        "closure with captures should compile: {result:?}"
    );
}

#[test]
fn compile_closure_zero_params() {
    // __closure_0() -> Int { return 42 }
    let closure_fn = MirFunction {
        name: "__closure_0".to_string(),
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
            instructions: vec![Instruction::Call {
                dest: LocalId(0),
                callee: "__closure_0".to_string(),
                args: vec![],
            }],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    let result = compile_module(&[main_fn, closure_fn], &CodegenOptions::default(), None);
    assert!(
        result.is_ok(),
        "zero-param closure should compile: {result:?}"
    );
}
