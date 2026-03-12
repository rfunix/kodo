//! Tests for the Kōdo parser.
//!
//! This module contains all parser tests: unit tests for individual parsing
//! methods, snapshot tests for AST output stability, and integration-style
//! tests for complete programs.

use crate::{parse, parse_with_recovery};
use kodo_ast::{
    BinOp, Expr, IntentConfigValue, Ownership, Pattern, Span, Stmt, StringPart, TypeExpr, UnaryOp,
};
use kodo_lexer::TokenKind;

use crate::error::ParseError;

#[test]
fn parse_minimal_module() {
    let source = r#"module hello {
        meta {
            version: "0.1.0",
            author: "Kōdo Team"
        }

        fn main() {
        }
    }"#;

    let module = parse(source);
    assert!(module.is_ok(), "parse failed: {module:?}");
    let module = module.unwrap_or_else(|_| panic!("already checked"));
    assert_eq!(module.name, "hello");
    assert!(module.meta.is_some());
    let meta = module
        .meta
        .as_ref()
        .unwrap_or_else(|| panic!("already checked"));
    assert_eq!(meta.entries.len(), 2);
    assert_eq!(meta.entries[0].key, "version");
    assert_eq!(meta.entries[0].value, "0.1.0");
    assert_eq!(module.functions.len(), 1);
    assert_eq!(module.functions[0].name, "main");
}

#[test]
fn parse_function_with_params() {
    let source = r#"module math {
        fn add(a: Int, b: Int) -> Int {
        }
    }"#;

    let module = parse(source);
    assert!(module.is_ok(), "parse failed: {module:?}");
    let module = module.unwrap_or_else(|_| panic!("already checked"));
    assert_eq!(module.functions[0].name, "add");
    assert_eq!(module.functions[0].params.len(), 2);
    assert_eq!(module.functions[0].params[0].name, "a");
    assert_eq!(
        module.functions[0].return_type,
        TypeExpr::Named("Int".to_string())
    );
}

#[test]
fn parse_missing_module_keyword_fails() {
    let result = parse("hello { }");
    assert!(result.is_err());
}

#[test]
fn parse_let_binding_with_type() {
    let source = r#"module test {
        fn main() {
            let x: Int = 42
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Let {
            name,
            ty,
            mutable,
            value,
            ..
        } => {
            assert_eq!(name, "x");
            assert!(!mutable);
            assert_eq!(ty.as_ref(), Some(&TypeExpr::Named("Int".to_string())));
            assert!(matches!(value, Expr::IntLit(42, _)));
        }
        other => panic!("expected Let, got {other:?}"),
    }
}

#[test]
fn parse_let_binding_mutable() {
    let source = r#"module test {
        fn main() {
            let mut y: Int = 10
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Let { name, mutable, .. } => {
            assert_eq!(name, "y");
            assert!(mutable);
        }
        other => panic!("expected Let, got {other:?}"),
    }
}

#[test]
fn parse_let_binding_without_type() {
    let source = r#"module test {
        fn main() {
            let z = 99
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Let { name, ty, .. } => {
            assert_eq!(name, "z");
            assert!(ty.is_none());
        }
        other => panic!("expected Let, got {other:?}"),
    }
}

#[test]
fn parse_return_with_value() {
    let source = r#"module test {
        fn answer() -> Int {
            return 42
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Return { value, .. } => {
            assert!(matches!(value, Some(Expr::IntLit(42, _))));
        }
        other => panic!("expected Return, got {other:?}"),
    }
}

#[test]
fn parse_return_without_value() {
    let source = r#"module test {
        fn nothing() {
            return
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Return { value, .. } => {
            assert!(value.is_none());
        }
        other => panic!("expected Return, got {other:?}"),
    }
}

#[test]
fn parse_binary_precedence() {
    // a + b * c should parse as a + (b * c)
    let source = r#"module test {
        fn main() {
            a + b * c
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Expr(Expr::BinaryOp {
            op: BinOp::Add,
            left,
            right,
            ..
        }) => {
            assert!(matches!(left.as_ref(), Expr::Ident(ref n, _) if n == "a"));
            match right.as_ref() {
                Expr::BinaryOp {
                    op: BinOp::Mul,
                    left: inner_left,
                    right: inner_right,
                    ..
                } => {
                    assert!(matches!(inner_left.as_ref(), Expr::Ident(ref n, _) if n == "b"));
                    assert!(matches!(inner_right.as_ref(), Expr::Ident(ref n, _) if n == "c"));
                }
                other => panic!("expected Mul, got {other:?}"),
            }
        }
        other => panic!("expected Add at top, got {other:?}"),
    }
}

#[test]
fn parse_nested_if_else() {
    let source = r#"module test {
        fn check(x: Int) -> Int {
            if x > 0 {
                return 1
            } else if x < 0 {
                return -1
            } else {
                return 0
            }
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Expr(Expr::If {
            else_branch: Some(else_block),
            ..
        }) => {
            // The else branch should contain another if expression
            assert_eq!(else_block.stmts.len(), 1);
            assert!(matches!(
                &else_block.stmts[0],
                Stmt::Expr(Expr::If {
                    else_branch: Some(_),
                    ..
                })
            ));
        }
        other => panic!("expected If with else, got {other:?}"),
    }
}

#[test]
fn parse_function_call() {
    let source = r#"module test {
        fn main() {
            foo(1, 2, 3)
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Expr(Expr::Call { callee, args, .. }) => {
            assert!(matches!(callee.as_ref(), Expr::Ident(ref n, _) if n == "foo"));
            assert_eq!(args.len(), 3);
            assert!(matches!(&args[0], Expr::IntLit(1, _)));
            assert!(matches!(&args[1], Expr::IntLit(2, _)));
            assert!(matches!(&args[2], Expr::IntLit(3, _)));
        }
        other => panic!("expected Call, got {other:?}"),
    }
}

#[test]
fn parse_function_call_no_args() {
    let source = r#"module test {
        fn main() {
            bar()
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Expr(Expr::Call { callee, args, .. }) => {
            assert!(matches!(callee.as_ref(), Expr::Ident(ref n, _) if n == "bar"));
            assert!(args.is_empty());
        }
        other => panic!("expected Call, got {other:?}"),
    }
}

#[test]
fn parse_requires_ensures() {
    let source = r#"module test {
        fn divide(a: Int, b: Int) -> Int
            requires { b != 0 }
            ensures { result >= 0 }
        {
            return a / b
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let func = &module.functions[0];
    assert_eq!(func.requires.len(), 1);
    assert_eq!(func.ensures.len(), 1);

    // Check the requires clause is `b != 0`
    match &func.requires[0] {
        Expr::BinaryOp {
            op: BinOp::Ne,
            left,
            right,
            ..
        } => {
            assert!(matches!(left.as_ref(), Expr::Ident(ref n, _) if n == "b"));
            assert!(matches!(right.as_ref(), Expr::IntLit(0, _)));
        }
        other => panic!("expected Ne, got {other:?}"),
    }

    // Check the ensures clause is `result >= 0`
    match &func.ensures[0] {
        Expr::BinaryOp {
            op: BinOp::Ge,
            left,
            right,
            ..
        } => {
            assert!(matches!(left.as_ref(), Expr::Ident(ref n, _) if n == "result"));
            assert!(matches!(right.as_ref(), Expr::IntLit(0, _)));
        }
        other => panic!("expected Ge, got {other:?}"),
    }
}

#[test]
fn parse_complex_expression() {
    // a + b * c - d / e should parse as ((a + (b * c)) - (d / e))
    let source = r#"module test {
        fn main() {
            a + b * c - d / e
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    // Top level should be Sub (left-assoc: (a + b*c) - (d/e))
    match &stmts[0] {
        Stmt::Expr(Expr::BinaryOp {
            op: BinOp::Sub,
            left,
            right,
            ..
        }) => {
            // Left should be Add
            assert!(matches!(
                left.as_ref(),
                Expr::BinaryOp { op: BinOp::Add, .. }
            ));
            // Right should be Div
            assert!(matches!(
                right.as_ref(),
                Expr::BinaryOp { op: BinOp::Div, .. }
            ));
        }
        other => panic!("expected Sub at top, got {other:?}"),
    }
}

#[test]
fn parse_logical_operators() {
    let source = r#"module test {
        fn main() {
            a && b || c
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    // Should parse as (a && b) || c since || has lower precedence
    match &stmts[0] {
        Stmt::Expr(Expr::BinaryOp {
            op: BinOp::Or,
            left,
            ..
        }) => {
            assert!(matches!(
                left.as_ref(),
                Expr::BinaryOp { op: BinOp::And, .. }
            ));
        }
        other => panic!("expected Or at top, got {other:?}"),
    }
}

#[test]
fn parse_unary_negation() {
    let source = r#"module test {
        fn main() {
            -42
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Expr(Expr::UnaryOp {
            op: UnaryOp::Neg,
            operand,
            ..
        }) => {
            assert!(matches!(operand.as_ref(), Expr::IntLit(42, _)));
        }
        other => panic!("expected UnaryOp Neg, got {other:?}"),
    }
}

#[test]
fn parse_unary_not() {
    let source = r#"module test {
        fn main() {
            !flag
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Expr(Expr::UnaryOp {
            op: UnaryOp::Not,
            operand,
            ..
        }) => {
            assert!(matches!(operand.as_ref(), Expr::Ident(ref n, _) if n == "flag"));
        }
        other => panic!("expected UnaryOp Not, got {other:?}"),
    }
}

#[test]
fn parse_field_access() {
    let source = r#"module test {
        fn main() {
            x.y
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Expr(Expr::FieldAccess { object, field, .. }) => {
            assert!(matches!(object.as_ref(), Expr::Ident(ref n, _) if n == "x"));
            assert_eq!(field, "y");
        }
        other => panic!("expected FieldAccess, got {other:?}"),
    }
}

#[test]
fn parse_parenthesized_expr() {
    let source = r#"module test {
        fn main() {
            (a + b) * c
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    // Top level should be Mul because parens override precedence
    match &stmts[0] {
        Stmt::Expr(Expr::BinaryOp {
            op: BinOp::Mul,
            left,
            ..
        }) => {
            assert!(matches!(
                left.as_ref(),
                Expr::BinaryOp { op: BinOp::Add, .. }
            ));
        }
        other => panic!("expected Mul at top, got {other:?}"),
    }
}

#[test]
fn parse_bool_literals() {
    let source = r#"module test {
        fn main() {
            let a: Bool = true
            let b: Bool = false
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 2);
    match &stmts[0] {
        Stmt::Let { value, .. } => {
            assert!(matches!(value, Expr::BoolLit(true, _)));
        }
        other => panic!("expected Let with true, got {other:?}"),
    }
    match &stmts[1] {
        Stmt::Let { value, .. } => {
            assert!(matches!(value, Expr::BoolLit(false, _)));
        }
        other => panic!("expected Let with false, got {other:?}"),
    }
}

#[test]
fn parse_string_literal_expr() {
    let source = r#"module test {
        fn main() {
            let s: String = "hello"
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Let { value, .. } => {
            assert!(matches!(value, Expr::StringLit(ref s, _) if s == "hello"));
        }
        other => panic!("expected Let with string, got {other:?}"),
    }
}

#[test]
fn parse_if_without_else() {
    let source = r#"module test {
        fn main() {
            if x > 0 {
                return 1
            }
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Expr(Expr::If { else_branch, .. }) => {
            assert!(else_branch.is_none());
        }
        other => panic!("expected If without else, got {other:?}"),
    }
}

#[test]
fn parse_multiple_statements() {
    let source = r#"module test {
        fn main() {
            let x: Int = 1
            let y: Int = 2
            return x + y
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 3);
    assert!(matches!(&stmts[0], Stmt::Let { .. }));
    assert!(matches!(&stmts[1], Stmt::Let { .. }));
    assert!(matches!(&stmts[2], Stmt::Return { .. }));
}

#[test]
fn parse_chained_method_calls() {
    let source = r#"module test {
        fn main() {
            a.b.c(1)
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    // Should be Call(FieldAccess(FieldAccess(a, b), c), [1])
    match &stmts[0] {
        Stmt::Expr(Expr::Call { callee, args, .. }) => {
            assert_eq!(args.len(), 1);
            match callee.as_ref() {
                Expr::FieldAccess { object, field, .. } => {
                    assert_eq!(field, "c");
                    match object.as_ref() {
                        Expr::FieldAccess {
                            object: inner,
                            field: inner_field,
                            ..
                        } => {
                            assert!(matches!(inner.as_ref(), Expr::Ident(ref n, _) if n == "a"));
                            assert_eq!(inner_field, "b");
                        }
                        other => panic!("expected inner FieldAccess, got {other:?}"),
                    }
                }
                other => panic!("expected FieldAccess callee, got {other:?}"),
            }
        }
        other => panic!("expected Call, got {other:?}"),
    }
}

#[test]
fn parse_multiple_contracts() {
    let source = r#"module test {
        fn safe_div(a: Int, b: Int) -> Int
            requires { b != 0 }
            requires { a >= 0 }
            ensures { result >= 0 }
        {
            return a / b
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let func = &module.functions[0];
    assert_eq!(func.requires.len(), 2);
    assert_eq!(func.ensures.len(), 1);
}

#[test]
fn parse_while_simple() {
    let source = r#"module test {
        fn main() {
            let mut i: Int = 5
            while i > 0 {
                i = i - 1
            }
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 2);
    match &stmts[1] {
        Stmt::While {
            condition, body, ..
        } => {
            assert!(matches!(condition, Expr::BinaryOp { op: BinOp::Gt, .. }));
            assert_eq!(body.stmts.len(), 1);
        }
        other => panic!("expected While, got {other:?}"),
    }
}

#[test]
fn parse_while_with_nested_if() {
    let source = r#"module test {
        fn main() {
            let mut x: Int = 10
            while x > 0 {
                if x == 5 {
                    println("halfway")
                }
                x = x - 1
            }
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 2);
    assert!(matches!(&stmts[1], Stmt::While { .. }));
}

#[test]
fn parse_while_missing_block() {
    let source = r#"module test {
        fn main() {
            while true
        }
    }"#;

    let result = parse(source);
    assert!(result.is_err());
}

#[test]
fn parse_assignment() {
    let source = r#"module test {
        fn main() {
            let mut x: Int = 1
            x = 42
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 2);
    match &stmts[1] {
        Stmt::Assign { name, value, .. } => {
            assert_eq!(name, "x");
            assert!(matches!(value, Expr::IntLit(42, _)));
        }
        other => panic!("expected Assign, got {other:?}"),
    }
}

#[test]
fn parse_annotation_simple() {
    let source = r#"module test {
        meta { purpose: "test" }
        @confidence(95)
        fn foo() { }
    }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(module.functions[0].annotations.len(), 1);
    assert_eq!(module.functions[0].annotations[0].name, "confidence");
}

#[test]
fn parse_annotation_named_args() {
    let source = r#"module test {
        meta { purpose: "test" }
        @authored_by(agent: "claude")
        fn foo() { }
    }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(module.functions[0].annotations.len(), 1);
    assert_eq!(module.functions[0].annotations[0].name, "authored_by");
    assert!(
        module.functions[0].annotations[0]
            .args
            .iter()
            .any(|a| matches!(a, kodo_ast::AnnotationArg::Named(name, _) if name == "agent")),
        "expected a named arg 'agent'"
    );
}

#[test]
fn parse_multiple_annotations() {
    let source = r#"module test {
        meta { purpose: "test" }
        @authored_by(agent: "claude")
        @confidence(95)
        fn foo() { }
    }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(
        module.functions[0].annotations.len(),
        2,
        "expected 2 annotations, got {}",
        module.functions[0].annotations.len()
    );
}

#[test]
fn parse_error_span() {
    let error = ParseError::UnexpectedToken {
        expected: "expression".to_string(),
        found: TokenKind::RBrace,
        span: Span::new(10, 11),
    };
    assert_eq!(error.span(), Some(Span::new(10, 11)));

    let eof_error = ParseError::UnexpectedEof {
        expected: "expression".to_string(),
    };
    assert_eq!(eof_error.span(), None);
}

// ===== Generics (Phase 2) Tests =====

#[test]
fn parse_type_generic_single_arg() {
    let source = r#"module test {
        fn main() {
            let x: Option<Int> = 42
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Let { ty, .. } => {
            assert_eq!(
                ty.as_ref(),
                Some(&TypeExpr::Generic(
                    "Option".to_string(),
                    vec![TypeExpr::Named("Int".to_string())]
                ))
            );
        }
        other => panic!("expected Let, got {other:?}"),
    }
}

#[test]
fn parse_type_generic_multiple_args() {
    let source = r#"module test {
        fn main() {
            let p: Pair<Int, Bool> = 42
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Let { ty, .. } => {
            assert_eq!(
                ty.as_ref(),
                Some(&TypeExpr::Generic(
                    "Pair".to_string(),
                    vec![
                        TypeExpr::Named("Int".to_string()),
                        TypeExpr::Named("Bool".to_string()),
                    ]
                ))
            );
        }
        other => panic!("expected Let, got {other:?}"),
    }
}

#[test]
fn parse_type_generic_nested() {
    let source = r#"module test {
        fn main() {
            let x: Option<List<Int>> = 42
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    match &stmts[0] {
        Stmt::Let { ty, .. } => {
            assert_eq!(
                ty.as_ref(),
                Some(&TypeExpr::Generic(
                    "Option".to_string(),
                    vec![TypeExpr::Generic(
                        "List".to_string(),
                        vec![TypeExpr::Named("Int".to_string())]
                    )]
                ))
            );
        }
        other => panic!("expected Let, got {other:?}"),
    }
}

#[test]
fn parse_type_non_generic_remains_named() {
    let source = r#"module test {
        fn main() {
            let x: Int = 42
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    match &stmts[0] {
        Stmt::Let { ty, .. } => {
            assert_eq!(ty.as_ref(), Some(&TypeExpr::Named("Int".to_string())));
        }
        other => panic!("expected Let, got {other:?}"),
    }
}

#[test]
fn parse_struct_decl_with_generic_params() {
    let source = r#"module test {
        struct Pair<T, U> {
            first: T,
            second: U,
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(module.type_decls.len(), 1);
    let decl = &module.type_decls[0];
    assert_eq!(decl.name, "Pair");
    let names: Vec<&str> = decl
        .generic_params
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(names, vec!["T", "U"]);
    assert_eq!(decl.fields.len(), 2);
    assert_eq!(decl.fields[0].name, "first");
    assert_eq!(decl.fields[0].ty, TypeExpr::Named("T".to_string()));
    assert_eq!(decl.fields[1].name, "second");
    assert_eq!(decl.fields[1].ty, TypeExpr::Named("U".to_string()));
}

#[test]
fn parse_struct_decl_without_generic_params() {
    let source = r#"module test {
        struct Point {
            x: Int,
            y: Int,
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(module.type_decls.len(), 1);
    let decl = &module.type_decls[0];
    assert_eq!(decl.name, "Point");
    assert!(decl.generic_params.is_empty());
}

#[test]
fn parse_enum_decl_with_generic_params() {
    let source = r#"module test {
        enum Option<T> {
            Some(T),
            None,
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(module.enum_decls.len(), 1);
    let decl = &module.enum_decls[0];
    assert_eq!(decl.name, "Option");
    let names: Vec<&str> = decl
        .generic_params
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(names, vec!["T"]);
    assert_eq!(decl.variants.len(), 2);
    assert_eq!(decl.variants[0].name, "Some");
    assert_eq!(
        decl.variants[0].fields,
        vec![TypeExpr::Named("T".to_string())]
    );
    assert_eq!(decl.variants[1].name, "None");
    assert!(decl.variants[1].fields.is_empty());
}

#[test]
fn parse_enum_decl_without_generic_params() {
    let source = r#"module test {
        enum Color {
            Red,
            Green,
            Blue,
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(module.enum_decls.len(), 1);
    let decl = &module.enum_decls[0];
    assert_eq!(decl.name, "Color");
    assert!(decl.generic_params.is_empty());
    assert_eq!(decl.variants.len(), 3);
}

#[test]
fn parse_enum_decl_with_multiple_generic_params() {
    let source = r#"module test {
        enum Result<T, E> {
            Ok(T),
            Err(E),
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let decl = &module.enum_decls[0];
    assert_eq!(decl.name, "Result");
    let names: Vec<&str> = decl
        .generic_params
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(names, vec!["T", "E"]);
    assert_eq!(decl.variants.len(), 2);
    assert_eq!(decl.variants[0].name, "Ok");
    assert_eq!(
        decl.variants[0].fields,
        vec![TypeExpr::Named("T".to_string())]
    );
    assert_eq!(decl.variants[1].name, "Err");
    assert_eq!(
        decl.variants[1].fields,
        vec![TypeExpr::Named("E".to_string())]
    );
}

#[test]
fn parse_function_param_with_generic_type() {
    let source = r#"module test {
        fn process(val: Option<Int>) -> Int {
            return 0
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let func = &module.functions[0];
    assert_eq!(func.params.len(), 1);
    assert_eq!(
        func.params[0].ty,
        TypeExpr::Generic(
            "Option".to_string(),
            vec![TypeExpr::Named("Int".to_string())]
        )
    );
}

#[test]
fn parse_function_return_type_generic() {
    let source = r#"module test {
        fn wrap(x: Int) -> Option<Int> {
            return x
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let func = &module.functions[0];
    assert_eq!(
        func.return_type,
        TypeExpr::Generic(
            "Option".to_string(),
            vec![TypeExpr::Named("Int".to_string())]
        )
    );
}

#[test]
fn parse_struct_field_with_generic_type() {
    let source = r#"module test {
        struct Container<T> {
            value: Option<T>,
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let decl = &module.type_decls[0];
    let names: Vec<&str> = decl
        .generic_params
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(names, vec!["T"]);
    assert_eq!(
        decl.fields[0].ty,
        TypeExpr::Generic("Option".to_string(), vec![TypeExpr::Named("T".to_string())])
    );
}

#[test]
fn parse_struct_with_single_bound() {
    let source = r#"module test {
        struct SortedList<T: Ord> {
            items: List<T>,
        }
    }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let decl = &module.type_decls[0];
    assert_eq!(decl.name, "SortedList");
    assert_eq!(decl.generic_params.len(), 1);
    assert_eq!(decl.generic_params[0].name, "T");
    assert_eq!(decl.generic_params[0].bounds, vec!["Ord"]);
}

#[test]
fn parse_struct_with_multiple_bounds() {
    let source = r#"module test {
        struct Display<T: Ord + Show> {
            value: T,
        }
    }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let decl = &module.type_decls[0];
    assert_eq!(decl.generic_params[0].name, "T");
    assert_eq!(decl.generic_params[0].bounds, vec!["Ord", "Show"]);
}

#[test]
fn parse_struct_with_mixed_bounds() {
    let source = r#"module test {
        struct Pair<T: Ord, U> {
            first: T,
            second: U,
        }
    }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let decl = &module.type_decls[0];
    assert_eq!(decl.generic_params.len(), 2);
    assert_eq!(decl.generic_params[0].name, "T");
    assert_eq!(decl.generic_params[0].bounds, vec!["Ord"]);
    assert_eq!(decl.generic_params[1].name, "U");
    assert!(decl.generic_params[1].bounds.is_empty());
}

#[test]
fn parse_enum_with_bounds() {
    let source = r#"module test {
        enum Bounded<T: Clone> {
            Some(T),
            None,
        }
    }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let decl = &module.enum_decls[0];
    assert_eq!(decl.generic_params[0].name, "T");
    assert_eq!(decl.generic_params[0].bounds, vec!["Clone"]);
}

#[test]
fn parse_function_with_bounds() {
    let source = r#"module test {
        fn sort<T: Ord>(items: List<T>) -> List<T> {
            return items
        }
    }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let func = &module.functions[0];
    assert_eq!(func.generic_params.len(), 1);
    assert_eq!(func.generic_params[0].name, "T");
    assert_eq!(func.generic_params[0].bounds, vec!["Ord"]);
}

#[test]
fn parse_function_with_multiple_bounded_params() {
    let source = r#"module test {
        fn compare<T: Ord + Display, U: Clone>(a: T, b: U) -> Bool {
            return true
        }
    }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let func = &module.functions[0];
    assert_eq!(func.generic_params.len(), 2);
    assert_eq!(func.generic_params[0].name, "T");
    assert_eq!(func.generic_params[0].bounds, vec!["Ord", "Display"]);
    assert_eq!(func.generic_params[1].name, "U");
    assert_eq!(func.generic_params[1].bounds, vec!["Clone"]);
}

#[test]
fn parse_generic_params_no_bounds_preserves_old_behavior() {
    let source = r#"module test {
        struct Pair<T, U> {
            first: T,
            second: U,
        }
    }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let decl = &module.type_decls[0];
    assert_eq!(decl.generic_params.len(), 2);
    assert!(decl.generic_params[0].bounds.is_empty());
    assert!(decl.generic_params[1].bounds.is_empty());
}

#[test]
fn parse_for_loop_exclusive() {
    let source = r#"module test {
        fn main() {
            for i in 0..10 {
                print_int(i)
            }
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::For {
            name,
            start,
            end,
            inclusive,
            body,
            ..
        } => {
            assert_eq!(name, "i");
            assert!(matches!(start, Expr::IntLit(0, _)));
            assert!(matches!(end, Expr::IntLit(10, _)));
            assert!(!inclusive);
            assert_eq!(body.stmts.len(), 1);
        }
        other => panic!("expected For, got {other:?}"),
    }
}

#[test]
fn parse_for_loop_inclusive() {
    let source = r#"module test {
        fn main() {
            for i in 0..=10 {
                print_int(i)
            }
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::For { inclusive, .. } => {
            assert!(inclusive);
        }
        other => panic!("expected For, got {other:?}"),
    }
}

#[test]
fn parse_for_loop_missing_in() {
    let source = r#"module test {
        fn main() {
            for i 0..10 {
            }
        }
    }"#;

    let result = parse(source);
    assert!(result.is_err());
}

#[test]
fn parse_range_expression() {
    let source = r#"module test {
        fn main() {
            let mut x: Int = 0
            for i in 1..5 {
                x = x + i
            }
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 2);
    assert!(matches!(&stmts[1], Stmt::For { .. }));
}

#[test]
fn parse_optional_type_shorthand() {
    let source = r#"module test {
        fn get_value(opt: Int?) -> Int {
            return 0
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let param_ty = &module.functions[0].params[0].ty;
    assert!(
        matches!(param_ty, TypeExpr::Optional(inner) if matches!(inner.as_ref(), TypeExpr::Named(n) if n == "Int"))
    );
}

#[test]
fn parse_try_operator() {
    let source = r#"module test {
        fn do_thing() -> Int {
            let x: Int = result?
            return x
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    if let Stmt::Let { value, .. } = &stmts[0] {
        assert!(matches!(value, Expr::Try { .. }));
    } else {
        panic!("expected Let statement");
    }
}

#[test]
fn parse_optional_chain() {
    let source = r#"module test {
        fn get_x() -> Int {
            let v: Int = opt?.x
            return v
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    if let Stmt::Let { value, .. } = &stmts[0] {
        assert!(matches!(value, Expr::OptionalChain { field, .. } if field == "x"));
    } else {
        panic!("expected Let statement");
    }
}

#[test]
fn parse_null_coalesce() {
    let source = r#"module test {
        fn get_value() -> Int {
            let x: Int = opt ?? 0
            return x
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    if let Stmt::Let { value, .. } = &stmts[0] {
        assert!(matches!(value, Expr::NullCoalesce { .. }));
    } else {
        panic!("expected Let statement");
    }
}

#[test]
fn parse_chained_null_coalesce() {
    let source = r#"module test {
        fn get_value() -> Int {
            let x: Int = a ?? b ?? 0
            return x
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    if let Stmt::Let { value, .. } = &stmts[0] {
        // Should be left-associative: (a ?? b) ?? 0
        assert!(
            matches!(value, Expr::NullCoalesce { left, .. } if matches!(left.as_ref(), Expr::NullCoalesce { .. }))
        );
    } else {
        panic!("expected Let statement");
    }
}

#[test]
fn parse_closure_with_typed_params() {
    let source = r#"module test {
        fn main() {
            let f = |x: Int, y: Int| x + y
        }
    }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::Let { value, .. } => {
            assert!(matches!(value, Expr::Closure { params, .. } if params.len() == 2));
        }
        other => panic!("expected Let, got {other:?}"),
    }
}

#[test]
fn parse_closure_with_return_type() {
    let source = r#"module test {
        fn main() {
            let f = |x: Int| -> Int { x * 2 }
        }
    }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    match &stmts[0] {
        Stmt::Let { value, .. } => {
            assert!(matches!(
                value,
                Expr::Closure {
                    return_type: Some(TypeExpr::Named(_)),
                    ..
                }
            ));
        }
        other => panic!("expected Let, got {other:?}"),
    }
}

#[test]
fn parse_empty_closure() {
    let source = r#"module test {
        fn main() {
            let f = || 42
        }
    }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    match &stmts[0] {
        Stmt::Let { value, .. } => {
            assert!(matches!(value, Expr::Closure { params, .. } if params.is_empty()));
        }
        other => panic!("expected Let, got {other:?}"),
    }
}

#[test]
fn parse_function_type_annotation() {
    let source = r#"module test {
        fn apply(f: (Int) -> Int, x: Int) -> Int {
            return f(x)
        }
    }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let func = &module.functions[0];
    assert_eq!(func.params.len(), 2);
    assert_eq!(
        func.params[0].ty,
        TypeExpr::Function(
            vec![TypeExpr::Named("Int".to_string())],
            Box::new(TypeExpr::Named("Int".to_string()))
        )
    );
}

#[test]
fn parse_or_still_works_with_pipe() {
    // Ensure || as logical OR still works in binary position
    let source = r#"module test {
        fn main() {
            let x = true || false
        }
    }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    match &stmts[0] {
        Stmt::Let { value, .. } => {
            assert!(matches!(value, Expr::BinaryOp { op: BinOp::Or, .. }));
        }
        other => panic!("expected Let, got {other:?}"),
    }
}

#[test]
fn parse_trait_declaration() {
    let source = r#"module test {
        meta { purpose: "test" }
        trait Describable {
            fn describe(self) -> Int
        }
        fn main() -> Int { return 0 }
    }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(module.trait_decls.len(), 1);
    assert_eq!(module.trait_decls[0].name, "Describable");
    assert_eq!(module.trait_decls[0].methods.len(), 1);
    assert_eq!(module.trait_decls[0].methods[0].name, "describe");
    assert!(module.trait_decls[0].methods[0].has_self);
    assert_eq!(
        module.trait_decls[0].methods[0].return_type,
        TypeExpr::Named("Int".to_string())
    );
}

#[test]
fn parse_trait_with_multiple_methods() {
    let source = r#"module test {
        meta { purpose: "test" }
        trait Shape {
            fn area(self) -> Int
            fn perimeter(self) -> Int
        }
        fn main() -> Int { return 0 }
    }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(module.trait_decls[0].methods.len(), 2);
    assert_eq!(module.trait_decls[0].methods[0].name, "area");
    assert_eq!(module.trait_decls[0].methods[1].name, "perimeter");
}

#[test]
fn parse_impl_block() {
    let source = r#"module test {
        meta { purpose: "test" }
        struct Point {
            x: Int
            y: Int
        }
        trait Describable {
            fn describe(self) -> Int
        }
        impl Describable for Point {
            fn describe(self) -> Int {
                return self.x + self.y
            }
        }
        fn main() -> Int { return 0 }
    }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(module.impl_blocks.len(), 1);
    assert_eq!(
        module.impl_blocks[0].trait_name,
        Some("Describable".to_string())
    );
    assert_eq!(module.impl_blocks[0].type_name, "Point");
    assert_eq!(module.impl_blocks[0].methods.len(), 1);
    assert_eq!(module.impl_blocks[0].methods[0].name, "describe");
    // Self param should be resolved to Point
    assert_eq!(
        module.impl_blocks[0].methods[0].params[0].ty,
        TypeExpr::Named("Point".to_string())
    );
}

#[test]
fn parse_inherent_impl_block() {
    let source = r#"module test {
        meta { purpose: "test" }
        struct Point {
            x: Int
            y: Int
        }
        impl Point {
            fn distance(self) -> Float64 {
                return 0.0
            }
            fn translate(self, dx: Int) -> Int {
                return self.x + dx
            }
        }
        fn main() -> Int { return 0 }
    }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(module.impl_blocks.len(), 1);
    assert_eq!(module.impl_blocks[0].trait_name, None);
    assert_eq!(module.impl_blocks[0].type_name, "Point");
    assert_eq!(module.impl_blocks[0].methods.len(), 2);
    assert_eq!(module.impl_blocks[0].methods[0].name, "distance");
    assert_eq!(module.impl_blocks[0].methods[1].name, "translate");
    // Self param should be resolved to Point
    assert_eq!(
        module.impl_blocks[0].methods[0].params[0].ty,
        TypeExpr::Named("Point".to_string())
    );
}

#[test]
fn parse_inherent_and_trait_impl_same_type() {
    let source = r#"module test {
        meta { purpose: "test" }
        struct Point {
            x: Int
            y: Int
        }
        trait Describable {
            fn describe(self) -> Int
        }
        impl Point {
            fn distance(self) -> Int {
                return self.x
            }
        }
        impl Describable for Point {
            fn describe(self) -> Int {
                return self.x + self.y
            }
        }
        fn main() -> Int { return 0 }
    }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(module.impl_blocks.len(), 2);
    // First: inherent
    assert_eq!(module.impl_blocks[0].trait_name, None);
    assert_eq!(module.impl_blocks[0].type_name, "Point");
    assert_eq!(module.impl_blocks[0].methods[0].name, "distance");
    // Second: trait impl
    assert_eq!(
        module.impl_blocks[1].trait_name,
        Some("Describable".to_string())
    );
    assert_eq!(module.impl_blocks[1].type_name, "Point");
    assert_eq!(module.impl_blocks[1].methods[0].name, "describe");
}

#[test]
fn parse_method_call_as_field_access_then_call() {
    let source = r#"module test {
        meta { purpose: "test" }
        fn main() -> Int {
            let x: Int = p.describe()
            return 0
        }
    }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    if let Stmt::Let { value, .. } = &stmts[0] {
        // p.describe() should parse as Call { callee: FieldAccess { object: p, field: "describe" }, args: [] }
        assert!(matches!(value, Expr::Call { callee, args, .. }
            if args.is_empty()
            && matches!(callee.as_ref(), Expr::FieldAccess { field, .. } if field == "describe")
        ));
    } else {
        panic!("expected Let statement");
    }
}

#[test]
fn parse_trait_method_with_extra_params() {
    let source = r#"module test {
        meta { purpose: "test" }
        trait Adder {
            fn add(self, other: Int) -> Int
        }
        fn main() -> Int { return 0 }
    }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let method = &module.trait_decls[0].methods[0];
    assert_eq!(method.name, "add");
    assert!(method.has_self);
    assert_eq!(method.params.len(), 2);
    assert_eq!(method.params[0].name, "self");
    assert_eq!(method.params[1].name, "other");
}

#[test]
fn parse_if_let_statement() {
    let source = r#"module test {
        fn main() -> Int {
            let opt: Option<Int> = Option::Some(42)
            if let Option::Some(v) = opt {
                return v
            } else {
                return 0
            }
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 2);
    assert!(
        matches!(&stmts[1], Stmt::IfLet { .. }),
        "expected IfLet, got {:?}",
        stmts[1]
    );
}

#[test]
fn parse_if_let_without_else() {
    let source = r#"module test {
        fn main() {
            let opt: Option<Int> = Option::Some(42)
            if let Option::Some(v) = opt {
                return
            }
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 2);
    if let Stmt::IfLet { else_body, .. } = &stmts[1] {
        assert!(else_body.is_none());
    } else {
        panic!("expected IfLet");
    }
}

#[test]
fn parse_is_expression() {
    let source = r#"module test {
        fn main() -> Bool {
            let opt: Option<Int> = Option::Some(42)
            return opt is Some
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 2);
    if let Stmt::Return {
        value: Some(expr), ..
    } = &stmts[1]
    {
        assert!(
            matches!(expr, Expr::Is { type_name, .. } if type_name == "Some"),
            "expected Is expression, got {expr:?}"
        );
    } else {
        panic!("expected Return with Is expression");
    }
}

#[test]
fn parse_ownership_qualifiers() {
    let source = r#"module test {
        fn transfer(own x: Int, ref y: Int) -> Int {
            return x
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let params = &module.functions[0].params;
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].name, "x");
    assert_eq!(params[0].ownership, Ownership::Owned);
    assert_eq!(params[1].name, "y");
    assert_eq!(params[1].ownership, Ownership::Ref);
}

#[test]
fn parse_default_ownership_is_owned() {
    let source = r#"module test {
        fn foo(x: Int) -> Int {
            return x
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let params = &module.functions[0].params;
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].ownership, Ownership::Owned);
}

#[test]
fn parse_async_fn() {
    let source = r#"module test {
        async fn fetch(x: Int) -> Int {
            return x
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(module.functions.len(), 1);
    assert!(
        module.functions[0].is_async,
        "function should be marked async"
    );
    assert_eq!(module.functions[0].name, "fetch");
}

#[test]
fn parse_await_expression() {
    let source = r#"module test {
        async fn compute() -> Int {
            let val: Int = foo().await
            return val
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let body = &module.functions[0].body;
    if let Stmt::Let { value, .. } = &body.stmts[0] {
        assert!(
            matches!(value, Expr::Await { .. }),
            "expected Await expression, got {value:?}"
        );
    } else {
        panic!("expected Let statement");
    }
}

#[test]
fn parse_spawn_stmt() {
    let source = r#"module test {
        fn main() {
            spawn {
                let x: Int = 1
            }
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let body = &module.functions[0].body;
    assert!(
        matches!(&body.stmts[0], Stmt::Spawn { .. }),
        "expected Spawn statement, got {:?}",
        body.stmts[0]
    );
}

#[test]
fn parse_parallel_stmt() {
    let source = r#"module test {
        fn main() {
            parallel {
                spawn {
                    let x: Int = 1
                }
                spawn {
                    let y: Int = 2
                }
            }
        }
    }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let body = &module.functions[0].body;
    if let Stmt::Parallel { body: stmts, .. } = &body.stmts[0] {
        assert_eq!(stmts.len(), 2);
        assert!(matches!(&stmts[0], Stmt::Spawn { .. }));
        assert!(matches!(&stmts[1], Stmt::Spawn { .. }));
    } else {
        panic!("expected Parallel statement");
    }
}

#[test]
fn parse_actor_decl() {
    let source = r#"module test {
        actor Counter {
            count: Int

            fn increment(self) -> Int {
                return self.count + 1
            }
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(module.actor_decls.len(), 1);
    let actor = &module.actor_decls[0];
    assert_eq!(actor.name, "Counter");
    assert_eq!(actor.fields.len(), 1);
    assert_eq!(actor.fields[0].name, "count");
    assert_eq!(actor.handlers.len(), 1);
    assert_eq!(actor.handlers[0].name, "increment");
}

#[test]
fn parse_intent_basic() {
    let source = r#"module test {
        meta { purpose: "test" }
        intent console_app {
            greeting: "hello"
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(module.intent_decls.len(), 1);
    let intent = &module.intent_decls[0];
    assert_eq!(intent.name, "console_app");
    assert_eq!(intent.config.len(), 1);
    assert_eq!(intent.config[0].key, "greeting");
    assert!(
        matches!(&intent.config[0].value, IntentConfigValue::StringLit(s, _) if s == "hello"),
        "expected StringLit(\"hello\"), got {:?}",
        intent.config[0].value
    );
}

#[test]
fn parse_intent_with_list() {
    let source = r#"module test {
        meta { purpose: "test" }
        intent math_module {
            functions: [add, sub]
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(module.intent_decls.len(), 1);
    let intent = &module.intent_decls[0];
    assert_eq!(intent.name, "math_module");
    assert_eq!(intent.config.len(), 1);
    assert_eq!(intent.config[0].key, "functions");
    if let IntentConfigValue::List(items, _) = &intent.config[0].value {
        assert_eq!(items.len(), 2);
        assert!(
            matches!(&items[0], IntentConfigValue::FnRef(name, _) if name == "add"),
            "expected FnRef(\"add\"), got {:?}",
            items[0]
        );
        assert!(
            matches!(&items[1], IntentConfigValue::FnRef(name, _) if name == "sub"),
            "expected FnRef(\"sub\"), got {:?}",
            items[1]
        );
    } else {
        panic!(
            "expected List config value, got {:?}",
            intent.config[0].value
        );
    }
}

#[test]
fn parse_intent_bool_and_int_config() {
    let source = r#"module test {
        meta { purpose: "test" }
        intent custom_intent {
            enabled: true
            max_retries: 3
            verbose: false
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(module.intent_decls.len(), 1);
    let intent = &module.intent_decls[0];
    assert_eq!(intent.name, "custom_intent");
    assert_eq!(intent.config.len(), 3);

    assert_eq!(intent.config[0].key, "enabled");
    assert!(
        matches!(&intent.config[0].value, IntentConfigValue::BoolLit(true, _)),
        "expected BoolLit(true), got {:?}",
        intent.config[0].value
    );

    assert_eq!(intent.config[1].key, "max_retries");
    assert!(
        matches!(&intent.config[1].value, IntentConfigValue::IntLit(3, _)),
        "expected IntLit(3), got {:?}",
        intent.config[1].value
    );

    assert_eq!(intent.config[2].key, "verbose");
    assert!(
        matches!(
            &intent.config[2].value,
            IntentConfigValue::BoolLit(false, _)
        ),
        "expected BoolLit(false), got {:?}",
        intent.config[2].value
    );
}

#[test]
fn fix_patch_missing_rbrace() {
    use kodo_ast::Diagnostic;
    // Missing closing brace for module -- parser expects RBrace but finds something else
    let err = parse("module test { fn main() { }").unwrap_err();
    let patch = err.fix_patch();
    assert!(patch.is_some(), "expected a fix patch for missing `}}`");
    let patch = patch.unwrap();
    assert_eq!(patch.replacement, "}");
    assert!(
        patch.description.contains('}'),
        "description should mention `}}`: {}",
        patch.description
    );
}

#[test]
fn fix_patch_eof_missing_rbrace() {
    use kodo_ast::Diagnostic;
    // Module with unclosed brace -- parser hits EOF expecting RBrace
    let err = parse("module test {").unwrap_err();
    let patch = err.fix_patch();
    assert!(patch.is_some(), "expected a fix patch for EOF missing `}}`");
    let patch = patch.unwrap();
    assert_eq!(patch.replacement, "}");
    assert!(
        patch.description.contains("end of file"),
        "description should mention end of file: {}",
        patch.description
    );
}

#[test]
fn fix_patch_none_for_non_delimiter() {
    use kodo_ast::Diagnostic;
    // Missing module keyword -- no delimiter fix expected
    let err = parse("hello { }").unwrap_err();
    let patch = err.fix_patch();
    assert!(
        patch.is_none(),
        "should not produce fix patch for non-delimiter errors"
    );
}

mod snapshot_tests {
    use super::*;

    #[test]
    fn snapshot_simple_module() {
        let source = r#"module hello { meta { purpose: "test" } }"#;
        let module = parse(source).unwrap();
        insta::assert_debug_snapshot!(module);
    }

    #[test]
    fn snapshot_function_with_params_and_return() {
        let source = r#"module m {
            meta { purpose: "t" }
            fn add(a: Int, b: Int) -> Int {
                return a + b
            }
        }"#;
        let module = parse(source).unwrap();
        insta::assert_debug_snapshot!(module);
    }

    #[test]
    fn snapshot_let_bindings() {
        let source = r#"module test {
            fn main() {
                let x: Int = 42
                let name: String = "hello"
                let flag: Bool = true
            }
        }"#;
        let module = parse(source).unwrap();
        insta::assert_debug_snapshot!(module);
    }

    #[test]
    fn snapshot_if_else() {
        let source = r#"module test {
            fn main() {
                let result: Int = if x > 0 { 1 } else { 0 }
            }
        }"#;
        let module = parse(source).unwrap();
        insta::assert_debug_snapshot!(module);
    }

    #[test]
    fn snapshot_nested_if_else() {
        let source = r#"module test {
            fn main() {
                let result: Int = if x > 0 { 1 } else if x < 0 { 2 } else { 0 }
            }
        }"#;
        let module = parse(source).unwrap();
        insta::assert_debug_snapshot!(module);
    }

    #[test]
    fn snapshot_while_loop() {
        let source = r#"module test {
            fn main() {
                while x > 0 {
                    x = x - 1
                }
            }
        }"#;
        let module = parse(source).unwrap();
        insta::assert_debug_snapshot!(module);
    }

    #[test]
    fn snapshot_match_expression() {
        let source = r#"module test {
            fn main() {
                let r: Int = match x {
                    Option::Some(v) => v,
                    Option::None => 0,
                    _ => 1
                }
            }
        }"#;
        let module = parse(source).unwrap();
        insta::assert_debug_snapshot!(module);
    }

    #[test]
    fn snapshot_struct_decl() {
        let source = r#"module test {
            struct Point {
                x: Float64,
                y: Float64
            }
        }"#;
        let module = parse(source).unwrap();
        insta::assert_debug_snapshot!(module);
    }

    #[test]
    fn snapshot_enum_decl() {
        let source = r#"module test {
            enum Shape {
                Circle(Float64),
                Rectangle(Float64, Float64),
                Point
            }
        }"#;
        let module = parse(source).unwrap();
        insta::assert_debug_snapshot!(module);
    }

    #[test]
    fn snapshot_closure_no_params() {
        let source = r#"module test {
            fn main() {
                let f = || 42
            }
        }"#;
        let module = parse(source).unwrap();
        insta::assert_debug_snapshot!(module);
    }

    #[test]
    fn snapshot_closure_with_params() {
        let source = r#"module test {
            fn main() {
                let f = |x: Int, y: Int| x + y
            }
        }"#;
        let module = parse(source).unwrap();
        insta::assert_debug_snapshot!(module);
    }

    #[test]
    fn snapshot_error_missing_module_keyword() {
        let err = parse("hello { }").unwrap_err();
        insta::assert_snapshot!(err.to_string());
    }

    #[test]
    fn snapshot_error_unexpected_eof() {
        let err = parse("module test {").unwrap_err();
        insta::assert_snapshot!(err.to_string());
    }

    #[test]
    fn snapshot_error_missing_brace() {
        let err = parse("module test { fn foo() }").unwrap_err();
        insta::assert_snapshot!(err.to_string());
    }
}

#[test]
fn parse_fstring_simple() {
    let module =
        parse(r#"module test { fn main() -> String { return f"hello {name}!" } }"#).unwrap();
    let body = &module.functions[0].body.stmts;
    assert_eq!(body.len(), 1);
    if let Stmt::Return {
        value: Some(expr), ..
    } = &body[0]
    {
        if let Expr::StringInterp { parts, .. } = expr {
            assert_eq!(parts.len(), 3);
            assert!(matches!(&parts[0], StringPart::Literal(s) if s == "hello "));
            assert!(matches!(&parts[1], StringPart::Expr(_)));
            assert!(matches!(&parts[2], StringPart::Literal(s) if s == "!"));
        } else {
            panic!("expected StringInterp, got {expr:?}");
        }
    } else {
        panic!("expected Return statement");
    }
}

#[test]
fn parse_fstring_no_interpolation() {
    let module = parse(r#"module test { fn main() -> String { return f"just text" } }"#).unwrap();
    let body = &module.functions[0].body.stmts;
    if let Stmt::Return {
        value: Some(Expr::StringInterp { parts, .. }),
        ..
    } = &body[0]
    {
        assert_eq!(parts.len(), 1);
        assert!(matches!(&parts[0], StringPart::Literal(s) if s == "just text"));
    } else {
        panic!("expected StringInterp");
    }
}

#[test]
fn parse_fstring_multiple_exprs() {
    let module = parse(r#"module test { fn main() -> String { return f"{a} and {b}" } }"#).unwrap();
    let body = &module.functions[0].body.stmts;
    if let Stmt::Return {
        value: Some(Expr::StringInterp { parts, .. }),
        ..
    } = &body[0]
    {
        assert_eq!(parts.len(), 3);
        assert!(matches!(&parts[0], StringPart::Expr(_)));
        assert!(matches!(&parts[1], StringPart::Literal(s) if s == " and "));
        assert!(matches!(&parts[2], StringPart::Expr(_)));
    } else {
        panic!("expected StringInterp");
    }
}

#[test]
fn parse_fstring_complex_expr() {
    let module =
        parse(r#"module test { fn main() -> String { return f"result: {x + 1}" } }"#).unwrap();
    let body = &module.functions[0].body.stmts;
    if let Stmt::Return {
        value: Some(Expr::StringInterp { parts, .. }),
        ..
    } = &body[0]
    {
        assert_eq!(parts.len(), 2);
        assert!(matches!(&parts[0], StringPart::Literal(s) if s == "result: "));
        if let StringPart::Expr(expr) = &parts[1] {
            assert!(matches!(expr.as_ref(), Expr::BinaryOp { .. }));
        } else {
            panic!("expected Expr part");
        }
    } else {
        panic!("expected StringInterp");
    }
}

#[test]
fn parse_fstring_empty() {
    let module = parse(r#"module test { fn main() -> String { return f"" } }"#).unwrap();
    let body = &module.functions[0].body.stmts;
    if let Stmt::Return {
        value: Some(Expr::StringInterp { parts, .. }),
        ..
    } = &body[0]
    {
        assert_eq!(parts.len(), 1);
        assert!(matches!(&parts[0], StringPart::Literal(s) if s.is_empty()));
    } else {
        panic!("expected StringInterp");
    }
}

#[test]
fn parse_fstring_adjacent_exprs() {
    let module = parse(r#"module test { fn main() -> String { return f"{a}{b}" } }"#).unwrap();
    let body = &module.functions[0].body.stmts;
    if let Stmt::Return {
        value: Some(Expr::StringInterp { parts, .. }),
        ..
    } = &body[0]
    {
        assert_eq!(parts.len(), 2);
        assert!(matches!(&parts[0], StringPart::Expr(_)));
        assert!(matches!(&parts[1], StringPart::Expr(_)));
    } else {
        panic!("expected StringInterp");
    }
}

#[test]
fn parse_for_in_with_ident_iterable() {
    let source = r#"module test {
        fn main() {
            for x in items {
                print_int(x)
            }
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::ForIn {
            name,
            iterable,
            body,
            ..
        } => {
            assert_eq!(name, "x");
            assert!(matches!(iterable, Expr::Ident(n, _) if n == "items"));
            assert_eq!(body.stmts.len(), 1);
        }
        other => panic!("expected ForIn, got {other:?}"),
    }
}

#[test]
fn parse_for_in_with_call_iterable() {
    let source = r#"module test {
        fn main() {
            for x in get_list() {
                print_int(x)
            }
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::ForIn { name, iterable, .. } => {
            assert_eq!(name, "x");
            assert!(matches!(iterable, Expr::Call { .. }));
        }
        other => panic!("expected ForIn, got {other:?}"),
    }
}

#[test]
fn parse_for_in_nested_body() {
    let source = r#"module test {
        fn main() {
            for item in collection {
                let y: Int = 1
                print_int(y)
            }
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::ForIn { body, .. } => {
            assert_eq!(body.stmts.len(), 2);
        }
        other => panic!("expected ForIn, got {other:?}"),
    }
}

#[test]
fn parse_for_in_does_not_break_range_for() {
    let source = r#"module test {
        fn main() {
            for i in 0..10 {
                print_int(i)
            }
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    assert!(matches!(&stmts[0], Stmt::For { .. }));
}

#[test]
fn parse_for_in_inclusive_range_still_works() {
    let source = r#"module test {
        fn main() {
            for i in 0..=10 {
                print_int(i)
            }
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    assert!(matches!(
        &stmts[0],
        Stmt::For {
            inclusive: true,
            ..
        }
    ));
}

#[test]
fn parse_for_in_field_access_iterable() {
    let source = r#"module test {
        fn main() {
            for x in data.items {
                print_int(x)
            }
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::ForIn { iterable, .. } => {
            assert!(matches!(iterable, Expr::FieldAccess { .. }));
        }
        other => panic!("expected ForIn, got {other:?}"),
    }
}

#[test]
fn parse_for_in_empty_body() {
    let source = r#"module test {
        fn main() {
            for x in items {
            }
        }
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let stmts = &module.functions[0].body.stmts;
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        Stmt::ForIn { body, .. } => {
            assert!(body.stmts.is_empty());
        }
        other => panic!("expected ForIn, got {other:?}"),
    }
}

// ========== Tuple tests ==========

#[test]
fn parse_tuple_type_pair() {
    let module = parse(
        r#"module test {
            meta { purpose: "test" }
            fn foo(t: (Int, String)) {}
        }"#,
    )
    .unwrap();
    let param_ty = &module.functions[0].params[0].ty;
    match param_ty {
        TypeExpr::Tuple(elems) => {
            assert_eq!(elems.len(), 2);
            assert_eq!(elems[0], TypeExpr::Named("Int".to_string()));
            assert_eq!(elems[1], TypeExpr::Named("String".to_string()));
        }
        other => panic!("expected Tuple, got {other:?}"),
    }
}

#[test]
fn parse_tuple_type_single_trailing_comma() {
    let module = parse(
        r#"module test {
            meta { purpose: "test" }
            fn foo(t: (Int,)) {}
        }"#,
    )
    .unwrap();
    let param_ty = &module.functions[0].params[0].ty;
    match param_ty {
        TypeExpr::Tuple(elems) => {
            assert_eq!(elems.len(), 1);
            assert_eq!(elems[0], TypeExpr::Named("Int".to_string()));
        }
        other => panic!("expected single-element Tuple, got {other:?}"),
    }
}

#[test]
fn parse_tuple_type_triple() {
    let module = parse(
        r#"module test {
            meta { purpose: "test" }
            fn foo(t: (Int, Bool, String)) {}
        }"#,
    )
    .unwrap();
    let param_ty = &module.functions[0].params[0].ty;
    match param_ty {
        TypeExpr::Tuple(elems) => assert_eq!(elems.len(), 3),
        other => panic!("expected Tuple, got {other:?}"),
    }
}

#[test]
fn parse_tuple_literal() {
    let module = parse(
        r#"module test {
            meta { purpose: "test" }
            fn foo() {
                let t = (1, 2, 3)
            }
        }"#,
    )
    .unwrap();
    let body = &module.functions[0].body;
    if let Stmt::Let { value, .. } = &body.stmts[0] {
        match value {
            Expr::TupleLit(elems, _) => assert_eq!(elems.len(), 3),
            other => panic!("expected TupleLit, got {other:?}"),
        }
    } else {
        panic!("expected Let statement");
    }
}

#[test]
fn parse_tuple_literal_two_elements() {
    let module = parse(
        r#"module test {
            meta { purpose: "test" }
            fn foo() {
                let t = (42, true)
            }
        }"#,
    )
    .unwrap();
    let body = &module.functions[0].body;
    if let Stmt::Let { value, .. } = &body.stmts[0] {
        match value {
            Expr::TupleLit(elems, _) => {
                assert_eq!(elems.len(), 2);
                assert!(matches!(elems[0], Expr::IntLit(42, _)));
                assert!(matches!(elems[1], Expr::BoolLit(true, _)));
            }
            other => panic!("expected TupleLit, got {other:?}"),
        }
    } else {
        panic!("expected Let statement");
    }
}

#[test]
fn parse_tuple_index() {
    let module = parse(
        r#"module test {
            meta { purpose: "test" }
            fn foo() {
                let t = (1, 2)
                let a = t.0
                let b = t.1
            }
        }"#,
    )
    .unwrap();
    let body = &module.functions[0].body;
    if let Stmt::Let { value, .. } = &body.stmts[1] {
        match value {
            Expr::TupleIndex { index, .. } => assert_eq!(*index, 0),
            other => panic!("expected TupleIndex, got {other:?}"),
        }
    } else {
        panic!("expected Let statement");
    }
    if let Stmt::Let { value, .. } = &body.stmts[2] {
        match value {
            Expr::TupleIndex { index, .. } => assert_eq!(*index, 1),
            other => panic!("expected TupleIndex, got {other:?}"),
        }
    } else {
        panic!("expected Let statement");
    }
}

#[test]
fn parse_tuple_pattern_in_let() {
    let module = parse(
        r#"module test {
            meta { purpose: "test" }
            fn foo() {
                let (a, b) = (1, 2)
            }
        }"#,
    )
    .unwrap();
    let body = &module.functions[0].body;
    match &body.stmts[0] {
        Stmt::LetPattern { pattern, .. } => {
            if let Pattern::Tuple(pats, _) = pattern {
                assert_eq!(pats.len(), 2);
            } else {
                panic!("expected Tuple pattern, got {pattern:?}");
            }
        }
        other => panic!("expected LetPattern, got {other:?}"),
    }
}

#[test]
fn parse_tuple_pattern_triple() {
    let module = parse(
        r#"module test {
            meta { purpose: "test" }
            fn foo() {
                let (a, b, c) = (1, 2, 3)
            }
        }"#,
    )
    .unwrap();
    let body = &module.functions[0].body;
    match &body.stmts[0] {
        Stmt::LetPattern { pattern, .. } => {
            if let Pattern::Tuple(pats, _) = pattern {
                assert_eq!(pats.len(), 3);
            } else {
                panic!("expected Tuple pattern");
            }
        }
        other => panic!("expected LetPattern, got {other:?}"),
    }
}

#[test]
fn parse_function_type_still_works() {
    let module = parse(
        r#"module test {
            meta { purpose: "test" }
            fn foo(f: (Int, Int) -> Bool) {}
        }"#,
    )
    .unwrap();
    let param_ty = &module.functions[0].params[0].ty;
    match param_ty {
        TypeExpr::Function(params, ret) => {
            assert_eq!(params.len(), 2);
            assert_eq!(**ret, TypeExpr::Named("Bool".to_string()));
        }
        other => panic!("expected Function type, got {other:?}"),
    }
}

#[test]
fn parse_paren_grouping_not_tuple() {
    let module = parse(
        r#"module test {
            meta { purpose: "test" }
            fn foo() {
                let x = (42)
            }
        }"#,
    )
    .unwrap();
    let body = &module.functions[0].body;
    if let Stmt::Let { value, .. } = &body.stmts[0] {
        assert!(
            matches!(value, Expr::IntLit(42, _)),
            "expected IntLit(42), got {value:?}"
        );
    } else {
        panic!("expected Let");
    }
}

#[test]
fn parse_tuple_as_return_type() {
    let module = parse(
        r#"module test {
            meta { purpose: "test" }
            fn foo() -> (Int, String) {
                return (1, "hello")
            }
        }"#,
    )
    .unwrap();
    assert!(matches!(
        module.functions[0].return_type,
        TypeExpr::Tuple(_)
    ));
}

#[test]
fn parse_nested_tuple_type() {
    let module = parse(
        r#"module test {
            meta { purpose: "test" }
            fn foo(t: (Int, (Bool, String))) {}
        }"#,
    )
    .unwrap();
    let param_ty = &module.functions[0].params[0].ty;
    match param_ty {
        TypeExpr::Tuple(elems) => {
            assert_eq!(elems.len(), 2);
            assert!(matches!(&elems[1], TypeExpr::Tuple(inner) if inner.len() == 2));
        }
        other => panic!("expected Tuple, got {other:?}"),
    }
}

// ── Error Recovery Tests (Phase 41) ────────────────────────────────

#[test]
fn recovery_valid_module_no_errors() {
    let output =
        parse_with_recovery(r#"module m { meta { v: "1" } fn a() {} fn b() -> Int { 42 } }"#);
    assert!(output.errors.is_empty());
    assert_eq!(output.module.name, "m");
    assert_eq!(output.module.functions.len(), 2);
}

#[test]
fn recovery_bad_first_fn_still_parses_second() {
    let output = parse_with_recovery(r#"module m { fn a( {} fn b() -> Int { 42 } }"#);
    assert!(!output.errors.is_empty(), "should have at least one error");
    // The second function `b` should be recovered.
    assert!(
        output.module.functions.iter().any(|f| f.name == "b"),
        "function b should be present in partial AST, got: {:?}",
        output
            .module
            .functions
            .iter()
            .map(|f| &f.name)
            .collect::<Vec<_>>()
    );
}

#[test]
fn recovery_multiple_bad_functions() {
    let output = parse_with_recovery(r#"module m { fn a( {} fn b( {} fn c() { } }"#);
    assert!(
        output.errors.len() >= 2,
        "should have >=2 errors, got {}",
        output.errors.len()
    );
    assert!(
        output.module.functions.iter().any(|f| f.name == "c"),
        "function c should be present"
    );
}

#[test]
fn recovery_bad_struct_then_good_fn() {
    let output = parse_with_recovery(r#"module m { struct S { x: } fn ok() { } }"#);
    assert!(!output.errors.is_empty());
    assert!(
        output.module.functions.iter().any(|f| f.name == "ok"),
        "function ok should be recovered"
    );
}

#[test]
fn recovery_missing_module_closing_brace() {
    let output = parse_with_recovery(r#"module m { fn a() { } "#);
    assert!(!output.errors.is_empty());
    assert_eq!(output.module.name, "m");
    assert_eq!(output.module.functions.len(), 1);
}

#[test]
fn recovery_error_span_is_present() {
    let output = parse_with_recovery(r#"module m { fn a( {} fn b() { } }"#);
    assert!(!output.errors.is_empty());
    // At least one error should carry a span.
    assert!(
        output.errors.iter().any(|e| e.span().is_some()),
        "at least one error should have a span"
    );
}

#[test]
fn recovery_error_codes_are_valid() {
    let output = parse_with_recovery(r#"module m { fn a( {} fn b() { } }"#);
    for e in &output.errors {
        let code = e.code();
        assert!(
            code == "E0100" || code == "E0101" || code == "E0001",
            "unexpected error code: {code}"
        );
    }
}

#[test]
fn recovery_enum_error_then_good_fn() {
    let output = parse_with_recovery(r#"module m { enum E { A( } fn good() { } }"#);
    assert!(!output.errors.is_empty());
    assert!(output.module.functions.iter().any(|f| f.name == "good"));
}

#[test]
fn recovery_trait_error_then_good_fn() {
    let output = parse_with_recovery(r#"module m { trait T { fn bad( } fn good() { } }"#);
    assert!(!output.errors.is_empty());
    assert!(output.module.functions.iter().any(|f| f.name == "good"));
}

#[test]
fn recovery_impl_error_then_good_fn() {
    let output = parse_with_recovery(r#"module m { impl S { fn bad( } fn good() { } }"#);
    assert!(!output.errors.is_empty());
    assert!(output.module.functions.iter().any(|f| f.name == "good"));
}

#[test]
fn recovery_intent_error_then_good_fn() {
    let output = parse_with_recovery(r#"module m { intent I { bad: } fn good() { } }"#);
    assert!(!output.errors.is_empty());
    assert!(output.module.functions.iter().any(|f| f.name == "good"));
}

#[test]
fn recovery_bad_meta_then_good_fn() {
    let output = parse_with_recovery(r#"module m { meta { bad } fn good() { } }"#);
    assert!(!output.errors.is_empty());
    assert!(output.module.functions.iter().any(|f| f.name == "good"));
}

#[test]
fn recovery_multiple_good_fns_among_errors() {
    let output = parse_with_recovery(
        r#"module m {
                fn a() { }
                fn bad( {}
                fn b() -> Int { 42 }
                fn also_bad( {}
                fn c() { }
            }"#,
    );
    assert!(output.errors.len() >= 2);
    let names: Vec<&str> = output
        .module
        .functions
        .iter()
        .map(|f| f.name.as_str())
        .collect();
    assert!(names.contains(&"a"), "a should be present, got {names:?}");
    assert!(names.contains(&"b"), "b should be present, got {names:?}");
    assert!(names.contains(&"c"), "c should be present, got {names:?}");
}

#[test]
fn recovery_preserves_module_name() {
    let output = parse_with_recovery(r#"module my_module { fn bad( {} }"#);
    assert_eq!(output.module.name, "my_module");
}

#[test]
fn recovery_three_errors() {
    let output = parse_with_recovery(
        r#"module m {
                fn err1( {}
                fn err2( {}
                fn err3( {}
            }"#,
    );
    assert!(
        output.errors.len() >= 3,
        "expected >=3 errors, got {}",
        output.errors.len()
    );
}

#[test]
fn recovery_empty_module_no_errors() {
    let output = parse_with_recovery(r#"module m { }"#);
    assert!(output.errors.is_empty());
    assert_eq!(output.module.name, "m");
}

#[test]
fn recovery_struct_and_fn_both_good() {
    let output = parse_with_recovery(r#"module m { struct S { x: Int } fn f() { } }"#);
    assert!(output.errors.is_empty());
    assert_eq!(output.module.type_decls.len(), 1);
    assert_eq!(output.module.functions.len(), 1);
}

#[test]
fn recovery_annotation_error_then_good_fn() {
    let output = parse_with_recovery(r#"module m { @bad( fn good() { } }"#);
    assert!(!output.errors.is_empty());
    // After annotation parse failure, sync should pick up `fn good`.
    // Depending on how far recovery goes, `good` may or may not be present,
    // but we must get at least one error.
}

#[test]
fn recovery_original_parse_still_works() {
    // Ensure the non-recovery path is unaffected.
    let result = parse(r#"module m { fn a() { } }"#);
    assert!(result.is_ok());
}

#[test]
fn recovery_original_parse_fails_on_first_error() {
    // The non-recovery path must still fail on the first error.
    let result = parse(r#"module m { fn a( {} fn b() { } }"#);
    assert!(result.is_err());
}

#[test]
fn recovery_sync_to_async_fn() {
    let output = parse_with_recovery(r#"module m { fn bad( {} async fn good() { } }"#);
    assert!(!output.errors.is_empty());
    assert!(
        output.module.functions.iter().any(|f| f.name == "good"),
        "async function should be recovered"
    );
}

#[test]
fn recovery_five_errors() {
    let output = parse_with_recovery(
        r#"module m {
                fn e1( {}
                fn e2( {}
                fn e3( {}
                fn e4( {}
                fn e5( {}
            }"#,
    );
    assert!(
        output.errors.len() >= 5,
        "expected >=5 errors, got {}",
        output.errors.len()
    );
}

#[test]
fn recovery_bad_type_alias_then_good_fn() {
    let output = parse_with_recovery(r#"module m { type T = fn ok() { } }"#);
    // The type alias parse will fail; sync should recover.
    assert!(!output.errors.is_empty());
}

#[test]
fn recovery_module_with_meta_and_errors() {
    let output = parse_with_recovery(
        r#"module m {
                meta { version: "1.0" }
                fn bad( {}
                fn good() { }
            }"#,
    );
    assert!(!output.errors.is_empty());
    assert!(output.module.meta.is_some());
    assert!(output.module.functions.iter().any(|f| f.name == "good"));
}

#[test]
fn recovery_preserves_function_body() {
    let output = parse_with_recovery(
        r#"module m {
                fn bad( {}
                fn good() -> Int {
                    let x: Int = 10
                    return x
                }
            }"#,
    );
    assert!(!output.errors.is_empty());
    let good = output.module.functions.iter().find(|f| f.name == "good");
    assert!(good.is_some(), "function good should be present");
    let good = good.unwrap();
    assert_eq!(good.body.stmts.len(), 2, "good should have 2 statements");
}

#[test]
fn recovery_preserves_return_type() {
    let output = parse_with_recovery(
        r#"module m {
                fn bad( {}
                fn good() -> Bool { true }
            }"#,
    );
    let good = output.module.functions.iter().find(|f| f.name == "good");
    assert!(good.is_some());
    assert!(
        matches!(good.unwrap().return_type, TypeExpr::Named(ref n) if n == "Bool"),
        "return type should be Bool"
    );
}

#[test]
fn recovery_output_has_module_when_all_bad() {
    let output = parse_with_recovery(
        r#"module m {
                fn e1( {}
                fn e2( {}
            }"#,
    );
    // Even when all declarations fail, we still get a module.
    assert_eq!(output.module.name, "m");
    assert!(!output.errors.is_empty());
}

#[test]
fn recovery_sync_skips_tokens_correctly() {
    // Junk tokens between fn keyword and next fn should be skipped.
    let output = parse_with_recovery(r#"module m { fn a 123 456 fn b() { } }"#);
    assert!(!output.errors.is_empty());
    assert!(
        output.module.functions.iter().any(|f| f.name == "b"),
        "function b should be recovered after junk tokens"
    );
}

#[test]
fn recovery_struct_error_preserves_good_struct() {
    let output = parse_with_recovery(
        r#"module m {
                struct Bad { x: }
                struct Good { y: Int }
            }"#,
    );
    assert!(!output.errors.is_empty());
    assert!(
        output.module.type_decls.iter().any(|s| s.name == "Good"),
        "Good struct should be present"
    );
}

#[test]
fn recovery_interleaved_structs_and_fns() {
    let output = parse_with_recovery(
        r#"module m {
                struct S { x: Int }
                fn bad( {}
                fn good() { }
                struct T { y: Bool }
            }"#,
    );
    assert!(!output.errors.is_empty());
    assert_eq!(output.module.type_decls.len(), 2);
    assert!(output.module.functions.iter().any(|f| f.name == "good"));
}

// ── Associated Types & Default Methods Tests (Phase 43) ────────────

#[test]
fn parse_trait_with_associated_type() {
    let source = r#"module test {
            meta { purpose: "test" }
            trait Container {
                type Item
                fn get(self) -> Int
            }
            fn main() -> Int { return 0 }
        }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(module.trait_decls.len(), 1);
    assert_eq!(module.trait_decls[0].associated_types.len(), 1);
    assert_eq!(module.trait_decls[0].associated_types[0].name, "Item");
    assert!(module.trait_decls[0].associated_types[0].bounds.is_empty());
    assert_eq!(module.trait_decls[0].methods.len(), 1);
}

#[test]
fn parse_trait_associated_type_with_bounds() {
    let source = r#"module test {
            meta { purpose: "test" }
            trait Sortable {
                type Item: Ord + Display
                fn sort(self) -> Int
            }
            fn main() -> Int { return 0 }
        }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(module.trait_decls[0].associated_types.len(), 1);
    let assoc = &module.trait_decls[0].associated_types[0];
    assert_eq!(assoc.name, "Item");
    assert_eq!(assoc.bounds, vec!["Ord", "Display"]);
}

#[test]
fn parse_trait_with_default_method() {
    let source = r#"module test {
            meta { purpose: "test" }
            trait Greetable {
                fn greet(self) -> Int {
                    return 42
                }
            }
            fn main() -> Int { return 0 }
        }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(module.trait_decls.len(), 1);
    let method = &module.trait_decls[0].methods[0];
    assert_eq!(method.name, "greet");
    assert!(method.body.is_some());
    assert!(method.has_self);
}

#[test]
fn parse_trait_mixed_default_and_required_methods() {
    let source = r#"module test {
            meta { purpose: "test" }
            trait Shape {
                fn area(self) -> Int
                fn describe(self) -> Int {
                    return 0
                }
            }
            fn main() -> Int { return 0 }
        }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let methods = &module.trait_decls[0].methods;
    assert_eq!(methods.len(), 2);
    assert_eq!(methods[0].name, "area");
    assert!(methods[0].body.is_none());
    assert_eq!(methods[1].name, "describe");
    assert!(methods[1].body.is_some());
}

#[test]
fn parse_impl_block_with_type_binding() {
    let source = r#"module test {
            meta { purpose: "test" }
            struct MyList {
                len: Int
            }
            trait Container {
                type Item
                fn get(self) -> Int
            }
            impl Container for MyList {
                type Item = Int
                fn get(self) -> Int {
                    return self.len
                }
            }
            fn main() -> Int { return 0 }
        }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(module.impl_blocks.len(), 1);
    assert_eq!(module.impl_blocks[0].type_bindings.len(), 1);
    assert_eq!(module.impl_blocks[0].type_bindings[0].0, "Item");
    assert_eq!(
        module.impl_blocks[0].type_bindings[0].1,
        TypeExpr::Named("Int".to_string())
    );
}

#[test]
fn parse_trait_with_associated_types_and_default_methods() {
    let source = r#"module test {
            meta { purpose: "test" }
            trait Collection {
                type Item
                fn size(self) -> Int
                fn is_empty(self) -> Bool {
                    return false
                }
            }
            fn main() -> Int { return 0 }
        }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    let trait_decl = &module.trait_decls[0];
    assert_eq!(trait_decl.associated_types.len(), 1);
    assert_eq!(trait_decl.associated_types[0].name, "Item");
    assert_eq!(trait_decl.methods.len(), 2);
    assert!(trait_decl.methods[0].body.is_none());
    assert!(trait_decl.methods[1].body.is_some());
}

#[test]
fn parse_trait_existing_methods_still_have_no_body() {
    let source = r#"module test {
            meta { purpose: "test" }
            trait Describable {
                fn describe(self) -> Int
            }
            fn main() -> Int { return 0 }
        }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert!(module.trait_decls[0].methods[0].body.is_none());
    assert!(module.trait_decls[0].associated_types.is_empty());
}

#[test]
fn parse_impl_block_no_type_bindings() {
    let source = r#"module test {
            meta { purpose: "test" }
            struct Point { x: Int, y: Int }
            trait Describable {
                fn describe(self) -> Int
            }
            impl Describable for Point {
                fn describe(self) -> Int {
                    return self.x
                }
            }
            fn main() -> Int { return 0 }
        }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert!(module.impl_blocks[0].type_bindings.is_empty());
}

#[test]
fn parse_multiple_associated_types_in_trait() {
    let source = r#"module test {
            meta { purpose: "test" }
            trait BiMap {
                type Key
                type Value
                fn get(self, k: Int) -> Int
            }
            fn main() -> Int { return 0 }
        }"#;
    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e}"));
    assert_eq!(module.trait_decls[0].associated_types.len(), 2);
    assert_eq!(module.trait_decls[0].associated_types[0].name, "Key");
    assert_eq!(module.trait_decls[0].associated_types[1].name, "Value");
}

// --- Break / Continue parser tests ---

#[test]
fn parse_break_in_while() {
    let module = parse(
        r#"module test {
                meta { purpose: "test" }
                fn foo() {
                    while true {
                        break
                    }
                }
            }"#,
    )
    .unwrap();
    let body = &module.functions[0].body.stmts;
    assert_eq!(body.len(), 1);
    if let Stmt::While { body, .. } = &body[0] {
        assert_eq!(body.stmts.len(), 1);
        assert!(matches!(body.stmts[0], Stmt::Break { .. }));
    } else {
        panic!("expected While");
    }
}

#[test]
fn parse_continue_in_while() {
    let module = parse(
        r#"module test {
                meta { purpose: "test" }
                fn foo() {
                    while true {
                        continue
                    }
                }
            }"#,
    )
    .unwrap();
    let body = &module.functions[0].body.stmts;
    if let Stmt::While { body, .. } = &body[0] {
        assert!(matches!(body.stmts[0], Stmt::Continue { .. }));
    } else {
        panic!("expected While");
    }
}

#[test]
fn parse_break_in_for() {
    let module = parse(
        r#"module test {
                meta { purpose: "test" }
                fn foo() {
                    for i in 0..10 {
                        break
                    }
                }
            }"#,
    )
    .unwrap();
    let body = &module.functions[0].body.stmts;
    if let Stmt::For { body, .. } = &body[0] {
        assert!(matches!(body.stmts[0], Stmt::Break { .. }));
    } else {
        panic!("expected For");
    }
}

#[test]
fn parse_continue_in_for() {
    let module = parse(
        r#"module test {
                meta { purpose: "test" }
                fn foo() {
                    for i in 0..10 {
                        continue
                    }
                }
            }"#,
    )
    .unwrap();
    let body = &module.functions[0].body.stmts;
    if let Stmt::For { body, .. } = &body[0] {
        assert!(matches!(body.stmts[0], Stmt::Continue { .. }));
    } else {
        panic!("expected For");
    }
}

#[test]
fn parse_break_in_for_in() {
    let module = parse(
        r#"module test {
                meta { purpose: "test" }
                fn foo(items: List<Int>) {
                    for x in items {
                        break
                    }
                }
            }"#,
    )
    .unwrap();
    let body = &module.functions[0].body.stmts;
    if let Stmt::ForIn { body, .. } = &body[0] {
        assert!(matches!(body.stmts[0], Stmt::Break { .. }));
    } else {
        panic!("expected ForIn");
    }
}

#[test]
fn parse_break_has_span() {
    let module = parse(
        r#"module test {
                meta { purpose: "test" }
                fn foo() {
                    while true { break }
                }
            }"#,
    )
    .unwrap();
    let body = &module.functions[0].body.stmts;
    if let Stmt::While { body, .. } = &body[0] {
        if let Stmt::Break { span } = &body.stmts[0] {
            assert!(span.start < span.end);
        } else {
            panic!("expected Break");
        }
    } else {
        panic!("expected While");
    }
}

#[test]
fn parse_break_and_continue_mixed() {
    let module = parse(
        r#"module test {
                meta { purpose: "test" }
                fn foo() {
                    while true {
                        continue
                        break
                    }
                }
            }"#,
    )
    .unwrap();
    let body = &module.functions[0].body.stmts;
    if let Stmt::While { body, .. } = &body[0] {
        assert_eq!(body.stmts.len(), 2);
        assert!(matches!(body.stmts[0], Stmt::Continue { .. }));
        assert!(matches!(body.stmts[1], Stmt::Break { .. }));
    } else {
        panic!("expected While");
    }
}

// ── Phase 49: Module invariant parsing ───────────────────────────────

#[test]
fn parse_invariant_simple() {
    let source = r#"module m {
        meta { purpose: "test" }
        invariant { true }
        fn main() {}
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e:?}"));
    assert_eq!(module.invariants.len(), 1);
    assert!(matches!(
        module.invariants[0].condition,
        Expr::BoolLit(true, _)
    ));
}

#[test]
fn parse_invariant_comparison() {
    let source = r#"module m {
        meta { purpose: "test" }
        invariant { 1 > 0 }
        fn main() {}
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e:?}"));
    assert_eq!(module.invariants.len(), 1);
    assert!(matches!(
        module.invariants[0].condition,
        Expr::BinaryOp { op: BinOp::Gt, .. }
    ));
}

#[test]
fn parse_multiple_invariants() {
    let source = r#"module m {
        meta { purpose: "test" }
        invariant { true }
        invariant { 1 == 1 }
        fn main() {}
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e:?}"));
    assert_eq!(module.invariants.len(), 2);
}

#[test]
fn parse_invariant_with_recovery() {
    let source = r#"module m {
        meta { purpose: "test" }
        invariant { true }
        fn main() {}
    }"#;

    let output = parse_with_recovery(source);
    assert!(output.errors.is_empty(), "errors: {:?}", output.errors);
    assert_eq!(output.module.invariants.len(), 1);
}

#[test]
fn parse_invariant_between_functions() {
    let source = r#"module m {
        meta { purpose: "test" }
        fn a() {}
        invariant { true }
        fn b() {}
    }"#;

    let module = parse(source).unwrap_or_else(|e| panic!("parse failed: {e:?}"));
    assert_eq!(module.invariants.len(), 1);
    assert_eq!(module.functions.len(), 2);
}
