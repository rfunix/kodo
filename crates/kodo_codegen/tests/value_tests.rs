//! Tests for MIR `Value` variant translation — verifying that each value kind
//! can be compiled through Cranelift to a valid object file.

use std::collections::HashMap;

use kodo_codegen::{
    compile_module, compile_module_with_structs, compile_module_with_types, CodegenOptions,
};
use kodo_mir::{BasicBlock, BlockId, Instruction, Local, LocalId, MirFunction, Terminator, Value};
use kodo_types::Type;

fn opts() -> CodegenOptions {
    CodegenOptions::default()
}

fn single_block(
    name: &str,
    ret_ty: Type,
    locals: Vec<Local>,
    instrs: Vec<Instruction>,
    terminator: Terminator,
) -> MirFunction {
    MirFunction {
        name: name.to_string(),
        return_type: ret_ty,
        param_count: 0,
        locals,
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: instrs,
            terminator,
        }],
        entry: BlockId(0),
    }
}

// ---------------------------------------------------------------------------
// Scalar constants
// ---------------------------------------------------------------------------

#[test]
fn value_int_const_zero() {
    let func = single_block(
        "main",
        Type::Int,
        vec![Local {
            id: LocalId(0),
            ty: Type::Int,
            mutable: false,
        }],
        vec![Instruction::Assign(LocalId(0), Value::IntConst(0))],
        Terminator::Return(Value::Local(LocalId(0))),
    );
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

#[test]
fn value_int_const_max() {
    let func = single_block(
        "main",
        Type::Int,
        vec![Local {
            id: LocalId(0),
            ty: Type::Int,
            mutable: false,
        }],
        vec![Instruction::Assign(LocalId(0), Value::IntConst(i64::MAX))],
        Terminator::Return(Value::Local(LocalId(0))),
    );
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

#[test]
fn value_int_const_negative() {
    let func = single_block(
        "main",
        Type::Int,
        vec![Local {
            id: LocalId(0),
            ty: Type::Int,
            mutable: false,
        }],
        vec![Instruction::Assign(LocalId(0), Value::IntConst(-1))],
        Terminator::Return(Value::Local(LocalId(0))),
    );
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

#[test]
fn value_bool_const_true_and_false() {
    for b in [true, false] {
        let func = single_block(
            "main",
            Type::Bool,
            vec![Local {
                id: LocalId(0),
                ty: Type::Bool,
                mutable: false,
            }],
            vec![Instruction::Assign(LocalId(0), Value::BoolConst(b))],
            Terminator::Return(Value::Local(LocalId(0))),
        );
        assert!(compile_module(&[func], &opts(), None).is_ok());
    }
}

#[test]
fn value_string_const_nonempty() {
    let func = single_block(
        "main",
        Type::Int,
        vec![Local {
            id: LocalId(0),
            ty: Type::String,
            mutable: false,
        }],
        vec![Instruction::Assign(
            LocalId(0),
            Value::StringConst("kodo!".to_string()),
        )],
        Terminator::Return(Value::IntConst(0)),
    );
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

#[test]
fn value_string_const_empty() {
    let func = single_block(
        "main",
        Type::Int,
        vec![Local {
            id: LocalId(0),
            ty: Type::String,
            mutable: false,
        }],
        vec![Instruction::Assign(
            LocalId(0),
            Value::StringConst(String::new()),
        )],
        Terminator::Return(Value::IntConst(0)),
    );
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

#[test]
fn value_string_const_unicode() {
    let func = single_block(
        "main",
        Type::Int,
        vec![Local {
            id: LocalId(0),
            ty: Type::String,
            mutable: false,
        }],
        vec![Instruction::Assign(
            LocalId(0),
            Value::StringConst("コード 🦀".to_string()),
        )],
        Terminator::Return(Value::IntConst(0)),
    );
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

#[test]
fn value_unit() {
    let func = single_block(
        "main",
        Type::Unit,
        vec![Local {
            id: LocalId(0),
            ty: Type::Unit,
            mutable: false,
        }],
        vec![Instruction::Assign(LocalId(0), Value::Unit)],
        Terminator::Return(Value::Unit),
    );
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

// ---------------------------------------------------------------------------
// Local references
// ---------------------------------------------------------------------------

#[test]
fn value_local_int() {
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
                Instruction::Assign(LocalId(0), Value::IntConst(77)),
                Instruction::Assign(LocalId(1), Value::Local(LocalId(0))),
            ],
            terminator: Terminator::Return(Value::Local(LocalId(1))),
        }],
        entry: BlockId(0),
    };
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

#[test]
fn value_local_bool() {
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
                Instruction::Assign(LocalId(0), Value::BoolConst(false)),
                Instruction::Assign(LocalId(1), Value::Local(LocalId(0))),
            ],
            terminator: Terminator::Return(Value::Local(LocalId(1))),
        }],
        entry: BlockId(0),
    };
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

// ---------------------------------------------------------------------------
// Unary operators
// ---------------------------------------------------------------------------

#[test]
fn value_not_bool_const() {
    let func = single_block(
        "main",
        Type::Bool,
        vec![Local {
            id: LocalId(0),
            ty: Type::Bool,
            mutable: false,
        }],
        vec![Instruction::Assign(
            LocalId(0),
            Value::Not(Box::new(Value::BoolConst(true))),
        )],
        Terminator::Return(Value::Local(LocalId(0))),
    );
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

#[test]
fn value_not_nested() {
    let func = single_block(
        "main",
        Type::Bool,
        vec![Local {
            id: LocalId(0),
            ty: Type::Bool,
            mutable: false,
        }],
        vec![Instruction::Assign(
            LocalId(0),
            Value::Not(Box::new(Value::Not(Box::new(Value::BoolConst(false))))),
        )],
        Terminator::Return(Value::Local(LocalId(0))),
    );
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

#[test]
fn value_neg_int_const() {
    let func = single_block(
        "main",
        Type::Int,
        vec![Local {
            id: LocalId(0),
            ty: Type::Int,
            mutable: false,
        }],
        vec![Instruction::Assign(
            LocalId(0),
            Value::Neg(Box::new(Value::IntConst(5))),
        )],
        Terminator::Return(Value::Local(LocalId(0))),
    );
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

#[test]
fn value_neg_local() {
    let func = MirFunction {
        name: "main".to_string(),
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
                ty: Type::Int,
                mutable: false,
            },
        ],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Assign(
                LocalId(1),
                Value::Neg(Box::new(Value::Local(LocalId(0)))),
            )],
            terminator: Terminator::Return(Value::Local(LocalId(1))),
        }],
        entry: BlockId(0),
    };
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

// ---------------------------------------------------------------------------
// BinOp with mixed value types
// ---------------------------------------------------------------------------

#[test]
fn value_binop_mixed_const_local() {
    let func = MirFunction {
        name: "main".to_string(),
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
                ty: Type::Int,
                mutable: false,
            },
        ],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![Instruction::Assign(
                LocalId(1),
                Value::BinOp(
                    kodo_ast::BinOp::Add,
                    Box::new(Value::Local(LocalId(0))),
                    Box::new(Value::IntConst(100)),
                ),
            )],
            terminator: Terminator::Return(Value::Local(LocalId(1))),
        }],
        entry: BlockId(0),
    };
    assert!(compile_module(&[func], &opts(), None).is_ok());
}

// ---------------------------------------------------------------------------
// FuncRef
// ---------------------------------------------------------------------------

#[test]
fn value_funcref() {
    // Store a function reference and call it indirectly.
    let helper = MirFunction {
        name: "noop".to_string(),
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
            instructions: vec![Instruction::IndirectCall {
                dest: LocalId(0),
                callee: Value::FuncRef("noop".to_string()),
                args: vec![],
                return_type: Type::Int,
                param_types: vec![],
            }],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };

    assert!(compile_module(&[main_fn, helper], &opts(), None).is_ok());
}

// ---------------------------------------------------------------------------
// Enum values
// ---------------------------------------------------------------------------

#[test]
fn value_enum_variant_zero_arity() {
    let mut enum_defs = HashMap::new();
    enum_defs.insert(
        "Dir".to_string(),
        vec![("North".to_string(), vec![]), ("South".to_string(), vec![])],
    );

    let func = single_block(
        "main",
        Type::Int,
        vec![
            Local {
                id: LocalId(0),
                ty: Type::Enum("Dir".to_string()),
                mutable: false,
            },
            Local {
                id: LocalId(1),
                ty: Type::Int,
                mutable: false,
            },
        ],
        vec![
            Instruction::Assign(
                LocalId(0),
                Value::EnumVariant {
                    enum_name: "Dir".to_string(),
                    variant: "North".to_string(),
                    discriminant: 0,
                    args: vec![],
                },
            ),
            Instruction::Assign(
                LocalId(1),
                Value::EnumDiscriminant(Box::new(Value::Local(LocalId(0)))),
            ),
        ],
        Terminator::Return(Value::Local(LocalId(1))),
    );

    let result = compile_module_with_types(&[func], &HashMap::new(), &enum_defs, &opts(), None);
    assert!(
        result.is_ok(),
        "zero-arity enum variant should compile: {result:?}"
    );
}

#[test]
fn value_enum_discriminant_of_variant() {
    let mut enum_defs = HashMap::new();
    enum_defs.insert(
        "Choice".to_string(),
        vec![
            ("A".to_string(), vec![]),
            ("B".to_string(), vec![]),
            ("C".to_string(), vec![]),
        ],
    );

    let func = single_block(
        "main",
        Type::Int,
        vec![
            Local {
                id: LocalId(0),
                ty: Type::Enum("Choice".to_string()),
                mutable: false,
            },
            Local {
                id: LocalId(1),
                ty: Type::Int,
                mutable: false,
            },
        ],
        vec![
            Instruction::Assign(
                LocalId(0),
                Value::EnumVariant {
                    enum_name: "Choice".to_string(),
                    variant: "C".to_string(),
                    discriminant: 2,
                    args: vec![],
                },
            ),
            Instruction::Assign(
                LocalId(1),
                Value::EnumDiscriminant(Box::new(Value::Local(LocalId(0)))),
            ),
        ],
        Terminator::Return(Value::Local(LocalId(1))),
    );

    let result = compile_module_with_types(&[func], &HashMap::new(), &enum_defs, &opts(), None);
    assert!(
        result.is_ok(),
        "enum discriminant read should compile: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// Struct values
// ---------------------------------------------------------------------------

#[test]
fn value_struct_lit_two_fields() {
    let mut struct_defs = HashMap::new();
    struct_defs.insert(
        "Vec2".to_string(),
        vec![("x".to_string(), Type::Int), ("y".to_string(), Type::Int)],
    );

    let func = single_block(
        "main",
        Type::Int,
        vec![
            Local {
                id: LocalId(0),
                ty: Type::Struct("Vec2".to_string()),
                mutable: false,
            },
            Local {
                id: LocalId(1),
                ty: Type::Int,
                mutable: false,
            },
        ],
        vec![
            Instruction::Assign(
                LocalId(0),
                Value::StructLit {
                    name: "Vec2".to_string(),
                    fields: vec![
                        ("x".to_string(), Value::IntConst(3)),
                        ("y".to_string(), Value::IntConst(4)),
                    ],
                },
            ),
            Instruction::Assign(
                LocalId(1),
                Value::FieldGet {
                    object: Box::new(Value::Local(LocalId(0))),
                    field: "y".to_string(),
                    struct_name: "Vec2".to_string(),
                },
            ),
        ],
        Terminator::Return(Value::Local(LocalId(1))),
    );

    let result = compile_module_with_structs(&[func], &struct_defs, &opts(), None);
    assert!(
        result.is_ok(),
        "two-field struct value should compile: {result:?}"
    );
}

#[test]
fn value_field_get_second_field() {
    let mut struct_defs = HashMap::new();
    struct_defs.insert(
        "Pair".to_string(),
        vec![
            ("first".to_string(), Type::Int),
            ("second".to_string(), Type::Bool),
        ],
    );

    let func = single_block(
        "main",
        Type::Bool,
        vec![
            Local {
                id: LocalId(0),
                ty: Type::Struct("Pair".to_string()),
                mutable: false,
            },
            Local {
                id: LocalId(1),
                ty: Type::Bool,
                mutable: false,
            },
        ],
        vec![
            Instruction::Assign(
                LocalId(0),
                Value::StructLit {
                    name: "Pair".to_string(),
                    fields: vec![
                        ("first".to_string(), Value::IntConst(1)),
                        ("second".to_string(), Value::BoolConst(true)),
                    ],
                },
            ),
            Instruction::Assign(
                LocalId(1),
                Value::FieldGet {
                    object: Box::new(Value::Local(LocalId(0))),
                    field: "second".to_string(),
                    struct_name: "Pair".to_string(),
                },
            ),
        ],
        Terminator::Return(Value::Local(LocalId(1))),
    );

    let result = compile_module_with_structs(&[func], &struct_defs, &opts(), None);
    assert!(
        result.is_ok(),
        "field get for second field should compile: {result:?}"
    );
}
