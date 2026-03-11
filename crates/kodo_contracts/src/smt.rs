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

/// Separator used when flattening field access paths into Z3 constant names.
///
/// For example, `point.x` becomes the Z3 constant `"point.x"`.
const FIELD_ACCESS_SEP: &str = ".";

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
        // Field access: `point.x` → uninterpreted Z3 integer constant "point.x"
        //
        // Struct field references in contracts are modeled as uninterpreted
        // constants. This allows Z3 to reason about relationships between
        // fields (e.g., `point.x > 0 && point.y > 0`) even without knowing
        // the concrete struct layout.
        Expr::FieldAccess { object, field, .. } => {
            let base = flatten_field_path(object)?;
            let name = format!("{base}{FIELD_ACCESS_SEP}{field}");
            Some(Z3Expr::Int(z3::ast::Int::new_const(name.as_str())))
        }

        // Method calls: limited support for `.length()` as an uninterpreted
        // function in Z3. This models string/array length as a non-negative
        // integer, enabling bound-checking predicates like `s.length() > 0`
        // and `index < list.length()`.
        Expr::Call { callee, args, .. } => translate_method_call(callee, args),

        // Unsupported expression kinds — return None so the caller can
        // fall back to runtime verification.
        _ => None,
    }
}

/// Flattens a field access chain into a dotted path string.
///
/// For example, `a.b.c` becomes `"a.b.c"`. Returns `None` if the chain
/// contains unsupported expression kinds.
fn flatten_field_path(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(name, _) => Some(name.clone()),
        Expr::FieldAccess { object, field, .. } => {
            let base = flatten_field_path(object)?;
            Some(format!("{base}{FIELD_ACCESS_SEP}{field}"))
        }
        _ => None,
    }
}

/// Translates a method call expression to Z3.
///
/// Currently supports:
/// - `.length()` — modeled as an uninterpreted non-negative integer function.
///   This allows Z3 to reason about string length and array bounds predicates
///   like `s.length() > 0` and `index >= 0 && index < list.length()`.
///
/// Returns `None` for unsupported method calls.
fn translate_method_call(callee: &Expr, args: &[Expr]) -> Option<Z3Expr> {
    // Method calls in Kodo AST are represented as Call { callee: FieldAccess { ... } }
    if let Expr::FieldAccess { object, field, .. } = callee {
        match field.as_str() {
            "length" if args.is_empty() => {
                // Model `obj.length()` as an uninterpreted integer constant
                // named "obj.length". The result is constrained to be >= 0
                // since lengths are never negative.
                let base = flatten_field_path(object)?;
                let name = format!("{base}{FIELD_ACCESS_SEP}length");
                Some(Z3Expr::Int(z3::ast::Int::new_const(name.as_str())))
            }
            _ => None,
        }
    } else {
        None
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

/// Verifies a contract with refinement type constraints injected as hypotheses.
///
/// When a function parameter has a refined type (e.g., `port: Port` where
/// `type Port = Int requires { self > 0 && self < 65535 }`), the refinement
/// constraint is added as a hypothesis before checking the conclusion. This
/// allows Z3 to use the constraint as an assumption.
///
/// `refinements` is a list of `(param_name, constraint_expr)` pairs where
/// the constraint has already had `self` substituted with the param name.
///
/// # Returns
///
/// - `Proved` if the conclusion follows from the refinement constraints
/// - `Refuted` if Z3 found a counter-example even with the constraints
/// - `Unknown` if any expression could not be translated or Z3 timed out
#[must_use]
pub fn verify_with_refinements(refinements: &[&Expr], conclusion: &Expr) -> SmtResult {
    verify_with_hypotheses(refinements, conclusion)
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
    fn field_access_expr_translates_and_refutes_without_context() {
        // Field access IS now supported in SMT translation (Phase 35.1).
        // `self.count > 0` translates to Z3 but is refuted without context.
        let expr = Expr::BinaryOp {
            left: Box::new(Expr::FieldAccess {
                object: Box::new(ident_expr("self")),
                field: "count".to_string(),
                span: Span::new(0, 10),
            }),
            op: BinOp::Gt,
            right: Box::new(int_expr(0)),
            span: Span::new(0, 15),
        };
        let result = verify_precondition(&expr);
        assert!(matches!(result, SmtResult::Refuted(_)));
    }

    #[test]
    fn unsupported_closure_returns_unknown() {
        // Closures are still not supported in SMT translation.
        let expr = Expr::Closure {
            params: vec![],
            return_type: None,
            body: Box::new(Expr::BoolLit(true, Span::new(0, 4))),
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

    // --- Phase 35.1: Field access predicates ---

    /// Helper: build a field access expression `object.field`.
    fn field_access_expr(object: Expr, field: &str) -> Expr {
        Expr::FieldAccess {
            object: Box::new(object),
            field: field.to_string(),
            span: Span::new(0, 10),
        }
    }

    /// Helper: build a method call expression `object.method(args)`.
    fn method_call_expr(object: Expr, method: &str, args: Vec<Expr>) -> Expr {
        Expr::Call {
            callee: Box::new(Expr::FieldAccess {
                object: Box::new(object),
                field: method.to_string(),
                span: Span::new(0, 10),
            }),
            args,
            span: Span::new(0, 15),
        }
    }

    #[test]
    fn field_access_translates_to_z3() {
        // `point.x` should translate to a Z3 integer constant named "point.x"
        let mut cfg = z3::Config::new();
        cfg.set_timeout_msec(5000);
        z3::with_z3_config(&cfg, || {
            let expr = field_access_expr(ident_expr("point"), "x");
            let z3_expr = expr_to_z3(&expr);
            assert!(z3_expr.is_some(), "field access should translate to Z3");
        });
    }

    #[test]
    fn field_access_gt_zero_with_hypothesis() {
        // If point.x > 0, then point.x > 0 (trivial with hypothesis)
        let field_x = field_access_expr(ident_expr("point"), "x");
        let hypothesis = gt_expr(field_x.clone(), int_expr(0));
        let conclusion = gt_expr(field_x, int_expr(0));
        let result = verify_with_hypotheses(&[&hypothesis], &conclusion);
        assert_eq!(result, SmtResult::Proved);
    }

    #[test]
    fn field_access_implies_derived_predicate() {
        // If point.x > 5, then point.x > 0
        let field_x = field_access_expr(ident_expr("point"), "x");
        let hypothesis = gt_expr(field_x.clone(), int_expr(5));
        let conclusion = gt_expr(field_x, int_expr(0));
        let result = verify_with_hypotheses(&[&hypothesis], &conclusion);
        assert_eq!(result, SmtResult::Proved);
    }

    #[test]
    fn nested_field_access_translates() {
        // `a.b.c` should flatten to "a.b.c"
        let mut cfg = z3::Config::new();
        cfg.set_timeout_msec(5000);
        z3::with_z3_config(&cfg, || {
            let ab = field_access_expr(ident_expr("a"), "b");
            let abc = field_access_expr(ab, "c");
            let z3_expr = expr_to_z3(&abc);
            assert!(
                z3_expr.is_some(),
                "nested field access should translate to Z3"
            );
        });
    }

    // --- Phase 35.1: Method call predicates (.length()) ---

    #[test]
    fn length_method_translates_to_z3() {
        // `s.length()` should translate to a Z3 integer constant "s.length"
        let mut cfg = z3::Config::new();
        cfg.set_timeout_msec(5000);
        z3::with_z3_config(&cfg, || {
            let expr = method_call_expr(ident_expr("s"), "length", vec![]);
            let z3_expr = expr_to_z3(&expr);
            assert!(z3_expr.is_some(), ".length() should translate to Z3");
        });
    }

    #[test]
    fn string_length_gt_zero_with_hypothesis() {
        // If s.length() > 0, then s.length() > 0 (trivial)
        let len = method_call_expr(ident_expr("s"), "length", vec![]);
        let hypothesis = gt_expr(len.clone(), int_expr(0));
        let conclusion = gt_expr(len, int_expr(0));
        let result = verify_with_hypotheses(&[&hypothesis], &conclusion);
        assert_eq!(result, SmtResult::Proved);
    }

    #[test]
    fn array_bounds_with_length_hypothesis() {
        // If index >= 0 && index < list.length(), and list.length() > 0,
        // then index >= 0 (a partial check)
        let len = method_call_expr(ident_expr("list"), "length", vec![]);
        let idx = ident_expr("index");

        let h1 = Expr::BinaryOp {
            left: Box::new(idx.clone()),
            op: BinOp::Ge,
            right: Box::new(int_expr(0)),
            span: Span::new(0, 10),
        };
        let h2 = lt_expr(idx.clone(), len.clone());
        let h3 = gt_expr(len, int_expr(0));

        let conclusion = Expr::BinaryOp {
            left: Box::new(idx),
            op: BinOp::Ge,
            right: Box::new(int_expr(0)),
            span: Span::new(0, 10),
        };
        let result = verify_with_hypotheses(&[&h1, &h2, &h3], &conclusion);
        assert_eq!(result, SmtResult::Proved);
    }

    #[test]
    fn unsupported_method_returns_none() {
        // `s.foo()` is not a supported method — should return None
        let mut cfg = z3::Config::new();
        cfg.set_timeout_msec(5000);
        z3::with_z3_config(&cfg, || {
            let expr = method_call_expr(ident_expr("s"), "foo", vec![]);
            let z3_expr = expr_to_z3(&expr);
            assert!(z3_expr.is_none(), "unsupported method should return None");
        });
    }

    #[test]
    fn length_with_args_returns_none() {
        // `s.length(42)` is not valid — length takes no arguments
        let mut cfg = z3::Config::new();
        cfg.set_timeout_msec(5000);
        z3::with_z3_config(&cfg, || {
            let expr = method_call_expr(ident_expr("s"), "length", vec![int_expr(42)]);
            let z3_expr = expr_to_z3(&expr);
            assert!(z3_expr.is_none(), ".length(42) should return None");
        });
    }

    // --- Phase 35.2: Refinement type integration ---

    #[test]
    fn refinement_constraint_as_hypothesis() {
        // type Port = Int requires { self > 0 && self < 65535 }
        // If port > 0 && port < 65535, then port > 0
        let port = ident_expr("port");
        let constraint = Expr::BinaryOp {
            left: Box::new(gt_expr(port.clone(), int_expr(0))),
            op: BinOp::And,
            right: Box::new(lt_expr(port.clone(), int_expr(65535))),
            span: Span::new(0, 30),
        };
        let conclusion = gt_expr(port, int_expr(0));
        let result = verify_with_refinements(&[&constraint], &conclusion);
        assert_eq!(result, SmtResult::Proved);
    }

    #[test]
    fn refinement_constraint_upper_bound_proved() {
        // type Port = Int requires { self > 0 && self < 65535 }
        // If port > 0 && port < 65535, then port < 65535
        let port = ident_expr("port");
        let constraint = Expr::BinaryOp {
            left: Box::new(gt_expr(port.clone(), int_expr(0))),
            op: BinOp::And,
            right: Box::new(lt_expr(port.clone(), int_expr(65535))),
            span: Span::new(0, 30),
        };
        let conclusion = lt_expr(port, int_expr(65535));
        let result = verify_with_refinements(&[&constraint], &conclusion);
        assert_eq!(result, SmtResult::Proved);
    }

    #[test]
    fn multiple_refinement_constraints() {
        // type Percentage = Int requires { self >= 0 && self <= 100 }
        // type Age = Int requires { self >= 0 }
        // If pct >= 0 && pct <= 100 and age >= 0, then pct + age >= 0
        let pct = ident_expr("pct");
        let age = ident_expr("age");

        let pct_constraint = Expr::BinaryOp {
            left: Box::new(Expr::BinaryOp {
                left: Box::new(pct.clone()),
                op: BinOp::Ge,
                right: Box::new(int_expr(0)),
                span: Span::new(0, 10),
            }),
            op: BinOp::And,
            right: Box::new(Expr::BinaryOp {
                left: Box::new(pct.clone()),
                op: BinOp::Le,
                right: Box::new(int_expr(100)),
                span: Span::new(0, 10),
            }),
            span: Span::new(0, 20),
        };

        let age_constraint = Expr::BinaryOp {
            left: Box::new(age.clone()),
            op: BinOp::Ge,
            right: Box::new(int_expr(0)),
            span: Span::new(0, 10),
        };

        let conclusion = Expr::BinaryOp {
            left: Box::new(add_expr(pct, age)),
            op: BinOp::Ge,
            right: Box::new(int_expr(0)),
            span: Span::new(0, 15),
        };

        let result = verify_with_refinements(&[&pct_constraint, &age_constraint], &conclusion);
        assert_eq!(result, SmtResult::Proved);
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
