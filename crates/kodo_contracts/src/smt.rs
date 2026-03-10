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
//!   from the contract sub-language to `QF_LIA` (quantifier-free linear integer
//!   arithmetic) with boolean connectives.

use kodo_ast::{BinOp, Expr, UnaryOp};

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
const Z3_TIMEOUT_MS: u64 = 5_000;

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
#[must_use]
pub fn expr_to_z3(expr: &Expr) -> Option<Z3Expr> {
    match expr {
        Expr::IntLit(value, _) => Some(Z3Expr::Int(z3::ast::Int::from_i64(*value))),
        Expr::BoolLit(value, _) => Some(Z3Expr::Bool(z3::ast::Bool::from_bool(*value))),
        Expr::Ident(name, _) => {
            // Identifiers are uninterpreted integer constants by default.
            // In a more sophisticated system we would track types, but for
            // contract verification of integer-typed params this is sufficient.
            Some(Z3Expr::Int(z3::ast::Int::new_const(name.as_str())))
        }
        Expr::UnaryOp { op, operand, .. } => {
            let inner = expr_to_z3(operand)?;
            match op {
                UnaryOp::Not => {
                    let b = inner.into_bool()?;
                    Some(Z3Expr::Bool(b.not()))
                }
                UnaryOp::Neg => {
                    let i = inner.into_int()?;
                    Some(Z3Expr::Int(i.unary_minus()))
                }
            }
        }
        Expr::BinaryOp {
            left, op, right, ..
        } => translate_binop(left, *op, right),
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
pub enum Z3Expr {
    /// A Z3 integer expression.
    Int(z3::ast::Int),
    /// A Z3 boolean expression.
    Bool(z3::ast::Bool),
}

impl Z3Expr {
    /// Extracts the inner boolean, returning `None` if this is an integer.
    fn into_bool(self) -> Option<z3::ast::Bool> {
        match self {
            Self::Bool(b) => Some(b),
            Self::Int(_) => None,
        }
    }

    /// Extracts the inner integer, returning `None` if this is a boolean.
    fn into_int(self) -> Option<z3::ast::Int> {
        match self {
            Self::Int(i) => Some(i),
            Self::Bool(_) => None,
        }
    }
}

/// Translates a binary operation to a Z3 expression.
fn translate_binop(left: &Expr, op: BinOp, right: &Expr) -> Option<Z3Expr> {
    let lhs = expr_to_z3(left)?;
    let rhs = expr_to_z3(right)?;

    match op {
        // Arithmetic — both sides must be integers
        BinOp::Add => {
            let l = lhs.into_int()?;
            let r = rhs.into_int()?;
            Some(Z3Expr::Int(z3::ast::Int::add(&[&l, &r])))
        }
        BinOp::Sub => {
            let l = lhs.into_int()?;
            let r = rhs.into_int()?;
            Some(Z3Expr::Int(z3::ast::Int::sub(&[&l, &r])))
        }
        BinOp::Mul => {
            let l = lhs.into_int()?;
            let r = rhs.into_int()?;
            Some(Z3Expr::Int(z3::ast::Int::mul(&[&l, &r])))
        }
        BinOp::Div => {
            let l = lhs.into_int()?;
            let r = rhs.into_int()?;
            Some(Z3Expr::Int(l.div(&r)))
        }
        BinOp::Mod => {
            let l = lhs.into_int()?;
            let r = rhs.into_int()?;
            Some(Z3Expr::Int(l.modulo(&r)))
        }
        // Comparisons — both sides must be integers, result is boolean
        BinOp::Eq => {
            let l = lhs.into_int()?;
            let r = rhs.into_int()?;
            Some(Z3Expr::Bool(l.eq(&r)))
        }
        BinOp::Ne => {
            let l = lhs.into_int()?;
            let r = rhs.into_int()?;
            Some(Z3Expr::Bool(l.ne(&r)))
        }
        BinOp::Lt => {
            let l = lhs.into_int()?;
            let r = rhs.into_int()?;
            Some(Z3Expr::Bool(l.lt(&r)))
        }
        BinOp::Gt => {
            let l = lhs.into_int()?;
            let r = rhs.into_int()?;
            Some(Z3Expr::Bool(l.gt(&r)))
        }
        BinOp::Le => {
            let l = lhs.into_int()?;
            let r = rhs.into_int()?;
            Some(Z3Expr::Bool(l.le(&r)))
        }
        BinOp::Ge => {
            let l = lhs.into_int()?;
            let r = rhs.into_int()?;
            Some(Z3Expr::Bool(l.ge(&r)))
        }
        // Logical — both sides must be booleans
        BinOp::And => {
            let l = lhs.into_bool()?;
            let r = rhs.into_bool()?;
            Some(Z3Expr::Bool(z3::ast::Bool::and(&[&l, &r])))
        }
        BinOp::Or => {
            let l = lhs.into_bool()?;
            let r = rhs.into_bool()?;
            Some(Z3Expr::Bool(z3::ast::Bool::or(&[&l, &r])))
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
#[must_use]
pub fn verify_precondition(expr: &Expr) -> SmtResult {
    verify_expr(expr)
}

/// Attempts to prove a postcondition using Z3.
///
/// Same strategy as [`verify_precondition`] — checks if the negation of the
/// postcondition is unsatisfiable. In practice, postcondition verification
/// would also need the function body as additional context.
#[must_use]
pub fn verify_postcondition(expr: &Expr) -> SmtResult {
    verify_expr(expr)
}

/// Verifies that a conclusion follows from a set of hypotheses.
///
/// Adds each hypothesis as an assumption, then checks if the negation
/// of the conclusion is unsatisfiable under those assumptions. This
/// allows proving contracts like `requires { x > 0 }` implies `x + 1 > 0`.
///
/// # Returns
///
/// - `Proved` if the conclusion necessarily follows from the hypotheses
/// - `Refuted` if Z3 found a counter-example (hypotheses true, conclusion false)
/// - `Unknown` if any expression could not be translated or Z3 timed out
#[must_use]
pub fn verify_with_hypotheses(hypotheses: &[&Expr], conclusion: &Expr) -> SmtResult {
    let mut cfg = z3::Config::new();
    cfg.set_timeout_msec(Z3_TIMEOUT_MS);

    z3::with_z3_config(&cfg, || {
        let solver = z3::Solver::new();

        // Add each hypothesis as an assumption.
        for hyp in hypotheses {
            let Some(z3_hyp) = expr_to_z3(hyp) else {
                return SmtResult::Unknown;
            };
            let bool_hyp = match z3_hyp {
                Z3Expr::Bool(b) => b,
                Z3Expr::Int(_) => return SmtResult::Unknown,
            };
            solver.assert(bool_hyp);
        }

        // Translate the conclusion.
        let Some(z3_concl) = expr_to_z3(conclusion) else {
            return SmtResult::Unknown;
        };
        let bool_concl = match z3_concl {
            Z3Expr::Bool(b) => b,
            Z3Expr::Int(_) => return SmtResult::Unknown,
        };

        // Assert the negation of the conclusion.
        solver.assert(bool_concl.not());

        match solver.check() {
            z3::SatResult::Unsat => SmtResult::Proved,
            z3::SatResult::Sat => {
                let model_str = solver
                    .get_model()
                    .map_or_else(|| "no model available".to_string(), |m| m.to_string());
                SmtResult::Refuted(format!("counter-example: {model_str}"))
            }
            z3::SatResult::Unknown => SmtResult::Unknown,
        }
    })
}

/// Core verification: checks if an expression is a tautology via Z3.
///
/// Creates a Z3 context with a timeout, translates the expression,
/// negates it, and checks satisfiability.
fn verify_expr(expr: &Expr) -> SmtResult {
    let mut cfg = z3::Config::new();
    cfg.set_timeout_msec(Z3_TIMEOUT_MS);

    z3::with_z3_config(&cfg, || {
        let Some(z3_expr) = expr_to_z3(expr) else {
            return SmtResult::Unknown;
        };

        let bool_expr = match z3_expr {
            Z3Expr::Bool(b) => b,
            Z3Expr::Int(_) => {
                // An integer expression cannot be a contract predicate.
                return SmtResult::Unknown;
            }
        };

        let solver = z3::Solver::new();

        // Assert the negation: if UNSAT, the original is always true.
        solver.assert(bool_expr.not());

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
    })
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
        #[allow(clippy::cast_possible_truncation)]
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
        cfg.set_timeout_msec(5000);
        z3::with_z3_config(&cfg, || {
            let expr = ne_expr(ident_expr("b"), int_expr(0));
            let z3_expr = expr_to_z3(&expr);
            assert!(z3_expr.is_some());
        });
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

    // --- Prioridade 4: More complete SMT tests ---

    /// Helper: build `left > right`.
    fn gt_expr(left: Expr, right: Expr) -> Expr {
        Expr::BinaryOp {
            left: Box::new(left),
            op: BinOp::Gt,
            right: Box::new(right),
            span: Span::new(0, 10),
        }
    }

    /// Helper: build `left + right`.
    fn add_expr(left: Expr, right: Expr) -> Expr {
        Expr::BinaryOp {
            left: Box::new(left),
            op: BinOp::Add,
            right: Box::new(right),
            span: Span::new(0, 10),
        }
    }

    /// Helper: build `left < right`.
    fn lt_expr(left: Expr, right: Expr) -> Expr {
        Expr::BinaryOp {
            left: Box::new(left),
            op: BinOp::Lt,
            right: Box::new(right),
            span: Span::new(0, 10),
        }
    }

    /// Helper: build `left * right`.
    fn mul_expr(left: Expr, right: Expr) -> Expr {
        Expr::BinaryOp {
            left: Box::new(left),
            op: BinOp::Mul,
            right: Box::new(right),
            span: Span::new(0, 10),
        }
    }

    #[test]
    fn hypothesis_implies_conclusion() {
        // requires { x > 0 } implies x + 1 > 0
        let hypothesis = gt_expr(ident_expr("x"), int_expr(0));
        let conclusion = gt_expr(add_expr(ident_expr("x"), int_expr(1)), int_expr(0));
        let result = verify_with_hypotheses(&[&hypothesis], &conclusion);
        assert_eq!(result, SmtResult::Proved);
    }

    #[test]
    fn ensures_result_positive_with_negative_return_refuted() {
        // ensures { result > 0 } but we "return -1" → result = -1
        // Hypothesis: result == -1
        // Conclusion: result > 0
        let hypothesis = Expr::BinaryOp {
            left: Box::new(ident_expr("result")),
            op: BinOp::Eq,
            right: Box::new(int_expr(-1)),
            span: Span::new(0, 15),
        };
        let conclusion = gt_expr(ident_expr("result"), int_expr(0));
        let result = verify_with_hypotheses(&[&hypothesis], &conclusion);
        assert!(matches!(result, SmtResult::Refuted(_)));
    }

    #[test]
    fn nonlinear_arithmetic_unknown() {
        // x * x > 0 is generally not provable without constraints in QF_LIA
        // (Z3 may return Refuted with x=0 since x*x == 0 when x == 0)
        let expr = gt_expr(mul_expr(ident_expr("x"), ident_expr("x")), int_expr(0));
        let result = verify_precondition(&expr);
        // x = 0 is a counter-example: 0*0 = 0, which is NOT > 0
        assert!(matches!(result, SmtResult::Refuted(_) | SmtResult::Unknown));
    }

    #[test]
    fn multiple_hypotheses_proved() {
        // If x > 0 and y > 0, then x + y > 0
        let h1 = gt_expr(ident_expr("x"), int_expr(0));
        let h2 = gt_expr(ident_expr("y"), int_expr(0));
        let conclusion = gt_expr(add_expr(ident_expr("x"), ident_expr("y")), int_expr(0));
        let result = verify_with_hypotheses(&[&h1, &h2], &conclusion);
        assert_eq!(result, SmtResult::Proved);
    }

    #[test]
    fn ensures_sum_bounds() {
        // If x < 100 and y < 100, then x + y < 200
        let h1 = lt_expr(ident_expr("x"), int_expr(100));
        let h2 = lt_expr(ident_expr("y"), int_expr(100));
        let conclusion = lt_expr(add_expr(ident_expr("x"), ident_expr("y")), int_expr(200));
        let result = verify_with_hypotheses(&[&h1, &h2], &conclusion);
        assert_eq!(result, SmtResult::Proved);
    }

    #[test]
    fn equality_tautology() {
        // x == x is always true
        let expr = Expr::BinaryOp {
            left: Box::new(ident_expr("x")),
            op: BinOp::Eq,
            right: Box::new(ident_expr("x")),
            span: Span::new(0, 10),
        };
        let result = verify_precondition(&expr);
        assert_eq!(result, SmtResult::Proved);
    }

    #[test]
    fn negation_of_tautology_is_refuted() {
        // !(x == x) should be refuted
        let eq = Expr::BinaryOp {
            left: Box::new(ident_expr("x")),
            op: BinOp::Eq,
            right: Box::new(ident_expr("x")),
            span: Span::new(0, 10),
        };
        let not_eq = Expr::UnaryOp {
            op: UnaryOp::Not,
            operand: Box::new(eq),
            span: Span::new(0, 12),
        };
        let result = verify_precondition(&not_eq);
        assert!(matches!(result, SmtResult::Refuted(_)));
    }

    #[test]
    fn unary_neg_in_z3() {
        // -(5) should translate to Z3
        let expr = Expr::UnaryOp {
            op: UnaryOp::Neg,
            operand: Box::new(int_expr(5)),
            span: Span::new(0, 4),
        };
        let z3_result = expr_to_z3(&expr);
        assert!(z3_result.is_some());
    }

    #[test]
    fn modulo_in_z3() {
        // x % 2 == 0 is not provable without constraints (x can be odd)
        let modulo = Expr::BinaryOp {
            left: Box::new(ident_expr("x")),
            op: BinOp::Mod,
            right: Box::new(int_expr(2)),
            span: Span::new(0, 10),
        };
        let expr = Expr::BinaryOp {
            left: Box::new(modulo),
            op: BinOp::Eq,
            right: Box::new(int_expr(0)),
            span: Span::new(0, 15),
        };
        let result = verify_precondition(&expr);
        assert!(matches!(result, SmtResult::Refuted(_)));
    }
}
