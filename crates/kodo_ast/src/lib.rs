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

/// An import declaration: `import std.option`
#[derive(Debug, Clone)]
pub struct ImportDecl {
    /// The import path segments (e.g. `["std", "option"]`).
    pub path: Vec<String>,
    /// Source span.
    pub span: Span,
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
    /// Import declarations.
    pub imports: Vec<ImportDecl>,
    /// Module metadata block.
    pub meta: Option<Meta>,
    /// User-defined struct type declarations.
    pub type_decls: Vec<TypeDecl>,
    /// User-defined enum type declarations.
    pub enum_decls: Vec<EnumDecl>,
    /// Trait declarations.
    pub trait_decls: Vec<TraitDecl>,
    /// Impl blocks.
    pub impl_blocks: Vec<ImplBlock>,
    /// Actor declarations.
    pub actor_decls: Vec<ActorDecl>,
    /// Intent declarations.
    pub intent_decls: Vec<IntentDecl>,
    /// Functions defined in this module.
    pub functions: Vec<Function>,
}

/// A trait declaration: `trait Name { fn method(self) -> Type }`
#[derive(Debug, Clone)]
pub struct TraitDecl {
    /// Unique identifier.
    pub id: NodeId,
    /// Source span.
    pub span: Span,
    /// Trait name.
    pub name: String,
    /// Method signatures (no bodies).
    pub methods: Vec<TraitMethod>,
}

/// A method signature within a trait declaration.
#[derive(Debug, Clone)]
pub struct TraitMethod {
    /// Method name.
    pub name: String,
    /// Parameters (first is typically `self`).
    pub params: Vec<Param>,
    /// Return type.
    pub return_type: TypeExpr,
    /// Whether the first parameter is `self`.
    pub has_self: bool,
    /// Source span.
    pub span: Span,
}

/// An impl block: `impl TraitName for TypeName { methods }`
#[derive(Debug, Clone)]
pub struct ImplBlock {
    /// Unique identifier.
    pub id: NodeId,
    /// Source span.
    pub span: Span,
    /// The trait being implemented.
    pub trait_name: String,
    /// The type implementing the trait.
    pub type_name: String,
    /// Method implementations.
    pub methods: Vec<Function>,
}

/// A value in an intent configuration block.
#[derive(Debug, Clone)]
pub enum IntentConfigValue {
    /// A string literal value: `key: "value"`.
    StringLit(String, Span),
    /// An integer literal value: `key: 42`.
    IntLit(i64, Span),
    /// A boolean literal value: `key: true`.
    BoolLit(bool, Span),
    /// A float literal value: `key: 0.95`.
    FloatLit(f64, Span),
    /// A function reference: `key: my_function`.
    FnRef(String, Span),
    /// A list of values: `key: ["a", "b"]`.
    List(Vec<IntentConfigValue>, Span),
}

/// A single configuration entry in an intent block: `key: value`.
#[derive(Debug, Clone)]
pub struct IntentConfigEntry {
    /// The configuration key.
    pub key: String,
    /// The configuration value.
    pub value: IntentConfigValue,
    /// Source span of the entire entry.
    pub span: Span,
}

/// An intent declaration: `intent name { key: value, ... }`.
///
/// Intents are Kōdo's most distinctive feature — they declare WHAT should
/// happen, and the resolver maps them to concrete implementations.
#[derive(Debug, Clone)]
pub struct IntentDecl {
    /// Unique identifier.
    pub id: NodeId,
    /// Source span.
    pub span: Span,
    /// The intent name (e.g., `console_app`, `math_module`).
    pub name: String,
    /// Configuration entries.
    pub config: Vec<IntentConfigEntry>,
}

/// An actor declaration: `actor Name { state + handlers }`
#[derive(Debug, Clone)]
pub struct ActorDecl {
    /// Unique identifier.
    pub id: NodeId,
    /// Source span.
    pub span: Span,
    /// Actor name.
    pub name: String,
    /// State fields (like struct fields).
    pub fields: Vec<FieldDef>,
    /// Handler functions.
    pub handlers: Vec<Function>,
}

/// A user-defined struct type declaration: `struct Name<T> { field: Type, ... }`
#[derive(Debug, Clone)]
pub struct TypeDecl {
    /// Unique identifier for this node.
    pub id: NodeId,
    /// Source span.
    pub span: Span,
    /// The struct name.
    pub name: String,
    /// Generic type parameter names (empty for non-generic structs).
    pub generic_params: Vec<String>,
    /// Fields of the struct.
    pub fields: Vec<FieldDef>,
}

/// A field definition within a struct declaration.
#[derive(Debug, Clone)]
pub struct FieldDef {
    /// The field name.
    pub name: String,
    /// The field type annotation.
    pub ty: TypeExpr,
    /// Source span.
    pub span: Span,
}

/// A field initializer in a struct literal: `name: value`.
#[derive(Debug, Clone)]
pub struct FieldInit {
    /// The field name.
    pub name: String,
    /// The value expression.
    pub value: Expr,
    /// Source span.
    pub span: Span,
}

/// A user-defined enum type declaration: `enum Name<T> { Variant1, Variant2(Type) }`
#[derive(Debug, Clone)]
pub struct EnumDecl {
    /// Unique identifier for this node.
    pub id: NodeId,
    /// Source span.
    pub span: Span,
    /// The enum name.
    pub name: String,
    /// Generic type parameter names (empty for non-generic enums).
    pub generic_params: Vec<String>,
    /// Variants of the enum.
    pub variants: Vec<EnumVariant>,
}

/// A variant within an enum declaration.
#[derive(Debug, Clone)]
pub struct EnumVariant {
    /// The variant name.
    pub name: String,
    /// Positional field types (empty for unit variants).
    pub fields: Vec<TypeExpr>,
    /// Source span.
    pub span: Span,
}

/// A match arm: `pattern => body`.
#[derive(Debug, Clone)]
pub struct MatchArm {
    /// The pattern to match against.
    pub pattern: Pattern,
    /// The body expression.
    pub body: Expr,
    /// Source span.
    pub span: Span,
}

/// A pattern in a match expression.
#[derive(Debug, Clone)]
pub enum Pattern {
    /// A variant pattern: `EnumName::Variant(a, b)`.
    Variant {
        /// The enum type name (optional, may be inferred).
        enum_name: Option<String>,
        /// The variant name.
        variant: String,
        /// Bound variable names.
        bindings: Vec<String>,
        /// Source span.
        span: Span,
    },
    /// A wildcard pattern: `_`.
    Wildcard(Span),
    /// A literal pattern.
    Literal(Expr),
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
    /// Optional type shorthand: `T?` is equivalent to `Option<T>`.
    Optional(Box<TypeExpr>),
}

/// A parameter in a closure expression.
#[derive(Debug, Clone)]
pub struct ClosureParam {
    /// Parameter name.
    pub name: String,
    /// Optional type annotation (can be inferred from context).
    pub ty: Option<TypeExpr>,
    /// Source span.
    pub span: Span,
}

/// Ownership qualifier for a parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    /// Owned value (default) — caller transfers ownership.
    Owned,
    /// Borrowed reference — caller retains ownership.
    Ref,
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
    /// Ownership qualifier (`own` or `ref`).
    pub ownership: Ownership,
}

/// An annotation on a function, e.g. `@confidence(0.95)`.
#[derive(Debug, Clone)]
pub struct Annotation {
    /// Annotation name (e.g., `confidence`, `authored_by`).
    pub name: String,
    /// Positional and named arguments.
    pub args: Vec<AnnotationArg>,
    /// Source span.
    pub span: Span,
}

/// An argument to an annotation.
#[derive(Debug, Clone)]
pub enum AnnotationArg {
    /// Positional: `@confidence(0.95)` → `Positional(Expr)`.
    Positional(Expr),
    /// Named: `@authored_by(agent: "claude")` → `Named("agent", Expr)`.
    Named(String, Expr),
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
    /// Whether this is an async function.
    pub is_async: bool,
    /// Generic type parameter names (empty for non-generic functions).
    pub generic_params: Vec<String>,
    /// Annotations (e.g. `@authored_by`, `@confidence`).
    pub annotations: Vec<Annotation>,
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
    /// A `let` binding: `let x: Int = 42` or `let mut y = expr`
    Let {
        /// Source span.
        span: Span,
        /// Whether the binding is mutable (`let mut`).
        mutable: bool,
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
    /// A `while` loop: `while <condition> { <body> }`
    While {
        /// Source span.
        span: Span,
        /// Loop condition (must be `Bool`).
        condition: Expr,
        /// Loop body.
        body: Block,
    },
    /// A `for` loop: `for <name> in <start>..<end> { <body> }`
    For {
        /// Source span.
        span: Span,
        /// Loop variable name.
        name: String,
        /// Start of the range (inclusive).
        start: Expr,
        /// End of the range.
        end: Expr,
        /// Whether the range is inclusive (`..=`).
        inclusive: bool,
        /// Loop body.
        body: Block,
    },
    /// An assignment to an existing variable: `name = value`
    Assign {
        /// Source span.
        span: Span,
        /// Variable name.
        name: String,
        /// New value.
        value: Expr,
    },
    /// An `if let` statement: `if let Pattern = expr { body } else { else_body }`
    IfLet {
        /// Source span.
        span: Span,
        /// The pattern to match.
        pattern: Pattern,
        /// The expression to match against.
        value: Expr,
        /// The body to execute if the pattern matches.
        body: Block,
        /// Optional else body.
        else_body: Option<Block>,
    },
    /// Spawn a structured task: `spawn { body }`
    Spawn {
        /// Source span.
        span: Span,
        /// The task body.
        body: Block,
    },
}

/// An expression.
#[derive(Debug, Clone)]
pub enum Expr {
    /// Integer literal.
    IntLit(i64, Span),
    /// Float literal (e.g., `0.95`, `3.14`).
    FloatLit(f64, Span),
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
    /// Unary operation.
    UnaryOp {
        /// Operator.
        op: UnaryOp,
        /// Operand.
        operand: Box<Expr>,
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
    /// Field access expression: `x.y`
    FieldAccess {
        /// The object being accessed.
        object: Box<Expr>,
        /// The field name.
        field: String,
        /// Source span.
        span: Span,
    },
    /// Struct literal: `Point { x: 1, y: 2 }`
    StructLit {
        /// The struct type name.
        name: String,
        /// Field initializers.
        fields: Vec<FieldInit>,
        /// Source span.
        span: Span,
    },
    /// Enum variant construction: `Color::Red` or `Option::Some(42)`
    EnumVariantExpr {
        /// The enum type name.
        enum_name: String,
        /// The variant name.
        variant: String,
        /// Arguments (empty for unit variants).
        args: Vec<Expr>,
        /// Source span.
        span: Span,
    },
    /// Match expression: `match expr { arms }`
    Match {
        /// The expression being matched.
        expr: Box<Expr>,
        /// The match arms.
        arms: Vec<MatchArm>,
        /// Source span.
        span: Span,
    },
    /// Try operator: `expr?` — propagates errors from Result types.
    Try {
        /// The expression to try.
        operand: Box<Expr>,
        /// Source span.
        span: Span,
    },
    /// Optional chaining: `expr?.field` — accesses field if Some.
    OptionalChain {
        /// The optional expression.
        object: Box<Expr>,
        /// The field to access.
        field: String,
        /// Source span.
        span: Span,
    },
    /// Null coalescing: `expr ?? default` — returns default if None.
    NullCoalesce {
        /// The optional expression.
        left: Box<Expr>,
        /// The default value.
        right: Box<Expr>,
        /// Source span.
        span: Span,
    },
    /// A range expression: `start..end` or `start..=end`
    Range {
        /// Start of the range.
        start: Box<Expr>,
        /// End of the range.
        end: Box<Expr>,
        /// Whether the range is inclusive.
        inclusive: bool,
        /// Source span.
        span: Span,
    },
    /// A closure expression: `|params| body` or `|params| -> RetType { body }`
    Closure {
        /// Closure parameters.
        params: Vec<ClosureParam>,
        /// Optional return type annotation.
        return_type: Option<TypeExpr>,
        /// Closure body (single expression or block).
        body: Box<Expr>,
        /// Source span.
        span: Span,
    },
    /// Type test expression: `expr is TypeName`
    Is {
        /// The expression to test.
        operand: Box<Expr>,
        /// The type/variant to test against.
        type_name: String,
        /// Source span.
        span: Span,
    },
    /// Await expression: `expr.await`
    Await {
        /// The future to await.
        operand: Box<Expr>,
        /// Source span.
        span: Span,
    },
    /// Block expression.
    Block(Block),
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// Logical negation `!`
    Not,
    /// Arithmetic negation `-`
    Neg,
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

/// Severity level for a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// A fatal error that prevents compilation.
    Error,
    /// A non-fatal warning.
    Warning,
    /// An informational note.
    Note,
}

/// A labeled span for richer error messages.
#[derive(Debug, Clone)]
pub struct DiagnosticLabel {
    /// The source span to highlight.
    pub span: Span,
    /// The message to display at this span.
    pub message: String,
}

/// Unified diagnostic trait for all compiler errors.
///
/// Every error type in the compiler should implement this trait
/// to enable unified rendering and structured JSON output.
pub trait Diagnostic: std::fmt::Display {
    /// Returns the unique error code (e.g., "E0200").
    fn code(&self) -> &'static str;
    /// Returns the severity level.
    fn severity(&self) -> Severity;
    /// Returns the primary source span, if available.
    fn span(&self) -> Option<Span>;
    /// Returns the primary error message.
    fn message(&self) -> String;
    /// Returns an optional fix suggestion.
    fn suggestion(&self) -> Option<String> {
        None
    }
    /// Returns additional labeled spans for context.
    fn labels(&self) -> Vec<DiagnosticLabel> {
        Vec::new()
    }
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
