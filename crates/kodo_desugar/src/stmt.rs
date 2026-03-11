//! Statement desugaring — dispatch for each statement kind.
//!
//! The main entry point is [`desugar_block`], which walks all statements
//! in a block and delegates to the appropriate transform.

use kodo_ast::{Block, Expr, MatchArm, Pattern, Span, Stmt};

use crate::expr::desugar_expr;
use crate::for_loop::{desugar_for_in_stmt, desugar_for_stmt};

/// Desugars all statements in a block.
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
