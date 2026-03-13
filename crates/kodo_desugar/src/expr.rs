//! Expression desugaring — recursive traversal of the AST expression tree.
//!
//! The main entry point is [`desugar_expr`], which dispatches sugar operators
//! to the appropriate transform and recursively desugars sub-expressions.

use kodo_ast::{Expr, MatchArm, StringPart};

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
        // StringInterp is passed through to MIR lowering, which has type
        // information needed to insert appropriate conversions (Int_to_string,
        // Float64_to_string, Bool_to_string) for non-String interpolated
        // expressions. Sub-expressions are still desugared recursively.
        Expr::StringInterp { parts, span } => Expr::StringInterp {
            parts: parts
                .into_iter()
                .map(|p| match p {
                    StringPart::Literal(_) => p,
                    StringPart::Expr(e) => StringPart::Expr(Box::new(desugar_expr(*e))),
                })
                .collect(),
            span,
        },
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

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_ast::Span;

    #[test]
    fn desugar_string_interp_passes_through() {
        let span = Span::new(0, 20);
        let parts = vec![
            StringPart::Literal("hello ".to_string()),
            StringPart::Expr(Box::new(Expr::Ident("name".to_string(), span))),
            StringPart::Literal("!".to_string()),
        ];
        let expr = Expr::StringInterp { parts, span };
        let result = desugar_expr(expr);
        // StringInterp is preserved (handled by MIR lowering for type-aware conversion).
        assert!(matches!(result, Expr::StringInterp { .. }));
    }

    #[test]
    fn desugar_string_interp_desugars_sub_exprs() {
        let span = Span::new(0, 20);
        // A NullCoalesce inside StringInterp should be desugared.
        let nc = Expr::NullCoalesce {
            left: Box::new(Expr::Ident("x".to_string(), span)),
            right: Box::new(Expr::IntLit(0, span)),
            span,
        };
        let parts = vec![
            StringPart::Literal("val: ".to_string()),
            StringPart::Expr(Box::new(nc)),
        ];
        let expr = Expr::StringInterp { parts, span };
        let result = desugar_expr(expr);
        // The outer StringInterp is preserved, but sub-expression should be desugared.
        match result {
            Expr::StringInterp { parts, .. } => {
                assert_eq!(parts.len(), 2);
                // The second part should be a desugared Match, not NullCoalesce.
                match &parts[1] {
                    StringPart::Expr(e) => assert!(matches!(e.as_ref(), Expr::Match { .. })),
                    _ => panic!("expected Expr part"),
                }
            }
            _ => panic!("expected StringInterp"),
        }
    }
}
