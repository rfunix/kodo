//! Integration tests for refinement type contract support.

use kodo_ast::{BinOp, Expr, Span, UnaryOp};
use kodo_contracts::{
    refinement_contract, substitute_self, verify_refinement, ContractKind, ContractMode,
};

fn ident(name: &str) -> Expr {
    Expr::Ident(name.to_string(), Span::new(0, name.len() as u32))
}

fn int_lit(val: i64) -> Expr {
    Expr::IntLit(val, Span::new(0, 3))
}

fn gt(left: Expr, right: Expr) -> Expr {
    Expr::BinaryOp {
        left: Box::new(left),
        op: BinOp::Gt,
        right: Box::new(right),
        span: Span::new(0, 10),
    }
}

#[test]
fn substitute_self_in_ident() {
    let expr = ident("self");
    let result = substitute_self(&expr, "port");
    assert!(matches!(result, Expr::Ident(ref n, _) if n == "port"));
}

#[test]
fn substitute_self_preserves_non_self() {
    let expr = ident("other");
    let result = substitute_self(&expr, "port");
    assert!(matches!(result, Expr::Ident(ref n, _) if n == "other"));
}

#[test]
fn substitute_self_in_binary_op() {
    let expr = gt(ident("self"), int_lit(0));
    let result = substitute_self(&expr, "x");
    if let Expr::BinaryOp { left, .. } = result {
        assert!(matches!(*left, Expr::Ident(ref n, _) if n == "x"));
    } else {
        panic!("expected BinaryOp");
    }
}

#[test]
fn substitute_self_in_compound_expr() {
    let left = gt(ident("self"), int_lit(0));
    let right = Expr::BinaryOp {
        left: Box::new(ident("self")),
        op: BinOp::Lt,
        right: Box::new(int_lit(100)),
        span: Span::new(0, 10),
    };
    let expr = Expr::BinaryOp {
        left: Box::new(left),
        op: BinOp::And,
        right: Box::new(right),
        span: Span::new(0, 20),
    };
    let result = substitute_self(&expr, "val");
    // Both `self` references should be replaced
    if let Expr::BinaryOp {
        left: outer_left,
        right: outer_right,
        ..
    } = result
    {
        if let Expr::BinaryOp { left: ll, .. } = *outer_left {
            assert!(matches!(*ll, Expr::Ident(ref n, _) if n == "val"));
        }
        if let Expr::BinaryOp { left: rl, .. } = *outer_right {
            assert!(matches!(*rl, Expr::Ident(ref n, _) if n == "val"));
        }
    }
}

#[test]
fn refinement_contract_creates_requires() {
    let constraint = gt(ident("self"), int_lit(0));
    let contract = refinement_contract(&constraint, "port", Span::new(0, 10));
    assert_eq!(contract.kind, ContractKind::Requires);
}

#[test]
fn verify_refinement_runtime_mode() {
    let constraint = gt(ident("self"), int_lit(0));
    let result = verify_refinement(&constraint, "port", Span::new(0, 10), ContractMode::Runtime);
    assert!(result.is_ok());
    let result = result.unwrap();
    assert_eq!(result.runtime_checks_needed, 1);
}

#[test]
fn substitute_self_in_unary_op() {
    let expr = Expr::UnaryOp {
        op: UnaryOp::Neg,
        operand: Box::new(ident("self")),
        span: Span::new(0, 5),
    };
    let result = substitute_self(&expr, "x");
    if let Expr::UnaryOp { operand, .. } = result {
        assert!(matches!(*operand, Expr::Ident(ref n, _) if n == "x"));
    } else {
        panic!("expected UnaryOp");
    }
}

#[test]
fn substitute_self_preserves_literals() {
    let expr = int_lit(42);
    let result = substitute_self(&expr, "x");
    assert!(matches!(result, Expr::IntLit(42, _)));
}
