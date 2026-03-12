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
/// let __iter_x = list_iter(iterable)
/// while list_iterator_advance(__iter_x) > 0 {
///     let x = list_iterator_value(__iter_x)
///     body
/// }
/// list_iterator_free(__iter_x)
/// ```
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

    // let __iter_x = list_iter(iterable)
    let let_iter = Stmt::Let {
        span,
        mutable: false,
        name: iter_name.clone(),
        ty: None,
        value: Expr::Call {
            callee: Box::new(Expr::Ident("list_iter".to_string(), span)),
            args: vec![iterable],
            span,
        },
    };

    // while list_iterator_advance(__iter_x) > 0 { ... }
    let condition = Expr::BinaryOp {
        left: Box::new(Expr::Call {
            callee: Box::new(Expr::Ident("list_iterator_advance".to_string(), span)),
            args: vec![Expr::Ident(iter_name.clone(), span)],
            span,
        }),
        op: BinOp::Gt,
        right: Box::new(Expr::IntLit(0, span)),
        span,
    };

    // let x = list_iterator_value(__iter_x)
    let let_elem = Stmt::Let {
        span,
        mutable: false,
        name: name.to_string(),
        ty: None,
        value: Expr::Call {
            callee: Box::new(Expr::Ident("list_iterator_value".to_string(), span)),
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

    // list_iterator_free(__iter_x)
    let free_iter = Stmt::Expr(Expr::Call {
        callee: Box::new(Expr::Ident("list_iterator_free".to_string(), span)),
        args: vec![Expr::Ident(iter_name, span)],
        span,
    });

    new_stmts.push(let_iter);
    new_stmts.push(while_stmt);
    new_stmts.push(free_iter);
}
