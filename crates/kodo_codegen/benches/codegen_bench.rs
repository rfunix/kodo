//! Benchmarks for the Kōdo code generator.
//!
//! Measures code generation throughput for MIR functions of varying complexity.

use criterion::{criterion_group, criterion_main, Criterion};
use std::collections::HashMap;
use std::hint::black_box;

use kodo_codegen::{compile_module, compile_module_with_structs, CodegenOptions};
use kodo_mir::{BasicBlock, BlockId, Instruction, Local, LocalId, MirFunction, Terminator, Value};
use kodo_types::Type;

/// Build a simple function that returns a constant.
fn simple_function() -> MirFunction {
    MirFunction {
        name: "constant".to_string(),
        return_type: Type::Int,
        param_count: 0,
        locals: vec![],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: Terminator::Return(Value::IntConst(42)),
        }],
        entry: BlockId(0),
    }
}

/// Build a medium-complexity function with arithmetic, locals, and branching.
fn medium_function() -> MirFunction {
    MirFunction {
        name: "medium".to_string(),
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
            Local {
                id: LocalId(2),
                ty: Type::Int,
                mutable: true,
            },
            Local {
                id: LocalId(3),
                ty: Type::Bool,
                mutable: false,
            },
        ],
        blocks: vec![
            // bb0: compute a + b, compare > 10, branch
            BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction::Assign(
                        LocalId(2),
                        Value::BinOp(
                            kodo_ast::BinOp::Add,
                            Box::new(Value::Local(LocalId(0))),
                            Box::new(Value::Local(LocalId(1))),
                        ),
                    ),
                    Instruction::Assign(
                        LocalId(3),
                        Value::BinOp(
                            kodo_ast::BinOp::Gt,
                            Box::new(Value::Local(LocalId(2))),
                            Box::new(Value::IntConst(10)),
                        ),
                    ),
                ],
                terminator: Terminator::Branch {
                    condition: Value::Local(LocalId(3)),
                    true_block: BlockId(1),
                    false_block: BlockId(2),
                },
            },
            // bb1: return sum * 2
            BasicBlock {
                id: BlockId(1),
                instructions: vec![],
                terminator: Terminator::Return(Value::BinOp(
                    kodo_ast::BinOp::Mul,
                    Box::new(Value::Local(LocalId(2))),
                    Box::new(Value::IntConst(2)),
                )),
            },
            // bb2: return sum - 1
            BasicBlock {
                id: BlockId(2),
                instructions: vec![],
                terminator: Terminator::Return(Value::BinOp(
                    kodo_ast::BinOp::Sub,
                    Box::new(Value::Local(LocalId(2))),
                    Box::new(Value::IntConst(1)),
                )),
            },
        ],
        entry: BlockId(0),
    }
}

/// Build a set of multiple functions that call each other.
fn large_module() -> Vec<MirFunction> {
    let double = MirFunction {
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

    let square = MirFunction {
        name: "square".to_string(),
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
                Box::new(Value::Local(LocalId(0))),
            )),
        }],
        entry: BlockId(0),
    };

    let negate = MirFunction {
        name: "negate".to_string(),
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
            terminator: Terminator::Return(Value::Neg(Box::new(Value::Local(LocalId(0))))),
        }],
        entry: BlockId(0),
    };

    let abs_val = MirFunction {
        name: "abs_val".to_string(),
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
                instructions: vec![],
                terminator: Terminator::Return(Value::Neg(Box::new(Value::Local(LocalId(0))))),
            },
            BasicBlock {
                id: BlockId(2),
                instructions: vec![],
                terminator: Terminator::Return(Value::Local(LocalId(0))),
            },
        ],
        entry: BlockId(0),
    };

    // A caller that invokes double and square
    let orchestrator = MirFunction {
        name: "orchestrator".to_string(),
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
                    dest: LocalId(1),
                    callee: "double".to_string(),
                    args: vec![Value::Local(LocalId(0))],
                },
                Instruction::Call {
                    dest: LocalId(2),
                    callee: "square".to_string(),
                    args: vec![Value::Local(LocalId(1))],
                },
            ],
            terminator: Terminator::Return(Value::BinOp(
                kodo_ast::BinOp::Add,
                Box::new(Value::Local(LocalId(1))),
                Box::new(Value::Local(LocalId(2))),
            )),
        }],
        entry: BlockId(0),
    };

    vec![double, square, negate, abs_val, orchestrator]
}

/// Build a module with struct construction and field access.
fn struct_module() -> (Vec<MirFunction>, HashMap<String, Vec<(String, Type)>>) {
    let mut struct_defs = HashMap::new();
    struct_defs.insert(
        "Point".to_string(),
        vec![("x".to_string(), Type::Int), ("y".to_string(), Type::Int)],
    );

    let make_point = MirFunction {
        name: "make_point".to_string(),
        return_type: Type::Struct("Point".to_string()),
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
            terminator: Terminator::Return(Value::StructLit {
                name: "Point".to_string(),
                fields: vec![
                    ("x".to_string(), Value::Local(LocalId(0))),
                    ("y".to_string(), Value::Local(LocalId(1))),
                ],
            }),
        }],
        entry: BlockId(0),
    };

    let sum_point = MirFunction {
        name: "sum_point".to_string(),
        return_type: Type::Int,
        param_count: 1,
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
                    LocalId(1),
                    Value::FieldGet {
                        object: Box::new(Value::Local(LocalId(0))),
                        field: "x".to_string(),
                        struct_name: "Point".to_string(),
                    },
                ),
                Instruction::Assign(
                    LocalId(2),
                    Value::FieldGet {
                        object: Box::new(Value::Local(LocalId(0))),
                        field: "y".to_string(),
                        struct_name: "Point".to_string(),
                    },
                ),
            ],
            terminator: Terminator::Return(Value::BinOp(
                kodo_ast::BinOp::Add,
                Box::new(Value::Local(LocalId(1))),
                Box::new(Value::Local(LocalId(2))),
            )),
        }],
        entry: BlockId(0),
    };

    (vec![make_point, sum_point], struct_defs)
}

fn bench_codegen(c: &mut Criterion) {
    let mut group = c.benchmark_group("codegen");
    let options = CodegenOptions::default();

    // Simple: single function returning a constant
    group.bench_function("codegen_simple", |b| {
        b.iter(|| {
            let func = simple_function();
            compile_module(black_box(&[func]), &options, None)
        });
    });

    // Medium: function with locals, arithmetic, and branching
    group.bench_function("codegen_medium", |b| {
        b.iter(|| {
            let func = medium_function();
            compile_module(black_box(&[func]), &options, None)
        });
    });

    // Large: multiple functions with inter-function calls
    group.bench_function("codegen_large", |b| {
        b.iter(|| {
            let funcs = large_module();
            compile_module(black_box(&funcs), &options, None)
        });
    });

    // Structs: module with struct construction and field access
    group.bench_function("codegen_structs", |b| {
        b.iter(|| {
            let (funcs, struct_defs) = struct_module();
            compile_module_with_structs(black_box(&funcs), &struct_defs, &options, None)
        });
    });

    // Optimized: large module with optimizations enabled
    group.bench_function("codegen_optimized", |b| {
        let opt_options = CodegenOptions {
            optimize: true,
            debug_info: false,
        };
        b.iter(|| {
            let funcs = large_module();
            compile_module(black_box(&funcs), &opt_options, None)
        });
    });

    group.finish();
}

criterion_group!(benches, bench_codegen);
criterion_main!(benches);
