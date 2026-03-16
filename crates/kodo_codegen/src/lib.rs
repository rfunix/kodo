//! # `kodo_codegen` — Code Generation Backend for the Kōdo Compiler
//!
//! This crate translates [`kodo_mir`] into native machine code using the
//! [Cranelift](https://cranelift.dev/) code generator.
//!
//! Cranelift was chosen over LLVM for the initial implementation because:
//! - Faster compilation (critical for tight AI agent feedback loops)
//! - Pure Rust (no C++ dependency)
//! - Good enough optimization for development builds
//!
//! An LLVM backend may be added later for optimized release builds.
//!
//! ## Architecture
//!
//! The crate is split into the following internal modules:
//!
//! - `module` — Module-level compilation orchestration
//! - `function` — Per-function translation and variable mapping
//! - `instruction` — MIR instruction translation
//! - `terminator` — MIR terminator translation
//! - `value` — MIR value translation
//! - `layout` — Struct and enum memory layout computation
//! - `builtins` — Runtime builtin function declarations
//!
//! ## Academic References
//!
//! - **\[Tiger\]** *Modern Compiler Implementation in ML* Ch. 9–11 — Instruction
//!   selection via tree-pattern matching, register allocation via graph coloring.
//! - **\[EC\]** *Engineering a Compiler* Ch. 11–13 — Instruction selection,
//!   scheduling, and register allocation (delegated to Cranelift).
//!
//! See `docs/REFERENCES.md` for the full bibliography.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![warn(clippy::pedantic)]

mod builtins;
mod function;
mod instruction;
mod layout;
mod module;
mod terminator;
mod value;

use std::collections::HashMap;

use kodo_mir::MirFunction;
use kodo_types::Type;
use thiserror::Error;

/// Errors from code generation.
#[derive(Debug, Error)]
pub enum CodegenError {
    /// A Cranelift error occurred.
    #[error("cranelift error: {0}")]
    Cranelift(String),
    /// An unsupported MIR construct was encountered.
    #[error("unsupported MIR construct: {0}")]
    Unsupported(String),
    /// The target architecture is not supported.
    #[error("unsupported target: {0}")]
    UnsupportedTarget(String),
    /// A module-level error occurred.
    #[error("module error: {0}")]
    ModuleError(String),
}

/// Alias for results in this crate.
pub type Result<T> = std::result::Result<T, CodegenError>;

/// Code generation options.
#[derive(Debug, Clone)]
pub struct CodegenOptions {
    /// Whether to optimize the generated code.
    pub optimize: bool,
    /// Whether to emit debug information.
    pub debug_info: bool,
    /// Whether contract failures should be recoverable (log + continue)
    /// instead of aborting.
    pub recoverable_contracts: bool,
}

impl Default for CodegenOptions {
    fn default() -> Self {
        Self {
            optimize: false,
            debug_info: true,
            recoverable_contracts: false,
        }
    }
}

/// Compiles MIR functions with struct type definitions into a native object file.
///
/// # Errors
///
/// Returns [`CodegenError`] if code generation fails.
#[allow(clippy::implicit_hasher)]
pub fn compile_module_with_structs(
    mir_functions: &[MirFunction],
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    options: &CodegenOptions,
    metadata_json: Option<&str>,
) -> Result<Vec<u8>> {
    module::compile_module_inner(
        mir_functions,
        struct_defs,
        &HashMap::new(),
        &HashMap::new(),
        options,
        metadata_json,
    )
}

/// Compiles MIR functions with struct and enum type definitions into a native object file.
///
/// # Errors
///
/// Returns [`CodegenError`] if code generation fails.
#[allow(clippy::implicit_hasher)]
pub fn compile_module_with_types(
    mir_functions: &[MirFunction],
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    options: &CodegenOptions,
    metadata_json: Option<&str>,
) -> Result<Vec<u8>> {
    module::compile_module_inner(
        mir_functions,
        struct_defs,
        enum_defs,
        &HashMap::new(),
        options,
        metadata_json,
    )
}

/// A vtable entry mapping `(concrete_type, trait_name)` to method function names.
///
/// The function names are ordered by trait method declaration order,
/// matching the vtable slot indices used by [`kodo_mir::Instruction::VirtualCall`].
pub type VtableDef = Vec<String>;

/// Compiles MIR functions with struct, enum, and vtable definitions into a native object file.
///
/// The `vtable_defs` map keys are `(concrete_type, trait_name)` pairs, and values
/// are ordered lists of function names corresponding to trait method declaration order.
///
/// # Errors
///
/// Returns [`CodegenError`] if code generation fails.
#[allow(clippy::implicit_hasher)]
pub fn compile_module_with_vtables(
    mir_functions: &[MirFunction],
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    vtable_defs: &HashMap<(String, String), VtableDef>,
    options: &CodegenOptions,
    metadata_json: Option<&str>,
) -> Result<Vec<u8>> {
    module::compile_module_inner(
        mir_functions,
        struct_defs,
        enum_defs,
        vtable_defs,
        options,
        metadata_json,
    )
}

/// Compiles a set of MIR functions into a native object file.
///
/// The returned `Vec<u8>` contains a complete object file (e.g. Mach-O or ELF)
/// ready to be linked with the Kōdo runtime.
///
/// The `main` function in the MIR is renamed to `kodo_main` so that the
/// runtime's `main` wrapper can call it.
///
/// If `metadata_json` is provided, it is embedded as exported data symbols
/// (`kodo_meta` and `kodo_meta_len`) so the runtime can respond to `--describe`.
///
/// # Errors
///
/// Returns [`CodegenError`] if code generation fails.
pub fn compile_module(
    mir_functions: &[MirFunction],
    options: &CodegenOptions,
    metadata_json: Option<&str>,
) -> Result<Vec<u8>> {
    module::compile_module_inner(
        mir_functions,
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        options,
        metadata_json,
    )
}

/// Compiles a single MIR function (legacy API, kept for tests).
///
/// # Errors
///
/// Returns [`CodegenError`] if code generation fails.
pub fn compile_function(function: &MirFunction, options: &CodegenOptions) -> Result<Vec<u8>> {
    compile_module(std::slice::from_ref(function), options, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::function::HeapKind;
    use crate::module::is_composite;
    use kodo_mir::{
        BasicBlock, BlockId, Instruction, Local, LocalId, MirFunction, Terminator, Value,
    };
    use kodo_types::Type;

    #[test]
    fn compile_empty_function_produces_object() {
        let func = MirFunction {
            name: "test".to_string(),
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
        let result = compile_function(&func, &CodegenOptions::default());
        assert!(result.is_ok());
        let bytes = result.ok().unwrap_or_default();
        assert!(!bytes.is_empty(), "object file should not be empty");
    }

    #[test]
    fn compile_return_42_produces_code() {
        let func = MirFunction {
            name: "answer".to_string(),
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
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok());
        let bytes = result.ok().unwrap_or_default();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn compile_with_branch() {
        let func = MirFunction {
            name: "branchy".to_string(),
            return_type: Type::Int,
            param_count: 1,
            locals: vec![Local {
                id: LocalId(0),
                ty: Type::Bool,
                mutable: false,
            }],
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
                    terminator: Terminator::Return(Value::IntConst(2)),
                },
            ],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn default_options_no_optimize() {
        let opts = CodegenOptions::default();
        assert!(!opts.optimize);
        assert!(opts.debug_info);
    }

    #[test]
    fn compile_arithmetic_operations() {
        let func = MirFunction {
            name: "arith".to_string(),
            return_type: Type::Int,
            param_count: 0,
            locals: vec![],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Return(Value::BinOp(
                    kodo_ast::BinOp::Add,
                    Box::new(Value::IntConst(1)),
                    Box::new(Value::BinOp(
                        kodo_ast::BinOp::Mul,
                        Box::new(Value::IntConst(2)),
                        Box::new(Value::IntConst(3)),
                    )),
                )),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn compile_function_with_params() {
        let func = MirFunction {
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
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn compile_function_call_between_functions() {
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
        let caller = MirFunction {
            name: "use_double".to_string(),
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
        let result = compile_module(&[callee, caller], &CodegenOptions::default(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn compile_with_contract_check() {
        let func = MirFunction {
            name: "checked".to_string(),
            return_type: Type::Unit,
            param_count: 0,
            locals: vec![Local {
                id: LocalId(0),
                ty: Type::Unit,
                mutable: false,
            }],
            blocks: vec![
                BasicBlock {
                    id: BlockId(0),
                    instructions: vec![],
                    terminator: Terminator::Branch {
                        condition: Value::BoolConst(true),
                        true_block: BlockId(2),
                        false_block: BlockId(1),
                    },
                },
                BasicBlock {
                    id: BlockId(1),
                    instructions: vec![Instruction::Call {
                        dest: LocalId(0),
                        callee: "kodo_contract_fail".to_string(),
                        args: vec![Value::StringConst("contract failed".to_string())],
                    }],
                    terminator: Terminator::Unreachable,
                },
                BasicBlock {
                    id: BlockId(2),
                    instructions: vec![],
                    terminator: Terminator::Return(Value::Unit),
                },
            ],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn compile_string_constant() {
        let func = MirFunction {
            name: "greet".to_string(),
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
                    callee: "println".to_string(),
                    args: vec![Value::StringConst("hello".to_string())],
                }],
                terminator: Terminator::Return(Value::Unit),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn compile_boolean_operations() {
        let func = MirFunction {
            name: "bools".to_string(),
            return_type: Type::Int,
            param_count: 0,
            locals: vec![],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Return(Value::BinOp(
                    kodo_ast::BinOp::Eq,
                    Box::new(Value::IntConst(1)),
                    Box::new(Value::IntConst(1)),
                )),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn compile_unary_operations() {
        let func = MirFunction {
            name: "unary".to_string(),
            return_type: Type::Int,
            param_count: 0,
            locals: vec![],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Return(Value::Neg(Box::new(Value::IntConst(42)))),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn compile_with_metadata_produces_object() {
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
        let result = compile_module(
            &[func],
            &CodegenOptions::default(),
            Some("{\"test\": true}"),
        );
        let bytes = result.unwrap_or_else(|e| panic!("compile_module failed: {e}"));
        assert!(
            !bytes.is_empty(),
            "object file with metadata should not be empty"
        );
    }

    #[test]
    fn compile_if_else_cfg() {
        let func = MirFunction {
            name: "ifelse".to_string(),
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
                    instructions: vec![Instruction::Assign(LocalId(0), Value::IntConst(10))],
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
                    terminator: Terminator::Return(Value::Local(LocalId(0))),
                },
            ],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn compile_struct_param_function() {
        let struct_defs = HashMap::from([(
            "Point".to_string(),
            vec![("x".to_string(), Type::Int), ("y".to_string(), Type::Int)],
        )]);
        let get_x = MirFunction {
            name: "get_x".to_string(),
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
                    ty: Type::Unknown,
                    mutable: false,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Assign(
                    LocalId(1),
                    Value::FieldGet {
                        object: Box::new(Value::Local(LocalId(0))),
                        field: "x".to_string(),
                        struct_name: "Point".to_string(),
                    },
                )],
                terminator: Terminator::Return(Value::Local(LocalId(1))),
            }],
            entry: BlockId(0),
        };
        let result = compile_module_with_types(
            &[get_x],
            &struct_defs,
            &HashMap::new(),
            &CodegenOptions::default(),
            None,
        );
        assert!(result.is_ok(), "failed: {result:?}");
    }

    #[test]
    fn compile_struct_return_function() {
        let struct_defs = HashMap::from([(
            "Point".to_string(),
            vec![("x".to_string(), Type::Int), ("y".to_string(), Type::Int)],
        )]);
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
                Local {
                    id: LocalId(2),
                    ty: Type::Struct("Point".to_string()),
                    mutable: false,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Assign(
                    LocalId(2),
                    Value::StructLit {
                        name: "Point".to_string(),
                        fields: vec![
                            ("x".to_string(), Value::Local(LocalId(0))),
                            ("y".to_string(), Value::Local(LocalId(1))),
                        ],
                    },
                )],
                terminator: Terminator::Return(Value::Local(LocalId(2))),
            }],
            entry: BlockId(0),
        };
        let result = compile_module_with_types(
            &[make_point],
            &struct_defs,
            &HashMap::new(),
            &CodegenOptions::default(),
            None,
        );
        assert!(result.is_ok(), "failed: {result:?}");
    }

    #[test]
    fn is_composite_correctly_identifies_types() {
        assert!(is_composite(&Type::Struct("Foo".to_string())));
        assert!(is_composite(&Type::Enum("Bar".to_string())));
        assert!(!is_composite(&Type::Int));
        assert!(!is_composite(&Type::Bool));
        assert!(!is_composite(&Type::Unit));
    }

    #[test]
    fn compile_indirect_call() {
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
        let apply = MirFunction {
            name: "apply".to_string(),
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
                    mutable: false,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::IndirectCall {
                    dest: LocalId(2),
                    callee: Value::Local(LocalId(0)),
                    args: vec![Value::Local(LocalId(1))],
                    return_type: Type::Int,
                    param_types: vec![Type::Int],
                }],
                terminator: Terminator::Return(Value::Local(LocalId(2))),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[double, apply], &CodegenOptions::default(), None);
        assert!(
            result.is_ok(),
            "indirect call compilation failed: {result:?}"
        );
    }

    #[test]
    fn compile_func_ref_value() {
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
        let get_ptr = MirFunction {
            name: "get_ptr".to_string(),
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
        let result = compile_module(&[target, get_ptr], &CodegenOptions::default(), None);
        assert!(result.is_ok(), "func_ref compilation failed: {result:?}");
    }

    #[test]
    fn test_is_composite_includes_string() {
        assert!(is_composite(&Type::String));
    }

    #[test]
    fn test_string_local_stack_slot() {
        let func = MirFunction {
            name: "test".to_string(),
            return_type: Type::Unit,
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
                    Value::StringConst("hello".to_string()),
                )],
                terminator: Terminator::Return(Value::Unit),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok(), "string local stack slot failed: {result:?}");
    }

    #[test]
    fn test_string_param_composite() {
        let func = MirFunction {
            name: "test".to_string(),
            return_type: Type::Unit,
            param_count: 1,
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
                instructions: vec![Instruction::Call {
                    dest: LocalId(1),
                    callee: "println".to_string(),
                    args: vec![Value::Local(LocalId(0))],
                }],
                terminator: Terminator::Return(Value::Unit),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok(), "string param composite failed: {result:?}");
    }

    #[test]
    fn test_string_const_assign() {
        let func = MirFunction {
            name: "test".to_string(),
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
                    Instruction::Assign(LocalId(0), Value::StringConst("test".to_string())),
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
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok(), "string const assign failed: {result:?}");
    }

    #[test]
    fn test_string_builtin_expansion() {
        let func = MirFunction {
            name: "test".to_string(),
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
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(
            result.is_ok(),
            "string builtin expansion failed: {result:?}"
        );
    }

    #[test]
    fn test_string_returning_builtin() {
        let func = MirFunction {
            name: "test".to_string(),
            return_type: Type::Unit,
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
                terminator: Terminator::Return(Value::Unit),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(
            result.is_ok(),
            "string returning builtin failed: {result:?}"
        );
    }

    #[test]
    fn test_string_copy_between_locals() {
        let func = MirFunction {
            name: "test".to_string(),
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
                    ty: Type::String,
                    mutable: false,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction::Assign(LocalId(0), Value::StringConst("x".to_string())),
                    Instruction::Assign(LocalId(1), Value::Local(LocalId(0))),
                ],
                terminator: Terminator::Return(Value::Unit),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(
            result.is_ok(),
            "string copy between locals failed: {result:?}"
        );
    }

    #[test]
    fn test_string_sret_return() {
        let func = MirFunction {
            name: "make_greeting".to_string(),
            return_type: Type::String,
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
                    Value::StringConst("hi".to_string()),
                )],
                terminator: Terminator::Return(Value::Local(LocalId(0))),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok(), "string sret return failed: {result:?}");
    }

    #[test]
    fn test_string_in_struct() {
        let struct_defs = HashMap::from([(
            "Greeting".to_string(),
            vec![("msg".to_string(), Type::String)],
        )]);
        let func = MirFunction {
            name: "make".to_string(),
            return_type: Type::Struct("Greeting".to_string()),
            param_count: 1,
            locals: vec![
                Local {
                    id: LocalId(0),
                    ty: Type::String,
                    mutable: false,
                },
                Local {
                    id: LocalId(1),
                    ty: Type::Struct("Greeting".to_string()),
                    mutable: false,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Assign(
                    LocalId(1),
                    Value::StructLit {
                        name: "Greeting".to_string(),
                        fields: vec![("msg".to_string(), Value::Local(LocalId(0)))],
                    },
                )],
                terminator: Terminator::Return(Value::Local(LocalId(1))),
            }],
            entry: BlockId(0),
        };
        let result = compile_module_with_types(
            &[func],
            &struct_defs,
            &HashMap::new(),
            &CodegenOptions::default(),
            None,
        );
        assert!(result.is_ok(), "string in struct failed: {result:?}");
    }

    #[test]
    fn test_nested_struct_with_string() {
        let struct_defs = HashMap::from([
            ("Inner".to_string(), vec![("msg".to_string(), Type::String)]),
            (
                "Outer".to_string(),
                vec![("inner".to_string(), Type::Struct("Inner".to_string()))],
            ),
        ]);
        let func = MirFunction {
            name: "make_outer".to_string(),
            return_type: Type::Struct("Outer".to_string()),
            param_count: 0,
            locals: vec![
                Local {
                    id: LocalId(0),
                    ty: Type::Struct("Inner".to_string()),
                    mutable: false,
                },
                Local {
                    id: LocalId(1),
                    ty: Type::Struct("Outer".to_string()),
                    mutable: false,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction::Assign(
                        LocalId(0),
                        Value::StructLit {
                            name: "Inner".to_string(),
                            fields: vec![(
                                "msg".to_string(),
                                Value::StringConst("hello".to_string()),
                            )],
                        },
                    ),
                    Instruction::Assign(
                        LocalId(1),
                        Value::StructLit {
                            name: "Outer".to_string(),
                            fields: vec![("inner".to_string(), Value::Local(LocalId(0)))],
                        },
                    ),
                ],
                terminator: Terminator::Return(Value::Local(LocalId(1))),
            }],
            entry: BlockId(0),
        };
        let result = compile_module_with_types(
            &[func],
            &struct_defs,
            &HashMap::new(),
            &CodegenOptions::default(),
            None,
        );
        assert!(
            result.is_ok(),
            "nested struct with string failed: {result:?}"
        );
    }

    #[test]
    fn test_multiple_string_return_paths() {
        let func = MirFunction {
            name: "choose".to_string(),
            return_type: Type::String,
            param_count: 1,
            locals: vec![
                Local {
                    id: LocalId(0),
                    ty: Type::Bool,
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
            ],
            blocks: vec![
                BasicBlock {
                    id: BlockId(0),
                    instructions: vec![],
                    terminator: Terminator::Branch {
                        condition: Value::Local(LocalId(0)),
                        true_block: BlockId(1),
                        false_block: BlockId(2),
                    },
                },
                BasicBlock {
                    id: BlockId(1),
                    instructions: vec![Instruction::Assign(
                        LocalId(1),
                        Value::StringConst("yes".to_string()),
                    )],
                    terminator: Terminator::Return(Value::Local(LocalId(1))),
                },
                BasicBlock {
                    id: BlockId(2),
                    instructions: vec![Instruction::Assign(
                        LocalId(2),
                        Value::StringConst("no".to_string()),
                    )],
                    terminator: Terminator::Return(Value::Local(LocalId(2))),
                },
            ],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(
            result.is_ok(),
            "multiple string return paths failed: {result:?}"
        );
    }

    #[test]
    fn test_string_comparison_eq() {
        let func = MirFunction {
            name: "compare".to_string(),
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
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction::Assign(LocalId(0), Value::StringConst("a".to_string())),
                    Instruction::Assign(LocalId(1), Value::StringConst("b".to_string())),
                ],
                terminator: Terminator::Return(Value::BinOp(
                    kodo_ast::BinOp::Eq,
                    Box::new(Value::Local(LocalId(0))),
                    Box::new(Value::Local(LocalId(1))),
                )),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(
            result.is_ok(),
            "string comparison (Eq) should succeed: {result:?}"
        );
    }

    #[test]
    fn test_string_comparison_ne() {
        let func = MirFunction {
            name: "compare_ne".to_string(),
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
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction::Assign(LocalId(0), Value::StringConst("a".to_string())),
                    Instruction::Assign(LocalId(1), Value::StringConst("b".to_string())),
                ],
                terminator: Terminator::Return(Value::BinOp(
                    kodo_ast::BinOp::Ne,
                    Box::new(Value::Local(LocalId(0))),
                    Box::new(Value::Local(LocalId(1))),
                )),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(
            result.is_ok(),
            "string comparison (Ne) should succeed: {result:?}"
        );
    }

    #[test]
    fn test_string_comparison_const_eq() {
        let func = MirFunction {
            name: "compare_const".to_string(),
            return_type: Type::Bool,
            param_count: 0,
            locals: vec![],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Return(Value::BinOp(
                    kodo_ast::BinOp::Eq,
                    Box::new(Value::StringConst("hello".to_string())),
                    Box::new(Value::StringConst("hello".to_string())),
                )),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(
            result.is_ok(),
            "string constant comparison should succeed: {result:?}"
        );
    }

    #[test]
    fn test_struct_with_multiple_string_fields() {
        let struct_defs = HashMap::from([(
            "Person".to_string(),
            vec![
                ("name".to_string(), Type::String),
                ("email".to_string(), Type::String),
            ],
        )]);
        let func = MirFunction {
            name: "make_person".to_string(),
            return_type: Type::Struct("Person".to_string()),
            param_count: 0,
            locals: vec![Local {
                id: LocalId(0),
                ty: Type::Struct("Person".to_string()),
                mutable: false,
            }],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Assign(
                    LocalId(0),
                    Value::StructLit {
                        name: "Person".to_string(),
                        fields: vec![
                            ("name".to_string(), Value::StringConst("Alice".to_string())),
                            (
                                "email".to_string(),
                                Value::StringConst("alice@example.com".to_string()),
                            ),
                        ],
                    },
                )],
                terminator: Terminator::Return(Value::Local(LocalId(0))),
            }],
            entry: BlockId(0),
        };
        let result = compile_module_with_types(
            &[func],
            &struct_defs,
            &HashMap::new(),
            &CodegenOptions::default(),
            None,
        );
        assert!(
            result.is_ok(),
            "struct with multiple string fields failed: {result:?}"
        );
    }

    #[test]
    fn test_string_concat_literals() {
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
                instructions: vec![Instruction::Assign(
                    LocalId(0),
                    Value::BinOp(
                        kodo_ast::BinOp::Add,
                        Box::new(Value::StringConst("hello ".to_string())),
                        Box::new(Value::StringConst("world".to_string())),
                    ),
                )],
                terminator: Terminator::Return(Value::Unit),
            }],
            entry: BlockId(0),
        };
        let result = compile_module_with_types(
            &[func],
            &HashMap::new(),
            &HashMap::new(),
            &CodegenOptions::default(),
            None,
        );
        assert!(result.is_ok(), "string concat literals failed: {result:?}");
    }

    #[test]
    fn test_string_concat_var_and_literal() {
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
                    ty: Type::String,
                    mutable: false,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction::Assign(LocalId(0), Value::StringConst("hello ".to_string())),
                    Instruction::Assign(
                        LocalId(1),
                        Value::BinOp(
                            kodo_ast::BinOp::Add,
                            Box::new(Value::Local(LocalId(0))),
                            Box::new(Value::StringConst("world".to_string())),
                        ),
                    ),
                ],
                terminator: Terminator::Return(Value::Unit),
            }],
            entry: BlockId(0),
        };
        let result = compile_module_with_types(
            &[func],
            &HashMap::new(),
            &HashMap::new(),
            &CodegenOptions::default(),
            None,
        );
        assert!(
            result.is_ok(),
            "string concat var + literal failed: {result:?}"
        );
    }

    #[test]
    fn test_string_concat_var_and_var() {
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
                    ty: Type::String,
                    mutable: false,
                },
                Local {
                    id: LocalId(2),
                    ty: Type::String,
                    mutable: false,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction::Assign(LocalId(0), Value::StringConst("foo".to_string())),
                    Instruction::Assign(LocalId(1), Value::StringConst("bar".to_string())),
                    Instruction::Assign(
                        LocalId(2),
                        Value::BinOp(
                            kodo_ast::BinOp::Add,
                            Box::new(Value::Local(LocalId(0))),
                            Box::new(Value::Local(LocalId(1))),
                        ),
                    ),
                ],
                terminator: Terminator::Return(Value::Unit),
            }],
            entry: BlockId(0),
        };
        let result = compile_module_with_types(
            &[func],
            &HashMap::new(),
            &HashMap::new(),
            &CodegenOptions::default(),
            None,
        );
        assert!(result.is_ok(), "string concat var + var failed: {result:?}");
    }

    #[test]
    fn compile_float64_return_constant() {
        let func = MirFunction {
            name: "float_const".to_string(),
            return_type: Type::Float64,
            param_count: 0,
            locals: vec![],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Return(Value::FloatConst(3.14)),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok(), "Float64 constant return failed: {result:?}");
    }

    #[test]
    fn compile_float64_addition() {
        let func = MirFunction {
            name: "float_add".to_string(),
            return_type: Type::Float64,
            param_count: 0,
            locals: vec![],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Return(Value::BinOp(
                    kodo_ast::BinOp::Add,
                    Box::new(Value::FloatConst(1.5)),
                    Box::new(Value::FloatConst(2.5)),
                )),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok(), "Float64 addition failed: {result:?}");
    }

    #[test]
    fn compile_float64_comparison() {
        let func = MirFunction {
            name: "float_cmp".to_string(),
            return_type: Type::Int,
            param_count: 0,
            locals: vec![],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Return(Value::BinOp(
                    kodo_ast::BinOp::Lt,
                    Box::new(Value::FloatConst(1.0)),
                    Box::new(Value::FloatConst(2.0)),
                )),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok(), "Float64 comparison failed: {result:?}");
    }

    #[test]
    fn compile_float64_negation() {
        let func = MirFunction {
            name: "float_neg".to_string(),
            return_type: Type::Float64,
            param_count: 0,
            locals: vec![],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Return(Value::Neg(Box::new(Value::FloatConst(1.5)))),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok(), "Float64 negation failed: {result:?}");
    }

    #[test]
    fn compile_lambda_lifted_closure_with_capture() {
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
                    Box::new(Value::Local(LocalId(1))),
                    Box::new(Value::Local(LocalId(0))),
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
        let result = compile_module(&[closure_fn, main_fn], &CodegenOptions::default(), None);
        assert!(
            result.is_ok(),
            "lambda-lifted closure compilation failed: {result:?}"
        );
    }

    #[test]
    fn compile_lambda_lifted_closure_returning_bool() {
        let closure_fn = MirFunction {
            name: "__closure_0".to_string(),
            return_type: Type::Bool,
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
                    kodo_ast::BinOp::Gt,
                    Box::new(Value::Local(LocalId(0))),
                    Box::new(Value::IntConst(0)),
                )),
            }],
            entry: BlockId(0),
        };
        let caller = MirFunction {
            name: "caller".to_string(),
            return_type: Type::Int,
            param_count: 0,
            locals: vec![Local {
                id: LocalId(0),
                ty: Type::Bool,
                mutable: false,
            }],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction::Call {
                    dest: LocalId(0),
                    callee: "__closure_0".to_string(),
                    args: vec![Value::IntConst(5)],
                }],
                terminator: Terminator::Return(Value::IntConst(0)),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[closure_fn, caller], &CodegenOptions::default(), None);
        assert!(
            result.is_ok(),
            "closure returning Bool compilation failed: {result:?}"
        );
    }

    #[test]
    fn compile_string_concat_emits_free() {
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
                    ty: Type::String,
                    mutable: false,
                },
                Local {
                    id: LocalId(2),
                    ty: Type::String,
                    mutable: false,
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction::Assign(LocalId(0), Value::StringConst("hello".to_string())),
                    Instruction::Assign(LocalId(1), Value::StringConst(" world".to_string())),
                    Instruction::Assign(
                        LocalId(2),
                        Value::BinOp(
                            kodo_ast::BinOp::Add,
                            Box::new(Value::Local(LocalId(0))),
                            Box::new(Value::Local(LocalId(1))),
                        ),
                    ),
                    Instruction::Call {
                        dest: LocalId(0),
                        callee: "println".to_string(),
                        args: vec![Value::Local(LocalId(2))],
                    },
                ],
                terminator: Terminator::Return(Value::Unit),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok(), "String concat with free failed: {result:?}");
        let object_bytes = result.as_ref().ok();
        assert!(object_bytes.is_some());
        let bytes = object_bytes.map(|b| String::from_utf8_lossy(b).to_string());
        assert!(
            bytes
                .as_ref()
                .map_or(false, |b| b.contains("kodo_string_free")),
            "compiled object should reference kodo_string_free"
        );
    }

    #[test]
    fn compile_list_new_emits_free() {
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
                instructions: vec![Instruction::Call {
                    dest: LocalId(0),
                    callee: "list_new".to_string(),
                    args: vec![],
                }],
                terminator: Terminator::Return(Value::Unit),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok(), "List new with free failed: {result:?}");
        let bytes = result
            .as_ref()
            .map(|b| String::from_utf8_lossy(b).to_string())
            .ok();
        assert!(
            bytes
                .as_ref()
                .map_or(false, |b| b.contains("kodo_list_free")),
            "compiled object should reference kodo_list_free"
        );
    }

    #[test]
    fn compile_map_new_emits_free() {
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
                instructions: vec![Instruction::Call {
                    dest: LocalId(0),
                    callee: "map_new".to_string(),
                    args: vec![],
                }],
                terminator: Terminator::Return(Value::Unit),
            }],
            entry: BlockId(0),
        };
        let result = compile_module(&[func], &CodegenOptions::default(), None);
        assert!(result.is_ok(), "Map new with free failed: {result:?}");
        let bytes = result
            .as_ref()
            .map(|b| String::from_utf8_lossy(b).to_string())
            .ok();
        assert!(
            bytes
                .as_ref()
                .map_or(false, |b| b.contains("kodo_map_free")),
            "compiled object should reference kodo_map_free"
        );
    }

    #[test]
    fn heap_kind_enum_variants() {
        assert_ne!(HeapKind::String, HeapKind::List);
        assert_ne!(HeapKind::List, HeapKind::Map);
        assert_eq!(HeapKind::String, HeapKind::String);
    }

    // ---------------------------------------------------------------
    // CodegenOptions tests
    // ---------------------------------------------------------------

    #[test]
    fn codegen_options_default_optimize_is_false() {
        let opts = CodegenOptions::default();
        assert!(!opts.optimize);
    }

    #[test]
    fn codegen_options_default_debug_info_is_true() {
        let opts = CodegenOptions::default();
        assert!(opts.debug_info);
    }

    #[test]
    fn codegen_options_default_recoverable_contracts_is_false() {
        let opts = CodegenOptions::default();
        assert!(!opts.recoverable_contracts);
    }

    #[test]
    fn codegen_options_custom_values() {
        let opts = CodegenOptions {
            optimize: true,
            debug_info: false,
            recoverable_contracts: true,
        };
        assert!(opts.optimize);
        assert!(!opts.debug_info);
        assert!(opts.recoverable_contracts);
    }

    // ---------------------------------------------------------------
    // CodegenError tests
    // ---------------------------------------------------------------

    #[test]
    fn codegen_error_cranelift_display() {
        let err = CodegenError::Cranelift("test error".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("test error"));
    }

    #[test]
    fn codegen_error_unsupported_target_display() {
        let err = CodegenError::UnsupportedTarget("arm64".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("arm64"));
    }

    #[test]
    fn codegen_error_module_error_display() {
        let err = CodegenError::ModuleError("link failed".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("link failed"));
    }
}
