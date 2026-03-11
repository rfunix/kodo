//! Expression desugaring — recursive traversal of the AST expression tree.
//!
//! The main entry point is [`desugar_expr`], which dispatches sugar operators
//! to the appropriate transform and recursively desugars sub-expressions.

use kodo_ast::{BinOp, Expr, MatchArm, StringPart};

use crate::operators::{desugar_is, desugar_null_coalesce, desugar_optional_chain, desugar_try};
use crate::stmt::desugar_block;

/// Recursively desugars an expression, transforming sugar operators
/// (`??`, `?`, `?.`) into match expressions.
pub(crate) fn desugar_expr(expr: Expr) -> Expr {
    match expr {
        Expr::NullCoalesce { left, right, span } => desugar_null_coalesce(*left, *right, span),
        Expr::Try { operand, span } => desugar_try(*operand, span),
        Expr::OptionalChain {
            object,
            field,
            span,
        } => desugar_optional_chain(*object, field, span),
        Expr::Is {
            operand,
            type_name,
            span,
        } => desugar_is(*operand, type_name, span),
        Expr::BinaryOp {
            left,
            op,
            right,
            span,
        } => Expr::BinaryOp {
            left: Box::new(desugar_expr(*left)),
            op,
            right: Box::new(desugar_expr(*right)),
            span,
        },
        Expr::UnaryOp { op, operand, span } => Expr::UnaryOp {
            op,
            operand: Box::new(desugar_expr(*operand)),
            span,
        },
        Expr::Call { callee, args, span } => Expr::Call {
            callee: Box::new(desugar_expr(*callee)),
            args: args.into_iter().map(desugar_expr).collect(),
            span,
        },
        Expr::FieldAccess {
            object,
            field,
            span,
        } => Expr::FieldAccess {
            object: Box::new(desugar_expr(*object)),
            field,
            span,
        },
        Expr::EnumVariantExpr {
            enum_name,
            variant,
            args,
            span,
        } => Expr::EnumVariantExpr {
            enum_name,
            variant,
            args: args.into_iter().map(desugar_expr).collect(),
            span,
        },
        Expr::Range {
            start,
            end,
            inclusive,
            span,
        } => Expr::Range {
            start: Box::new(desugar_expr(*start)),
            end: Box::new(desugar_expr(*end)),
            inclusive,
            span,
        },
        Expr::Await { operand, span } => Expr::Await {
            operand: Box::new(desugar_expr(*operand)),
            span,
        },
        // StringInterp: `f"hello {name}!"` =>
        // "hello " + to_string(name) + "!"
        // where to_string is resolved via method call rewriting for non-String types.
        Expr::StringInterp { parts, span } => desugar_string_interp(parts, span),
        Expr::TupleLit(elems, span) => {
            Expr::TupleLit(elems.into_iter().map(desugar_expr).collect(), span)
        }
        Expr::TupleIndex { tuple, index, span } => Expr::TupleIndex {
            tuple: Box::new(desugar_expr(*tuple)),
            index,
            span,
        },
        e @ (Expr::IntLit(_, _)
        | Expr::FloatLit(_, _)
        | Expr::StringLit(_, _)
        | Expr::BoolLit(_, _)
        | Expr::Ident(_, _)) => e,
        other => desugar_compound_expr(other),
    }
}

/// Desugars compound expressions that contain sub-expressions.
fn desugar_compound_expr(expr: Expr) -> Expr {
    match expr {
        Expr::If {
            condition,
            mut then_branch,
            else_branch,
            span,
        } => {
            let condition = desugar_expr(*condition);
            desugar_block(&mut then_branch);
            let else_branch = else_branch.map(|mut b| {
                desugar_block(&mut b);
                b
            });
            Expr::If {
                condition: Box::new(condition),
                then_branch,
                else_branch,
                span,
            }
        }
        Expr::StructLit { name, fields, span } => Expr::StructLit {
            name,
            fields: fields
                .into_iter()
                .map(|f| kodo_ast::FieldInit {
                    name: f.name,
                    value: desugar_expr(f.value),
                    span: f.span,
                })
                .collect(),
            span,
        },
        Expr::Match { expr, arms, span } => Expr::Match {
            expr: Box::new(desugar_expr(*expr)),
            arms: arms
                .into_iter()
                .map(|arm| MatchArm {
                    pattern: arm.pattern,
                    body: desugar_expr(arm.body),
                    span: arm.span,
                })
                .collect(),
            span,
        },
        Expr::Block(mut block) => {
            desugar_block(&mut block);
            Expr::Block(block)
        }
        Expr::Closure {
            params,
            return_type,
            body,
            span,
        } => Expr::Closure {
            params,
            return_type,
            body: Box::new(desugar_expr(*body)),
            span,
        },
        other => other,
    }
}

/// Desugars a string interpolation expression into a chain of string
/// concatenation using `+`.
///
/// `f"hello {name}!"` becomes `"hello " + name + "!"`
///
/// Each expression part is concatenated directly. Non-string expressions must
/// have `.to_string()` called explicitly within the `{...}` braces — this is
/// consistent with Kodo's "no implicit conversions" principle.
fn desugar_string_interp(parts: Vec<StringPart>, span: kodo_ast::Span) -> Expr {
    let mut exprs: Vec<Expr> = Vec::with_capacity(parts.len());
    for part in parts {
        match part {
            StringPart::Literal(s) => {
                exprs.push(Expr::StringLit(s, span));
            }
            StringPart::Expr(expr) => {
                exprs.push(desugar_expr(*expr));
            }
        }
    }

    // Build a left-associative chain of BinaryOp::Add
    let mut result = exprs.remove(0);
    for expr in exprs {
        result = Expr::BinaryOp {
            left: Box::new(result),
            op: BinOp::Add,
            right: Box::new(expr),
            span,
        };
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_ast::Span;

    #[test]
    fn desugar_string_interp_literal_only() {
        let span = Span::new(0, 10);
        let parts = vec![StringPart::Literal("hello".to_string())];
        let result = desugar_string_interp(parts, span);
        assert!(
            matches!(result, Expr::StringLit(ref s, _) if s == "hello"),
            "single literal part should produce StringLit"
        );
    }

    #[test]
    fn desugar_string_interp_with_expr() {
        let span = Span::new(0, 20);
        let parts = vec![
            StringPart::Literal("hello ".to_string()),
            StringPart::Expr(Box::new(Expr::Ident("name".to_string(), span))),
            StringPart::Literal("!".to_string()),
        ];
        let result = desugar_string_interp(parts, span);
        assert!(matches!(result, Expr::BinaryOp { op: BinOp::Add, .. }));
    }

    #[test]
    fn desugar_string_interp_single_expr() {
        let span = Span::new(0, 10);
        let parts = vec![StringPart::Expr(Box::new(Expr::IntLit(42, span)))];
        let result = desugar_string_interp(parts, span);
        assert!(matches!(result, Expr::IntLit(42, _)));
    }
}
