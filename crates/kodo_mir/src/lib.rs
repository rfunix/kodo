//! # `kodo_mir` — Mid-Level Intermediate Representation
//!
//! This crate defines the MIR (Mid-level Intermediate Representation) used
//! between the typed AST and code generation. MIR is a control-flow graph
//! representation that enables optimizations and borrow checking.
//!
//! The MIR is designed to be a stable intermediate format that both the
//! Cranelift backend and future LLVM backend can consume.
//!
//! ## Structure
//!
//! - [`MirFunction`] — A function as a control-flow graph of basic blocks
//! - [`BasicBlock`] — A sequence of instructions with a terminator
//! - [`Instruction`] — A single MIR operation
//! - [`Terminator`] — How a basic block transfers control
//!
//! ## Academic References
//!
//! - **\[Tiger\]** *Modern Compiler Implementation in ML* Ch. 7–8 — IR trees,
//!   canonical form, basic blocks, and traces for CFG construction.
//! - **\[EC\]** *Engineering a Compiler* Ch. 5, 8–10 — IR design choices,
//!   data-flow analysis, SSA form, and optimization frameworks.
//!
//! See `docs/REFERENCES.md` for the full bibliography.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![warn(clippy::pedantic)]

pub mod lowering;
pub mod optimize;

use kodo_types::Type;
use thiserror::Error;

/// Errors that can occur during MIR construction or validation.
#[derive(Debug, Error)]
pub enum MirError {
    /// A basic block was referenced but not defined.
    #[error("undefined basic block: {0}")]
    UndefinedBlock(BlockId),
    /// A local variable was referenced but not defined.
    #[error("undefined local: {0}")]
    UndefinedLocal(LocalId),
    /// The MIR function has no entry block.
    #[error("function `{0}` has no entry block")]
    NoEntryBlock(String),
    /// A variable name could not be resolved to a local.
    #[error("undefined variable `{0}`")]
    UndefinedVariable(String),
    /// A type expression could not be resolved.
    #[error("type resolution error: {0}")]
    TypeResolution(String),
    /// A function call callee is not an identifier.
    #[error("non-identifier callee in function call")]
    NonIdentCallee,
}

/// Alias for results in this crate.
pub type Result<T> = std::result::Result<T, MirError>;

/// Identifier for a basic block within a function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32);

impl std::fmt::Display for BlockId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bb{}", self.0)
    }
}

/// Identifier for a local variable within a function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalId(pub u32);

impl std::fmt::Display for LocalId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "_{}", self.0)
    }
}

/// A function in MIR form — a control-flow graph of basic blocks.
#[derive(Debug)]
pub struct MirFunction {
    /// The function name.
    pub name: String,
    /// The return type.
    pub return_type: Type,
    /// Number of function parameters (first N locals).
    pub param_count: usize,
    /// Local variable declarations (params + temporaries).
    pub locals: Vec<Local>,
    /// Basic blocks forming the control-flow graph.
    pub blocks: Vec<BasicBlock>,
    /// The entry block.
    pub entry: BlockId,
}

impl MirFunction {
    /// Returns the number of function parameters.
    #[must_use]
    pub fn param_count(&self) -> usize {
        self.param_count
    }
}

/// A local variable declaration.
#[derive(Debug, Clone)]
pub struct Local {
    /// The local identifier.
    pub id: LocalId,
    /// The type of this local.
    pub ty: Type,
    /// Whether this local is mutable.
    pub mutable: bool,
}

/// A basic block — a straight-line sequence of instructions ending in a terminator.
#[derive(Debug)]
pub struct BasicBlock {
    /// The block identifier.
    pub id: BlockId,
    /// Instructions in this block.
    pub instructions: Vec<Instruction>,
    /// How this block transfers control.
    pub terminator: Terminator,
}

/// A single MIR instruction.
#[derive(Debug, Clone, PartialEq)]
pub enum Instruction {
    /// Assign a value to a local: `local = value`
    Assign(LocalId, Value),
    /// Call a function: `dest = callee(args...)`
    Call {
        /// Where to store the result.
        dest: LocalId,
        /// The function to call.
        callee: String,
        /// The arguments.
        args: Vec<Value>,
    },
    /// Call a function indirectly through a function pointer: `dest = (*func_ptr)(args...)`
    IndirectCall {
        /// Where to store the result.
        dest: LocalId,
        /// The value holding the function pointer.
        callee: Value,
        /// The arguments.
        args: Vec<Value>,
        /// The return type of the function being called.
        return_type: Type,
        /// The parameter types of the function being called.
        param_types: Vec<Type>,
    },
    /// Increment reference count for a heap-allocated value.
    IncRef(LocalId),
    /// Decrement reference count for a heap-allocated value (may free).
    DecRef(LocalId),
}

/// A value in MIR — either a constant, a local reference, or a binary operation.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// An integer constant.
    IntConst(i64),
    /// A 64-bit floating-point constant.
    FloatConst(f64),
    /// A boolean constant.
    BoolConst(bool),
    /// A string constant.
    StringConst(String),
    /// A reference to a local variable.
    Local(LocalId),
    /// A binary operation on two values.
    BinOp(kodo_ast::BinOp, Box<Value>, Box<Value>),
    /// Logical negation of a value.
    Not(Box<Value>),
    /// Arithmetic negation of a value.
    Neg(Box<Value>),
    /// A struct literal value.
    StructLit {
        /// The struct type name.
        name: String,
        /// Field values in declaration order.
        fields: Vec<(String, Value)>,
    },
    /// A field access on a struct value.
    FieldGet {
        /// The struct value being accessed.
        object: Box<Value>,
        /// The field name.
        field: String,
        /// The struct type name (needed for layout in codegen).
        struct_name: String,
    },
    /// An enum variant construction: discriminant + payload values.
    EnumVariant {
        /// The enum type name.
        enum_name: String,
        /// The variant name.
        variant: String,
        /// The discriminant index.
        discriminant: u8,
        /// Payload values for this variant.
        args: Vec<Value>,
    },
    /// Extract the discriminant from an enum value.
    EnumDiscriminant(Box<Value>),
    /// Extract a payload field from an enum value by variant and index.
    EnumPayload {
        /// The enum value.
        value: Box<Value>,
        /// The field index within the variant payload.
        field_index: u32,
    },
    /// The unit value.
    Unit,
    /// A reference to a named function, yielding a function pointer.
    FuncRef(String),
}

/// How a basic block transfers control flow.
#[derive(Debug, Clone, PartialEq)]
pub enum Terminator {
    /// Return from the function with a value.
    Return(Value),
    /// Unconditional jump to another block.
    Goto(BlockId),
    /// Conditional branch: if value then `true_block` else `false_block`.
    Branch {
        /// The condition.
        condition: Value,
        /// Block to jump to if true.
        true_block: BlockId,
        /// Block to jump to if false.
        false_block: BlockId,
    },
    /// Unreachable — the block should never be entered.
    Unreachable,
}

impl MirFunction {
    /// Validates the MIR function for internal consistency.
    ///
    /// # Errors
    ///
    /// Returns [`MirError`] if the function has structural problems.
    pub fn validate(&self) -> Result<()> {
        if self.blocks.is_empty() {
            return Err(MirError::NoEntryBlock(self.name.clone()));
        }

        let block_ids: std::collections::HashSet<_> = self.blocks.iter().map(|b| b.id).collect();

        if !block_ids.contains(&self.entry) {
            return Err(MirError::NoEntryBlock(self.name.clone()));
        }

        Ok(())
    }
}

impl std::fmt::Display for Instruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Assign(local, _value) => write!(f, "{local} = <value>"),
            Self::Call { dest, callee, args } => {
                write!(f, "{dest} = {callee}(")?;
                for (i, _arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "<arg>")?;
                }
                write!(f, ")")
            }
            Self::IndirectCall { dest, .. } => write!(f, "{dest} = <indirect_call>(...)"),
            Self::IncRef(local) => write!(f, "inc_ref {local}"),
            Self::DecRef(local) => write!(f, "dec_ref {local}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_function_fails_validation() {
        let func = MirFunction {
            name: "test".to_string(),
            return_type: Type::Unit,
            param_count: 0,
            locals: vec![],
            blocks: vec![],
            entry: BlockId(0),
        };
        assert!(func.validate().is_err());
    }

    #[test]
    fn valid_function_passes_validation() {
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
        assert!(func.validate().is_ok());
    }

    #[test]
    fn block_id_display() {
        assert_eq!(BlockId(0).to_string(), "bb0");
        assert_eq!(BlockId(42).to_string(), "bb42");
    }

    #[test]
    fn local_id_display() {
        assert_eq!(LocalId(0).to_string(), "_0");
        assert_eq!(LocalId(5).to_string(), "_5");
    }

    #[test]
    fn test_incref_decref_display() {
        let inc = Instruction::IncRef(LocalId(3));
        assert_eq!(inc.to_string(), "inc_ref _3");

        let dec = Instruction::DecRef(LocalId(7));
        assert_eq!(dec.to_string(), "dec_ref _7");
    }

    #[test]
    fn test_incref_decref_in_function() {
        let func = MirFunction {
            name: "test_rc".to_string(),
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
        assert!(func.validate().is_ok());
        assert_eq!(func.blocks[0].instructions.len(), 3);
        assert_eq!(
            func.blocks[0].instructions[1],
            Instruction::IncRef(LocalId(0))
        );
        assert_eq!(
            func.blocks[0].instructions[2],
            Instruction::DecRef(LocalId(0))
        );
    }
}
