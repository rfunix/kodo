//! # `kodo_ast` — Abstract Syntax Tree for the Kōdo Language
//!
//! This crate defines the core AST types shared across all compiler phases.
//! It is the foundational crate with no internal dependencies.
//!
//! Kōdo is designed to be the preferred language for AI agents while remaining
//! fully transparent and auditable by humans. The AST reflects this dual nature:
//! every node carries source location ([`Span`]) and a unique [`NodeId`] for
//! precise error reporting and traceability.
//!
//! ## Key Types
//!
//! - [`Span`] — Source location (byte offsets) for error reporting
//! - [`NodeId`] — Unique identifier for each AST node
//! - [`Module`] — Top-level compilation unit
//! - [`Meta`] — Module metadata block (author, version, intent)
//! - [`Function`] — Function definition with optional contracts
//!
//! ## Academic References
//!
//! - **\[CI\]** *Crafting Interpreters* Ch. 5 — AST node design, expression/statement
//!   hierarchy, and the visitor pattern that informs our enum-based AST.
//! - **\[EC\]** *Engineering a Compiler* Ch. 4–5 — IR taxonomy (AST vs CST), symbol
//!   tables, and the rationale for keeping spans on every node.
//!
//! See `docs/REFERENCES.md` for the full bibliography.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![warn(clippy::pedantic)]

use thiserror::Error;

/// Errors that can occur when constructing or validating AST nodes.
#[derive(Debug, Error)]
pub enum AstError {
    /// A required field was missing from a node.
    #[error("missing required field `{field}` on {node}")]
    MissingField {
        /// The name of the missing field.
        field: String,
        /// The kind of node where the field was expected.
        node: String,
    },
    /// A `NodeId` was duplicated in the tree.
    #[error("duplicate node id: {0}")]
    DuplicateNodeId(u32),
}

/// Alias for results in this crate.
pub type Result<T> = std::result::Result<T, AstError>;

/// A span of source code, represented as byte offsets.
///
/// Used throughout the compiler to map errors and diagnostics
/// back to the original source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Span {
    /// Byte offset of the start of the span (inclusive).
    pub start: u32,
    /// Byte offset of the end of the span (exclusive).
    pub end: u32,
}

impl Span {
    /// Creates a new span from start and end byte offsets.
    #[must_use]
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    /// Returns the length of the span in bytes.
    #[must_use]
    pub fn len(&self) -> u32 {
        self.end - self.start
    }

    /// Returns `true` if the span has zero length.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Merges two spans into one that covers both.
    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// A unique identifier for each AST node, used for cross-referencing
/// between compiler phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NodeId(pub u32);

/// A generator for unique [`NodeId`] values.
#[derive(Debug, Default)]
pub struct NodeIdGen {
    next: u32,
}

impl NodeIdGen {
    /// Creates a new generator starting from 0.
    #[must_use]
    pub fn new() -> Self {
        Self { next: 0 }
    }

    /// Returns the next unique [`NodeId`].
    pub fn next_id(&mut self) -> NodeId {
        let id = NodeId(self.next);
        self.next += 1;
        id
    }
}

/// The top-level compilation unit representing a `.ko` file.
#[derive(Debug, Clone)]
pub struct Module {
    /// Unique identifier for this module node.
    pub id: NodeId,
    /// Source span of the entire module.
    pub span: Span,
    /// The module name.
    pub name: String,
    /// Module metadata block.
    pub meta: Option<Meta>,
    /// Functions defined in this module.
    pub functions: Vec<Function>,
}

/// Metadata block declared with the `meta` keyword.
///
/// Contains information for AI agents and humans about the module's
/// purpose, version, and intended behavior.
#[derive(Debug, Clone)]
pub struct Meta {
    /// Unique identifier for this meta node.
    pub id: NodeId,
    /// Source span.
    pub span: Span,
    /// Key-value pairs in the meta block.
    pub entries: Vec<MetaEntry>,
}

/// A single key-value pair inside a `meta` block.
#[derive(Debug, Clone)]
pub struct MetaEntry {
    /// The key name.
    pub key: String,
    /// The value as a string literal.
    pub value: String,
    /// Source span of the entire entry.
    pub span: Span,
}

/// A type annotation in the source code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpr {
    /// A named type, e.g. `Int`, `String`, `Bool`.
    Named(String),
    /// A generic type, e.g. `List<Int>`.
    Generic(String, Vec<TypeExpr>),
    /// A function type, e.g. `(Int, Int) -> Int`.
    Function(Vec<TypeExpr>, Box<TypeExpr>),
    /// The unit type `()`.
    Unit,
}

/// A function parameter.
#[derive(Debug, Clone)]
pub struct Param {
    /// Parameter name.
    pub name: String,
    /// Type annotation.
    pub ty: TypeExpr,
    /// Source span.
    pub span: Span,
}

/// A function definition.
#[derive(Debug, Clone)]
pub struct Function {
    /// Unique identifier for this function node.
    pub id: NodeId,
    /// Source span of the entire function.
    pub span: Span,
    /// Function name.
    pub name: String,
    /// Parameters.
    pub params: Vec<Param>,
    /// Return type (defaults to unit).
    pub return_type: TypeExpr,
    /// Preconditions (`requires` clauses).
    pub requires: Vec<Expr>,
    /// Postconditions (`ensures` clauses).
    pub ensures: Vec<Expr>,
    /// Function body.
    pub body: Block,
}

/// A block of statements.
#[derive(Debug, Clone)]
pub struct Block {
    /// Source span.
    pub span: Span,
    /// Statements in the block.
    pub stmts: Vec<Stmt>,
}

/// A statement.
#[derive(Debug, Clone)]
pub enum Stmt {
    /// A `let` binding: `let x: Int = 42`
    Let {
        /// Source span.
        span: Span,
        /// Variable name.
        name: String,
        /// Optional type annotation.
        ty: Option<TypeExpr>,
        /// Initializer expression.
        value: Expr,
    },
    /// An expression statement.
    Expr(Expr),
    /// A return statement.
    Return {
        /// Source span.
        span: Span,
        /// Optional return value.
        value: Option<Expr>,
    },
}

/// An expression.
#[derive(Debug, Clone)]
pub enum Expr {
    /// Integer literal.
    IntLit(i64, Span),
    /// String literal.
    StringLit(String, Span),
    /// Boolean literal.
    BoolLit(bool, Span),
    /// Variable reference.
    Ident(String, Span),
    /// Binary operation.
    BinaryOp {
        /// Left operand.
        left: Box<Expr>,
        /// Operator.
        op: BinOp,
        /// Right operand.
        right: Box<Expr>,
        /// Source span.
        span: Span,
    },
    /// Function call.
    Call {
        /// The callee expression.
        callee: Box<Expr>,
        /// Arguments.
        args: Vec<Expr>,
        /// Source span.
        span: Span,
    },
    /// If expression.
    If {
        /// Condition.
        condition: Box<Expr>,
        /// Then branch.
        then_branch: Block,
        /// Optional else branch.
        else_branch: Option<Block>,
        /// Source span.
        span: Span,
    },
    /// Block expression.
    Block(Block),
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `%`
    Mod,
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `<=`
    Le,
    /// `>=`
    Ge,
    /// `&&`
    And,
    /// `||`
    Or,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_merge_covers_both() {
        let a = Span::new(5, 10);
        let b = Span::new(15, 20);
        let merged = a.merge(b);
        assert_eq!(merged.start, 5);
        assert_eq!(merged.end, 20);
    }

    #[test]
    fn span_length_and_empty() {
        let span = Span::new(3, 7);
        assert_eq!(span.len(), 4);
        assert!(!span.is_empty());

        let empty = Span::new(5, 5);
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
    }

    #[test]
    fn node_id_gen_produces_unique_ids() {
        let mut gen = NodeIdGen::new();
        let a = gen.next_id();
        let b = gen.next_id();
        let c = gen.next_id();
        assert_eq!(a, NodeId(0));
        assert_eq!(b, NodeId(1));
        assert_eq!(c, NodeId(2));
    }
}
