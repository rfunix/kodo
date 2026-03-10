//! # SMT Verification via Z3
//!
//! Translates Kodo contract expressions into Z3 assertions and checks
//! satisfiability. This module is only available when the `smt` feature is
//! enabled — without it, all contracts fall back to runtime checks.
//!
//! ## Academic References
//!
//! - **\[CC\]** *The Calculus of Computation* Ch. 10–12 — Decision procedures
//!   and SMT solving. Our `expr_to_z3` function implements the translation
//!   from the contract sub-language to QF_LIA (quantifier-free linear integer
//!   arithmetic) with boolean connectives.

use kodo_ast::{BinOp, Expr, UnaryOp};
use z3::ast::Ast;

/// The outcome of an SMT verification attempt.
///
/// Maps directly to Z3 solver results:
/// - `Proved` means the negation of the contract is unsatisfiable (the contract always holds).
/// - `Refuted` means Z3 found a counter-example.
/// - `Unknown` means Z3 could not determine satisfiability (e.g., timeout).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmtResult {
    /// The contract was proven to hold in all cases.
    Proved,
    /// The contract was refuted — Z3 found a counter-example.
    Refuted(String),
    /// Z3 could not determine the result (timeout or incomplete theory).
    Unknown,
}

/// Z3 solver timeout in milliseconds.
const Z3_TIMEOUT_MS: u32 = 5_000;

/// Translates a Kodo [`Expr`] to a Z3 boolean or integer AST node.
///
/// Supports:
/// - Integer literals → Z3 `Int` constants
/// - Boolean literals → Z3 `Bool` constants
/// - Identifiers → Z3 `Int` or `Bool` constants (treated as uninterpreted)
/// - Arithmetic: `+`, `-`, `*`, `/`, `%`
/// - Comparisons: `==`, `!=`, `<`, `>`, `<=`, `>=`
/// - Logical: `&&`, `||`, `!`
///
/// Returns `None` if the expression contains unsupported constructs (e.g.,
/// field access, closures, function calls).
pub fn expr_to_z3<'ctx>(ctx: &'ctx z3::Context, expr: &Expr) -> Option<Z3Expr<'ctx>> {
    match expr {
        Expr::IntLit(value, _) => Some(Z3Expr::Int(z3::ast::Int::from_i64(ctx, *value))),
        Expr::BoolLit(value, _) => Some(Z3Expr::Bool(z3::ast::Bool::from_bool(ctx, *value))),
        Expr::Ident(name, _) => {
            // Identifiers are uninterpreted integer constants by default.
            // In a more sophisticated system we would track types, but for
            // contract verification of integer-typed params this is sufficient.
            Some(Z3Expr::Int(z3::ast::Int::new_const(ctx, name.as_str())))
        }
        Expr::UnaryOp { op, operand, .. } => {
            let inner = expr_to_z3(ctx, operand)?;
            match op {
                UnaryOp::Not => {
                    let b = inner.as_bool()?;
                    Some(Z3Expr::Bool(b.not()))
                }
                UnaryOp::Neg => {
                    let i = inner.as_int()?;
                    Some(Z3Expr::Int(i.unary_minus()))
                }
            }
        }
        Expr::BinaryOp {
            left, op, right, ..
        } => translate_binop(ctx, left, *op, right),
        // Unsupported expression kinds — return None so the caller can
        // fall back to runtime verification.
        _ => None,
    }
}

/// A Z3 expression that is either an integer or a boolean.
///
/// This enum is needed because Kodo's `Expr` can represent both integer
/// arithmetic sub-expressions and boolean predicates, and Z3 distinguishes
/// between the two sorts.
#[derive(Debug)]
pub enum Z3Expr<'ctx> {
    /// A Z3 integer expression.
    Int(z3::ast::Int<'ctx>),
    /// A Z3 boolean expression.
    Bool(z3::ast::Bool<'ctx>),
}

impl<'ctx> Z3Expr<'ctx> {
    /// Extracts the inner boolean, returning `None` if this is an integer.
    fn as_bool(self) -> Option<z3::ast::Bool<'ctx>> {
        match self {
            Self::Bool(b) => Some(b),
            Self::Int(_) => None,
        }
    }

    /// Extracts the inner integer, returning `None` if this is a boolean.
    fn as_int(self) -> Option<z3::ast::Int<'ctx>> {
        match self {
            Self::Int(i) => Some(i),
            Self::Bool(_) => None,
        }
    }
}

/// Translates a binary operation to a Z3 expression.
fn translate_binop<'ctx>(
    ctx: &'ctx z3::Context,
    left: &Expr,
    op: BinOp,
    right: &Expr,
) -> Option<Z3Expr<'ctx>> {
    let lhs = expr_to_z3(ctx, left)?;
    let rhs = expr_to_z3(ctx, right)?;

    match op {
        // Arithmetic — both sides must be integers
        BinOp::Add => {
            let l = lhs.as_int()?;
            let r = rhs.as_int()?;
            Some(Z3Expr::Int(z3::ast::Int::add(ctx, &[&l, &r])))
        }
        BinOp::Sub => {
            let l = lhs.as_int()?;
            let r = rhs.as_int()?;
            Some(Z3Expr::Int(z3::ast::Int::sub(ctx, &[&l, &r])))
        }
        BinOp::Mul => {
            let l = lhs.as_int()?;
            let r = rhs.as_int()?;
            Some(Z3Expr::Int(z3::ast::Int::mul(ctx, &[&l, &r])))
        }
        BinOp::Div => {
            let l = lhs.as_int()?;
            let r = rhs.as_int()?;
            Some(Z3Expr::Int(l.div(&r)))
        }
        BinOp::Mod => {
            let l = lhs.as_int()?;
            let r = rhs.as_int()?;
            Some(Z3Expr::Int(l.modulo(&r)))
        }
        // Comparisons — both sides must be integers, result is boolean
        BinOp::Eq => {
            let l = lhs.as_int()?;
            let r = rhs.as_int()?;
            Some(Z3Expr::Bool(l._eq(&r)))
        }
        BinOp::Ne => {
            let l = lhs.as_int()?;
            let r = rhs.as_int()?;
            Some(Z3Expr::Bool(z3::ast::Ast::distinct(ctx, &[&l, &r])))
        }
        BinOp::Lt => {
            let l = lhs.as_int()?;
            let r = rhs.as_int()?;
            Some(Z3Expr::Bool(l.lt(&r)))
        }
        BinOp::Gt => {
            let l = lhs.as_int()?;
            let r = rhs.as_int()?;
            Some(Z3Expr::Bool(l.gt(&r)))
        }
        BinOp::Le => {
            let l = lhs.as_int()?;
            let r = rhs.as_int()?;
            Some(Z3Expr::Bool(l.le(&r)))
        }
        BinOp::Ge => {
            let l = lhs.as_int()?;
            let r = rhs.as_int()?;
            Some(Z3Expr::Bool(l.ge(&r)))
        }
        // Logical — both sides must be booleans
        BinOp::And => {
            let l = lhs.as_bool()?;
            let r = rhs.as_bool()?;
            Some(Z3Expr::Bool(z3::ast::Bool::and(ctx, &[&l, &r])))
        }
        BinOp::Or => {
            let l = lhs.as_bool()?;
            let r = rhs.as_bool()?;
            Some(Z3Expr::Bool(z3::ast::Bool::or(ctx, &[&l, &r])))
        }
    }
}

/// Attempts to prove a precondition using Z3.
///
/// To prove a precondition, we check whether the **negation** of the contract
/// expression is unsatisfiable. If the negation is unsat, the original
/// expression is a tautology (always true) and the precondition is proved.
///
/// Note: without additional context (e.g., call-site argument constraints),
/// this can only prove trivially true contracts like `requires { true }`.
/// More sophisticated verification would add caller constraints as assumptions.
pub fn verify_precondition(expr: &Expr) -> SmtResult {
    verify_expr(expr)
}

/// Attempts to prove a postcondition using Z3.
///
/// Same strategy as [`verify_precondition`] — checks if the negation of the
/// postcondition is unsatisfiable. In practice, postcondition verification
/// would also need the function body as additional context.
pub fn verify_postcondition(expr: &Expr) -> SmtResult {
    verify_expr(expr)
}

/// Core verification: checks if an expression is a tautology via Z3.
///
/// Creates a Z3 context with a timeout, translates the expression,
/// negates it, and checks satisfiability.
fn verify_expr(expr: &Expr) -> SmtResult {
    let mut cfg = z3::Config::new();
    cfg.set_param_value("timeout", &Z3_TIMEOUT_MS.to_string());
    let ctx = z3::Context::new(&cfg);
    let solver = z3::Solver::new(&ctx);

    let z3_expr = match expr_to_z3(&ctx, expr) {
        Some(e) => e,
        None => return SmtResult::Unknown,
    };

    let bool_expr = match z3_expr {
        Z3Expr::Bool(b) => b,
        Z3Expr::Int(_) => {
            // An integer expression cannot be a contract predicate.
            return SmtResult::Unknown;
        }
    };

    // Assert the negation: if UNSAT, the original is always true.
    solver.assert(&bool_expr.not());

    match solver.check() {
        z3::SatResult::Unsat => SmtResult::Proved,
        z3::SatResult::Sat => {
            // Extract a counter-example from the model.
            let model_str = solver
                .get_model()
                .map_or_else(|| "no model available".to_string(), |m| m.to_string());
            SmtResult::Refuted(format!("counter-example: {model_str}"))
        }
        z3::SatResult::Unknown => SmtResult::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_ast::Span;

    fn bool_expr(val: bool) -> Expr {
        Expr::BoolLit(val, Span::new(0, 4))
    }

    fn int_expr(val: i64) -> Expr {
        Expr::IntLit(val, Span::new(0, 3))
    }

    fn ident_expr(name: &str) -> Expr {
        Expr::Ident(name.to_string(), Span::new(0, name.len() as u32))
    }

    fn ne_expr(left: Expr, right: Expr) -> Expr {
        Expr::BinaryOp {
            left: Box::new(left),
            op: BinOp::Ne,
            right: Box::new(right),
            span: Span::new(0, 10),
        }
    }

    #[test]
    fn trivial_true_is_proved() {
        let result = verify_precondition(&bool_expr(true));
        assert_eq!(result, SmtResult::Proved);
    }

    #[test]
    fn trivial_false_is_refuted() {
        let result = verify_precondition(&bool_expr(false));
        assert!(matches!(result, SmtResult::Refuted(_)));
    }

    #[test]
    fn ne_zero_is_not_trivially_provable() {
        // `b != 0` cannot be proved without context — Z3 finds counter-example b=0
        let expr = ne_expr(ident_expr("b"), int_expr(0));
        let result = verify_precondition(&expr);
        assert!(matches!(result, SmtResult::Refuted(_)));
    }

    #[test]
    fn ne_zero_is_representable_in_z3() {
        // Verify that `b != 0` can be translated to Z3
        let mut cfg = z3::Config::new();
        cfg.set_param_value("timeout", "5000");
        let ctx = z3::Context::new(&cfg);
        let expr = ne_expr(ident_expr("b"), int_expr(0));
        let z3_expr = expr_to_z3(&ctx, &expr);
        assert!(z3_expr.is_some());
    }

    #[test]
    fn smt_result_enum_variants() {
        let proved = SmtResult::Proved;
        let refuted = SmtResult::Refuted("counter-example: b = 0".to_string());
        let unknown = SmtResult::Unknown;

        assert_eq!(proved, SmtResult::Proved);
        assert_ne!(proved, unknown);
        assert!(matches!(refuted, SmtResult::Refuted(_)));
    }

    #[test]
    fn unsupported_expr_returns_unknown() {
        // Field access is not supported in SMT translation
        let expr = Expr::FieldAccess {
            object: Box::new(ident_expr("self")),
            field: "count".to_string(),
            span: Span::new(0, 10),
        };
        let result = verify_precondition(&expr);
        assert_eq!(result, SmtResult::Unknown);
    }

    #[test]
    fn postcondition_verification_works() {
        let result = verify_postcondition(&bool_expr(true));
        assert_eq!(result, SmtResult::Proved);
    }

    #[test]
    fn arithmetic_in_z3() {
        // Verify `a + b > 0` translates correctly (not provable without context)
        let add = Expr::BinaryOp {
            left: Box::new(ident_expr("a")),
            op: BinOp::Add,
            right: Box::new(ident_expr("b")),
            span: Span::new(0, 5),
        };
        let expr = Expr::BinaryOp {
            left: Box::new(add),
            op: BinOp::Gt,
            right: Box::new(int_expr(0)),
            span: Span::new(0, 10),
        };
        let result = verify_precondition(&expr);
        // Not provable without constraints on a and b
        assert!(matches!(result, SmtResult::Refuted(_)));
    }

    #[test]
    fn logical_and_in_z3() {
        // `true && true` should be proved
        let expr = Expr::BinaryOp {
            left: Box::new(bool_expr(true)),
            op: BinOp::And,
            right: Box::new(bool_expr(true)),
            span: Span::new(0, 15),
        };
        let result = verify_precondition(&expr);
        assert_eq!(result, SmtResult::Proved);
    }

    #[test]
    fn logical_or_in_z3() {
        // `false || true` should be proved
        let expr = Expr::BinaryOp {
            left: Box::new(bool_expr(false)),
            op: BinOp::Or,
            right: Box::new(bool_expr(true)),
            span: Span::new(0, 15),
        };
        let result = verify_precondition(&expr);
        assert_eq!(result, SmtResult::Proved);
    }

    #[test]
    fn unary_not_in_z3() {
        // `!false` should be proved
        let expr = Expr::UnaryOp {
            op: UnaryOp::Not,
            operand: Box::new(bool_expr(false)),
            span: Span::new(0, 6),
        };
        let result = verify_precondition(&expr);
        assert_eq!(result, SmtResult::Proved);
    }
}
