//! Desugaring of `for` and `for-in` loops.
//!
//! Transforms range-based `for` loops into `let mut` + `while` and
//! collection-based `for-in` loops into indexed access with a `while` loop.

use kodo_ast::{BinOp, Block, Expr, Span, Stmt};

use crate::{desugar_block, desugar_expr};

/// Desugars a `for` loop into `let mut` + `while` loop.
pub(crate) fn desugar_for_stmt(
    new_stmts: &mut Vec<Stmt>,
    span: Span,
    name: &str,
    start: Expr,
    end: Expr,
    inclusive: bool,
    mut body: Block,
) {
    desugar_block(&mut body);
    let start = desugar_expr(start);
    let end = desugar_expr(end);

    let let_stmt = Stmt::Let {
        span,
        mutable: true,
        name: name.to_string(),
        ty: None,
        value: start,
    };

    let op = if inclusive { BinOp::Le } else { BinOp::Lt };
    let condition = Expr::BinaryOp {
        left: Box::new(Expr::Ident(name.to_string(), span)),
        op,
        right: Box::new(end),
        span,
    };

    let increment = Stmt::Assign {
        span,
        name: name.to_string(),
        value: Expr::BinaryOp {
            left: Box::new(Expr::Ident(name.to_string(), span)),
            op: BinOp::Add,
            right: Box::new(Expr::IntLit(1, span)),
            span,
        },
    };
    body.stmts.push(increment);

    let while_stmt = Stmt::While {
        span,
        condition,
        body,
    };

    new_stmts.push(let_stmt);
    new_stmts.push(while_stmt);
}

/// Desugars a `for-in` loop over a collection using the Iterator protocol.
///
/// Transforms `for x in iterable { body }` into:
/// ```text
/// let __iter_x = init_fn(iterable)
/// while advance_fn(__iter_x) > 0 {
///     let x = value_fn(__iter_x)
///     body
/// }
/// free_fn(__iter_x)
/// ```
///
/// The iterator protocol functions are selected based on the iterable
/// expression. When the iterable is a call to `Map_keys(...)`,
/// `Map_values(...)`, or `String_chars(...)` (as resolved by the type
/// checker from method syntax), the corresponding map/string iterator
/// functions are used instead of the default list iterator.
pub(crate) fn desugar_for_in_stmt(
    new_stmts: &mut Vec<Stmt>,
    span: Span,
    name: &str,
    iterable: Expr,
    mut body: Block,
) {
    desugar_block(&mut body);
    let iterable = desugar_expr(iterable);

    let iter_name = format!("__iter_{name}");

    // Detect if iterating over map keys/values or string chars.
    // The type checker resolves `map.keys()` to `Map_keys(map)` etc.
    let (init_fn, advance_fn, value_fn, free_fn) = detect_iterator_protocol(&iterable);

    // let __iter_x = init_fn(iterable)
    let let_iter = Stmt::Let {
        span,
        mutable: false,
        name: iter_name.clone(),
        ty: None,
        value: if is_already_iterator_call(&iterable) {
            // The iterable is already a call like Map_keys(map) — use it directly
            iterable
        } else {
            Expr::Call {
                callee: Box::new(Expr::Ident(init_fn.to_string(), span)),
                args: vec![iterable],
                span,
            }
        },
    };

    // while advance_fn(__iter_x) > 0 { ... }
    let condition = Expr::BinaryOp {
        left: Box::new(Expr::Call {
            callee: Box::new(Expr::Ident(advance_fn.to_string(), span)),
            args: vec![Expr::Ident(iter_name.clone(), span)],
            span,
        }),
        op: BinOp::Gt,
        right: Box::new(Expr::IntLit(0, span)),
        span,
    };

    // let x = value_fn(__iter_x)
    let let_elem = Stmt::Let {
        span,
        mutable: false,
        name: name.to_string(),
        ty: None,
        value: Expr::Call {
            callee: Box::new(Expr::Ident(value_fn.to_string(), span)),
            args: vec![Expr::Ident(iter_name.clone(), span)],
            span,
        },
    };

    let mut while_stmts = vec![let_elem];
    while_stmts.extend(body.stmts);

    let while_stmt = Stmt::While {
        span,
        condition,
        body: Block {
            span,
            stmts: while_stmts,
        },
    };

    // free_fn(__iter_x)
    let free_iter = Stmt::Expr(Expr::Call {
        callee: Box::new(Expr::Ident(free_fn.to_string(), span)),
        args: vec![Expr::Ident(iter_name, span)],
        span,
    });

    new_stmts.push(let_iter);
    new_stmts.push(while_stmt);
    new_stmts.push(free_iter);
}

/// Detects which iterator protocol to use based on the iterable expression.
///
/// Returns `(init_fn, advance_fn, value_fn, free_fn)` names.
fn detect_iterator_protocol(
    iterable: &Expr,
) -> (&'static str, &'static str, &'static str, &'static str) {
    if let Expr::Call { callee, .. } = iterable {
        if let Expr::Ident(name, _) = callee.as_ref() {
            match name.as_str() {
                "Map_keys" => {
                    return (
                        "Map_keys",
                        "map_keys_advance",
                        "map_keys_value",
                        "map_keys_free",
                    )
                }
                "Map_values" => {
                    return (
                        "Map_values",
                        "map_values_advance",
                        "map_values_value",
                        "map_values_free",
                    )
                }
                "String_chars" => {
                    return (
                        "String_chars",
                        "string_chars_advance",
                        "string_chars_value",
                        "string_chars_free",
                    )
                }
                _ => {}
            }
        }
    }
    (
        "list_iter",
        "list_iterator_advance",
        "list_iterator_value",
        "list_iterator_free",
    )
}

/// Checks if the iterable expression is already an iterator creation call.
///
/// `Map_keys(map)` and `Map_values(map)` are already iterator constructors —
/// we don't need to wrap them in another init call.
fn is_already_iterator_call(iterable: &Expr) -> bool {
    if let Expr::Call { callee, .. } = iterable {
        if let Expr::Ident(name, _) = callee.as_ref() {
            return matches!(name.as_str(), "Map_keys" | "Map_values" | "String_chars");
        }
    }
    false
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

    // ── desugar_for_stmt tests ──────────────────────────────────────

    #[test]
    fn for_exclusive_emits_let_and_while() {
        let mut stmts = Vec::new();
        desugar_for_stmt(
            &mut stmts,
            s(),
            "i",
            Expr::IntLit(0, s()),
            Expr::IntLit(10, s()),
            false,
            empty_block(),
        );
        assert_eq!(stmts.len(), 2);
        assert!(matches!(&stmts[0], Stmt::Let { mutable: true, name, .. } if name == "i"));
        assert!(matches!(&stmts[1], Stmt::While { .. }));
    }

    #[test]
    fn for_exclusive_uses_lt_operator() {
        let mut stmts = Vec::new();
        desugar_for_stmt(
            &mut stmts,
            s(),
            "i",
            Expr::IntLit(0, s()),
            Expr::IntLit(5, s()),
            false,
            empty_block(),
        );
        if let Stmt::While { condition, .. } = &stmts[1] {
            if let Expr::BinaryOp { op, .. } = condition {
                assert_eq!(*op, BinOp::Lt);
            } else {
                panic!("expected BinaryOp");
            }
        } else {
            panic!("expected While");
        }
    }

    #[test]
    fn for_inclusive_uses_le_operator() {
        let mut stmts = Vec::new();
        desugar_for_stmt(
            &mut stmts,
            s(),
            "i",
            Expr::IntLit(0, s()),
            Expr::IntLit(5, s()),
            true,
            empty_block(),
        );
        if let Stmt::While { condition, .. } = &stmts[1] {
            if let Expr::BinaryOp { op, .. } = condition {
                assert_eq!(*op, BinOp::Le);
            } else {
                panic!("expected BinaryOp");
            }
        } else {
            panic!("expected While");
        }
    }

    #[test]
    fn for_loop_appends_increment_to_body() {
        let mut stmts = Vec::new();
        desugar_for_stmt(
            &mut stmts,
            s(),
            "i",
            Expr::IntLit(0, s()),
            Expr::IntLit(3, s()),
            false,
            empty_block(),
        );
        if let Stmt::While { body, .. } = &stmts[1] {
            // Empty body + increment = 1 statement
            assert_eq!(body.stmts.len(), 1);
            if let Stmt::Assign { name, value, .. } = &body.stmts[0] {
                assert_eq!(name, "i");
                if let Expr::BinaryOp { op, .. } = value {
                    assert_eq!(*op, BinOp::Add);
                } else {
                    panic!("expected BinaryOp for increment");
                }
            } else {
                panic!("expected Assign for increment");
            }
        } else {
            panic!("expected While");
        }
    }

    #[test]
    fn for_loop_preserves_body_stmts_before_increment() {
        let body = Block {
            span: s(),
            stmts: vec![
                Stmt::Expr(Expr::IntLit(1, s())),
                Stmt::Expr(Expr::IntLit(2, s())),
            ],
        };
        let mut stmts = Vec::new();
        desugar_for_stmt(
            &mut stmts,
            s(),
            "i",
            Expr::IntLit(0, s()),
            Expr::IntLit(5, s()),
            false,
            body,
        );
        if let Stmt::While { body, .. } = &stmts[1] {
            assert_eq!(body.stmts.len(), 3); // 2 original + 1 increment
            assert!(matches!(&body.stmts[0], Stmt::Expr(Expr::IntLit(1, _))));
            assert!(matches!(&body.stmts[1], Stmt::Expr(Expr::IntLit(2, _))));
            assert!(matches!(&body.stmts[2], Stmt::Assign { .. }));
        } else {
            panic!("expected While");
        }
    }

    #[test]
    fn for_loop_let_has_correct_start_value() {
        let mut stmts = Vec::new();
        desugar_for_stmt(
            &mut stmts,
            s(),
            "x",
            Expr::IntLit(42, s()),
            Expr::IntLit(100, s()),
            false,
            empty_block(),
        );
        if let Stmt::Let { name, value, .. } = &stmts[0] {
            assert_eq!(name, "x");
            assert!(matches!(value, Expr::IntLit(42, _)));
        } else {
            panic!("expected Let");
        }
    }

    #[test]
    fn for_loop_desugars_sugar_in_start_and_end() {
        let start = Expr::NullCoalesce {
            left: Box::new(Expr::Ident("a".to_string(), s())),
            right: Box::new(Expr::IntLit(0, s())),
            span: s(),
        };
        let end = Expr::NullCoalesce {
            left: Box::new(Expr::Ident("b".to_string(), s())),
            right: Box::new(Expr::IntLit(10, s())),
            span: s(),
        };
        let mut stmts = Vec::new();
        desugar_for_stmt(&mut stmts, s(), "i", start, end, false, empty_block());
        // Start expr should be desugared
        if let Stmt::Let { value, .. } = &stmts[0] {
            assert!(matches!(value, Expr::Match { .. }));
        } else {
            panic!("expected Let");
        }
        // End expr in condition should be desugared
        if let Stmt::While { condition, .. } = &stmts[1] {
            if let Expr::BinaryOp { right, .. } = condition {
                assert!(matches!(right.as_ref(), Expr::Match { .. }));
            }
        }
    }

    // ── desugar_for_in_stmt tests ───────────────────────────────────

    #[test]
    fn for_in_emits_let_while_free() {
        let mut stmts = Vec::new();
        desugar_for_in_stmt(
            &mut stmts,
            s(),
            "x",
            Expr::Ident("items".to_string(), s()),
            empty_block(),
        );
        assert_eq!(stmts.len(), 3);
        assert!(matches!(&stmts[0], Stmt::Let { name, .. } if name == "__iter_x"));
        assert!(matches!(&stmts[1], Stmt::While { .. }));
        assert!(matches!(&stmts[2], Stmt::Expr(Expr::Call { .. })));
    }

    #[test]
    fn for_in_uses_list_iterator_for_plain_iterable() {
        let mut stmts = Vec::new();
        desugar_for_in_stmt(
            &mut stmts,
            s(),
            "x",
            Expr::Ident("my_list".to_string(), s()),
            empty_block(),
        );
        // Init call should be list_iter
        if let Stmt::Let { value, .. } = &stmts[0] {
            if let Expr::Call { callee, .. } = value {
                assert!(matches!(callee.as_ref(), Expr::Ident(name, _) if name == "list_iter"));
            } else {
                panic!("expected Call");
            }
        } else {
            panic!("expected Let");
        }
        // Free call should be list_iterator_free
        if let Stmt::Expr(Expr::Call { callee, .. }) = &stmts[2] {
            assert!(
                matches!(callee.as_ref(), Expr::Ident(name, _) if name == "list_iterator_free")
            );
        }
    }

    #[test]
    fn for_in_uses_map_keys_protocol_for_map_keys() {
        let mut stmts = Vec::new();
        let iterable = Expr::Call {
            callee: Box::new(Expr::Ident("Map_keys".to_string(), s())),
            args: vec![Expr::Ident("my_map".to_string(), s())],
            span: s(),
        };
        desugar_for_in_stmt(&mut stmts, s(), "k", iterable, empty_block());
        // Init should use the Map_keys call directly (already iterator)
        if let Stmt::Let { value, .. } = &stmts[0] {
            if let Expr::Call { callee, .. } = value {
                assert!(matches!(callee.as_ref(), Expr::Ident(name, _) if name == "Map_keys"));
            } else {
                panic!("expected Call");
            }
        }
        // Advance should use map_keys_advance
        if let Stmt::While { condition, .. } = &stmts[1] {
            if let Expr::BinaryOp { left, .. } = condition {
                if let Expr::Call { callee, .. } = left.as_ref() {
                    assert!(
                        matches!(callee.as_ref(), Expr::Ident(name, _) if name == "map_keys_advance")
                    );
                }
            }
        }
        // Free should use map_keys_free
        if let Stmt::Expr(Expr::Call { callee, .. }) = &stmts[2] {
            assert!(matches!(callee.as_ref(), Expr::Ident(name, _) if name == "map_keys_free"));
        }
    }

    #[test]
    fn for_in_uses_map_values_protocol() {
        let mut stmts = Vec::new();
        let iterable = Expr::Call {
            callee: Box::new(Expr::Ident("Map_values".to_string(), s())),
            args: vec![Expr::Ident("m".to_string(), s())],
            span: s(),
        };
        desugar_for_in_stmt(&mut stmts, s(), "v", iterable, empty_block());
        if let Stmt::While { condition, .. } = &stmts[1] {
            if let Expr::BinaryOp { left, .. } = condition {
                if let Expr::Call { callee, .. } = left.as_ref() {
                    assert!(
                        matches!(callee.as_ref(), Expr::Ident(name, _) if name == "map_values_advance")
                    );
                }
            }
        }
    }

    #[test]
    fn for_in_uses_string_chars_protocol() {
        let mut stmts = Vec::new();
        let iterable = Expr::Call {
            callee: Box::new(Expr::Ident("String_chars".to_string(), s())),
            args: vec![Expr::Ident("text".to_string(), s())],
            span: s(),
        };
        desugar_for_in_stmt(&mut stmts, s(), "ch", iterable, empty_block());
        if let Stmt::While { condition, .. } = &stmts[1] {
            if let Expr::BinaryOp { left, .. } = condition {
                if let Expr::Call { callee, .. } = left.as_ref() {
                    assert!(
                        matches!(callee.as_ref(), Expr::Ident(name, _) if name == "string_chars_advance")
                    );
                }
            }
        }
        if let Stmt::Expr(Expr::Call { callee, .. }) = &stmts[2] {
            assert!(matches!(callee.as_ref(), Expr::Ident(name, _) if name == "string_chars_free"));
        }
    }

    #[test]
    fn for_in_while_body_starts_with_let_binding() {
        let body = Block {
            span: s(),
            stmts: vec![Stmt::Expr(Expr::IntLit(99, s()))],
        };
        let mut stmts = Vec::new();
        desugar_for_in_stmt(
            &mut stmts,
            s(),
            "elem",
            Expr::Ident("list".to_string(), s()),
            body,
        );
        if let Stmt::While { body, .. } = &stmts[1] {
            // First stmt is `let elem = list_iterator_value(__iter_elem)`
            assert!(matches!(&body.stmts[0], Stmt::Let { name, .. } if name == "elem"));
            // Second stmt is the original body stmt
            assert!(matches!(&body.stmts[1], Stmt::Expr(Expr::IntLit(99, _))));
        } else {
            panic!("expected While");
        }
    }

    #[test]
    fn for_in_condition_checks_advance_gt_zero() {
        let mut stmts = Vec::new();
        desugar_for_in_stmt(
            &mut stmts,
            s(),
            "x",
            Expr::Ident("items".to_string(), s()),
            empty_block(),
        );
        if let Stmt::While { condition, .. } = &stmts[1] {
            if let Expr::BinaryOp { op, right, .. } = condition {
                assert_eq!(*op, BinOp::Gt);
                assert!(matches!(right.as_ref(), Expr::IntLit(0, _)));
            } else {
                panic!("expected BinaryOp");
            }
        } else {
            panic!("expected While");
        }
    }

    // ── detect_iterator_protocol tests ──────────────────────────────

    #[test]
    fn detect_protocol_default_for_plain_ident() {
        let expr = Expr::Ident("list".to_string(), s());
        let (init, advance, value, free) = detect_iterator_protocol(&expr);
        assert_eq!(init, "list_iter");
        assert_eq!(advance, "list_iterator_advance");
        assert_eq!(value, "list_iterator_value");
        assert_eq!(free, "list_iterator_free");
    }

    #[test]
    fn detect_protocol_for_map_keys() {
        let expr = Expr::Call {
            callee: Box::new(Expr::Ident("Map_keys".to_string(), s())),
            args: vec![],
            span: s(),
        };
        let (_, advance, value, free) = detect_iterator_protocol(&expr);
        assert_eq!(advance, "map_keys_advance");
        assert_eq!(value, "map_keys_value");
        assert_eq!(free, "map_keys_free");
    }

    #[test]
    fn detect_protocol_for_map_values() {
        let expr = Expr::Call {
            callee: Box::new(Expr::Ident("Map_values".to_string(), s())),
            args: vec![],
            span: s(),
        };
        let (_, advance, value, free) = detect_iterator_protocol(&expr);
        assert_eq!(advance, "map_values_advance");
        assert_eq!(value, "map_values_value");
        assert_eq!(free, "map_values_free");
    }

    #[test]
    fn detect_protocol_for_string_chars() {
        let expr = Expr::Call {
            callee: Box::new(Expr::Ident("String_chars".to_string(), s())),
            args: vec![],
            span: s(),
        };
        let (_, advance, value, free) = detect_iterator_protocol(&expr);
        assert_eq!(advance, "string_chars_advance");
        assert_eq!(value, "string_chars_value");
        assert_eq!(free, "string_chars_free");
    }

    #[test]
    fn detect_protocol_unknown_call_uses_list() {
        let expr = Expr::Call {
            callee: Box::new(Expr::Ident("something_else".to_string(), s())),
            args: vec![],
            span: s(),
        };
        let (init, _, _, _) = detect_iterator_protocol(&expr);
        assert_eq!(init, "list_iter");
    }

    // ── is_already_iterator_call tests ──────────────────────────────

    #[test]
    fn is_iterator_call_for_map_keys() {
        let expr = Expr::Call {
            callee: Box::new(Expr::Ident("Map_keys".to_string(), s())),
            args: vec![],
            span: s(),
        };
        assert!(is_already_iterator_call(&expr));
    }

    #[test]
    fn is_iterator_call_for_map_values() {
        let expr = Expr::Call {
            callee: Box::new(Expr::Ident("Map_values".to_string(), s())),
            args: vec![],
            span: s(),
        };
        assert!(is_already_iterator_call(&expr));
    }

    #[test]
    fn is_iterator_call_for_string_chars() {
        let expr = Expr::Call {
            callee: Box::new(Expr::Ident("String_chars".to_string(), s())),
            args: vec![],
            span: s(),
        };
        assert!(is_already_iterator_call(&expr));
    }

    #[test]
    fn is_not_iterator_call_for_plain_ident() {
        let expr = Expr::Ident("list".to_string(), s());
        assert!(!is_already_iterator_call(&expr));
    }

    #[test]
    fn is_not_iterator_call_for_unknown_call() {
        let expr = Expr::Call {
            callee: Box::new(Expr::Ident("foo".to_string(), s())),
            args: vec![],
            span: s(),
        };
        assert!(!is_already_iterator_call(&expr));
    }
}
