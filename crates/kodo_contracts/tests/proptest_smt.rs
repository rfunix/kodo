//! Property-based tests for the Kodo contract verification system.

use kodo_ast::{BinOp, Block, Expr, Function, NodeId, Span, TypeExpr, Visibility};
use kodo_contracts::{
    extract_contracts, generate_runtime_check, validate_contract_expr, ContractKind,
};
use proptest::prelude::*;

fn span() -> Span {
    Span::new(0, 10)
}

fn make_function_with_contracts(requires: Vec<Expr>, ensures: Vec<Expr>) -> Function {
    Function {
        id: NodeId(0),
        span: span(),
        name: "test_fn".to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![],
        return_type: TypeExpr::Named("Int".to_string()),
        requires,
        ensures,
        body: Block {
            span: span(),
            stmts: vec![],
        },
    }
}

/// Strategy for valid contract expressions (booleans, comparisons, identifiers).
fn valid_contract_expr_strategy() -> impl Strategy<Value = Expr> {
    prop_oneof![
        // Boolean literals
        prop::bool::ANY.prop_map(|b| Expr::BoolLit(b, Span::new(0, 4))),
        // Integer literals
        (-1000i64..1000).prop_map(|n| Expr::IntLit(n, Span::new(0, 5))),
        // Identifiers
        "[a-z]{1,8}".prop_map(|s| Expr::Ident(s, Span::new(0, 5))),
        // Comparisons: x > 0
        (-100i64..100).prop_map(|n| Expr::BinaryOp {
            left: Box::new(Expr::Ident("x".to_string(), Span::new(0, 1))),
            op: BinOp::Gt,
            right: Box::new(Expr::IntLit(n, Span::new(4, 6))),
            span: Span::new(0, 6),
        }),
        // Logical AND
        prop::bool::ANY.prop_map(|b| Expr::BinaryOp {
            left: Box::new(Expr::BoolLit(true, Span::new(0, 4))),
            op: BinOp::And,
            right: Box::new(Expr::BoolLit(b, Span::new(8, 12))),
            span: Span::new(0, 12),
        }),
    ]
}

/// Strategy for invalid contract expressions.
fn invalid_contract_expr_strategy() -> impl Strategy<Value = Expr> {
    prop_oneof![
        // String literals are not valid in contracts
        "[a-z]{1,10}".prop_map(|s| Expr::StringLit(s, Span::new(0, 10))),
        // If expressions are not valid in contracts
        Just(Expr::If {
            condition: Box::new(Expr::BoolLit(true, Span::new(0, 4))),
            then_branch: Block {
                span: Span::new(0, 10),
                stmts: vec![]
            },
            else_branch: None,
            span: Span::new(0, 10),
        }),
    ]
}

proptest! {
    /// Contract extraction never panics on functions with random valid contracts.
    #[test]
    fn extract_contracts_never_panics(
        req_count in 0usize..5,
        ens_count in 0usize..5,
    ) {
        let requires: Vec<Expr> = (0..req_count)
            .map(|_| Expr::BoolLit(true, span()))
            .collect();
        let ensures: Vec<Expr> = (0..ens_count)
            .map(|_| Expr::BoolLit(true, span()))
            .collect();
        let func = make_function_with_contracts(requires, ensures);
        let contracts = extract_contracts(&func);
        prop_assert_eq!(contracts.len(), req_count + ens_count);

        // Verify correct kinds.
        let req_contracts = contracts.iter().filter(|c| c.kind == ContractKind::Requires).count();
        let ens_contracts = contracts.iter().filter(|c| c.kind == ContractKind::Ensures).count();
        prop_assert_eq!(req_contracts, req_count);
        prop_assert_eq!(ens_contracts, ens_count);
    }

    /// validate_contract_expr accepts all valid contract expressions.
    #[test]
    fn valid_contract_exprs_pass_validation(expr in valid_contract_expr_strategy()) {
        let result = validate_contract_expr(&expr);
        prop_assert!(result.is_ok(), "expected valid, got: {:?}", result);
    }

    /// validate_contract_expr rejects invalid contract expressions.
    #[test]
    fn invalid_contract_exprs_fail_validation(expr in invalid_contract_expr_strategy()) {
        let result = validate_contract_expr(&expr);
        prop_assert!(result.is_err(), "expected error for: {:?}", expr);
    }

    /// Runtime check generation never panics.
    #[test]
    fn runtime_check_generation_never_panics(b in prop::bool::ANY) {
        let contract = kodo_contracts::Contract {
            kind: if b { ContractKind::Requires } else { ContractKind::Ensures },
            expr: Expr::BoolLit(true, span()),
            span: span(),
        };
        let check = generate_runtime_check(&contract);
        prop_assert!(!check.message.is_empty());
    }
}

#[test]
fn true_literal_is_valid_contract() {
    let expr = Expr::BoolLit(true, Span::new(0, 4));
    assert!(validate_contract_expr(&expr).is_ok());
}

#[test]
fn false_literal_is_valid_contract() {
    let expr = Expr::BoolLit(false, Span::new(0, 5));
    assert!(validate_contract_expr(&expr).is_ok());
}

#[test]
fn string_literal_is_invalid_contract() {
    let expr = Expr::StringLit("hello".to_string(), Span::new(0, 7));
    assert!(validate_contract_expr(&expr).is_err());
}

#[test]
fn nested_binop_is_valid_contract() {
    // x > 0 && y < 10
    let expr = Expr::BinaryOp {
        left: Box::new(Expr::BinaryOp {
            left: Box::new(Expr::Ident("x".to_string(), Span::new(0, 1))),
            op: BinOp::Gt,
            right: Box::new(Expr::IntLit(0, Span::new(4, 5))),
            span: Span::new(0, 5),
        }),
        op: BinOp::And,
        right: Box::new(Expr::BinaryOp {
            left: Box::new(Expr::Ident("y".to_string(), Span::new(9, 10))),
            op: BinOp::Lt,
            right: Box::new(Expr::IntLit(10, Span::new(13, 15))),
            span: Span::new(9, 15),
        }),
        span: Span::new(0, 15),
    };
    assert!(validate_contract_expr(&expr).is_ok());
}

#[test]
fn extract_no_contracts_from_plain_function() {
    let func = make_function_with_contracts(vec![], vec![]);
    let contracts = extract_contracts(&func);
    assert!(contracts.is_empty());
}

// ── SMT-specific tests (only when Z3 is available) ─────────────────

#[cfg(feature = "smt")]
mod smt_tests {
    use super::*;
    use kodo_contracts::smt::{verify_precondition, SmtResult};

    #[test]
    fn true_literal_always_verifies() {
        let expr = Expr::BoolLit(true, Span::new(0, 4));
        let result = verify_precondition(&expr);
        assert_eq!(result, SmtResult::Proved);
    }

    #[test]
    fn false_literal_is_refuted() {
        let expr = Expr::BoolLit(false, Span::new(0, 5));
        let result = verify_precondition(&expr);
        assert!(matches!(result, SmtResult::Refuted(_)));
    }

    #[test]
    fn tautology_x_eq_x_proves() {
        let expr = Expr::BinaryOp {
            left: Box::new(Expr::Ident("x".to_string(), Span::new(0, 1))),
            op: BinOp::Eq,
            right: Box::new(Expr::Ident("x".to_string(), Span::new(5, 6))),
            span: Span::new(0, 6),
        };
        let result = verify_precondition(&expr);
        assert_eq!(result, SmtResult::Proved);
    }
}
