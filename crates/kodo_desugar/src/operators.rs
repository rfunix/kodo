//! Desugaring of sugar operators (`??`, `?`, `?.`, `is`).
//!
//! Transforms syntactic sugar operators into match expressions
//! over `Option` and `Result` types.

use kodo_ast::{Block, Expr, MatchArm, Pattern, Span, Stmt};

use crate::desugar_expr;

/// Desugars `expr ?? default` into a match on `Option`.
pub(crate) fn desugar_null_coalesce(left: Expr, right: Expr, span: Span) -> Expr {
    let left = desugar_expr(left);
    let right = desugar_expr(right);
    Expr::Match {
        expr: Box::new(left),
        arms: vec![
            MatchArm {
                pattern: Pattern::Variant {
                    enum_name: Some("Option".to_string()),
                    variant: "Some".to_string(),
                    bindings: vec!["__coalesce_val".to_string()],
                    span,
                },
                body: Expr::Ident("__coalesce_val".to_string(), span),
                span,
            },
            MatchArm {
                pattern: Pattern::Variant {
                    enum_name: Some("Option".to_string()),
                    variant: "None".to_string(),
                    bindings: vec![],
                    span,
                },
                body: right,
                span,
            },
        ],
        span,
    }
}

/// Desugars `expr?` into a match on `Result`.
pub(crate) fn desugar_try(operand: Expr, span: Span) -> Expr {
    let operand = desugar_expr(operand);
    let return_err = Expr::Block(Block {
        span,
        stmts: vec![Stmt::Return {
            span,
            value: Some(Expr::EnumVariantExpr {
                enum_name: "Result".to_string(),
                variant: "Err".to_string(),
                args: vec![Expr::Ident("__try_err".to_string(), span)],
                span,
            }),
        }],
    });
    Expr::Match {
        expr: Box::new(operand),
        arms: vec![
            MatchArm {
                pattern: Pattern::Variant {
                    enum_name: Some("Result".to_string()),
                    variant: "Ok".to_string(),
                    bindings: vec!["__try_val".to_string()],
                    span,
                },
                body: Expr::Ident("__try_val".to_string(), span),
                span,
            },
            MatchArm {
                pattern: Pattern::Variant {
                    enum_name: Some("Result".to_string()),
                    variant: "Err".to_string(),
                    bindings: vec!["__try_err".to_string()],
                    span,
                },
                body: return_err,
                span,
            },
        ],
        span,
    }
}

/// Desugars `expr?.field` into a match on `Option` with field access.
pub(crate) fn desugar_optional_chain(object: Expr, field: String, span: Span) -> Expr {
    let object = desugar_expr(object);
    Expr::Match {
        expr: Box::new(object),
        arms: vec![
            MatchArm {
                pattern: Pattern::Variant {
                    enum_name: Some("Option".to_string()),
                    variant: "Some".to_string(),
                    bindings: vec!["__chain_val".to_string()],
                    span,
                },
                body: Expr::EnumVariantExpr {
                    enum_name: "Option".to_string(),
                    variant: "Some".to_string(),
                    args: vec![Expr::FieldAccess {
                        object: Box::new(Expr::Ident("__chain_val".to_string(), span)),
                        field,
                        span,
                    }],
                    span,
                },
                span,
            },
            MatchArm {
                pattern: Pattern::Variant {
                    enum_name: Some("Option".to_string()),
                    variant: "None".to_string(),
                    bindings: vec![],
                    span,
                },
                body: Expr::EnumVariantExpr {
                    enum_name: "Option".to_string(),
                    variant: "None".to_string(),
                    args: vec![],
                    span,
                },
                span,
            },
        ],
        span,
    }
}

/// Desugars `expr is VariantName` into a match that returns bool.
pub(crate) fn desugar_is(operand: Expr, type_name: String, span: Span) -> Expr {
    let operand = desugar_expr(operand);
    Expr::Match {
        expr: Box::new(operand),
        arms: vec![
            MatchArm {
                pattern: Pattern::Variant {
                    enum_name: None,
                    variant: type_name,
                    bindings: vec![],
                    span,
                },
                body: Expr::BoolLit(true, span),
                span,
            },
            MatchArm {
                pattern: Pattern::Wildcard(span),
                body: Expr::BoolLit(false, span),
                span,
            },
        ],
        span,
    }
}
