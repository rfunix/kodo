//! # `kodo_contracts` — Contract Verification for the Kodo Language
//!
//! This crate handles the verification of `requires` (preconditions) and
//! `ensures` (postconditions) contracts attached to function signatures.
//!
//! Contracts are first-class citizens in Kodo — not comments, not assertions,
//! but part of the type system that affects compilation. This makes AI-authored
//! code **correct by construction**: agents declare what must be true, and the
//! compiler proves it.
//!
//! ## Verification Modes
//!
//! - **Static** (`smt` feature): Uses Z3 SMT solver to prove contracts at compile time
//! - **Runtime**: Generates runtime checks for contracts that can't be statically verified
//! - **Both**: Static where possible, runtime fallback
//! - **None**: Skip contract checking (not recommended)
//!
//! ## Current Status
//!
//! Runtime check generation is implemented. Contract expressions are validated
//! for well-formedness (must be boolean expressions without side effects).
//! When the `smt` feature is enabled, Z3 is used to attempt static verification
//! of contracts before falling back to runtime checks.
//!
//! ## Academic References
//!
//! - **\[SF\]** *Software Foundations* Vol. 1–2 — Hoare logic foundations;
//!   `requires`/`ensures` map directly to Hoare triples `{P} code {Q}`.
//! - **\[CC\]** *The Calculus of Computation* Ch. 1–6 — Propositional and
//!   first-order logic as the language of contract expressions.
//! - **\[CC\]** *The Calculus of Computation* Ch. 10–12 — Decision procedures
//!   and SMT solving; informs our Z3 integration for static verification.
//!
//! See `docs/REFERENCES.md` for the full bibliography.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![warn(clippy::pedantic)]

#[cfg(feature = "smt")]
pub mod smt;

use kodo_ast::{Expr, Function, Span};
use thiserror::Error;

/// Errors from contract verification.
#[derive(Debug, Error)]
pub enum ContractError {
    /// A precondition could not be statically verified.
    #[error("precondition cannot be verified at {span:?}: {message}")]
    PreconditionUnverifiable {
        /// Human-readable description of the issue.
        message: String,
        /// Source location.
        span: Span,
    },
    /// A postcondition could not be statically verified.
    #[error("postcondition cannot be verified at {span:?}: {message}")]
    PostconditionUnverifiable {
        /// Human-readable description of the issue.
        message: String,
        /// Source location.
        span: Span,
    },
    /// A contract expression is malformed.
    #[error("invalid contract expression at {span:?}: {message}")]
    InvalidExpression {
        /// Human-readable description of the issue.
        message: String,
        /// Source location.
        span: Span,
    },
    /// A contract was statically refuted by the SMT solver.
    #[error("contract refuted at {span:?}: {counter_example}")]
    StaticRefutation {
        /// The counter-example found by Z3.
        counter_example: String,
        /// Source location.
        span: Span,
    },
}

/// Alias for results in this crate.
pub type Result<T> = std::result::Result<T, ContractError>;

/// The mode in which contracts should be checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContractMode {
    /// Use Z3 to prove contracts at compile time.
    Static,
    /// Insert runtime checks for contracts.
    #[default]
    Runtime,
    /// Static where possible, runtime fallback.
    Both,
    /// Skip all contract checking.
    None,
    /// Insert runtime checks that log warnings but do not abort.
    ///
    /// In this mode, contract violations print a warning to stderr and
    /// execution continues with a default return value. Useful for production
    /// services that should not crash on contract violations.
    Recoverable,
}

/// A contract attached to a function.
#[derive(Debug, Clone)]
pub struct Contract {
    /// The kind of contract (precondition or postcondition).
    pub kind: ContractKind,
    /// The expression that must hold.
    pub expr: kodo_ast::Expr,
    /// Source span.
    pub span: Span,
}

/// Whether a contract is a precondition or postcondition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractKind {
    /// A `requires` clause — must hold when the function is called.
    Requires,
    /// An `ensures` clause — must hold when the function returns.
    Ensures,
}

/// A runtime check to be inserted into the compiled output.
///
/// This is a data structure representing what check to insert — it does not
/// perform code generation itself. Later compiler phases (MIR, codegen) use
/// this to emit the actual assertion code.
#[derive(Debug, Clone)]
pub struct RuntimeCheck {
    /// Whether this is a precondition or postcondition check.
    pub kind: ContractKind,
    /// Human-readable message to display when the check fails at runtime.
    pub message: String,
    /// The boolean expression to evaluate at runtime.
    pub expr: Expr,
    /// Source span of the original contract clause.
    pub span: Span,
}

/// A summary of contracts attached to a single function.
///
/// Useful for diagnostics and reporting — gives a quick overview of how
/// many preconditions and postconditions a function declares, and whether
/// all of them are well-formed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractSummary {
    /// Number of `requires` (precondition) clauses.
    pub preconditions: usize,
    /// Number of `ensures` (postcondition) clauses.
    pub postconditions: usize,
    /// Whether all contract expressions passed validation.
    pub all_valid: bool,
}

/// Extracts the span from an expression.
///
/// Every `Expr` variant carries a `Span`; this helper retrieves it
/// without requiring a match at every call site.
#[must_use]
pub fn expr_span(expr: &Expr) -> Span {
    match expr {
        Expr::IntLit(_, span)
        | Expr::FloatLit(_, span)
        | Expr::StringLit(_, span)
        | Expr::BoolLit(_, span)
        | Expr::Ident(_, span)
        | Expr::BinaryOp { span, .. }
        | Expr::UnaryOp { span, .. }
        | Expr::Call { span, .. }
        | Expr::If { span, .. }
        | Expr::FieldAccess { span, .. }
        | Expr::StructLit { span, .. }
        | Expr::EnumVariantExpr { span, .. }
        | Expr::Match { span, .. }
        | Expr::Try { span, .. }
        | Expr::OptionalChain { span, .. }
        | Expr::NullCoalesce { span, .. }
        | Expr::Range { span, .. }
        | Expr::Closure { span, .. }
        | Expr::Is { span, .. }
        | Expr::Await { span, .. }
        | Expr::StringInterp { span, .. }
        | Expr::TupleLit(_, span)
        | Expr::TupleIndex { span, .. } => *span,
        Expr::Block(block) => block.span,
    }
}

/// Extracts [`Contract`] structs from a function's `requires` and `ensures` clauses.
///
/// Each expression in `function.requires` becomes a [`ContractKind::Requires`] contract,
/// and each expression in `function.ensures` becomes a [`ContractKind::Ensures`] contract.
#[must_use]
pub fn extract_contracts(function: &Function) -> Vec<Contract> {
    let mut contracts = Vec::with_capacity(function.requires.len() + function.ensures.len());

    for expr in &function.requires {
        contracts.push(Contract {
            kind: ContractKind::Requires,
            span: expr_span(expr),
            expr: expr.clone(),
        });
    }

    for expr in &function.ensures {
        contracts.push(Contract {
            kind: ContractKind::Ensures,
            span: expr_span(expr),
            expr: expr.clone(),
        });
    }

    contracts
}

/// Validates that a contract expression is well-formed.
///
/// A valid contract expression must be a boolean expression — comparisons,
/// logical operators, boolean literals, or identifiers. Function calls are
/// rejected because contracts must be side-effect-free.
///
/// # Errors
///
/// Returns [`ContractError::InvalidExpression`] if the expression contains
/// constructs not allowed in contracts (e.g., function calls, string literals,
/// if expressions, or blocks).
pub fn validate_contract_expr(expr: &Expr) -> Result<()> {
    match expr {
        // Boolean literals, identifiers, and integer literals are all valid
        // in contract expressions — booleans and identifiers as top-level
        // predicates, integers as operands in comparisons.
        Expr::BoolLit(..) | Expr::Ident(..) | Expr::IntLit(..) | Expr::FloatLit(..) => Ok(()),

        // Binary operations: both operands must be valid. All binary ops are
        // acceptable in contracts — comparison ops produce booleans, arithmetic
        // ops are valid as sub-expressions of comparisons, and logical ops
        // combine booleans.
        Expr::BinaryOp { left, right, .. } => {
            validate_contract_expr(left)?;
            validate_contract_expr(right)
        }

        // Unary operations: the operand must be valid.
        Expr::UnaryOp { operand, .. } => validate_contract_expr(operand),

        // String literals are not valid boolean expressions in contracts.
        Expr::StringLit(_, span) => Err(ContractError::InvalidExpression {
            message: "string literals are not valid in contract expressions".to_string(),
            span: *span,
        }),

        // Method calls: allow pure, side-effect-free built-in methods like
        // `.length()` in contracts. These are essential for string length and
        // array bounds predicates. General function calls are still rejected.
        Expr::Call { callee, args, span } => {
            if is_allowed_contract_method(callee, args) {
                // Validate the receiver object in the method call.
                if let Expr::FieldAccess { object, .. } = callee.as_ref() {
                    validate_contract_expr(object)
                } else {
                    Ok(())
                }
            } else {
                Err(ContractError::InvalidExpression {
                    message: "function calls are not allowed in contract expressions \
                              (only .length() is permitted)"
                        .to_string(),
                    span: *span,
                })
            }
        }

        // If expressions are too complex for contract expressions.
        Expr::If { span, .. } => Err(ContractError::InvalidExpression {
            message: "if expressions are not allowed in contract expressions".to_string(),
            span: *span,
        }),

        // Field access is valid — allows checking struct fields in contracts.
        Expr::FieldAccess { object, .. } => validate_contract_expr(object),

        // Block expressions are not valid in contracts.
        Expr::Block(block) => Err(ContractError::InvalidExpression {
            message: "block expressions are not allowed in contract expressions".to_string(),
            span: block.span,
        }),

        // Struct literals are not valid in contracts.
        Expr::StructLit { span, .. } => Err(ContractError::InvalidExpression {
            message: "struct literals are not allowed in contract expressions".to_string(),
            span: *span,
        }),

        // Enum variants are not valid in contracts.
        Expr::EnumVariantExpr { span, .. } => Err(ContractError::InvalidExpression {
            message: "enum variants are not allowed in contract expressions".to_string(),
            span: *span,
        }),

        // Match expressions are not valid in contracts.
        Expr::Match { span, .. } => Err(ContractError::InvalidExpression {
            message: "match expressions are not allowed in contract expressions".to_string(),
            span: *span,
        }),

        // Range expressions are not valid in contracts.
        Expr::Range { span, .. } => Err(ContractError::InvalidExpression {
            message: "range expressions are not allowed in contract expressions".to_string(),
            span: *span,
        }),

        // Try operator is not valid in contracts.
        Expr::Try { span, .. } => Err(ContractError::InvalidExpression {
            message: "try operator `?` is not allowed in contract expressions".to_string(),
            span: *span,
        }),

        // Optional chaining is not valid in contracts.
        Expr::OptionalChain { span, .. } => Err(ContractError::InvalidExpression {
            message: "optional chaining `?.` is not allowed in contract expressions".to_string(),
            span: *span,
        }),

        // Null coalescing is not valid in contracts.
        Expr::NullCoalesce { span, .. } => Err(ContractError::InvalidExpression {
            message: "null coalescing `??` is not allowed in contract expressions".to_string(),
            span: *span,
        }),

        // Closures are not valid in contracts.
        Expr::Closure { span, .. } => Err(ContractError::InvalidExpression {
            message: "closures are not allowed in contract expressions".to_string(),
            span: *span,
        }),

        // Type test (is) is not valid in contracts.
        Expr::Is { span, .. } => Err(ContractError::InvalidExpression {
            message: "type test `is` is not allowed in contract expressions".to_string(),
            span: *span,
        }),

        // Await is not valid in contracts.
        Expr::Await { span, .. } => Err(ContractError::InvalidExpression {
            message: "await is not allowed in contract expressions".to_string(),
            span: *span,
        }),

        // String interpolation is not valid in contracts.
        Expr::StringInterp { span, .. } => Err(ContractError::InvalidExpression {
            message: "string interpolation is not allowed in contract expressions".to_string(),
            span: *span,
        }),

        // Tuple literals are not valid in contracts.
        Expr::TupleLit(_, span) => Err(ContractError::InvalidExpression {
            message: "tuple literals are not allowed in contract expressions".to_string(),
            span: *span,
        }),

        // Tuple index is not valid in contracts.
        Expr::TupleIndex { span, .. } => Err(ContractError::InvalidExpression {
            message: "tuple index is not allowed in contract expressions".to_string(),
            span: *span,
        }),
    }
}

/// Checks whether a method call is allowed in a contract expression.
///
/// Only pure, side-effect-free built-in methods are permitted:
/// - `.length()` on strings and arrays — returns the length as an integer.
///
/// This whitelist approach ensures contracts remain side-effect-free
/// while allowing common predicate patterns like `s.length() > 0`.
fn is_allowed_contract_method(callee: &Expr, args: &[Expr]) -> bool {
    if let Expr::FieldAccess { field, .. } = callee {
        matches!(field.as_str(), "length") && args.is_empty()
    } else {
        false
    }
}

/// Generates a [`RuntimeCheck`] from a [`Contract`].
///
/// The returned `RuntimeCheck` is a data structure describing the assertion
/// to insert — it does not emit code itself. The `message` field contains a
/// human-readable description of the contract for runtime error reporting.
#[must_use]
pub fn generate_runtime_check(contract: &Contract) -> RuntimeCheck {
    let kind_label = match contract.kind {
        ContractKind::Requires => "precondition",
        ContractKind::Ensures => "postcondition",
    };

    let message = format!(
        "{kind_label} violated at {}..{}",
        contract.span.start, contract.span.end
    );

    RuntimeCheck {
        kind: contract.kind,
        message,
        expr: contract.expr.clone(),
        span: contract.span,
    }
}

/// Verifies contracts according to the specified mode.
///
/// Validates each contract expression for well-formedness and determines
/// how many contracts need runtime checks versus static verification.
///
/// - In [`ContractMode::None`]: skips all validation, returns zero counts.
/// - In [`ContractMode::Runtime`]: validates expressions, counts all as runtime checks.
/// - In [`ContractMode::Static`] (with `smt` feature): attempts Z3 proof first;
///   proved contracts increment `static_verified`, refuted contracts become errors,
///   unknown contracts fall back to runtime.
/// - In [`ContractMode::Both`]: same as `Static` — static where possible, runtime fallback.
/// - Without the `smt` feature, `Static` and `Both` modes fall back entirely to runtime.
///
/// # Errors
///
/// Returns [`ContractError`] if a contract expression is malformed and the
/// mode is not [`ContractMode::None`], or if a contract is statically refuted.
pub fn verify_contracts(contracts: &[Contract], mode: ContractMode) -> Result<VerificationResult> {
    if mode == ContractMode::None {
        return Ok(VerificationResult {
            static_verified: 0,
            runtime_checks_needed: 0,
            failures: Vec::new(),
        });
    }

    let mut failures = Vec::new();
    let mut static_verified: usize = 0;
    let mut runtime_checks_needed: usize = 0;
    let use_smt = mode == ContractMode::Static || mode == ContractMode::Both;

    for contract in contracts {
        match validate_contract_expr(&contract.expr) {
            Ok(()) => {
                if use_smt {
                    match try_smt_verify(contract) {
                        SmtOutcome::Proved => {
                            static_verified += 1;
                        }
                        SmtOutcome::Refuted(counter_example) => {
                            // In `Both` mode, a refuted contract falls back to
                            // runtime — Z3 found a counter-example but the
                            // contract may still hold at specific call sites.
                            // In strict `Static` mode, a refutation is an error.
                            if mode == ContractMode::Both {
                                runtime_checks_needed += 1;
                            } else {
                                failures.push(ContractError::StaticRefutation {
                                    counter_example,
                                    span: contract.span,
                                });
                            }
                        }
                        SmtOutcome::Fallback => {
                            runtime_checks_needed += 1;
                        }
                    }
                } else {
                    runtime_checks_needed += 1;
                }
            }
            Err(err) => {
                failures.push(err);
            }
        }
    }

    Ok(VerificationResult {
        static_verified,
        runtime_checks_needed,
        failures,
    })
}

/// Internal outcome of an SMT verification attempt.
#[allow(dead_code)]
enum SmtOutcome {
    /// Z3 proved the contract holds.
    Proved,
    /// Z3 refuted the contract with a counter-example.
    Refuted(String),
    /// SMT verification was not available or inconclusive — fall back to runtime.
    Fallback,
}

/// Attempts SMT verification of a single contract.
///
/// When the `smt` feature is enabled, delegates to the Z3-based verifier.
/// Without it, always returns `Fallback`.
fn try_smt_verify(contract: &Contract) -> SmtOutcome {
    #[cfg(feature = "smt")]
    {
        let smt_fn = match contract.kind {
            ContractKind::Requires => smt::verify_precondition,
            ContractKind::Ensures => smt::verify_postcondition,
        };
        match smt_fn(&contract.expr) {
            smt::SmtResult::Proved => SmtOutcome::Proved,
            smt::SmtResult::Refuted(msg) => SmtOutcome::Refuted(msg),
            smt::SmtResult::Unknown => SmtOutcome::Fallback,
        }
    }
    #[cfg(not(feature = "smt"))]
    {
        let _ = contract;
        SmtOutcome::Fallback
    }
}

/// The result of contract verification.
#[derive(Debug)]
pub struct VerificationResult {
    /// Number of contracts proven statically.
    pub static_verified: usize,
    /// Number of contracts that need runtime checks.
    pub runtime_checks_needed: usize,
    /// Any verification failures.
    pub failures: Vec<ContractError>,
}

/// Summarizes the contracts attached to a function.
///
/// Counts preconditions and postconditions separately and checks whether
/// all contract expressions are well-formed. This is useful for diagnostics,
/// LSP hover information, and compilation reports.
#[must_use]
pub fn summarize_function_contracts(function: &Function) -> ContractSummary {
    let contracts = extract_contracts(function);

    let preconditions = contracts
        .iter()
        .filter(|c| c.kind == ContractKind::Requires)
        .count();
    let postconditions = contracts
        .iter()
        .filter(|c| c.kind == ContractKind::Ensures)
        .count();

    let all_valid = contracts
        .iter()
        .all(|c| validate_contract_expr(&c.expr).is_ok());

    ContractSummary {
        preconditions,
        postconditions,
        all_valid,
    }
}

/// Creates a [`Contract`] from a refinement constraint.
///
/// Substitutes `self` in the constraint expression with the given variable name,
/// creating a `Requires` contract suitable for verification.
#[must_use]
pub fn refinement_contract(constraint: &Expr, var_name: &str, span: Span) -> Contract {
    let substituted = substitute_self(constraint, var_name);
    Contract {
        kind: ContractKind::Requires,
        expr: substituted,
        span,
    }
}

/// Replaces `Ident("self")` with `Ident(var_name)` recursively in an expression.
///
/// Used by refinement types to bind the constraint's `self` keyword to the
/// actual variable being constrained.
#[must_use]
pub fn substitute_self(expr: &Expr, var_name: &str) -> Expr {
    match expr {
        Expr::Ident(name, span) if name == "self" => Expr::Ident(var_name.to_string(), *span),
        Expr::BinaryOp {
            left,
            op,
            right,
            span,
        } => Expr::BinaryOp {
            left: Box::new(substitute_self(left, var_name)),
            op: *op,
            right: Box::new(substitute_self(right, var_name)),
            span: *span,
        },
        Expr::UnaryOp { op, operand, span } => Expr::UnaryOp {
            op: *op,
            operand: Box::new(substitute_self(operand, var_name)),
            span: *span,
        },
        Expr::FieldAccess {
            object,
            field,
            span,
        } => Expr::FieldAccess {
            object: Box::new(substitute_self(object, var_name)),
            field: field.clone(),
            span: *span,
        },
        other => other.clone(),
    }
}

/// Verifies a contract with refinement type constraints as assumptions.
///
/// When a function has parameters with refined types, the refinement constraints
/// should be injected as hypotheses before verifying the function's own contracts.
/// For example, if `port: Port` where `type Port = Int requires { self > 0 && self < 65535 }`,
/// the constraint `port > 0 && port < 65535` is assumed to hold when verifying
/// the function's `requires`/`ensures` clauses.
///
/// `refinement_assumptions` contains the substituted constraint expressions
/// (with `self` already replaced by the parameter name).
///
/// # Errors
///
/// Returns [`ContractError`] if a contract expression is malformed or refuted.
pub fn verify_contracts_with_refinements(
    contracts: &[Contract],
    refinement_assumptions: &[Expr],
    mode: ContractMode,
) -> Result<VerificationResult> {
    if mode == ContractMode::None {
        return Ok(VerificationResult {
            static_verified: 0,
            runtime_checks_needed: 0,
            failures: Vec::new(),
        });
    }

    let mut failures = Vec::new();
    let mut static_verified: usize = 0;
    let mut runtime_checks_needed: usize = 0;
    let use_smt = mode == ContractMode::Static || mode == ContractMode::Both;

    for contract in contracts {
        match validate_contract_expr(&contract.expr) {
            Ok(()) => {
                if use_smt && !refinement_assumptions.is_empty() {
                    match try_smt_verify_with_refinements(contract, refinement_assumptions) {
                        SmtOutcome::Proved => {
                            static_verified += 1;
                        }
                        SmtOutcome::Refuted(counter_example) => {
                            if mode == ContractMode::Both {
                                runtime_checks_needed += 1;
                            } else {
                                failures.push(ContractError::StaticRefutation {
                                    counter_example,
                                    span: contract.span,
                                });
                            }
                        }
                        SmtOutcome::Fallback => {
                            runtime_checks_needed += 1;
                        }
                    }
                } else if use_smt {
                    match try_smt_verify(contract) {
                        SmtOutcome::Proved => {
                            static_verified += 1;
                        }
                        SmtOutcome::Refuted(counter_example) => {
                            if mode == ContractMode::Both {
                                runtime_checks_needed += 1;
                            } else {
                                failures.push(ContractError::StaticRefutation {
                                    counter_example,
                                    span: contract.span,
                                });
                            }
                        }
                        SmtOutcome::Fallback => {
                            runtime_checks_needed += 1;
                        }
                    }
                } else {
                    runtime_checks_needed += 1;
                }
            }
            Err(err) => {
                failures.push(err);
            }
        }
    }

    Ok(VerificationResult {
        static_verified,
        runtime_checks_needed,
        failures,
    })
}

/// Attempts SMT verification with refinement type constraints as hypotheses.
fn try_smt_verify_with_refinements(
    contract: &Contract,
    refinement_assumptions: &[Expr],
) -> SmtOutcome {
    #[cfg(feature = "smt")]
    {
        let assumption_refs: Vec<&Expr> = refinement_assumptions.iter().collect();
        match smt::verify_with_refinements(&assumption_refs, &contract.expr) {
            smt::SmtResult::Proved => SmtOutcome::Proved,
            smt::SmtResult::Refuted(msg) => SmtOutcome::Refuted(msg),
            smt::SmtResult::Unknown => SmtOutcome::Fallback,
        }
    }
    #[cfg(not(feature = "smt"))]
    {
        let _ = (contract, refinement_assumptions);
        SmtOutcome::Fallback
    }
}

/// Verifies a refinement constraint via the contract infrastructure.
///
/// Substitutes `self` with the given variable name, creates a `Requires` contract,
/// and verifies it using the specified mode.
///
/// # Errors
///
/// Returns [`ContractError`] if the constraint expression is malformed or refuted.
pub fn verify_refinement(
    constraint: &Expr,
    var_name: &str,
    span: Span,
    mode: ContractMode,
) -> Result<VerificationResult> {
    let contract = refinement_contract(constraint, var_name, span);
    verify_contracts(&[contract], mode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_ast::{BinOp, Block, NodeId, Param, Span, TypeExpr, UnaryOp};

    /// Helper: creates a simple boolean literal expression.
    fn bool_expr(val: bool) -> Expr {
        Expr::BoolLit(val, Span::new(0, 4))
    }

    /// Helper: creates an identifier expression.
    fn ident_expr(name: &str) -> Expr {
        Expr::Ident(name.to_string(), Span::new(0, name.len() as u32))
    }

    /// Helper: creates a comparison expression `left > right`.
    fn gt_expr(left: Expr, right: Expr) -> Expr {
        let span = Span::new(0, 10);
        Expr::BinaryOp {
            left: Box::new(left),
            op: BinOp::Gt,
            right: Box::new(right),
            span,
        }
    }

    /// Helper: creates an integer literal expression.
    fn int_expr(val: i64) -> Expr {
        Expr::IntLit(val, Span::new(0, 3))
    }

    /// Helper: creates a minimal function with the given requires/ensures.
    fn make_function(requires: Vec<Expr>, ensures: Vec<Expr>) -> Function {
        Function {
            id: NodeId(0),
            span: Span::new(0, 100),
            name: "test_fn".to_string(),
            generic_params: vec![],
            annotations: vec![],
            params: vec![Param {
                name: "x".to_string(),
                ty: TypeExpr::Named("Int".to_string()),
                span: Span::new(10, 15),
                ownership: kodo_ast::Ownership::Owned,
            }],
            return_type: TypeExpr::Named("Int".to_string()),
            requires,
            ensures,
            is_async: false,
            body: Block {
                span: Span::new(50, 100),
                stmts: vec![],
            },
        }
    }

    #[test]
    fn contract_mode_default_is_runtime() {
        assert_eq!(ContractMode::default(), ContractMode::Runtime);
    }

    #[test]
    fn verify_empty_contracts() {
        let result = verify_contracts(&[], ContractMode::Runtime);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| panic!("already checked"));
        assert_eq!(result.static_verified, 0);
        assert_eq!(result.runtime_checks_needed, 0);
        assert!(result.failures.is_empty());
    }

    // --- validate_contract_expr tests ---

    #[test]
    fn validate_bool_literal_is_valid() {
        assert!(validate_contract_expr(&bool_expr(true)).is_ok());
        assert!(validate_contract_expr(&bool_expr(false)).is_ok());
    }

    #[test]
    fn validate_identifier_is_valid() {
        assert!(validate_contract_expr(&ident_expr("x")).is_ok());
    }

    #[test]
    fn validate_int_literal_is_valid() {
        assert!(validate_contract_expr(&int_expr(42)).is_ok());
    }

    #[test]
    fn validate_comparison_is_valid() {
        let expr = gt_expr(ident_expr("x"), int_expr(0));
        assert!(validate_contract_expr(&expr).is_ok());
    }

    #[test]
    fn validate_logical_and_is_valid() {
        let left = gt_expr(ident_expr("x"), int_expr(0));
        let right = bool_expr(true);
        let expr = Expr::BinaryOp {
            left: Box::new(left),
            op: BinOp::And,
            right: Box::new(right),
            span: Span::new(0, 20),
        };
        assert!(validate_contract_expr(&expr).is_ok());
    }

    #[test]
    fn validate_unary_not_is_valid() {
        let expr = Expr::UnaryOp {
            op: UnaryOp::Not,
            operand: Box::new(bool_expr(true)),
            span: Span::new(0, 5),
        };
        assert!(validate_contract_expr(&expr).is_ok());
    }

    #[test]
    fn validate_field_access_is_valid() {
        let expr = Expr::FieldAccess {
            object: Box::new(ident_expr("self")),
            field: "count".to_string(),
            span: Span::new(0, 10),
        };
        assert!(validate_contract_expr(&expr).is_ok());
    }

    #[test]
    fn validate_string_literal_is_invalid() {
        let expr = Expr::StringLit("hello".to_string(), Span::new(0, 7));
        let result = validate_contract_expr(&expr);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ContractError::InvalidExpression { .. }),
            "expected InvalidExpression, got {err:?}"
        );
    }

    #[test]
    fn validate_function_call_is_invalid() {
        let expr = Expr::Call {
            callee: Box::new(ident_expr("foo")),
            args: vec![],
            span: Span::new(0, 5),
        };
        let result = validate_contract_expr(&expr);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, ContractError::InvalidExpression { .. }),
            "expected InvalidExpression, got {err:?}"
        );
    }

    #[test]
    fn validate_if_expression_is_invalid() {
        let expr = Expr::If {
            condition: Box::new(bool_expr(true)),
            then_branch: Block {
                span: Span::new(0, 5),
                stmts: vec![],
            },
            else_branch: None,
            span: Span::new(0, 10),
        };
        let result = validate_contract_expr(&expr);
        assert!(result.is_err());
    }

    #[test]
    fn validate_block_expression_is_invalid() {
        let expr = Expr::Block(Block {
            span: Span::new(0, 5),
            stmts: vec![],
        });
        let result = validate_contract_expr(&expr);
        assert!(result.is_err());
    }

    #[test]
    fn validate_nested_call_in_binary_is_invalid() {
        let call = Expr::Call {
            callee: Box::new(ident_expr("len")),
            args: vec![ident_expr("x")],
            span: Span::new(0, 6),
        };
        let expr = gt_expr(call, int_expr(0));
        let result = validate_contract_expr(&expr);
        assert!(result.is_err());
    }

    // --- extract_contracts tests ---

    #[test]
    fn extract_contracts_from_function_with_no_contracts() {
        let func = make_function(vec![], vec![]);
        let contracts = extract_contracts(&func);
        assert!(contracts.is_empty());
    }

    #[test]
    fn extract_contracts_from_function_with_requires() {
        let func = make_function(vec![gt_expr(ident_expr("x"), int_expr(0))], vec![]);
        let contracts = extract_contracts(&func);
        assert_eq!(contracts.len(), 1);
        assert_eq!(contracts[0].kind, ContractKind::Requires);
    }

    #[test]
    fn extract_contracts_from_function_with_ensures() {
        let func = make_function(vec![], vec![bool_expr(true)]);
        let contracts = extract_contracts(&func);
        assert_eq!(contracts.len(), 1);
        assert_eq!(contracts[0].kind, ContractKind::Ensures);
    }

    #[test]
    fn extract_contracts_preserves_order() {
        let req1 = gt_expr(ident_expr("x"), int_expr(0));
        let req2 = gt_expr(ident_expr("y"), int_expr(0));
        let ens1 = bool_expr(true);
        let func = make_function(vec![req1, req2], vec![ens1]);
        let contracts = extract_contracts(&func);
        assert_eq!(contracts.len(), 3);
        assert_eq!(contracts[0].kind, ContractKind::Requires);
        assert_eq!(contracts[1].kind, ContractKind::Requires);
        assert_eq!(contracts[2].kind, ContractKind::Ensures);
    }

    // --- generate_runtime_check tests ---

    #[test]
    fn generate_runtime_check_precondition() {
        let contract = Contract {
            kind: ContractKind::Requires,
            expr: gt_expr(ident_expr("x"), int_expr(0)),
            span: Span::new(10, 20),
        };
        let check = generate_runtime_check(&contract);
        assert_eq!(check.kind, ContractKind::Requires);
        assert!(check.message.contains("precondition"));
        assert_eq!(check.span, Span::new(10, 20));
    }

    #[test]
    fn generate_runtime_check_postcondition() {
        let contract = Contract {
            kind: ContractKind::Ensures,
            expr: bool_expr(true),
            span: Span::new(30, 40),
        };
        let check = generate_runtime_check(&contract);
        assert_eq!(check.kind, ContractKind::Ensures);
        assert!(check.message.contains("postcondition"));
        assert_eq!(check.span, Span::new(30, 40));
    }

    // --- verify_contracts tests with different modes ---

    #[test]
    fn verify_contracts_none_mode_skips_validation() {
        // Even invalid contracts are accepted in None mode.
        let contracts = vec![Contract {
            kind: ContractKind::Requires,
            expr: Expr::StringLit("invalid".to_string(), Span::new(0, 9)),
            span: Span::new(0, 9),
        }];
        let result = verify_contracts(&contracts, ContractMode::None);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| panic!("already checked"));
        assert_eq!(result.runtime_checks_needed, 0);
        assert!(result.failures.is_empty());
    }

    #[test]
    fn verify_contracts_runtime_mode_counts_valid() {
        let contracts = vec![
            Contract {
                kind: ContractKind::Requires,
                expr: gt_expr(ident_expr("x"), int_expr(0)),
                span: Span::new(0, 10),
            },
            Contract {
                kind: ContractKind::Ensures,
                expr: bool_expr(true),
                span: Span::new(10, 20),
            },
        ];
        let result = verify_contracts(&contracts, ContractMode::Runtime);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| panic!("already checked"));
        assert_eq!(result.runtime_checks_needed, 2);
        assert_eq!(result.static_verified, 0);
        assert!(result.failures.is_empty());
    }

    #[test]
    fn verify_contracts_runtime_mode_reports_invalid() {
        let contracts = vec![
            Contract {
                kind: ContractKind::Requires,
                expr: gt_expr(ident_expr("x"), int_expr(0)),
                span: Span::new(0, 10),
            },
            Contract {
                kind: ContractKind::Ensures,
                expr: Expr::StringLit("bad".to_string(), Span::new(10, 15)),
                span: Span::new(10, 15),
            },
        ];
        let result = verify_contracts(&contracts, ContractMode::Runtime);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| panic!("already checked"));
        assert_eq!(result.runtime_checks_needed, 1);
        assert_eq!(result.failures.len(), 1);
    }

    #[test]
    fn verify_contracts_static_mode_without_smt_falls_back() {
        let contracts = vec![Contract {
            kind: ContractKind::Requires,
            expr: bool_expr(true),
            span: Span::new(0, 4),
        }];
        let result = verify_contracts(&contracts, ContractMode::Static);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| panic!("already checked"));
        // Without SMT feature: falls back to runtime.
        // With SMT feature: Z3 proves `true`, so static_verified = 1.
        #[cfg(not(feature = "smt"))]
        {
            assert_eq!(result.static_verified, 0);
            assert_eq!(result.runtime_checks_needed, 1);
        }
        #[cfg(feature = "smt")]
        {
            assert_eq!(result.static_verified, 1);
            assert_eq!(result.runtime_checks_needed, 0);
        }
    }

    #[test]
    fn verify_contracts_both_mode_without_smt_falls_back() {
        let contracts = vec![Contract {
            kind: ContractKind::Requires,
            expr: bool_expr(true),
            span: Span::new(0, 4),
        }];
        let result = verify_contracts(&contracts, ContractMode::Both);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| panic!("already checked"));
        #[cfg(not(feature = "smt"))]
        {
            assert_eq!(result.static_verified, 0);
            assert_eq!(result.runtime_checks_needed, 1);
        }
        #[cfg(feature = "smt")]
        {
            assert_eq!(result.static_verified, 1);
            assert_eq!(result.runtime_checks_needed, 0);
        }
    }

    // --- summarize_function_contracts tests ---

    #[test]
    fn summarize_empty_function() {
        let func = make_function(vec![], vec![]);
        let summary = summarize_function_contracts(&func);
        assert_eq!(summary.preconditions, 0);
        assert_eq!(summary.postconditions, 0);
        assert!(summary.all_valid);
    }

    #[test]
    fn summarize_function_with_valid_contracts() {
        let func = make_function(
            vec![gt_expr(ident_expr("x"), int_expr(0))],
            vec![bool_expr(true)],
        );
        let summary = summarize_function_contracts(&func);
        assert_eq!(summary.preconditions, 1);
        assert_eq!(summary.postconditions, 1);
        assert!(summary.all_valid);
    }

    #[test]
    fn summarize_function_with_invalid_contract() {
        let func = make_function(
            vec![Expr::Call {
                callee: Box::new(ident_expr("foo")),
                args: vec![],
                span: Span::new(0, 5),
            }],
            vec![bool_expr(true)],
        );
        let summary = summarize_function_contracts(&func);
        assert_eq!(summary.preconditions, 1);
        assert_eq!(summary.postconditions, 1);
        assert!(!summary.all_valid);
    }

    // --- expr_span tests ---

    #[test]
    fn expr_span_extracts_correct_span() {
        let span = Span::new(5, 15);
        assert_eq!(expr_span(&Expr::BoolLit(true, span)), span);
        assert_eq!(expr_span(&Expr::IntLit(42, span)), span);
        assert_eq!(expr_span(&Expr::Ident("x".to_string(), span)), span);
    }

    #[test]
    fn contract_error_static_refutation_variant() {
        let err = ContractError::StaticRefutation {
            counter_example: "b = 0".to_string(),
            span: Span::new(0, 10),
        };
        let msg = format!("{err}");
        assert!(msg.contains("refuted"));
        assert!(msg.contains("b = 0"));
    }

    #[test]
    fn smt_outcome_fallback_without_feature() {
        // Regardless of feature, runtime mode always uses runtime checks
        let ne_expr = Expr::BinaryOp {
            left: Box::new(ident_expr("b")),
            op: BinOp::Ne,
            right: Box::new(int_expr(0)),
            span: Span::new(0, 10),
        };
        let contracts = vec![Contract {
            kind: ContractKind::Requires,
            expr: ne_expr,
            span: Span::new(0, 10),
        }];
        let result = verify_contracts(&contracts, ContractMode::Runtime);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| panic!("already checked"));
        assert_eq!(result.runtime_checks_needed, 1);
        assert_eq!(result.static_verified, 0);
    }

    // --- E2E SMT verification tests (require `smt` feature) ---

    #[cfg(feature = "smt")]
    #[test]
    fn smt_e2e_trivial_true_statically_proved() {
        // `requires { true }` should be statically proved by Z3
        let contracts = vec![Contract {
            kind: ContractKind::Requires,
            expr: bool_expr(true),
            span: Span::new(0, 4),
        }];
        let result = verify_contracts(&contracts, ContractMode::Static);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| panic!("already checked"));
        assert_eq!(result.static_verified, 1);
        assert_eq!(result.runtime_checks_needed, 0);
        assert!(result.failures.is_empty());
    }

    #[cfg(feature = "smt")]
    #[test]
    fn smt_e2e_trivial_false_statically_refuted() {
        // `requires { false }` should be statically refuted by Z3
        let contracts = vec![Contract {
            kind: ContractKind::Requires,
            expr: bool_expr(false),
            span: Span::new(0, 5),
        }];
        let result = verify_contracts(&contracts, ContractMode::Static);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| panic!("already checked"));
        assert_eq!(result.static_verified, 0);
        assert_eq!(result.failures.len(), 1);
        assert!(matches!(
            result.failures[0],
            ContractError::StaticRefutation { .. }
        ));
    }

    #[cfg(feature = "smt")]
    #[test]
    fn smt_e2e_ne_zero_falls_back_to_runtime() {
        // `requires { b != 0 }` cannot be proved without caller context,
        // so Z3 refutes it (counter-example b=0). In Static mode this is
        // reported as a refutation.
        let ne = Expr::BinaryOp {
            left: Box::new(ident_expr("b")),
            op: BinOp::Ne,
            right: Box::new(int_expr(0)),
            span: Span::new(0, 10),
        };
        let contracts = vec![Contract {
            kind: ContractKind::Requires,
            expr: ne,
            span: Span::new(0, 10),
        }];
        let result = verify_contracts(&contracts, ContractMode::Static);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| panic!("already checked"));
        // Z3 finds counter-example b=0, so it's a refutation
        assert_eq!(result.failures.len(), 1);
        assert!(matches!(
            result.failures[0],
            ContractError::StaticRefutation { .. }
        ));
    }

    #[cfg(feature = "smt")]
    #[test]
    fn smt_e2e_field_access_expr_translates_to_z3() {
        // Field access expressions ARE now translatable to Z3 (Phase 35.1).
        // `self.count > 0` translates but cannot be proved without context,
        // so Z3 refutes it with a counter-example (e.g., self.count = 0).
        let field_access = Expr::FieldAccess {
            object: Box::new(ident_expr("self")),
            field: "count".to_string(),
            span: Span::new(0, 10),
        };
        let gt = Expr::BinaryOp {
            left: Box::new(field_access),
            op: BinOp::Gt,
            right: Box::new(int_expr(0)),
            span: Span::new(0, 15),
        };
        let contracts = vec![Contract {
            kind: ContractKind::Requires,
            expr: gt,
            span: Span::new(0, 15),
        }];
        let result = verify_contracts(&contracts, ContractMode::Static);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| panic!("already checked"));
        // Z3 can now translate field access; without context, it refutes
        assert_eq!(result.failures.len(), 1);
        assert!(matches!(
            result.failures[0],
            ContractError::StaticRefutation { .. }
        ));
    }

    #[cfg(feature = "smt")]
    #[test]
    fn smt_e2e_closure_expr_falls_back_to_runtime() {
        // Closure expressions are still unsupported in Z3, so they
        // should fall back to runtime checks (Unknown → Fallback).
        let closure = Expr::Closure {
            params: vec![],
            return_type: None,
            body: Box::new(Expr::BoolLit(true, Span::new(0, 4))),
            span: Span::new(0, 10),
        };
        let contracts = vec![Contract {
            kind: ContractKind::Requires,
            expr: closure,
            span: Span::new(0, 10),
        }];
        // Closures are rejected by validate_contract_expr, so they become failures
        let result = verify_contracts(&contracts, ContractMode::Static);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| panic!("already checked"));
        assert_eq!(result.failures.len(), 1);
        assert!(matches!(
            result.failures[0],
            ContractError::InvalidExpression { .. }
        ));
    }

    #[cfg(feature = "smt")]
    #[test]
    fn smt_e2e_both_mode_proves_tautology() {
        // `ensures { true }` in Both mode should be statically proved
        let contracts = vec![Contract {
            kind: ContractKind::Ensures,
            expr: bool_expr(true),
            span: Span::new(0, 4),
        }];
        let result = verify_contracts(&contracts, ContractMode::Both);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| panic!("already checked"));
        assert_eq!(result.static_verified, 1);
        assert_eq!(result.runtime_checks_needed, 0);
        assert!(result.failures.is_empty());
    }

    #[cfg(feature = "smt")]
    #[test]
    fn smt_e2e_equality_tautology_proved() {
        // `requires { x == x }` is a tautology — Z3 proves it
        let eq = Expr::BinaryOp {
            left: Box::new(ident_expr("x")),
            op: BinOp::Eq,
            right: Box::new(ident_expr("x")),
            span: Span::new(0, 10),
        };
        let contracts = vec![Contract {
            kind: ContractKind::Requires,
            expr: eq,
            span: Span::new(0, 10),
        }];
        let result = verify_contracts(&contracts, ContractMode::Static);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| panic!("already checked"));
        assert_eq!(result.static_verified, 1);
        assert_eq!(result.runtime_checks_needed, 0);
        assert!(result.failures.is_empty());
    }

    #[cfg(feature = "smt")]
    #[test]
    fn smt_e2e_mixed_contracts_proved_and_refuted() {
        // Two contracts: `requires { true }` (provable) and `requires { false }` (refuted)
        let contracts = vec![
            Contract {
                kind: ContractKind::Requires,
                expr: bool_expr(true),
                span: Span::new(0, 4),
            },
            Contract {
                kind: ContractKind::Requires,
                expr: bool_expr(false),
                span: Span::new(5, 10),
            },
        ];
        let result = verify_contracts(&contracts, ContractMode::Static);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| panic!("already checked"));
        assert_eq!(result.static_verified, 1);
        assert_eq!(result.failures.len(), 1);
        assert!(matches!(
            result.failures[0],
            ContractError::StaticRefutation { .. }
        ));
    }

    #[cfg(feature = "smt")]
    #[test]
    fn smt_e2e_both_mode_refuted_falls_back_to_runtime() {
        // In `Both` mode, a refuted contract falls back to runtime instead of
        // being reported as a failure. This allows contracts like `requires { b != 0 }`
        // to be checked at call sites at runtime.
        let ne = Expr::BinaryOp {
            left: Box::new(ident_expr("b")),
            op: BinOp::Ne,
            right: Box::new(int_expr(0)),
            span: Span::new(0, 10),
        };
        let contracts = vec![Contract {
            kind: ContractKind::Requires,
            expr: ne,
            span: Span::new(0, 10),
        }];
        let result = verify_contracts(&contracts, ContractMode::Both);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| panic!("already checked"));
        // In Both mode, refuted → runtime fallback (not a failure)
        assert_eq!(result.runtime_checks_needed, 1);
        assert_eq!(result.static_verified, 0);
        assert!(result.failures.is_empty());
    }

    #[cfg(feature = "smt")]
    #[test]
    fn smt_e2e_function_with_contracts_static_mode() {
        // Full pipeline: extract contracts from a function, verify statically
        let func = make_function(vec![bool_expr(true)], vec![bool_expr(true)]);
        let contracts = extract_contracts(&func);
        assert_eq!(contracts.len(), 2);

        let result = verify_contracts(&contracts, ContractMode::Static);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| panic!("already checked"));
        assert_eq!(result.static_verified, 2);
        assert_eq!(result.runtime_checks_needed, 0);
        assert!(result.failures.is_empty());
    }

    // --- Phase 35.1: Struct field predicate validation ---

    #[test]
    fn validate_field_access_in_contract_is_valid() {
        // `point.x > 0` should be valid in a contract
        let field = Expr::FieldAccess {
            object: Box::new(ident_expr("point")),
            field: "x".to_string(),
            span: Span::new(0, 7),
        };
        let expr = gt_expr(field, int_expr(0));
        assert!(validate_contract_expr(&expr).is_ok());
    }

    #[test]
    fn validate_length_method_call_in_contract_is_valid() {
        // `s.length() > 0` should be valid in a contract
        let length_call = Expr::Call {
            callee: Box::new(Expr::FieldAccess {
                object: Box::new(ident_expr("s")),
                field: "length".to_string(),
                span: Span::new(0, 8),
            }),
            args: vec![],
            span: Span::new(0, 10),
        };
        let expr = gt_expr(length_call, int_expr(0));
        assert!(validate_contract_expr(&expr).is_ok());
    }

    #[test]
    fn validate_generic_method_call_in_contract_is_invalid() {
        // `s.foo()` should still be invalid — only .length() is whitelisted
        let foo_call = Expr::Call {
            callee: Box::new(Expr::FieldAccess {
                object: Box::new(ident_expr("s")),
                field: "foo".to_string(),
                span: Span::new(0, 5),
            }),
            args: vec![],
            span: Span::new(0, 7),
        };
        let result = validate_contract_expr(&foo_call);
        assert!(result.is_err());
    }

    #[test]
    fn validate_plain_function_call_still_invalid() {
        // `foo(x)` should still be invalid — only method calls are considered
        let expr = Expr::Call {
            callee: Box::new(ident_expr("foo")),
            args: vec![ident_expr("x")],
            span: Span::new(0, 6),
        };
        let result = validate_contract_expr(&expr);
        assert!(result.is_err());
    }

    // --- Phase 35.2: Refinement type SMT integration ---

    #[test]
    fn substitute_self_replaces_correctly() {
        // `self > 0` with var_name "port" → `port > 0`
        let constraint = gt_expr(ident_expr("self"), int_expr(0));
        let substituted = substitute_self(&constraint, "port");
        if let Expr::BinaryOp { left, .. } = &substituted {
            if let Expr::Ident(name, _) = left.as_ref() {
                assert_eq!(name, "port");
            } else {
                panic!("expected Ident");
            }
        } else {
            panic!("expected BinaryOp");
        }
    }

    #[test]
    fn substitute_self_in_field_access() {
        // `self.count > 0` with var_name "obj" → `obj.count > 0`
        let field = Expr::FieldAccess {
            object: Box::new(ident_expr("self")),
            field: "count".to_string(),
            span: Span::new(0, 10),
        };
        let constraint = gt_expr(field, int_expr(0));
        let substituted = substitute_self(&constraint, "obj");
        if let Expr::BinaryOp { left, .. } = &substituted {
            if let Expr::FieldAccess { object, field, .. } = left.as_ref() {
                if let Expr::Ident(name, _) = object.as_ref() {
                    assert_eq!(name, "obj");
                    assert_eq!(field, "count");
                } else {
                    panic!("expected Ident");
                }
            } else {
                panic!("expected FieldAccess");
            }
        } else {
            panic!("expected BinaryOp");
        }
    }

    #[cfg(feature = "smt")]
    #[test]
    fn smt_e2e_verify_with_refinement_port_constraint() {
        // Simulate: type Port = Int requires { self > 0 && self < 65535 }
        // Function: fn serve(port: Port) requires { port > 0 }
        // The refinement constraint should allow Z3 to prove the requires clause.
        let port_constraint = Expr::BinaryOp {
            left: Box::new(gt_expr(ident_expr("port"), int_expr(0))),
            op: BinOp::And,
            right: Box::new(Expr::BinaryOp {
                left: Box::new(ident_expr("port")),
                op: BinOp::Lt,
                right: Box::new(int_expr(65535)),
                span: Span::new(0, 15),
            }),
            span: Span::new(0, 30),
        };
        let contracts = vec![Contract {
            kind: ContractKind::Requires,
            expr: gt_expr(ident_expr("port"), int_expr(0)),
            span: Span::new(0, 10),
        }];
        let result =
            verify_contracts_with_refinements(&contracts, &[port_constraint], ContractMode::Static);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| panic!("already checked"));
        assert_eq!(result.static_verified, 1);
        assert_eq!(result.runtime_checks_needed, 0);
        assert!(result.failures.is_empty());
    }

    #[cfg(feature = "smt")]
    #[test]
    fn smt_e2e_field_access_predicate_proved() {
        // `point.x > 0` should be translatable to Z3 and provable with hypothesis
        let field_x = Expr::FieldAccess {
            object: Box::new(ident_expr("point")),
            field: "x".to_string(),
            span: Span::new(0, 7),
        };
        let hypothesis = gt_expr(field_x.clone(), int_expr(0));
        let contracts = vec![Contract {
            kind: ContractKind::Requires,
            expr: gt_expr(field_x, int_expr(0)),
            span: Span::new(0, 15),
        }];
        let result =
            verify_contracts_with_refinements(&contracts, &[hypothesis], ContractMode::Static);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| panic!("already checked"));
        assert_eq!(result.static_verified, 1);
    }

    #[cfg(feature = "smt")]
    #[test]
    fn smt_e2e_refinements_none_mode_skips() {
        let contracts = vec![Contract {
            kind: ContractKind::Requires,
            expr: bool_expr(true),
            span: Span::new(0, 4),
        }];
        let result = verify_contracts_with_refinements(&contracts, &[], ContractMode::None);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| panic!("already checked"));
        assert_eq!(result.static_verified, 0);
        assert_eq!(result.runtime_checks_needed, 0);
    }
}
