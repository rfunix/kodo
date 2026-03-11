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

/// Desugars a `for-in` loop over a collection into indexed access with a `while` loop.
///
/// Transforms `for x in iterable { body }` into:
/// ```text
/// let mut __forin_idx_x = 0
/// let __forin_len_x = len(iterable)
/// while __forin_idx_x < __forin_len_x {
///     let x = get(iterable, __forin_idx_x)
///     body
///     __forin_idx_x = __forin_idx_x + 1
/// }
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

    let idx_name = format!("__forin_idx_{name}");
    let let_idx = Stmt::Let {
        span,
        mutable: true,
        name: idx_name.clone(),
        ty: None,
        value: Expr::IntLit(0, span),
    };

    let len_name = format!("__forin_len_{name}");
    let let_len = Stmt::Let {
        span,
        mutable: false,
        name: len_name.clone(),
        ty: None,
        value: Expr::Call {
            callee: Box::new(Expr::Ident("len".to_string(), span)),
            args: vec![iterable.clone()],
            span,
        },
    };

    let condition = Expr::BinaryOp {
        left: Box::new(Expr::Ident(idx_name.clone(), span)),
        op: BinOp::Lt,
        right: Box::new(Expr::Ident(len_name, span)),
        span,
    };

    let let_elem = Stmt::Let {
        span,
        mutable: false,
        name: name.to_string(),
        ty: None,
        value: Expr::Call {
            callee: Box::new(Expr::Ident("get".to_string(), span)),
            args: vec![iterable, Expr::Ident(idx_name.clone(), span)],
            span,
        },
    };

    let increment = Stmt::Assign {
        span,
        name: idx_name.clone(),
        value: Expr::BinaryOp {
            left: Box::new(Expr::Ident(idx_name, span)),
            op: BinOp::Add,
            right: Box::new(Expr::IntLit(1, span)),
            span,
        },
    };

    let mut while_stmts = vec![let_elem];
    while_stmts.extend(body.stmts);
    while_stmts.push(increment);

    let while_stmt = Stmt::While {
        span,
        condition,
        body: Block {
            span,
            stmts: while_stmts,
        },
    };

    new_stmts.push(let_idx);
    new_stmts.push(let_len);
    new_stmts.push(while_stmt);
}
