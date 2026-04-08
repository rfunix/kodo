//! Tests for runtime builtin function calls via `Instruction::Call`.
//!
//! Each test builds a MIR function that calls one or more builtins and verifies
//! that the codegen backend can compile the call without errors. These tests
//! confirm that every builtin family (IO, string, list, map, math, conversion)
//! is correctly declared and dispatch is emitted.

use kodo_codegen::{compile_module, CodegenOptions};
use kodo_mir::{BasicBlock, BlockId, Instruction, Local, LocalId, MirFunction, Terminator, Value};
use kodo_types::Type;

fn opts() -> CodegenOptions {
    CodegenOptions::default()
}

fn compile(func: MirFunction) -> Result<Vec<u8>, String> {
    compile_module(&[func], &opts(), None).map_err(|e| format!("{e}"))
}

// ---------------------------------------------------------------------------
// IO builtins
// ---------------------------------------------------------------------------

#[test]
fn builtin_println_string() {
    let func = MirFunction {
        name: "main".to_string(),
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
                ty: Type::Unit,
                mutable: false,
            },
        ],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Assign(LocalId(0), Value::StringConst("hello".to_string())),
                Instruction::Call {
                    dest: LocalId(1),
                    callee: "println".to_string(),
                    args: vec![Value::Local(LocalId(0))],
                },
            ],
            terminator: Terminator::Return(Value::Unit),
        }],
        entry: BlockId(0),
    };
    assert!(compile(func).is_ok(), "println builtin should compile");
}

#[test]
fn builtin_print_int() {
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
                args: vec![Value::IntConst(123)],
            }],
            terminator: Terminator::Return(Value::Unit),
        }],
        entry: BlockId(0),
    };
    assert!(compile(func).is_ok(), "print_int builtin should compile");
}

#[test]
fn builtin_print_int_bool_values() {
    // Kōdo uses print_int(1)/print_int(0) for booleans; no separate print_bool exists.
    // Verify print_int with 0/1 as bool representation compiles.
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Unit,
        param_count: 0,
        locals: vec![
            Local {
                id: LocalId(0),
                ty: Type::Unit,
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
                Instruction::Call {
                    dest: LocalId(0),
                    callee: "print_int".to_string(),
                    args: vec![Value::IntConst(1)],
                },
                Instruction::Call {
                    dest: LocalId(1),
                    callee: "print_int".to_string(),
                    args: vec![Value::IntConst(0)],
                },
            ],
            terminator: Terminator::Return(Value::Unit),
        }],
        entry: BlockId(0),
    };
    assert!(
        compile(func).is_ok(),
        "print_int for bool values should compile"
    );
}

// ---------------------------------------------------------------------------
// String builtins
// ---------------------------------------------------------------------------

#[test]
fn builtin_string_length() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![
            Local {
                id: LocalId(0),
                ty: Type::String,
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
                Instruction::Assign(LocalId(0), Value::StringConst("hello".to_string())),
                Instruction::Call {
                    dest: LocalId(1),
                    callee: "String_length".to_string(),
                    args: vec![Value::Local(LocalId(0))],
                },
            ],
            terminator: Terminator::Return(Value::Local(LocalId(1))),
        }],
        entry: BlockId(0),
    };
    assert!(compile(func).is_ok(), "String_length should compile");
}

#[test]
fn builtin_string_contains() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Bool,
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
            Local {
                id: LocalId(2),
                ty: Type::Bool,
                mutable: false,
            },
        ],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Assign(LocalId(0), Value::StringConst("hello world".to_string())),
                Instruction::Assign(LocalId(1), Value::StringConst("world".to_string())),
                Instruction::Call {
                    dest: LocalId(2),
                    callee: "String_contains".to_string(),
                    args: vec![Value::Local(LocalId(0)), Value::Local(LocalId(1))],
                },
            ],
            terminator: Terminator::Return(Value::Local(LocalId(2))),
        }],
        entry: BlockId(0),
    };
    assert!(compile(func).is_ok(), "String_contains should compile");
}

#[test]
fn builtin_string_starts_with() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Bool,
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
            Local {
                id: LocalId(2),
                ty: Type::Bool,
                mutable: false,
            },
        ],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Assign(LocalId(0), Value::StringConst("foobar".to_string())),
                Instruction::Assign(LocalId(1), Value::StringConst("foo".to_string())),
                Instruction::Call {
                    dest: LocalId(2),
                    callee: "String_starts_with".to_string(),
                    args: vec![Value::Local(LocalId(0)), Value::Local(LocalId(1))],
                },
            ],
            terminator: Terminator::Return(Value::Local(LocalId(2))),
        }],
        entry: BlockId(0),
    };
    assert!(compile(func).is_ok(), "String_starts_with should compile");
}

#[test]
fn builtin_string_ends_with() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Bool,
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
            Local {
                id: LocalId(2),
                ty: Type::Bool,
                mutable: false,
            },
        ],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Assign(LocalId(0), Value::StringConst("foobar".to_string())),
                Instruction::Assign(LocalId(1), Value::StringConst("bar".to_string())),
                Instruction::Call {
                    dest: LocalId(2),
                    callee: "String_ends_with".to_string(),
                    args: vec![Value::Local(LocalId(0)), Value::Local(LocalId(1))],
                },
            ],
            terminator: Terminator::Return(Value::Local(LocalId(2))),
        }],
        entry: BlockId(0),
    };
    assert!(compile(func).is_ok(), "String_ends_with should compile");
}

#[test]
fn builtin_string_concat() {
    // Kōdo's `+` operator on strings dispatches to String_concat
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
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
            Local {
                id: LocalId(2),
                ty: Type::String,
                mutable: false,
            },
            Local {
                id: LocalId(3),
                ty: Type::Unit,
                mutable: false,
            },
        ],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Assign(LocalId(0), Value::StringConst("hello".to_string())),
                Instruction::Assign(LocalId(1), Value::StringConst(" world".to_string())),
                Instruction::Call {
                    dest: LocalId(2),
                    callee: "String_concat".to_string(),
                    args: vec![Value::Local(LocalId(0)), Value::Local(LocalId(1))],
                },
                Instruction::Call {
                    dest: LocalId(3),
                    callee: "println".to_string(),
                    args: vec![Value::Local(LocalId(2))],
                },
            ],
            terminator: Terminator::Return(Value::IntConst(0)),
        }],
        entry: BlockId(0),
    };
    assert!(compile(func).is_ok(), "String_concat should compile");
}

#[test]
fn builtin_string_to_upper() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
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
            instructions: vec![
                Instruction::Assign(LocalId(0), Value::StringConst("kodo".to_string())),
                Instruction::Call {
                    dest: LocalId(1),
                    callee: "String_to_upper".to_string(),
                    args: vec![Value::Local(LocalId(0))],
                },
            ],
            terminator: Terminator::Return(Value::IntConst(0)),
        }],
        entry: BlockId(0),
    };
    assert!(compile(func).is_ok(), "String_to_upper should compile");
}

#[test]
fn builtin_string_to_lower() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
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
            instructions: vec![
                Instruction::Assign(LocalId(0), Value::StringConst("KODO".to_string())),
                Instruction::Call {
                    dest: LocalId(1),
                    callee: "String_to_lower".to_string(),
                    args: vec![Value::Local(LocalId(0))],
                },
            ],
            terminator: Terminator::Return(Value::IntConst(0)),
        }],
        entry: BlockId(0),
    };
    assert!(compile(func).is_ok(), "String_to_lower should compile");
}

#[test]
fn builtin_string_trim() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
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
            instructions: vec![
                Instruction::Assign(LocalId(0), Value::StringConst("  kodo  ".to_string())),
                Instruction::Call {
                    dest: LocalId(1),
                    callee: "String_trim".to_string(),
                    args: vec![Value::Local(LocalId(0))],
                },
            ],
            terminator: Terminator::Return(Value::IntConst(0)),
        }],
        entry: BlockId(0),
    };
    assert!(compile(func).is_ok(), "String_trim should compile");
}

// ---------------------------------------------------------------------------
// Type conversion builtins
// ---------------------------------------------------------------------------

#[test]
fn builtin_int_to_string() {
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
            instructions: vec![Instruction::Call {
                dest: LocalId(0),
                callee: "Int_to_string".to_string(),
                args: vec![Value::IntConst(42)],
            }],
            terminator: Terminator::Return(Value::IntConst(0)),
        }],
        entry: BlockId(0),
    };
    assert!(compile(func).is_ok(), "Int_to_string should compile");
}

#[test]
fn builtin_bool_to_string() {
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
            instructions: vec![Instruction::Call {
                dest: LocalId(0),
                callee: "Bool_to_string".to_string(),
                args: vec![Value::BoolConst(false)],
            }],
            terminator: Terminator::Return(Value::IntConst(0)),
        }],
        entry: BlockId(0),
    };
    assert!(compile(func).is_ok(), "Bool_to_string should compile");
}

// ---------------------------------------------------------------------------
// List builtins
// ---------------------------------------------------------------------------

#[test]
fn builtin_list_new_and_push() {
    // let nums = list_new(); list_push(nums, 1); list_push(nums, 2)
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Unit,
        param_count: 0,
        locals: vec![
            Local {
                id: LocalId(0),
                ty: Type::Generic("List".to_string(), vec![Type::Int]),
                mutable: false,
            },
            Local {
                id: LocalId(1),
                ty: Type::Unit,
                mutable: false,
            },
            Local {
                id: LocalId(2),
                ty: Type::Unit,
                mutable: false,
            },
        ],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Call {
                    dest: LocalId(0),
                    callee: "list_new".to_string(),
                    args: vec![],
                },
                Instruction::Call {
                    dest: LocalId(1),
                    callee: "list_push".to_string(),
                    args: vec![Value::Local(LocalId(0)), Value::IntConst(1)],
                },
                Instruction::Call {
                    dest: LocalId(2),
                    callee: "list_push".to_string(),
                    args: vec![Value::Local(LocalId(0)), Value::IntConst(2)],
                },
            ],
            terminator: Terminator::Return(Value::Unit),
        }],
        entry: BlockId(0),
    };
    assert!(compile(func).is_ok(), "list_new + list_push should compile");
}

#[test]
fn builtin_list_length() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![
            Local {
                id: LocalId(0),
                ty: Type::Generic("List".to_string(), vec![Type::Int]),
                mutable: false,
            },
            Local {
                id: LocalId(1),
                ty: Type::Unit,
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
                Instruction::Call {
                    dest: LocalId(0),
                    callee: "list_new".to_string(),
                    args: vec![],
                },
                Instruction::Call {
                    dest: LocalId(1),
                    callee: "list_push".to_string(),
                    args: vec![Value::Local(LocalId(0)), Value::IntConst(42)],
                },
                Instruction::Call {
                    dest: LocalId(2),
                    callee: "list_length".to_string(),
                    args: vec![Value::Local(LocalId(0))],
                },
            ],
            terminator: Terminator::Return(Value::Local(LocalId(2))),
        }],
        entry: BlockId(0),
    };
    assert!(compile(func).is_ok(), "list_length should compile");
}

#[test]
fn builtin_list_get() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![
            Local {
                id: LocalId(0),
                ty: Type::Generic("List".to_string(), vec![Type::Int]),
                mutable: false,
            },
            Local {
                id: LocalId(1),
                ty: Type::Unit,
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
                Instruction::Call {
                    dest: LocalId(0),
                    callee: "list_new".to_string(),
                    args: vec![],
                },
                Instruction::Call {
                    dest: LocalId(1),
                    callee: "list_push".to_string(),
                    args: vec![Value::Local(LocalId(0)), Value::IntConst(77)],
                },
                Instruction::Call {
                    dest: LocalId(2),
                    callee: "list_get".to_string(),
                    args: vec![Value::Local(LocalId(0)), Value::IntConst(0)],
                },
            ],
            terminator: Terminator::Return(Value::Local(LocalId(2))),
        }],
        entry: BlockId(0),
    };
    assert!(compile(func).is_ok(), "list_get should compile");
}

#[test]
fn builtin_list_contains() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Bool,
        param_count: 0,
        locals: vec![
            Local {
                id: LocalId(0),
                ty: Type::Generic("List".to_string(), vec![Type::Int]),
                mutable: false,
            },
            Local {
                id: LocalId(1),
                ty: Type::Unit,
                mutable: false,
            },
            Local {
                id: LocalId(2),
                ty: Type::Bool,
                mutable: false,
            },
        ],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Call {
                    dest: LocalId(0),
                    callee: "list_new".to_string(),
                    args: vec![],
                },
                Instruction::Call {
                    dest: LocalId(1),
                    callee: "list_push".to_string(),
                    args: vec![Value::Local(LocalId(0)), Value::IntConst(5)],
                },
                Instruction::Call {
                    dest: LocalId(2),
                    callee: "list_contains".to_string(),
                    args: vec![Value::Local(LocalId(0)), Value::IntConst(5)],
                },
            ],
            terminator: Terminator::Return(Value::Local(LocalId(2))),
        }],
        entry: BlockId(0),
    };
    assert!(compile(func).is_ok(), "list_contains should compile");
}

// ---------------------------------------------------------------------------
// Map builtins
// ---------------------------------------------------------------------------

#[test]
fn builtin_map_new_and_insert() {
    // map_new() -> Map<Int, Int>; map_insert(m, 1, 100)
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Unit,
        param_count: 0,
        locals: vec![
            Local {
                id: LocalId(0),
                ty: Type::Generic("Map".to_string(), vec![Type::Int, Type::Int]),
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
                Instruction::Call {
                    dest: LocalId(0),
                    callee: "map_new".to_string(),
                    args: vec![],
                },
                Instruction::Call {
                    dest: LocalId(1),
                    callee: "map_insert".to_string(),
                    args: vec![
                        Value::Local(LocalId(0)),
                        Value::IntConst(1),
                        Value::IntConst(100),
                    ],
                },
            ],
            terminator: Terminator::Return(Value::Unit),
        }],
        entry: BlockId(0),
    };
    assert!(compile(func).is_ok(), "map_new + map_insert should compile");
}

#[test]
fn builtin_map_length() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![
            Local {
                id: LocalId(0),
                ty: Type::Generic("Map".to_string(), vec![Type::Int, Type::Int]),
                mutable: false,
            },
            Local {
                id: LocalId(1),
                ty: Type::Unit,
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
                Instruction::Call {
                    dest: LocalId(0),
                    callee: "map_new".to_string(),
                    args: vec![],
                },
                Instruction::Call {
                    dest: LocalId(1),
                    callee: "map_insert".to_string(),
                    args: vec![
                        Value::Local(LocalId(0)),
                        Value::IntConst(1),
                        Value::IntConst(10),
                    ],
                },
                Instruction::Call {
                    dest: LocalId(2),
                    callee: "map_length".to_string(),
                    args: vec![Value::Local(LocalId(0))],
                },
            ],
            terminator: Terminator::Return(Value::Local(LocalId(2))),
        }],
        entry: BlockId(0),
    };
    assert!(compile(func).is_ok(), "map_length should compile");
}

#[test]
fn builtin_map_contains_key() {
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Bool,
        param_count: 0,
        locals: vec![
            Local {
                id: LocalId(0),
                ty: Type::Generic("Map".to_string(), vec![Type::Int, Type::Int]),
                mutable: false,
            },
            Local {
                id: LocalId(1),
                ty: Type::Unit,
                mutable: false,
            },
            Local {
                id: LocalId(2),
                ty: Type::Bool,
                mutable: false,
            },
        ],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Call {
                    dest: LocalId(0),
                    callee: "map_new".to_string(),
                    args: vec![],
                },
                Instruction::Call {
                    dest: LocalId(1),
                    callee: "map_insert".to_string(),
                    args: vec![
                        Value::Local(LocalId(0)),
                        Value::IntConst(42),
                        Value::IntConst(1),
                    ],
                },
                Instruction::Call {
                    dest: LocalId(2),
                    callee: "map_contains_key".to_string(),
                    args: vec![Value::Local(LocalId(0)), Value::IntConst(42)],
                },
            ],
            terminator: Terminator::Return(Value::Local(LocalId(2))),
        }],
        entry: BlockId(0),
    };
    assert!(compile(func).is_ok(), "map_contains_key should compile");
}

// ---------------------------------------------------------------------------
// Math builtins
// ---------------------------------------------------------------------------

#[test]
fn builtin_abs_int() {
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
            instructions: vec![Instruction::Call {
                dest: LocalId(0),
                callee: "abs".to_string(),
                args: vec![Value::IntConst(-5)],
            }],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(compile(func).is_ok(), "abs_int should compile");
}

#[test]
fn builtin_min_int() {
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
            instructions: vec![Instruction::Call {
                dest: LocalId(0),
                callee: "min".to_string(),
                args: vec![Value::IntConst(3), Value::IntConst(7)],
            }],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(compile(func).is_ok(), "min_int should compile");
}

#[test]
fn builtin_max_int() {
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
            instructions: vec![Instruction::Call {
                dest: LocalId(0),
                callee: "max".to_string(),
                args: vec![Value::IntConst(3), Value::IntConst(7)],
            }],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };
    assert!(compile(func).is_ok(), "max_int should compile");
}

// ---------------------------------------------------------------------------
// Multiple builtin calls in sequence
// ---------------------------------------------------------------------------

#[test]
fn builtin_sequence_print_and_list() {
    // Combine IO + list in one function to verify sequencing doesn't break.
    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Unit,
        param_count: 0,
        locals: vec![
            Local {
                id: LocalId(0),
                ty: Type::Generic("List".to_string(), vec![Type::Int]),
                mutable: false,
            },
            Local {
                id: LocalId(1),
                ty: Type::Unit,
                mutable: false,
            },
            Local {
                id: LocalId(2),
                ty: Type::Int,
                mutable: false,
            },
            Local {
                id: LocalId(3),
                ty: Type::Unit,
                mutable: false,
            },
        ],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction::Call {
                    dest: LocalId(0),
                    callee: "list_new".to_string(),
                    args: vec![],
                },
                Instruction::Call {
                    dest: LocalId(1),
                    callee: "list_push".to_string(),
                    args: vec![Value::Local(LocalId(0)), Value::IntConst(7)],
                },
                Instruction::Call {
                    dest: LocalId(2),
                    callee: "list_length".to_string(),
                    args: vec![Value::Local(LocalId(0))],
                },
                Instruction::Call {
                    dest: LocalId(3),
                    callee: "print_int".to_string(),
                    args: vec![Value::Local(LocalId(2))],
                },
            ],
            terminator: Terminator::Return(Value::Unit),
        }],
        entry: BlockId(0),
    };
    assert!(
        compile(func).is_ok(),
        "sequence of IO + list builtins should compile"
    );
}
