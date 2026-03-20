//! Statement desugaring — dispatch for each statement kind.
//!
//! The main entry point is [`desugar_block`], which walks all statements
//! in a block and delegates to the appropriate transform.

use kodo_ast::{Block, Expr, MatchArm, Pattern, Span, Stmt};

use crate::expr::desugar_expr;
use crate::for_loop::{desugar_for_in_stmt, desugar_for_stmt};

/// Desugars all statements in a block.
#[allow(clippy::too_many_lines)]
pub(crate) fn desugar_block(block: &mut Block) {
    let mut new_stmts = Vec::new();
    for stmt in std::mem::take(&mut block.stmts) {
        match stmt {
            Stmt::For {
                span,
                name,
                start,
                end,
                inclusive,
                body,
            } => desugar_for_stmt(&mut new_stmts, span, &name, start, end, inclusive, body),
            Stmt::ForIn {
                span,
                name,
                iterable,
                body,
            } => desugar_for_in_stmt(&mut new_stmts, span, &name, iterable, body),
            Stmt::While {
                span,
                condition,
                mut body,
            } => {
                desugar_block(&mut body);
                let condition = desugar_expr(condition);
                new_stmts.push(Stmt::While {
                    span,
                    condition,
                    body,
                });
            }
            Stmt::Let {
                span,
                mutable,
                name,
                ty,
                value,
            } => {
                let value = desugar_expr(value);
                new_stmts.push(Stmt::Let {
                    span,
                    mutable,
                    name,
                    ty,
                    value,
                });
            }
            Stmt::Expr(expr) => {
                new_stmts.push(Stmt::Expr(desugar_expr(expr)));
            }
            Stmt::Return { span, value } => {
                let value = value.map(desugar_expr);
                new_stmts.push(Stmt::Return { span, value });
            }
            Stmt::Assign { span, name, value } => {
                let value = desugar_expr(value);
                new_stmts.push(Stmt::Assign { span, name, value });
            }
            Stmt::IfLet {
                span,
                pattern,
                value,
                body,
                else_body,
            } => desugar_if_let_stmt(&mut new_stmts, span, pattern, value, body, else_body),
            Stmt::LetPattern {
                span,
                mutable,
                pattern,
                ty,
                value,
            } => {
                let value = desugar_expr(value);
                new_stmts.push(Stmt::LetPattern {
                    span,
                    mutable,
                    pattern,
                    ty,
                    value,
                });
            }
            Stmt::Spawn { span, mut body } => {
                desugar_block(&mut body);
                new_stmts.push(Stmt::Spawn { span, body });
            }
            Stmt::Parallel { span, body } => {
                desugar_parallel_stmt(&mut new_stmts, span, body);
            }
            Stmt::Break { span } => {
                new_stmts.push(Stmt::Break { span });
            }
            Stmt::Continue { span } => {
                new_stmts.push(Stmt::Continue { span });
            }
            Stmt::ForAll {
                span,
                bindings,
                mut body,
            } => {
                desugar_block(&mut body);
                new_stmts.push(Stmt::ForAll {
                    span,
                    bindings,
                    body,
                });
            }
        }
    }
    block.stmts = new_stmts;
}

/// Desugars an `if let` into a `match` expression.
fn desugar_if_let_stmt(
    new_stmts: &mut Vec<Stmt>,
    span: Span,
    pattern: Pattern,
    value: Expr,
    mut body: Block,
    else_body: Option<Block>,
) {
    desugar_block(&mut body);
    let value = desugar_expr(value);

    let mut else_block = else_body.unwrap_or(Block {
        span,
        stmts: vec![],
    });
    desugar_block(&mut else_block);

    let then_expr = Expr::Block(body);
    let else_expr = Expr::Block(else_block);

    let match_expr = Expr::Match {
        expr: Box::new(value),
        arms: vec![
            MatchArm {
                pattern,
                body: then_expr,
                span,
            },
            MatchArm {
                pattern: Pattern::Wildcard(span),
                body: else_expr,
                span,
            },
        ],
        span,
    };
    new_stmts.push(Stmt::Expr(match_expr));
}

/// Desugars a `parallel` block by recursively desugaring inner spawn blocks.
fn desugar_parallel_stmt(new_stmts: &mut Vec<Stmt>, span: Span, body: Vec<Stmt>) {
    let mut desugared = Vec::new();
    for stmt in body {
        match stmt {
            Stmt::Spawn { span: s, mut body } => {
                desugar_block(&mut body);
                desugared.push(Stmt::Spawn { span: s, body });
            }
            other => desugared.push(other),
        }
    }
    new_stmts.push(Stmt::Parallel {
        span,
        body: desugared,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_ast::Span;

    fn s() -> Span {
        Span::new(0, 10)
    }

    fn empty_block() -> Block {
        Block {
            span: s(),
            stmts: vec![],
        }
    }

    // ── desugar_block basic tests ───────────────────────────────────

    #[test]
    fn desugar_block_empty() {
        let mut block = empty_block();
        desugar_block(&mut block);
        assert!(block.stmts.is_empty());
    }

    #[test]
    fn desugar_block_preserves_let() {
        let mut block = Block {
            span: s(),
            stmts: vec![Stmt::Let {
                span: s(),
                mutable: false,
                name: "x".to_string(),
                ty: None,
                value: Expr::IntLit(1, s()),
            }],
        };
        desugar_block(&mut block);
        assert_eq!(block.stmts.len(), 1);
        assert!(matches!(&block.stmts[0], Stmt::Let { name, .. } if name == "x"));
    }

    #[test]
    fn desugar_block_preserves_return() {
        let mut block = Block {
            span: s(),
            stmts: vec![Stmt::Return {
                span: s(),
                value: Some(Expr::IntLit(42, s())),
            }],
        };
        desugar_block(&mut block);
        assert_eq!(block.stmts.len(), 1);
        assert!(matches!(&block.stmts[0], Stmt::Return { .. }));
    }

    #[test]
    fn desugar_block_preserves_assign() {
        let mut block = Block {
            span: s(),
            stmts: vec![Stmt::Assign {
                span: s(),
                name: "x".to_string(),
                value: Expr::IntLit(10, s()),
            }],
        };
        desugar_block(&mut block);
        assert_eq!(block.stmts.len(), 1);
        assert!(matches!(&block.stmts[0], Stmt::Assign { .. }));
    }

    #[test]
    fn desugar_block_preserves_break_and_continue() {
        let mut block = Block {
            span: s(),
            stmts: vec![Stmt::Break { span: s() }, Stmt::Continue { span: s() }],
        };
        desugar_block(&mut block);
        assert_eq!(block.stmts.len(), 2);
        assert!(matches!(&block.stmts[0], Stmt::Break { .. }));
        assert!(matches!(&block.stmts[1], Stmt::Continue { .. }));
    }

    // ── Let with sugar value ────────────────────────────────────────

    #[test]
    fn desugar_block_desugars_let_value() {
        let mut block = Block {
            span: s(),
            stmts: vec![Stmt::Let {
                span: s(),
                mutable: false,
                name: "x".to_string(),
                ty: None,
                value: Expr::NullCoalesce {
                    left: Box::new(Expr::Ident("opt".to_string(), s())),
                    right: Box::new(Expr::IntLit(0, s())),
                    span: s(),
                },
            }],
        };
        desugar_block(&mut block);
        if let Stmt::Let { value, .. } = &block.stmts[0] {
            assert!(matches!(value, Expr::Match { .. }));
        } else {
            panic!("expected Let");
        }
    }

    // ── Assign with sugar value ─────────────────────────────────────

    #[test]
    fn desugar_block_desugars_assign_value() {
        let mut block = Block {
            span: s(),
            stmts: vec![Stmt::Assign {
                span: s(),
                name: "x".to_string(),
                value: Expr::Try {
                    operand: Box::new(Expr::Ident("res".to_string(), s())),
                    span: s(),
                },
            }],
        };
        desugar_block(&mut block);
        if let Stmt::Assign { value, .. } = &block.stmts[0] {
            assert!(matches!(value, Expr::Match { .. }));
        } else {
            panic!("expected Assign");
        }
    }

    // ── Return with sugar value ─────────────────────────────────────

    #[test]
    fn desugar_block_desugars_return_value() {
        let mut block = Block {
            span: s(),
            stmts: vec![Stmt::Return {
                span: s(),
                value: Some(Expr::NullCoalesce {
                    left: Box::new(Expr::Ident("x".to_string(), s())),
                    right: Box::new(Expr::IntLit(0, s())),
                    span: s(),
                }),
            }],
        };
        desugar_block(&mut block);
        if let Stmt::Return {
            value: Some(val), ..
        } = &block.stmts[0]
        {
            assert!(matches!(val, Expr::Match { .. }));
        } else {
            panic!("expected Return with Match");
        }
    }

    #[test]
    fn desugar_block_return_none_unchanged() {
        let mut block = Block {
            span: s(),
            stmts: vec![Stmt::Return {
                span: s(),
                value: None,
            }],
        };
        desugar_block(&mut block);
        assert!(matches!(&block.stmts[0], Stmt::Return { value: None, .. }));
    }

    // ── Expr statement ──────────────────────────────────────────────

    #[test]
    fn desugar_block_desugars_expr_stmt() {
        let mut block = Block {
            span: s(),
            stmts: vec![Stmt::Expr(Expr::Is {
                operand: Box::new(Expr::Ident("x".to_string(), s())),
                type_name: "Some".to_string(),
                span: s(),
            })],
        };
        desugar_block(&mut block);
        assert!(matches!(&block.stmts[0], Stmt::Expr(Expr::Match { .. })));
    }

    // ── While desugars body ─────────────────────────────────────────

    #[test]
    fn desugar_block_while_desugars_body_and_condition() {
        let mut block = Block {
            span: s(),
            stmts: vec![Stmt::While {
                span: s(),
                condition: Expr::Is {
                    operand: Box::new(Expr::Ident("x".to_string(), s())),
                    type_name: "Some".to_string(),
                    span: s(),
                },
                body: Block {
                    span: s(),
                    stmts: vec![Stmt::Expr(Expr::NullCoalesce {
                        left: Box::new(Expr::Ident("a".to_string(), s())),
                        right: Box::new(Expr::IntLit(0, s())),
                        span: s(),
                    })],
                },
            }],
        };
        desugar_block(&mut block);
        if let Stmt::While {
            condition, body, ..
        } = &block.stmts[0]
        {
            assert!(matches!(condition, Expr::Match { .. }));
            assert!(matches!(&body.stmts[0], Stmt::Expr(Expr::Match { .. })));
        } else {
            panic!("expected While");
        }
    }

    // ── For loop expansion ──────────────────────────────────────────

    #[test]
    fn desugar_block_for_expands_to_let_while() {
        let mut block = Block {
            span: s(),
            stmts: vec![Stmt::For {
                span: s(),
                name: "i".to_string(),
                start: Expr::IntLit(0, s()),
                end: Expr::IntLit(5, s()),
                inclusive: false,
                body: empty_block(),
            }],
        };
        desugar_block(&mut block);
        assert_eq!(block.stmts.len(), 2);
        assert!(matches!(&block.stmts[0], Stmt::Let { mutable: true, .. }));
        assert!(matches!(&block.stmts[1], Stmt::While { .. }));
    }

    // ── ForIn expansion ─────────────────────────────────────────────

    #[test]
    fn desugar_block_for_in_expands_to_let_while_free() {
        let mut block = Block {
            span: s(),
            stmts: vec![Stmt::ForIn {
                span: s(),
                name: "x".to_string(),
                iterable: Expr::Ident("items".to_string(), s()),
                body: empty_block(),
            }],
        };
        desugar_block(&mut block);
        assert_eq!(block.stmts.len(), 3);
    }

    // ── IfLet to match ──────────────────────────────────────────────

    #[test]
    fn desugar_block_if_let_becomes_match() {
        let mut block = Block {
            span: s(),
            stmts: vec![Stmt::IfLet {
                span: s(),
                pattern: Pattern::Variant {
                    enum_name: Some("Option".to_string()),
                    variant: "Some".to_string(),
                    bindings: vec!["v".to_string()],
                    span: s(),
                },
                value: Expr::Ident("opt".to_string(), s()),
                body: Block {
                    span: s(),
                    stmts: vec![Stmt::Expr(Expr::Ident("v".to_string(), s()))],
                },
                else_body: None,
            }],
        };
        desugar_block(&mut block);
        assert_eq!(block.stmts.len(), 1);
        if let Stmt::Expr(Expr::Match { arms, .. }) = &block.stmts[0] {
            assert_eq!(arms.len(), 2);
            assert!(matches!(
                &arms[0].pattern,
                Pattern::Variant { variant, .. } if variant == "Some"
            ));
            assert!(matches!(&arms[1].pattern, Pattern::Wildcard(_)));
        } else {
            panic!("expected Match expression from if-let desugar");
        }
    }

    #[test]
    fn desugar_block_if_let_with_else_body() {
        let mut block = Block {
            span: s(),
            stmts: vec![Stmt::IfLet {
                span: s(),
                pattern: Pattern::Variant {
                    enum_name: Some("Option".to_string()),
                    variant: "Some".to_string(),
                    bindings: vec!["v".to_string()],
                    span: s(),
                },
                value: Expr::Ident("opt".to_string(), s()),
                body: Block {
                    span: s(),
                    stmts: vec![Stmt::Return {
                        span: s(),
                        value: Some(Expr::Ident("v".to_string(), s())),
                    }],
                },
                else_body: Some(Block {
                    span: s(),
                    stmts: vec![Stmt::Return {
                        span: s(),
                        value: Some(Expr::IntLit(0, s())),
                    }],
                }),
            }],
        };
        desugar_block(&mut block);
        if let Stmt::Expr(Expr::Match { arms, .. }) = &block.stmts[0] {
            // Wildcard arm should have the else body
            if let Expr::Block(else_block) = &arms[1].body {
                assert_eq!(else_block.stmts.len(), 1);
                assert!(matches!(&else_block.stmts[0], Stmt::Return { .. }));
            } else {
                panic!("expected Block in else arm");
            }
        } else {
            panic!("expected Match");
        }
    }

    // ── LetPattern preservation ─────────────────────────────────────

    #[test]
    fn desugar_block_let_pattern_desugars_value() {
        let mut block = Block {
            span: s(),
            stmts: vec![Stmt::LetPattern {
                span: s(),
                mutable: false,
                pattern: Pattern::Variant {
                    enum_name: Some("Option".to_string()),
                    variant: "Some".to_string(),
                    bindings: vec!["v".to_string()],
                    span: s(),
                },
                ty: None,
                value: Expr::NullCoalesce {
                    left: Box::new(Expr::Ident("a".to_string(), s())),
                    right: Box::new(Expr::IntLit(0, s())),
                    span: s(),
                },
            }],
        };
        desugar_block(&mut block);
        if let Stmt::LetPattern { value, .. } = &block.stmts[0] {
            assert!(matches!(value, Expr::Match { .. }));
        } else {
            panic!("expected LetPattern");
        }
    }

    // ── Spawn desugars body ─────────────────────────────────────────

    #[test]
    fn desugar_block_spawn_desugars_inner_body() {
        let mut block = Block {
            span: s(),
            stmts: vec![Stmt::Spawn {
                span: s(),
                body: Block {
                    span: s(),
                    stmts: vec![Stmt::Expr(Expr::NullCoalesce {
                        left: Box::new(Expr::Ident("x".to_string(), s())),
                        right: Box::new(Expr::IntLit(0, s())),
                        span: s(),
                    })],
                },
            }],
        };
        desugar_block(&mut block);
        if let Stmt::Spawn { body, .. } = &block.stmts[0] {
            assert!(matches!(&body.stmts[0], Stmt::Expr(Expr::Match { .. })));
        } else {
            panic!("expected Spawn");
        }
    }

    // ── Parallel desugars inner spawns ──────────────────────────────

    #[test]
    fn desugar_block_parallel_desugars_spawn_bodies() {
        let mut block = Block {
            span: s(),
            stmts: vec![Stmt::Parallel {
                span: s(),
                body: vec![Stmt::Spawn {
                    span: s(),
                    body: Block {
                        span: s(),
                        stmts: vec![Stmt::Expr(Expr::Try {
                            operand: Box::new(Expr::Ident("r".to_string(), s())),
                            span: s(),
                        })],
                    },
                }],
            }],
        };
        desugar_block(&mut block);
        if let Stmt::Parallel { body, .. } = &block.stmts[0] {
            if let Stmt::Spawn { body, .. } = &body[0] {
                assert!(matches!(&body.stmts[0], Stmt::Expr(Expr::Match { .. })));
            } else {
                panic!("expected Spawn inside Parallel");
            }
        } else {
            panic!("expected Parallel");
        }
    }

    #[test]
    fn desugar_block_parallel_preserves_non_spawn() {
        let mut block = Block {
            span: s(),
            stmts: vec![Stmt::Parallel {
                span: s(),
                body: vec![Stmt::Expr(Expr::IntLit(42, s()))],
            }],
        };
        desugar_block(&mut block);
        if let Stmt::Parallel { body, .. } = &block.stmts[0] {
            assert!(matches!(&body[0], Stmt::Expr(Expr::IntLit(42, _))));
        } else {
            panic!("expected Parallel");
        }
    }

    // ── Multiple statements ordering ────────────────────────────────

    #[test]
    fn desugar_block_multiple_stmts_preserves_order() {
        let mut block = Block {
            span: s(),
            stmts: vec![
                Stmt::Let {
                    span: s(),
                    mutable: false,
                    name: "a".to_string(),
                    ty: None,
                    value: Expr::IntLit(1, s()),
                },
                Stmt::Expr(Expr::IntLit(2, s())),
                Stmt::Return {
                    span: s(),
                    value: Some(Expr::IntLit(3, s())),
                },
            ],
        };
        desugar_block(&mut block);
        assert_eq!(block.stmts.len(), 3);
        assert!(matches!(&block.stmts[0], Stmt::Let { .. }));
        assert!(matches!(&block.stmts[1], Stmt::Expr(_)));
        assert!(matches!(&block.stmts[2], Stmt::Return { .. }));
    }

    #[test]
    fn desugar_block_for_expands_inline_preserving_surrounding() {
        let mut block = Block {
            span: s(),
            stmts: vec![
                Stmt::Let {
                    span: s(),
                    mutable: false,
                    name: "before".to_string(),
                    ty: None,
                    value: Expr::IntLit(1, s()),
                },
                Stmt::For {
                    span: s(),
                    name: "i".to_string(),
                    start: Expr::IntLit(0, s()),
                    end: Expr::IntLit(3, s()),
                    inclusive: false,
                    body: empty_block(),
                },
                Stmt::Return {
                    span: s(),
                    value: Some(Expr::IntLit(0, s())),
                },
            ],
        };
        desugar_block(&mut block);
        // let before + let i + while + return = 4
        assert_eq!(block.stmts.len(), 4);
        assert!(matches!(&block.stmts[0], Stmt::Let { name, .. } if name == "before"));
        assert!(matches!(&block.stmts[1], Stmt::Let { name, .. } if name == "i"));
        assert!(matches!(&block.stmts[2], Stmt::While { .. }));
        assert!(matches!(&block.stmts[3], Stmt::Return { .. }));
    }
}
