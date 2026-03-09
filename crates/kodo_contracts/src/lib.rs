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
//! The `smt` feature flag gates Z3 integration (not yet implemented).
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
        | Expr::StringLit(_, span)
        | Expr::BoolLit(_, span)
        | Expr::Ident(_, span)
        | Expr::BinaryOp { span, .. }
        | Expr::UnaryOp { span, .. }
        | Expr::Call { span, .. }
        | Expr::If { span, .. }
        | Expr::FieldAccess { span, .. } => *span,
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
        Expr::BoolLit(..) | Expr::Ident(..) | Expr::IntLit(..) => Ok(()),

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

        // Function calls may have side effects — reject them in contracts.
        Expr::Call { span, .. } => Err(ContractError::InvalidExpression {
            message: "function calls are not allowed in contract expressions".to_string(),
            span: *span,
        }),

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
/// - In [`ContractMode::Static`]: validates expressions, counts all as runtime checks
///   (Z3 integration is not yet available, so all fall back to runtime).
/// - In [`ContractMode::Both`]: same as `Static` for now.
///
/// # Errors
///
/// Returns [`ContractError`] if a contract expression is malformed and the
/// mode is not [`ContractMode::None`].
pub fn verify_contracts(contracts: &[Contract], mode: ContractMode) -> Result<VerificationResult> {
    if mode == ContractMode::None {
        return Ok(VerificationResult {
            static_verified: 0,
            runtime_checks_needed: 0,
            failures: Vec::new(),
        });
    }

    let mut failures = Vec::new();
    let mut valid_count: usize = 0;

    for contract in contracts {
        match validate_contract_expr(&contract.expr) {
            Ok(()) => {
                valid_count += 1;
            }
            Err(err) => {
                failures.push(err);
            }
        }
    }

    // Z3 is not yet integrated, so all valid contracts need runtime checks
    // regardless of whether Static or Runtime mode was requested.
    Ok(VerificationResult {
        static_verified: 0,
        runtime_checks_needed: valid_count,
        failures,
    })
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
            params: vec![Param {
                name: "x".to_string(),
                ty: TypeExpr::Named("Int".to_string()),
                span: Span::new(10, 15),
            }],
            return_type: TypeExpr::Named("Int".to_string()),
            requires,
            ensures,
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
    fn verify_contracts_static_mode_falls_back_to_runtime() {
        let contracts = vec![Contract {
            kind: ContractKind::Requires,
            expr: bool_expr(true),
            span: Span::new(0, 4),
        }];
        let result = verify_contracts(&contracts, ContractMode::Static);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| panic!("already checked"));
        // Z3 not integrated, so nothing is statically verified.
        assert_eq!(result.static_verified, 0);
        assert_eq!(result.runtime_checks_needed, 1);
    }

    #[test]
    fn verify_contracts_both_mode_falls_back_to_runtime() {
        let contracts = vec![Contract {
            kind: ContractKind::Requires,
            expr: bool_expr(true),
            span: Span::new(0, 4),
        }];
        let result = verify_contracts(&contracts, ContractMode::Both);
        assert!(result.is_ok());
        let result = result.unwrap_or_else(|_| panic!("already checked"));
        assert_eq!(result.static_verified, 0);
        assert_eq!(result.runtime_checks_needed, 1);
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
}
