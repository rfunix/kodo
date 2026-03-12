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
