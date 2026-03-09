//! # `kodo_contracts` — Contract Verification for the Kōdo Language
//!
//! This crate handles the verification of `requires` (preconditions) and
//! `ensures` (postconditions) contracts attached to function signatures.
//!
//! Contracts are first-class citizens in Kōdo — not comments, not assertions,
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
//! Stub implementation — contract AST nodes are parsed but verification
//! is not yet implemented. The `smt` feature flag gates Z3 integration.
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

use kodo_ast::Span;
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

/// Verifies contracts for a function (stub implementation).
///
/// In the future, this will use the Z3 SMT solver (with the `smt` feature)
/// to attempt static verification, falling back to runtime checks.
///
/// # Errors
///
/// Returns [`ContractError`] if a contract cannot be verified according
/// to the specified mode.
pub fn verify_contracts(
    _contracts: &[Contract],
    _mode: ContractMode,
) -> Result<VerificationResult> {
    // Stub: mark all contracts as needing runtime checks
    Ok(VerificationResult {
        static_verified: 0,
        runtime_checks_needed: 0,
        failures: Vec::new(),
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
