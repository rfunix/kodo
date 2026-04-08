//! Extended instruction translation tests — struct, enum, indirect call, and virtual dispatch.
//!
//! These tests exercise the codegen paths that require type layout information:
//! `Value::StructLit`, `Value::FieldGet`, `Value::EnumVariant`, `Instruction::IndirectCall`,
//! and `Instruction::VirtualCall`.

use std::collections::HashMap;

use kodo_codegen::{
    compile_module, compile_module_with_structs, compile_module_with_types,
    compile_module_with_vtables, CodegenOptions,
};
use kodo_mir::{BasicBlock, BlockId, Instruction, Local, LocalId, MirFunction, Terminator, Value};
use kodo_types::Type;

fn opts() -> CodegenOptions {
    CodegenOptions::default()
}

// ---------------------------------------------------------------------------
// Struct literal + field access
// ---------------------------------------------------------------------------

#[test]
fn codegen_struct_lit_and_field_get() {
    // struct Point { x: Int, y: Int }
    // fn main() -> Int { let p = Point { x: 10, y: 20 }; return p.x }
    let mut struct_defs = HashMap::new();
    struct_defs.insert(
        "Point".to_string(),
        vec![("x".to_string(), Type::Int), ("y".to_string(), Type::Int)],
    );

    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![
            Local {
                id: LocalId(0),
                ty: Type::Struct("Point".to_string()),
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
                Instruction::Assign(
                    LocalId(0),
                    Value::StructLit {
                        name: "Point".to_string(),
                        fields: vec![
                            ("x".to_string(), Value::IntConst(10)),
                            ("y".to_string(), Value::IntConst(20)),
                        ],
                    },
                ),
                Instruction::Assign(
                    LocalId(1),
                    Value::FieldGet {
                        object: Box::new(Value::Local(LocalId(0))),
                        field: "x".to_string(),
                        struct_name: "Point".to_string(),
                    },
                ),
            ],
            terminator: Terminator::Return(Value::Local(LocalId(1))),
        }],
        entry: BlockId(0),
    };

    let result = compile_module_with_structs(&[func], &struct_defs, &opts(), None);
    assert!(
        result.is_ok(),
        "struct lit + field get should compile: {result:?}"
    );
}

#[test]
fn codegen_struct_single_field() {
    // struct Wrapper { value: Int }
    // fn main() -> Int { let w = Wrapper { value: 99 }; return w.value }
    let mut struct_defs = HashMap::new();
    struct_defs.insert(
        "Wrapper".to_string(),
        vec![("value".to_string(), Type::Int)],
    );

    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![
            Local {
                id: LocalId(0),
                ty: Type::Struct("Wrapper".to_string()),
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
                Instruction::Assign(
                    LocalId(0),
                    Value::StructLit {
                        name: "Wrapper".to_string(),
                        fields: vec![("value".to_string(), Value::IntConst(99))],
                    },
                ),
                Instruction::Assign(
                    LocalId(1),
                    Value::FieldGet {
                        object: Box::new(Value::Local(LocalId(0))),
                        field: "value".to_string(),
                        struct_name: "Wrapper".to_string(),
                    },
                ),
            ],
            terminator: Terminator::Return(Value::Local(LocalId(1))),
        }],
        entry: BlockId(0),
    };

    let result = compile_module_with_structs(&[func], &struct_defs, &opts(), None);
    assert!(
        result.is_ok(),
        "single-field struct should compile: {result:?}"
    );
}

#[test]
fn codegen_struct_bool_field() {
    // struct Flag { active: Bool }
    // fn main() -> Bool { let f = Flag { active: true }; return f.active }
    let mut struct_defs = HashMap::new();
    struct_defs.insert("Flag".to_string(), vec![("active".to_string(), Type::Bool)]);

    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Bool,
        param_count: 0,
        locals: vec![
            Local {
                id: LocalId(0),
                ty: Type::Struct("Flag".to_string()),
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
                Instruction::Assign(
                    LocalId(0),
                    Value::StructLit {
                        name: "Flag".to_string(),
                        fields: vec![("active".to_string(), Value::BoolConst(true))],
                    },
                ),
                Instruction::Assign(
                    LocalId(1),
                    Value::FieldGet {
                        object: Box::new(Value::Local(LocalId(0))),
                        field: "active".to_string(),
                        struct_name: "Flag".to_string(),
                    },
                ),
            ],
            terminator: Terminator::Return(Value::Local(LocalId(1))),
        }],
        entry: BlockId(0),
    };

    let result = compile_module_with_structs(&[func], &struct_defs, &opts(), None);
    assert!(
        result.is_ok(),
        "bool-field struct should compile: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// Enum variant construction + discriminant extraction
// ---------------------------------------------------------------------------

#[test]
fn codegen_enum_unit_variant_discriminant() {
    // enum Color { Red, Green, Blue }
    // fn main() -> Int { let c = Color::Green; return discriminant(c) }
    let mut enum_defs = HashMap::new();
    enum_defs.insert(
        "Color".to_string(),
        vec![
            ("Red".to_string(), vec![]),
            ("Green".to_string(), vec![]),
            ("Blue".to_string(), vec![]),
        ],
    );

    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![
            Local {
                id: LocalId(0),
                ty: Type::Enum("Color".to_string()),
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
                Instruction::Assign(
                    LocalId(0),
                    Value::EnumVariant {
                        enum_name: "Color".to_string(),
                        variant: "Green".to_string(),
                        discriminant: 1,
                        args: vec![],
                    },
                ),
                Instruction::Assign(
                    LocalId(1),
                    Value::EnumDiscriminant(Box::new(Value::Local(LocalId(0)))),
                ),
            ],
            terminator: Terminator::Return(Value::Local(LocalId(1))),
        }],
        entry: BlockId(0),
    };

    let result = compile_module_with_types(&[func], &HashMap::new(), &enum_defs, &opts(), None);
    assert!(
        result.is_ok(),
        "unit enum variant + discriminant should compile: {result:?}"
    );
}

#[test]
fn codegen_enum_payload_extraction() {
    // enum Shape { Circle(Int), Square(Int) }
    // fn main() -> Int { let s = Shape::Circle(42); return s.payload[0] }
    let mut enum_defs = HashMap::new();
    enum_defs.insert(
        "Shape".to_string(),
        vec![
            ("Circle".to_string(), vec![Type::Int]),
            ("Square".to_string(), vec![Type::Int]),
        ],
    );

    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![
            Local {
                id: LocalId(0),
                ty: Type::Enum("Shape".to_string()),
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
                Instruction::Assign(
                    LocalId(0),
                    Value::EnumVariant {
                        enum_name: "Shape".to_string(),
                        variant: "Circle".to_string(),
                        discriminant: 0,
                        args: vec![Value::IntConst(42)],
                    },
                ),
                Instruction::Assign(
                    LocalId(1),
                    Value::EnumPayload {
                        value: Box::new(Value::Local(LocalId(0))),
                        field_index: 0,
                    },
                ),
            ],
            terminator: Terminator::Return(Value::Local(LocalId(1))),
        }],
        entry: BlockId(0),
    };

    let result = compile_module_with_types(&[func], &HashMap::new(), &enum_defs, &opts(), None);
    assert!(
        result.is_ok(),
        "enum payload extraction should compile: {result:?}"
    );
}

#[test]
fn codegen_enum_branch_on_discriminant() {
    // enum Status { Ok, Err }
    // fn main() -> Int { let s = Status::Ok; if discriminant(s) == 0 { return 1 } else { return 2 } }
    let mut enum_defs = HashMap::new();
    enum_defs.insert(
        "Status".to_string(),
        vec![("Ok".to_string(), vec![]), ("Err".to_string(), vec![])],
    );

    let func = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![
            Local {
                id: LocalId(0),
                ty: Type::Enum("Status".to_string()),
                mutable: false,
            },
            Local {
                id: LocalId(1),
                ty: Type::Int,
                mutable: false,
            },
            Local {
                id: LocalId(2),
                ty: Type::Bool,
                mutable: false,
            },
            Local {
                id: LocalId(3),
                ty: Type::Int,
                mutable: true,
            },
        ],
        blocks: vec![
            BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction::Assign(
                        LocalId(0),
                        Value::EnumVariant {
                            enum_name: "Status".to_string(),
                            variant: "Ok".to_string(),
                            discriminant: 0,
                            args: vec![],
                        },
                    ),
                    Instruction::Assign(
                        LocalId(1),
                        Value::EnumDiscriminant(Box::new(Value::Local(LocalId(0)))),
                    ),
                    Instruction::Assign(
                        LocalId(2),
                        Value::BinOp(
                            kodo_ast::BinOp::Eq,
                            Box::new(Value::Local(LocalId(1))),
                            Box::new(Value::IntConst(0)),
                        ),
                    ),
                ],
                terminator: Terminator::Branch {
                    condition: Value::Local(LocalId(2)),
                    true_block: BlockId(1),
                    false_block: BlockId(2),
                },
            },
            BasicBlock {
                id: BlockId(1),
                instructions: vec![Instruction::Assign(LocalId(3), Value::IntConst(1))],
                terminator: Terminator::Goto(BlockId(3)),
            },
            BasicBlock {
                id: BlockId(2),
                instructions: vec![Instruction::Assign(LocalId(3), Value::IntConst(2))],
                terminator: Terminator::Goto(BlockId(3)),
            },
            BasicBlock {
                id: BlockId(3),
                instructions: vec![],
                terminator: Terminator::Return(Value::Local(LocalId(3))),
            },
        ],
        entry: BlockId(0),
    };

    let result = compile_module_with_types(&[func], &HashMap::new(), &enum_defs, &opts(), None);
    assert!(
        result.is_ok(),
        "branch on enum discriminant should compile: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// Indirect call (function pointer dispatch)
// ---------------------------------------------------------------------------

#[test]
fn codegen_indirect_call_via_funcref() {
    // fn double(x: Int) -> Int { return x * 2 }
    // fn main() -> Int { let f = &double; return f(21) }
    let double_fn = MirFunction {
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
            instructions: vec![Instruction::IndirectCall {
                dest: LocalId(0),
                callee: Value::FuncRef("double".to_string()),
                args: vec![Value::IntConst(21)],
                return_type: Type::Int,
                param_types: vec![Type::Int],
            }],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };

    let result = compile_module(&[main_fn, double_fn], &opts(), None);
    assert!(
        result.is_ok(),
        "indirect call via FuncRef should compile: {result:?}"
    );
}

#[test]
fn codegen_indirect_call_no_args() {
    // fn get_answer() -> Int { return 42 }
    // fn main() -> Int { let f = &get_answer; return f() }
    let get_answer = MirFunction {
        name: "get_answer".to_string(),
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
            instructions: vec![Instruction::IndirectCall {
                dest: LocalId(0),
                callee: Value::FuncRef("get_answer".to_string()),
                args: vec![],
                return_type: Type::Int,
                param_types: vec![],
            }],
            terminator: Terminator::Return(Value::Local(LocalId(0))),
        }],
        entry: BlockId(0),
    };

    let result = compile_module(&[main_fn, get_answer], &opts(), None);
    assert!(
        result.is_ok(),
        "indirect call with no args should compile: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// Virtual call (trait dispatch)
// ---------------------------------------------------------------------------

#[test]
fn codegen_virtual_call_single_method() {
    // trait Area { fn area(self) -> Int }
    // struct Rect { w: Int, h: Int }
    // impl Area for Rect { fn area(self) -> Int { return self.w * self.h } }
    // fn main() -> Int { ... virtual call ... }
    let mut struct_defs = HashMap::new();
    struct_defs.insert(
        "Rect".to_string(),
        vec![("w".to_string(), Type::Int), ("h".to_string(), Type::Int)],
    );

    let mut vtable_defs = HashMap::new();
    vtable_defs.insert(
        ("Rect".to_string(), "Area".to_string()),
        vec!["Rect_area".to_string()],
    );

    // Concrete method: Rect_area(self_ptr: *Rect) -> Int
    let rect_area = MirFunction {
        name: "Rect_area".to_string(),
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
            terminator: Terminator::Return(Value::IntConst(100)),
        }],
        entry: BlockId(0),
    };

    // Main: construct dyn trait fat pointer, then virtual call
    let main_fn = MirFunction {
        name: "main".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![
            Local {
                id: LocalId(0),
                ty: Type::Struct("Rect".to_string()),
                mutable: false,
            },
            // fat pointer: two i64s (data_ptr, vtable_ptr) — stored as Named("__dyn_Area")
            Local {
                id: LocalId(1),
                ty: Type::DynTrait("Area".to_string()),
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
                Instruction::Assign(
                    LocalId(0),
                    Value::StructLit {
                        name: "Rect".to_string(),
                        fields: vec![
                            ("w".to_string(), Value::IntConst(10)),
                            ("h".to_string(), Value::IntConst(10)),
                        ],
                    },
                ),
                Instruction::Assign(
                    LocalId(1),
                    Value::MakeDynTrait {
                        value: Box::new(Value::Local(LocalId(0))),
                        concrete_type: "Rect".to_string(),
                        trait_name: "Area".to_string(),
                    },
                ),
                Instruction::VirtualCall {
                    dest: LocalId(2),
                    object: LocalId(1),
                    vtable_index: 0,
                    args: vec![],
                    return_type: Type::Int,
                    param_types: vec![],
                },
            ],
            terminator: Terminator::Return(Value::Local(LocalId(2))),
        }],
        entry: BlockId(0),
    };

    let result = compile_module_with_vtables(
        &[main_fn, rect_area],
        &struct_defs,
        &HashMap::new(),
        &vtable_defs,
        &opts(),
        None,
    );
    assert!(result.is_ok(), "virtual call should compile: {result:?}");
}

// ---------------------------------------------------------------------------
// IncRef / DecRef
// ---------------------------------------------------------------------------

#[test]
fn codegen_incref_decref() {
    // Verify that IncRef/DecRef instructions on a heap local compile correctly.
    // The local is typed as String (a heap-allocated composite in Kōdo).
    let func = MirFunction {
        name: "main".to_string(),
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
                Instruction::Assign(LocalId(0), Value::StringConst("hello".to_string())),
                Instruction::IncRef(LocalId(0)),
                Instruction::DecRef(LocalId(0)),
            ],
            terminator: Terminator::Return(Value::Unit),
        }],
        entry: BlockId(0),
    };

    let result = compile_module(&[func], &opts(), None);
    assert!(result.is_ok(), "IncRef/DecRef should compile: {result:?}");
}
