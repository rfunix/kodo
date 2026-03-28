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
                    bindings: vec![Pattern::Binding("__coalesce_val".to_string(), span)],
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
                    bindings: vec![Pattern::Binding("__try_val".to_string(), span)],
                    span,
                },
                body: Expr::Ident("__try_val".to_string(), span),
                span,
            },
            MatchArm {
                pattern: Pattern::Variant {
                    enum_name: Some("Result".to_string()),
                    variant: "Err".to_string(),
                    bindings: vec![Pattern::Binding("__try_err".to_string(), span)],
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
                    bindings: vec![Pattern::Binding("__chain_val".to_string(), span)],
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

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_ast::Span;

    fn s() -> Span {
        Span::new(0, 10)
    }

    // ── NullCoalesce tests ──────────────────────────────────────────

    #[test]
    fn null_coalesce_produces_match_with_two_arms() {
        let result = desugar_null_coalesce(
            Expr::Ident("opt".to_string(), s()),
            Expr::IntLit(42, s()),
            s(),
        );
        if let Expr::Match { arms, .. } = &result {
            assert_eq!(arms.len(), 2);
        } else {
            panic!("expected Match");
        }
    }

    #[test]
    fn null_coalesce_some_arm_binds_value() {
        let result = desugar_null_coalesce(
            Expr::Ident("opt".to_string(), s()),
            Expr::IntLit(0, s()),
            s(),
        );
        if let Expr::Match { arms, .. } = &result {
            if let Pattern::Variant {
                enum_name,
                variant,
                bindings,
                ..
            } = &arms[0].pattern
            {
                assert_eq!(enum_name.as_deref(), Some("Option"));
                assert_eq!(variant, "Some");
                assert_eq!(bindings.len(), 1);
                assert!(matches!(&bindings[0], Pattern::Binding(n, _) if n == "__coalesce_val"));
            } else {
                panic!("expected Variant pattern in Some arm");
            }
            // The body of the Some arm should be the bound variable.
            assert!(matches!(
                &arms[0].body,
                Expr::Ident(name, _) if name == "__coalesce_val"
            ));
        } else {
            panic!("expected Match");
        }
    }

    #[test]
    fn null_coalesce_none_arm_returns_default() {
        let result = desugar_null_coalesce(
            Expr::Ident("opt".to_string(), s()),
            Expr::IntLit(99, s()),
            s(),
        );
        if let Expr::Match { arms, .. } = &result {
            if let Pattern::Variant { variant, .. } = &arms[1].pattern {
                assert_eq!(variant, "None");
            } else {
                panic!("expected Variant pattern in None arm");
            }
            assert!(matches!(&arms[1].body, Expr::IntLit(99, _)));
        } else {
            panic!("expected Match");
        }
    }

    #[test]
    fn null_coalesce_recursively_desugars_operands() {
        // Nested NullCoalesce inside left operand
        let inner = Expr::NullCoalesce {
            left: Box::new(Expr::Ident("a".to_string(), s())),
            right: Box::new(Expr::IntLit(1, s())),
            span: s(),
        };
        let result = desugar_null_coalesce(inner, Expr::IntLit(2, s()), s());
        if let Expr::Match { expr, .. } = &result {
            // The inner NullCoalesce should have been desugared to Match too.
            assert!(matches!(expr.as_ref(), Expr::Match { .. }));
        } else {
            panic!("expected Match");
        }
    }

    // ── Try tests ───────────────────────────────────────────────────

    #[test]
    fn try_produces_match_with_ok_and_err_arms() {
        let result = desugar_try(Expr::Ident("res".to_string(), s()), s());
        if let Expr::Match { arms, .. } = &result {
            assert_eq!(arms.len(), 2);
            if let Pattern::Variant {
                enum_name, variant, ..
            } = &arms[0].pattern
            {
                assert_eq!(enum_name.as_deref(), Some("Result"));
                assert_eq!(variant, "Ok");
            } else {
                panic!("expected Variant pattern");
            }
            if let Pattern::Variant {
                enum_name, variant, ..
            } = &arms[1].pattern
            {
                assert_eq!(enum_name.as_deref(), Some("Result"));
                assert_eq!(variant, "Err");
            } else {
                panic!("expected Variant pattern");
            }
        } else {
            panic!("expected Match");
        }
    }

    #[test]
    fn try_err_arm_returns_err_variant() {
        let result = desugar_try(Expr::Ident("res".to_string(), s()), s());
        if let Expr::Match { arms, .. } = &result {
            // Err arm body should be a Block containing a Return with EnumVariantExpr
            if let Expr::Block(block) = &arms[1].body {
                assert_eq!(block.stmts.len(), 1);
                if let Stmt::Return {
                    value: Some(Expr::EnumVariantExpr { variant, .. }),
                    ..
                } = &block.stmts[0]
                {
                    assert_eq!(variant, "Err");
                } else {
                    panic!("expected Return with EnumVariantExpr");
                }
            } else {
                panic!("expected Block in Err arm body");
            }
        } else {
            panic!("expected Match");
        }
    }

    #[test]
    fn try_ok_arm_extracts_value() {
        let result = desugar_try(Expr::Ident("res".to_string(), s()), s());
        if let Expr::Match { arms, .. } = &result {
            if let Pattern::Variant { bindings, .. } = &arms[0].pattern {
                assert_eq!(bindings.len(), 1);
                assert!(matches!(&bindings[0], Pattern::Binding(n, _) if n == "__try_val"));
            }
            assert!(matches!(
                &arms[0].body,
                Expr::Ident(name, _) if name == "__try_val"
            ));
        } else {
            panic!("expected Match");
        }
    }

    #[test]
    fn try_recursively_desugars_operand() {
        let inner = Expr::NullCoalesce {
            left: Box::new(Expr::Ident("x".to_string(), s())),
            right: Box::new(Expr::IntLit(0, s())),
            span: s(),
        };
        let result = desugar_try(inner, s());
        if let Expr::Match { expr, .. } = &result {
            assert!(matches!(expr.as_ref(), Expr::Match { .. }));
        } else {
            panic!("expected Match");
        }
    }

    // ── OptionalChain tests ─────────────────────────────────────────

    #[test]
    fn optional_chain_produces_some_with_field_access() {
        let result =
            desugar_optional_chain(Expr::Ident("obj".to_string(), s()), "name".to_string(), s());
        if let Expr::Match { arms, .. } = &result {
            assert_eq!(arms.len(), 2);
            // Some arm wraps field access in Some
            if let Expr::EnumVariantExpr { variant, args, .. } = &arms[0].body {
                assert_eq!(variant, "Some");
                assert_eq!(args.len(), 1);
                if let Expr::FieldAccess { field, .. } = &args[0] {
                    assert_eq!(field, "name");
                } else {
                    panic!("expected FieldAccess");
                }
            } else {
                panic!("expected EnumVariantExpr in Some arm");
            }
        } else {
            panic!("expected Match");
        }
    }

    #[test]
    fn optional_chain_none_arm_returns_none() {
        let result =
            desugar_optional_chain(Expr::Ident("obj".to_string(), s()), "x".to_string(), s());
        if let Expr::Match { arms, .. } = &result {
            if let Expr::EnumVariantExpr { variant, args, .. } = &arms[1].body {
                assert_eq!(variant, "None");
                assert!(args.is_empty());
            } else {
                panic!("expected EnumVariantExpr None in None arm");
            }
        } else {
            panic!("expected Match");
        }
    }

    #[test]
    fn optional_chain_recursively_desugars_object() {
        let inner = Expr::NullCoalesce {
            left: Box::new(Expr::Ident("a".to_string(), s())),
            right: Box::new(Expr::Ident("b".to_string(), s())),
            span: s(),
        };
        let result = desugar_optional_chain(inner, "field".to_string(), s());
        if let Expr::Match { expr, .. } = &result {
            assert!(matches!(expr.as_ref(), Expr::Match { .. }));
        } else {
            panic!("expected Match");
        }
    }

    // ── Is tests ────────────────────────────────────────────────────

    #[test]
    fn is_produces_match_returning_bools() {
        let result = desugar_is(Expr::Ident("val".to_string(), s()), "Some".to_string(), s());
        if let Expr::Match { arms, .. } = &result {
            assert_eq!(arms.len(), 2);
            assert!(matches!(&arms[0].body, Expr::BoolLit(true, _)));
            assert!(matches!(&arms[1].body, Expr::BoolLit(false, _)));
        } else {
            panic!("expected Match");
        }
    }

    #[test]
    fn is_first_arm_uses_variant_pattern() {
        let result = desugar_is(
            Expr::Ident("val".to_string(), s()),
            "MyVariant".to_string(),
            s(),
        );
        if let Expr::Match { arms, .. } = &result {
            if let Pattern::Variant {
                enum_name,
                variant,
                bindings,
                ..
            } = &arms[0].pattern
            {
                assert!(enum_name.is_none());
                assert_eq!(variant, "MyVariant");
                assert!(bindings.is_empty());
            } else {
                panic!("expected Variant pattern");
            }
        } else {
            panic!("expected Match");
        }
    }

    #[test]
    fn is_second_arm_is_wildcard() {
        let result = desugar_is(Expr::Ident("val".to_string(), s()), "X".to_string(), s());
        if let Expr::Match { arms, .. } = &result {
            assert!(matches!(&arms[1].pattern, Pattern::Wildcard(_)));
        } else {
            panic!("expected Match");
        }
    }

    #[test]
    fn is_recursively_desugars_operand() {
        let inner = Expr::Try {
            operand: Box::new(Expr::Ident("r".to_string(), s())),
            span: s(),
        };
        let result = desugar_is(inner, "Ok".to_string(), s());
        if let Expr::Match { expr, .. } = &result {
            assert!(matches!(expr.as_ref(), Expr::Match { .. }));
        } else {
            panic!("expected Match");
        }
    }
}
