//! Translation of MIR functions to LLVM IR function definitions.
//!
//! Each `MirFunction` is translated to a complete LLVM IR function definition
//! with entry block, basic blocks, and a proper SSA register numbering scheme.

use std::collections::HashMap;

use kodo_mir::{LocalId, MirFunction};
use kodo_types::Type;

use crate::emitter::LLVMEmitter;
use crate::instruction::{emit_instruction, fresh_reg};
use crate::terminator::emit_terminator;
use crate::types::{llvm_return_type, llvm_type};

/// Set of `LocalId`s that have stack slots (alloca) for cross-block SSA safety.
pub(crate) type StackLocals = HashMap<LocalId, String>;

/// Emits LLVM IR for a single MIR function.
///
/// Returns the function definition lines (including `define` and closing `}`).
///
/// # Arguments
/// * `func` - The MIR function to translate.
/// * `struct_defs` - Struct type definitions.
/// * `enum_defs` - Enum type definitions.
/// * `string_constants` - Accumulated string constants (mutated).
/// * `user_functions` - Names of all user-defined functions.
pub(crate) fn emit_function(
    func: &MirFunction,
    struct_defs: &HashMap<String, Vec<(String, Type)>>,
    enum_defs: &HashMap<String, Vec<(String, Vec<Type>)>>,
    string_constants: &mut Vec<(String, String)>,
    user_functions: &[String],
) -> String {
    let mut emitter = LLVMEmitter::new();
    let mut next_reg: u32 = 0;

    // Build the local type map.
    let local_types: HashMap<LocalId, Type> =
        func.locals.iter().map(|l| (l.id, l.ty.clone())).collect();

    // Build the local register map, pre-populating parameters.
    let mut local_regs: HashMap<LocalId, String> = HashMap::new();

    // Build parameter list.
    let mut param_strs = Vec::new();
    for i in 0..func.param_count {
        let local = &func.locals[i];
        let ty_str = llvm_type(&local.ty, struct_defs, enum_defs);
        let reg = fresh_reg(&mut next_reg);
        param_strs.push(format!("{ty_str} {reg}"));
        local_regs.insert(local.id, reg);
    }

    // The exported function name: `main` is renamed to `kodo_main`.
    let fn_name = if func.name == "main" {
        "kodo_main".to_string()
    } else {
        func.name.clone()
    };

    let ret_ty = llvm_return_type(&func.return_type, struct_defs, enum_defs);
    let params_str = param_strs.join(", ");
    emitter.line(&format!("define {ret_ty} @{fn_name}({params_str}) {{"));

    // For multi-block functions, allocate stack slots for non-parameter locals
    // to avoid SSA domination issues when a value defined in one block is used
    // in another block that it doesn't dominate.
    let mut stack_locals: StackLocals = HashMap::new();
    let needs_stack = func.blocks.len() > 1;

    // Emit basic blocks.
    for (block_idx, block) in func.blocks.iter().enumerate() {
        // Block label (entry block is special in LLVM — first block has no explicit label).
        if block_idx == 0 {
            emitter.line("entry:");

            // Emit allocas for non-parameter locals in the entry block.
            if needs_stack {
                for local in func.locals.iter().skip(func.param_count) {
                    let ty_str = llvm_type(&local.ty, struct_defs, enum_defs);
                    if ty_str == "void" {
                        continue;
                    }
                    let alloca_reg = fresh_reg(&mut next_reg);
                    emitter.indent(&format!("{alloca_reg} = alloca {ty_str}, align 8"));
                    stack_locals.insert(local.id, alloca_reg);
                }
            }
        } else {
            emitter.line(&format!("bb{}:", block.id.0));
        }

        // Emit instructions.
        for instr in &block.instructions {
            emit_instruction(
                instr,
                &mut emitter,
                &mut local_regs,
                &local_types,
                &mut next_reg,
                struct_defs,
                enum_defs,
                string_constants,
                user_functions,
                &stack_locals,
            );
        }

        // Emit terminator.
        emit_terminator(
            &block.terminator,
            &mut emitter,
            &mut local_regs,
            &local_types,
            &mut next_reg,
            &func.return_type,
            struct_defs,
            enum_defs,
            string_constants,
            &stack_locals,
        );
    }

    emitter.line("}");
    emitter.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_mir::{BasicBlock, BlockId, Local, Terminator, Value};

    #[test]
    fn emit_empty_void_function() {
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
        let mut string_constants = Vec::new();
        let ir = emit_function(
            &func,
            &HashMap::new(),
            &HashMap::new(),
            &mut string_constants,
            &[],
        );
        assert!(ir.contains("define void @test()"));
        assert!(ir.contains("ret void"));
    }

    #[test]
    fn emit_return_42() {
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
        let mut string_constants = Vec::new();
        let ir = emit_function(
            &func,
            &HashMap::new(),
            &HashMap::new(),
            &mut string_constants,
            &[],
        );
        assert!(ir.contains("define i64 @answer()"));
        assert!(ir.contains("ret i64 42"));
    }

    #[test]
    fn main_renamed_to_kodo_main() {
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
        let mut string_constants = Vec::new();
        let ir = emit_function(
            &func,
            &HashMap::new(),
            &HashMap::new(),
            &mut string_constants,
            &[],
        );
        assert!(ir.contains("@kodo_main"));
        assert!(!ir.contains("@main("));
    }

    #[test]
    fn emit_function_with_params() {
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
        let mut string_constants = Vec::new();
        let ir = emit_function(
            &func,
            &HashMap::new(),
            &HashMap::new(),
            &mut string_constants,
            &[],
        );
        assert!(ir.contains("define i64 @add(i64 %0, i64 %1)"));
        assert!(ir.contains("add i64"));
        assert!(ir.contains("ret i64"));
    }

    #[test]
    fn emit_function_with_branch() {
        let func = MirFunction {
            name: "branchy".to_string(),
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
                    terminator: Terminator::Return(Value::IntConst(0)),
                },
            ],
            entry: BlockId(0),
        };
        let mut string_constants = Vec::new();
        let ir = emit_function(
            &func,
            &HashMap::new(),
            &HashMap::new(),
            &mut string_constants,
            &[],
        );
        assert!(ir.contains("br i1"));
        assert!(ir.contains("bb1:"));
        assert!(ir.contains("bb2:"));
    }
}
