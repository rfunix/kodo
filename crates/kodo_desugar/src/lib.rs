//! # `kodo_desugar` — AST Desugaring Pass
//!
//! Transforms syntactic sugar in the AST into simpler forms before
//! MIR lowering. This simplifies all subsequent compiler passes.
//!
//! Currently desugars:
//! - `for i in start..end { body }` into `let mut i = start; while i < end { body; i = i + 1 }`
//! - `for i in start..=end { body }` into `let mut i = start; while i <= end { body; i = i + 1 }`
//! - `expr ?? default` into `match expr { Option::Some(val) => val, Option::None => default }`
//! - `expr?` into `match expr { Result::Ok(val) => val, Result::Err(e) => return Result::Err(e) }`
//! - `expr?.field` into `match expr { Option::Some(val) => Option::Some(val.field), Option::None => Option::None }`

#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![warn(clippy::pedantic)]

use kodo_ast::{BinOp, Block, Expr, MatchArm, Module, Pattern, Stmt, StringPart};

/// Desugars an entire module in-place.
///
/// This function walks all functions in the module and transforms
/// syntactic sugar into simpler AST forms.
pub fn desugar_module(module: &mut Module) {
    for func in &mut module.functions {
        desugar_block(&mut func.body);
    }
    for impl_block in &mut module.impl_blocks {
        for method in &mut impl_block.methods {
            desugar_block(&mut method.body);
        }
    }
    for actor_decl in &mut module.actor_decls {
        for handler in &mut actor_decl.handlers {
            desugar_block(&mut handler.body);
        }
    }
}

/// Desugars a `for` loop into `let mut` + `while` loop.
fn desugar_for_stmt(
    new_stmts: &mut Vec<Stmt>,
    span: kodo_ast::Span,
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
fn desugar_for_in_stmt(
    new_stmts: &mut Vec<Stmt>,
    span: kodo_ast::Span,
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

/// Desugars an `if let` into a `match` expression.
fn desugar_if_let_stmt(
    new_stmts: &mut Vec<Stmt>,
    span: kodo_ast::Span,
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

/// Desugars all statements in a block.
fn desugar_block(block: &mut Block) {
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

/// Desugars a `parallel` block by recursively desugaring inner spawn blocks.
fn desugar_parallel_stmt(new_stmts: &mut Vec<Stmt>, span: kodo_ast::Span, body: Vec<Stmt>) {
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

/// Desugars `expr ?? default` into a match on `Option`.
fn desugar_null_coalesce(left: Expr, right: Expr, span: kodo_ast::Span) -> Expr {
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
fn desugar_try(operand: Expr, span: kodo_ast::Span) -> Expr {
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
fn desugar_optional_chain(object: Expr, field: String, span: kodo_ast::Span) -> Expr {
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
fn desugar_is(operand: Expr, type_name: String, span: kodo_ast::Span) -> Expr {
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

/// Recursively desugars an expression, transforming sugar operators
/// (`??`, `?`, `?.`) into match expressions.
fn desugar_expr(expr: Expr) -> Expr {
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
    use kodo_ast::{NodeIdGen, Span};

    fn make_test_module(stmts: Vec<Stmt>) -> Module {
        let mut id_gen = NodeIdGen::new();
        Module {
            id: id_gen.next_id(),
            span: Span::new(0, 100),
            name: "test".to_string(),
            imports: vec![],
            meta: Some(kodo_ast::Meta {
                id: id_gen.next_id(),
                span: Span::new(0, 50),
                entries: vec![kodo_ast::MetaEntry {
                    key: "purpose".to_string(),
                    value: "test".to_string(),
                    span: Span::new(0, 20),
                }],
            }),
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            functions: vec![kodo_ast::Function {
                id: id_gen.next_id(),
                span: Span::new(0, 100),
                name: "main".to_string(),
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![],
                return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                requires: vec![],
                ensures: vec![],
                body: Block {
                    span: Span::new(0, 100),
                    stmts,
                },
            }],
        }
    }

    #[test]
    fn desugar_for_loop_exclusive() {
        let for_stmt = Stmt::For {
            span: Span::new(0, 50),
            name: "i".to_string(),
            start: Expr::IntLit(0, Span::new(10, 11)),
            end: Expr::IntLit(10, Span::new(14, 16)),
            inclusive: false,
            body: Block {
                span: Span::new(20, 50),
                stmts: vec![],
            },
        };
        let mut module = make_test_module(vec![for_stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 2, "for should desugar into let + while");
        assert!(matches!(stmts[0], Stmt::Let { mutable: true, .. }));
        assert!(matches!(stmts[1], Stmt::While { .. }));

        // Check the while body has the increment
        if let Stmt::While { body, .. } = &stmts[1] {
            assert_eq!(body.stmts.len(), 1, "while body should have increment");
            assert!(matches!(body.stmts[0], Stmt::Assign { .. }));
        }
    }

    #[test]
    fn desugar_for_loop_inclusive() {
        let for_stmt = Stmt::For {
            span: Span::new(0, 50),
            name: "i".to_string(),
            start: Expr::IntLit(0, Span::new(10, 11)),
            end: Expr::IntLit(10, Span::new(14, 16)),
            inclusive: true,
            body: Block {
                span: Span::new(20, 50),
                stmts: vec![],
            },
        };
        let mut module = make_test_module(vec![for_stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        // Check the condition uses Le for inclusive
        if let Stmt::While { condition, .. } = &stmts[1] {
            if let Expr::BinaryOp { op, .. } = condition {
                assert_eq!(*op, BinOp::Le);
            } else {
                panic!("expected BinaryOp condition");
            }
        }
    }

    #[test]
    fn desugar_preserves_non_for_stmts() {
        let let_stmt = Stmt::Let {
            span: Span::new(0, 20),
            mutable: false,
            name: "x".to_string(),
            ty: None,
            value: Expr::IntLit(42, Span::new(10, 12)),
        };
        let mut module = make_test_module(vec![let_stmt]);
        desugar_module(&mut module);

        assert_eq!(module.functions[0].body.stmts.len(), 1);
        assert!(matches!(
            module.functions[0].body.stmts[0],
            Stmt::Let { .. }
        ));
    }

    #[test]
    fn desugar_idempotent() {
        let for_stmt = Stmt::For {
            span: Span::new(0, 50),
            name: "i".to_string(),
            start: Expr::IntLit(0, Span::new(10, 11)),
            end: Expr::IntLit(10, Span::new(14, 16)),
            inclusive: false,
            body: Block {
                span: Span::new(20, 50),
                stmts: vec![],
            },
        };
        let mut module = make_test_module(vec![for_stmt]);
        desugar_module(&mut module);
        let count_after_first = module.functions[0].body.stmts.len();
        desugar_module(&mut module);
        let count_after_second = module.functions[0].body.stmts.len();
        assert_eq!(
            count_after_first, count_after_second,
            "desugaring should be idempotent"
        );
    }

    #[test]
    fn desugar_nested_for_in_while() {
        let inner_for = Stmt::For {
            span: Span::new(30, 45),
            name: "j".to_string(),
            start: Expr::IntLit(0, Span::new(35, 36)),
            end: Expr::IntLit(5, Span::new(39, 40)),
            inclusive: false,
            body: Block {
                span: Span::new(41, 45),
                stmts: vec![],
            },
        };
        let while_stmt = Stmt::While {
            span: Span::new(0, 50),
            condition: Expr::BoolLit(true, Span::new(6, 10)),
            body: Block {
                span: Span::new(12, 50),
                stmts: vec![inner_for],
            },
        };
        let mut module = make_test_module(vec![while_stmt]);
        desugar_module(&mut module);

        // The while should still be there
        assert_eq!(module.functions[0].body.stmts.len(), 1);
        if let Stmt::While { body, .. } = &module.functions[0].body.stmts[0] {
            // The inner for should have been desugared into let + while
            assert_eq!(body.stmts.len(), 2);
            assert!(matches!(body.stmts[0], Stmt::Let { mutable: true, .. }));
            assert!(matches!(body.stmts[1], Stmt::While { .. }));
        } else {
            panic!("expected While statement");
        }
    }

    #[test]
    fn desugar_null_coalesce() {
        let span = Span::new(0, 20);
        let coalesce = Expr::NullCoalesce {
            left: Box::new(Expr::Ident("opt".to_string(), span)),
            right: Box::new(Expr::IntLit(0, span)),
            span,
        };
        let stmt = Stmt::Expr(coalesce);
        let mut module = make_test_module(vec![stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Expr(Expr::Match { arms, .. }) = &stmts[0] {
            assert_eq!(
                arms.len(),
                2,
                "coalesce should desugar into match with 2 arms"
            );
            // First arm: Option::Some(__coalesce_val) => __coalesce_val
            assert!(matches!(
                &arms[0].pattern,
                Pattern::Variant {
                    variant, ..
                } if variant == "Some"
            ));
            // Second arm: Option::None => default
            assert!(matches!(
                &arms[1].pattern,
                Pattern::Variant {
                    variant, ..
                } if variant == "None"
            ));
        } else {
            panic!("expected Match expression");
        }
    }

    #[test]
    fn desugar_try_operator() {
        let span = Span::new(0, 10);
        let try_expr = Expr::Try {
            operand: Box::new(Expr::Ident("result".to_string(), span)),
            span,
        };
        let stmt = Stmt::Expr(try_expr);
        let mut module = make_test_module(vec![stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Expr(Expr::Match { arms, .. }) = &stmts[0] {
            assert_eq!(arms.len(), 2, "try should desugar into match with 2 arms");
            // First arm: Result::Ok(__try_val) => __try_val
            assert!(matches!(
                &arms[0].pattern,
                Pattern::Variant {
                    variant, ..
                } if variant == "Ok"
            ));
            // Second arm: Result::Err(__try_err) => return Result::Err(__try_err)
            assert!(matches!(
                &arms[1].pattern,
                Pattern::Variant {
                    variant, ..
                } if variant == "Err"
            ));
        } else {
            panic!("expected Match expression");
        }
    }

    #[test]
    fn desugar_optional_chain() {
        let span = Span::new(0, 15);
        let chain = Expr::OptionalChain {
            object: Box::new(Expr::Ident("opt_point".to_string(), span)),
            field: "x".to_string(),
            span,
        };
        let stmt = Stmt::Expr(chain);
        let mut module = make_test_module(vec![stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Expr(Expr::Match { arms, .. }) = &stmts[0] {
            assert_eq!(
                arms.len(),
                2,
                "optional chain should desugar into match with 2 arms"
            );
            // First arm: Option::Some(__chain_val) => Option::Some(__chain_val.x)
            assert!(matches!(
                &arms[0].pattern,
                Pattern::Variant {
                    variant, ..
                } if variant == "Some"
            ));
            if let Expr::EnumVariantExpr { variant, args, .. } = &arms[0].body {
                assert_eq!(variant, "Some");
                assert_eq!(args.len(), 1);
                assert!(matches!(&args[0], Expr::FieldAccess { field, .. } if field == "x"));
            } else {
                panic!("expected EnumVariantExpr in Some arm body");
            }
            // Second arm: Option::None => Option::None
            assert!(matches!(
                &arms[1].pattern,
                Pattern::Variant {
                    variant, ..
                } if variant == "None"
            ));
        } else {
            panic!("expected Match expression");
        }
    }

    #[test]
    fn desugar_if_let_to_match() {
        let span = Span::new(0, 50);
        let if_let_stmt = Stmt::IfLet {
            span,
            pattern: Pattern::Variant {
                enum_name: Some("Option".to_string()),
                variant: "Some".to_string(),
                bindings: vec!["v".to_string()],
                span,
            },
            value: Expr::Ident("opt".to_string(), span),
            body: Block {
                span,
                stmts: vec![Stmt::Return {
                    span,
                    value: Some(Expr::Ident("v".to_string(), span)),
                }],
            },
            else_body: Some(Block {
                span,
                stmts: vec![Stmt::Return {
                    span,
                    value: Some(Expr::IntLit(0, span)),
                }],
            }),
        };
        let mut module = make_test_module(vec![if_let_stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(
            stmts.len(),
            1,
            "if let should desugar into one match expression statement"
        );
        if let Stmt::Expr(Expr::Match { arms, .. }) = &stmts[0] {
            assert_eq!(arms.len(), 2, "match should have 2 arms");
            // First arm: Option::Some(v) => { body }
            assert!(matches!(
                &arms[0].pattern,
                Pattern::Variant { variant, .. } if variant == "Some"
            ));
            // Second arm: _ => { else_body }
            assert!(matches!(&arms[1].pattern, Pattern::Wildcard(_)));
        } else {
            panic!("expected Match expression, got {:?}", stmts[0]);
        }
    }

    #[test]
    fn desugar_is_expression() {
        let span = Span::new(0, 20);
        let is_expr = Expr::Is {
            operand: Box::new(Expr::Ident("opt".to_string(), span)),
            type_name: "Some".to_string(),
            span,
        };
        let stmt = Stmt::Expr(is_expr);
        let mut module = make_test_module(vec![stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Expr(Expr::Match { arms, .. }) = &stmts[0] {
            assert_eq!(arms.len(), 2, "is should desugar into match with 2 arms");
            // First arm: Some => true
            assert!(matches!(
                &arms[0].pattern,
                Pattern::Variant { variant, .. } if variant == "Some"
            ));
            assert!(matches!(&arms[0].body, Expr::BoolLit(true, _)));
            // Second arm: _ => false
            assert!(matches!(&arms[1].pattern, Pattern::Wildcard(_)));
            assert!(matches!(&arms[1].body, Expr::BoolLit(false, _)));
        } else {
            panic!("expected Match expression");
        }
    }

    #[test]
    fn desugar_for_loop_inclusive_uses_le() {
        let for_stmt = Stmt::For {
            span: Span::new(0, 50),
            name: "i".to_string(),
            start: Expr::IntLit(0, Span::new(10, 11)),
            end: Expr::IntLit(10, Span::new(14, 16)),
            inclusive: true,
            body: Block {
                span: Span::new(20, 50),
                stmts: vec![Stmt::Expr(Expr::Ident("i".to_string(), Span::new(25, 26)))],
            },
        };
        let mut module = make_test_module(vec![for_stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(
            stmts.len(),
            2,
            "inclusive for should desugar into let + while"
        );

        // Verify the let binds the variable with mutable
        if let Stmt::Let { name, mutable, .. } = &stmts[0] {
            assert_eq!(name, "i");
            assert!(*mutable, "loop variable should be mutable");
        } else {
            panic!("expected Let statement");
        }

        // Verify the while condition uses Le (<=)
        if let Stmt::While {
            condition, body, ..
        } = &stmts[1]
        {
            if let Expr::BinaryOp { op, .. } = condition {
                assert_eq!(*op, BinOp::Le, "inclusive range should use <= operator");
            } else {
                panic!("expected BinaryOp condition");
            }
            // Body should have the original stmt + increment = 2 stmts
            assert_eq!(
                body.stmts.len(),
                2,
                "body should have original stmt + increment"
            );
        } else {
            panic!("expected While statement");
        }
    }

    #[test]
    fn desugar_for_loop_exclusive_uses_lt() {
        let for_stmt = Stmt::For {
            span: Span::new(0, 50),
            name: "i".to_string(),
            start: Expr::IntLit(0, Span::new(10, 11)),
            end: Expr::IntLit(10, Span::new(14, 16)),
            inclusive: false,
            body: Block {
                span: Span::new(20, 50),
                stmts: vec![Stmt::Expr(Expr::Ident("i".to_string(), Span::new(25, 26)))],
            },
        };
        let mut module = make_test_module(vec![for_stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        if let Stmt::While { condition, .. } = &stmts[1] {
            if let Expr::BinaryOp { op, .. } = condition {
                assert_eq!(*op, BinOp::Lt, "exclusive range should use < operator");
            } else {
                panic!("expected BinaryOp condition");
            }
        } else {
            panic!("expected While statement");
        }
    }

    #[test]
    fn desugar_nested_for_loops() {
        let inner_for = Stmt::For {
            span: Span::new(30, 50),
            name: "j".to_string(),
            start: Expr::IntLit(0, Span::new(35, 36)),
            end: Expr::IntLit(3, Span::new(39, 40)),
            inclusive: false,
            body: Block {
                span: Span::new(41, 49),
                stmts: vec![],
            },
        };
        let outer_for = Stmt::For {
            span: Span::new(0, 55),
            name: "i".to_string(),
            start: Expr::IntLit(0, Span::new(10, 11)),
            end: Expr::IntLit(5, Span::new(14, 15)),
            inclusive: false,
            body: Block {
                span: Span::new(20, 55),
                stmts: vec![inner_for],
            },
        };
        let mut module = make_test_module(vec![outer_for]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        // Outer for desugars to let + while
        assert_eq!(stmts.len(), 2);
        assert!(matches!(stmts[0], Stmt::Let { .. }));

        // The outer while body should contain the desugared inner for (let + while) + outer increment
        if let Stmt::While { body, .. } = &stmts[1] {
            // Inner for => let + while, plus outer increment => 3 stmts
            assert_eq!(
                body.stmts.len(),
                3,
                "outer body should have inner let + inner while + outer increment"
            );
            assert!(matches!(body.stmts[0], Stmt::Let { .. }), "inner let");
            assert!(matches!(body.stmts[1], Stmt::While { .. }), "inner while");
            assert!(
                matches!(body.stmts[2], Stmt::Assign { .. }),
                "outer increment"
            );
        } else {
            panic!("expected While statement");
        }
    }

    #[test]
    fn desugar_null_coalesce_chain() {
        let span = Span::new(0, 30);
        // a ?? b ?? c => (a ?? b) ?? c => nested coalescing
        let inner = Expr::NullCoalesce {
            left: Box::new(Expr::Ident("a".to_string(), span)),
            right: Box::new(Expr::Ident("b".to_string(), span)),
            span,
        };
        let outer = Expr::NullCoalesce {
            left: Box::new(inner),
            right: Box::new(Expr::Ident("c".to_string(), span)),
            span,
        };
        let stmt = Stmt::Expr(outer);
        let mut module = make_test_module(vec![stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        // Outer match
        if let Stmt::Expr(Expr::Match { arms, expr, .. }) = &stmts[0] {
            assert_eq!(arms.len(), 2);
            // The matched expression should itself be a Match (inner coalesce desugared)
            assert!(
                matches!(expr.as_ref(), Expr::Match { .. }),
                "inner coalesce should also be desugared to Match"
            );
            // None arm should have the fallback "c"
            if let Expr::Ident(name, _) = &arms[1].body {
                assert_eq!(name, "c", "outer fallback should be c");
            } else {
                panic!("expected Ident 'c' in None arm");
            }
        } else {
            panic!("expected Match expression");
        }
    }

    #[test]
    fn desugar_optional_chain_nested() {
        let span = Span::new(0, 25);
        // Nested optional chain: (obj?.field1)?.field2
        let inner_chain = Expr::OptionalChain {
            object: Box::new(Expr::Ident("obj".to_string(), span)),
            field: "field1".to_string(),
            span,
        };
        let outer_chain = Expr::OptionalChain {
            object: Box::new(inner_chain),
            field: "field2".to_string(),
            span,
        };
        let stmt = Stmt::Expr(outer_chain);
        let mut module = make_test_module(vec![stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        // Outer match
        if let Stmt::Expr(Expr::Match { expr, arms, .. }) = &stmts[0] {
            assert_eq!(arms.len(), 2);
            // The matched expression should be the inner chain's desugared Match
            assert!(
                matches!(expr.as_ref(), Expr::Match { .. }),
                "inner optional chain should be desugared to Match"
            );
            // Some arm should wrap field access in Option::Some
            if let Expr::EnumVariantExpr { variant, args, .. } = &arms[0].body {
                assert_eq!(variant, "Some");
                assert_eq!(args.len(), 1);
                assert!(matches!(&args[0], Expr::FieldAccess { field, .. } if field == "field2"));
            } else {
                panic!("expected EnumVariantExpr in Some arm");
            }
        } else {
            panic!("expected Match expression");
        }
    }

    #[test]
    fn desugar_empty_block_unchanged() {
        // An empty function body should pass through unchanged
        let mut module = make_test_module(vec![]);
        desugar_module(&mut module);
        assert!(
            module.functions[0].body.stmts.is_empty(),
            "empty block should remain empty after desugaring"
        );
    }

    #[test]
    fn desugar_mixed_sugar_in_one_function() {
        let span = Span::new(0, 80);
        // for loop + null coalesce in the same function body
        let for_stmt = Stmt::For {
            span,
            name: "i".to_string(),
            start: Expr::IntLit(0, span),
            end: Expr::IntLit(5, span),
            inclusive: false,
            body: Block {
                span,
                stmts: vec![],
            },
        };
        let coalesce_stmt = Stmt::Expr(Expr::NullCoalesce {
            left: Box::new(Expr::Ident("x".to_string(), span)),
            right: Box::new(Expr::IntLit(0, span)),
            span,
        });
        let mut module = make_test_module(vec![for_stmt, coalesce_stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        // for => let + while (2 stmts), coalesce => match (1 stmt) = 3 total
        assert_eq!(stmts.len(), 3, "should have let + while + match");
        assert!(matches!(stmts[0], Stmt::Let { .. }));
        assert!(matches!(stmts[1], Stmt::While { .. }));
        assert!(matches!(stmts[2], Stmt::Expr(Expr::Match { .. })));
    }

    #[test]
    fn desugar_for_loop_variable_names_distinct() {
        let span = Span::new(0, 80);
        // Two for loops with different variable names should produce distinct lets
        let for1 = Stmt::For {
            span,
            name: "i".to_string(),
            start: Expr::IntLit(0, span),
            end: Expr::IntLit(5, span),
            inclusive: false,
            body: Block {
                span,
                stmts: vec![],
            },
        };
        let for2 = Stmt::For {
            span,
            name: "j".to_string(),
            start: Expr::IntLit(0, span),
            end: Expr::IntLit(3, span),
            inclusive: false,
            body: Block {
                span,
                stmts: vec![],
            },
        };
        let mut module = make_test_module(vec![for1, for2]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        // Each for => let + while = 4 stmts total
        assert_eq!(stmts.len(), 4);
        // First let should bind "i", second let should bind "j"
        if let Stmt::Let { name, .. } = &stmts[0] {
            assert_eq!(name, "i");
        } else {
            panic!("expected Let for i");
        }
        if let Stmt::Let { name, .. } = &stmts[2] {
            assert_eq!(name, "j");
        } else {
            panic!("expected Let for j");
        }
    }

    #[test]
    fn desugar_module_with_multiple_functions() {
        let span = Span::new(0, 100);
        let mut id_gen = NodeIdGen::new();
        let mut module = Module {
            id: id_gen.next_id(),
            span,
            name: "test".to_string(),
            imports: vec![],
            meta: Some(kodo_ast::Meta {
                id: id_gen.next_id(),
                span: Span::new(0, 50),
                entries: vec![kodo_ast::MetaEntry {
                    key: "purpose".to_string(),
                    value: "test".to_string(),
                    span: Span::new(0, 20),
                }],
            }),
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            functions: vec![
                kodo_ast::Function {
                    id: id_gen.next_id(),
                    span,
                    name: "func_a".to_string(),
                    is_async: false,
                    generic_params: vec![],
                    annotations: vec![],
                    params: vec![],
                    return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                    requires: vec![],
                    ensures: vec![],
                    body: Block {
                        span,
                        stmts: vec![Stmt::For {
                            span,
                            name: "i".to_string(),
                            start: Expr::IntLit(0, span),
                            end: Expr::IntLit(3, span),
                            inclusive: false,
                            body: Block {
                                span,
                                stmts: vec![],
                            },
                        }],
                    },
                },
                kodo_ast::Function {
                    id: id_gen.next_id(),
                    span,
                    name: "func_b".to_string(),
                    is_async: false,
                    generic_params: vec![],
                    annotations: vec![],
                    params: vec![],
                    return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
                    requires: vec![],
                    ensures: vec![],
                    body: Block {
                        span,
                        stmts: vec![Stmt::Expr(Expr::NullCoalesce {
                            left: Box::new(Expr::Ident("x".to_string(), span)),
                            right: Box::new(Expr::IntLit(0, span)),
                            span,
                        })],
                    },
                },
            ],
        };

        desugar_module(&mut module);

        // func_a: for loop => let + while
        assert_eq!(
            module.functions[0].body.stmts.len(),
            2,
            "func_a should have desugared for loop"
        );
        assert!(matches!(
            module.functions[0].body.stmts[0],
            Stmt::Let { .. }
        ));
        assert!(matches!(
            module.functions[0].body.stmts[1],
            Stmt::While { .. }
        ));

        // func_b: null coalesce => match
        assert_eq!(
            module.functions[1].body.stmts.len(),
            1,
            "func_b should have desugared coalesce"
        );
        assert!(matches!(
            module.functions[1].body.stmts[0],
            Stmt::Expr(Expr::Match { .. })
        ));
    }

    #[test]
    fn desugar_for_loop_body_with_early_return() {
        let span = Span::new(0, 60);
        let for_stmt = Stmt::For {
            span,
            name: "i".to_string(),
            start: Expr::IntLit(0, span),
            end: Expr::IntLit(10, span),
            inclusive: false,
            body: Block {
                span,
                stmts: vec![Stmt::Return {
                    span,
                    value: Some(Expr::Ident("i".to_string(), span)),
                }],
            },
        };
        let mut module = make_test_module(vec![for_stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 2);

        if let Stmt::While { body, .. } = &stmts[1] {
            // Body should have: return stmt + increment = 2 stmts
            assert_eq!(body.stmts.len(), 2);
            // First stmt is the return
            assert!(
                matches!(&body.stmts[0], Stmt::Return { value: Some(_), .. }),
                "return statement should be preserved in loop body"
            );
            // Second stmt is the increment
            assert!(
                matches!(&body.stmts[1], Stmt::Assign { .. }),
                "increment should still be appended after return"
            );
        } else {
            panic!("expected While statement");
        }
    }

    #[test]
    fn desugar_spawn_body_desugared() {
        let span = Span::new(0, 50);
        let spawn_stmt = Stmt::Spawn {
            span,
            body: Block {
                span: Span::new(5, 45),
                stmts: vec![Stmt::For {
                    span,
                    name: "i".to_string(),
                    start: Expr::IntLit(0, span),
                    end: Expr::IntLit(5, span),
                    inclusive: false,
                    body: Block {
                        span,
                        stmts: vec![],
                    },
                }],
            },
        };
        let mut module = make_test_module(vec![spawn_stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Spawn { body, .. } = &stmts[0] {
            // The for loop inside spawn should be desugared to let + while
            assert_eq!(body.stmts.len(), 2, "for in spawn should desugar");
            assert!(matches!(body.stmts[0], Stmt::Let { mutable: true, .. }));
            assert!(matches!(body.stmts[1], Stmt::While { .. }));
        } else {
            panic!("expected Spawn statement");
        }
    }

    #[test]
    fn desugar_parallel_body_desugared() {
        let span = Span::new(0, 80);
        let parallel_stmt = Stmt::Parallel {
            span,
            body: vec![
                Stmt::Spawn {
                    span,
                    body: Block {
                        span,
                        stmts: vec![Stmt::For {
                            span,
                            name: "i".to_string(),
                            start: Expr::IntLit(0, span),
                            end: Expr::IntLit(3, span),
                            inclusive: false,
                            body: Block {
                                span,
                                stmts: vec![],
                            },
                        }],
                    },
                },
                Stmt::Spawn {
                    span,
                    body: Block {
                        span,
                        stmts: vec![Stmt::Expr(Expr::IntLit(42, span))],
                    },
                },
            ],
        };
        let mut module = make_test_module(vec![parallel_stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Parallel { body, .. } = &stmts[0] {
            assert_eq!(body.len(), 2, "parallel should still have 2 spawn stmts");
            // First spawn's for loop should be desugared
            if let Stmt::Spawn { body, .. } = &body[0] {
                assert_eq!(body.stmts.len(), 2, "for in spawn should desugar");
            } else {
                panic!("expected Spawn in parallel body");
            }
        } else {
            panic!("expected Parallel statement");
        }
    }

    #[test]
    fn desugar_closure_body_desugared() {
        let span = Span::new(0, 30);
        let closure = Expr::Closure {
            params: vec![kodo_ast::ClosureParam {
                name: "x".to_string(),
                ty: None,
                span,
            }],
            return_type: None,
            body: Box::new(Expr::NullCoalesce {
                left: Box::new(Expr::Ident("x".to_string(), span)),
                right: Box::new(Expr::IntLit(0, span)),
                span,
            }),
            span,
        };
        let stmt = Stmt::Expr(closure);
        let mut module = make_test_module(vec![stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Expr(Expr::Closure { body, .. }) = &stmts[0] {
            // The closure body's NullCoalesce should be desugared to Match
            assert!(
                matches!(body.as_ref(), Expr::Match { .. }),
                "closure body coalesce should desugar to match"
            );
        } else {
            panic!("expected Closure expression");
        }
    }

    #[test]
    fn desugar_try_in_let_value() {
        let span = Span::new(0, 30);
        let let_stmt = Stmt::Let {
            span,
            mutable: false,
            name: "val".to_string(),
            ty: None,
            value: Expr::Try {
                operand: Box::new(Expr::Ident("result".to_string(), span)),
                span,
            },
        };
        let mut module = make_test_module(vec![let_stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Let { value, .. } = &stmts[0] {
            assert!(
                matches!(value, Expr::Match { .. }),
                "try in let value should desugar to match"
            );
        } else {
            panic!("expected Let statement");
        }
    }

    #[test]
    fn desugar_null_coalesce_with_complex_default() {
        let span = Span::new(0, 40);
        let coalesce = Expr::NullCoalesce {
            left: Box::new(Expr::Ident("opt".to_string(), span)),
            right: Box::new(Expr::BinaryOp {
                left: Box::new(Expr::IntLit(10, span)),
                op: BinOp::Mul,
                right: Box::new(Expr::IntLit(5, span)),
                span,
            }),
            span,
        };
        let stmt = Stmt::Expr(coalesce);
        let mut module = make_test_module(vec![stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Expr(Expr::Match { arms, .. }) = &stmts[0] {
            // None arm's body should be BinaryOp (10 * 5)
            assert!(
                matches!(&arms[1].body, Expr::BinaryOp { op: BinOp::Mul, .. }),
                "default should be the complex expression"
            );
        } else {
            panic!("expected Match expression");
        }
    }

    #[test]
    fn desugar_assign_with_sugar() {
        let span = Span::new(0, 30);
        let assign_stmt = Stmt::Assign {
            span,
            name: "x".to_string(),
            value: Expr::NullCoalesce {
                left: Box::new(Expr::Ident("opt".to_string(), span)),
                right: Box::new(Expr::IntLit(0, span)),
                span,
            },
        };
        let mut module = make_test_module(vec![assign_stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Assign { value, .. } = &stmts[0] {
            assert!(
                matches!(value, Expr::Match { .. }),
                "coalesce in assignment should desugar to match"
            );
        } else {
            panic!("expected Assign statement");
        }
    }

    #[test]
    fn desugar_return_with_try() {
        let span = Span::new(0, 20);
        let return_stmt = Stmt::Return {
            span,
            value: Some(Expr::Try {
                operand: Box::new(Expr::Ident("res".to_string(), span)),
                span,
            }),
        };
        let mut module = make_test_module(vec![return_stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Return {
            value: Some(val), ..
        } = &stmts[0]
        {
            assert!(
                matches!(val, Expr::Match { .. }),
                "try in return should desugar to match"
            );
        } else {
            panic!("expected Return statement with value");
        }
    }

    #[test]
    fn desugar_binary_op_with_nested_sugar() {
        let span = Span::new(0, 40);
        // (a ?? 0) + (b ?? 1) => both sides should desugar
        let left = Expr::NullCoalesce {
            left: Box::new(Expr::Ident("a".to_string(), span)),
            right: Box::new(Expr::IntLit(0, span)),
            span,
        };
        let right = Expr::NullCoalesce {
            left: Box::new(Expr::Ident("b".to_string(), span)),
            right: Box::new(Expr::IntLit(1, span)),
            span,
        };
        let binop = Expr::BinaryOp {
            left: Box::new(left),
            op: BinOp::Add,
            right: Box::new(right),
            span,
        };
        let stmt = Stmt::Expr(binop);
        let mut module = make_test_module(vec![stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Expr(Expr::BinaryOp { left, right, .. }) = &stmts[0] {
            assert!(
                matches!(left.as_ref(), Expr::Match { .. }),
                "left coalesce should desugar"
            );
            assert!(
                matches!(right.as_ref(), Expr::Match { .. }),
                "right coalesce should desugar"
            );
        } else {
            panic!("expected BinaryOp expression");
        }
    }

    #[test]
    fn desugar_call_with_sugar_args() {
        let span = Span::new(0, 40);
        let call = Expr::Call {
            callee: Box::new(Expr::Ident("foo".to_string(), span)),
            args: vec![
                Expr::NullCoalesce {
                    left: Box::new(Expr::Ident("a".to_string(), span)),
                    right: Box::new(Expr::IntLit(0, span)),
                    span,
                },
                Expr::Try {
                    operand: Box::new(Expr::Ident("b".to_string(), span)),
                    span,
                },
            ],
            span,
        };
        let stmt = Stmt::Expr(call);
        let mut module = make_test_module(vec![stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Expr(Expr::Call { args, .. }) = &stmts[0] {
            assert_eq!(args.len(), 2);
            assert!(
                matches!(&args[0], Expr::Match { .. }),
                "coalesce arg should desugar"
            );
            assert!(
                matches!(&args[1], Expr::Match { .. }),
                "try arg should desugar"
            );
        } else {
            panic!("expected Call expression");
        }
    }

    #[test]
    fn desugar_if_condition_with_sugar() {
        let span = Span::new(0, 50);
        let if_expr = Expr::If {
            condition: Box::new(Expr::Is {
                operand: Box::new(Expr::Ident("opt".to_string(), span)),
                type_name: "Some".to_string(),
                span,
            }),
            then_branch: Block {
                span,
                stmts: vec![Stmt::Expr(Expr::IntLit(1, span))],
            },
            else_branch: Some(Block {
                span,
                stmts: vec![Stmt::Expr(Expr::IntLit(0, span))],
            }),
            span,
        };
        let stmt = Stmt::Expr(if_expr);
        let mut module = make_test_module(vec![stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Expr(Expr::If { condition, .. }) = &stmts[0] {
            assert!(
                matches!(condition.as_ref(), Expr::Match { .. }),
                "is expr in condition should desugar to match"
            );
        } else {
            panic!("expected If expression");
        }
    }

    #[test]
    fn desugar_struct_lit_field_with_sugar() {
        let span = Span::new(0, 40);
        let struct_lit = Expr::StructLit {
            name: "Point".to_string(),
            fields: vec![kodo_ast::FieldInit {
                name: "x".to_string(),
                value: Expr::NullCoalesce {
                    left: Box::new(Expr::Ident("opt_x".to_string(), span)),
                    right: Box::new(Expr::IntLit(0, span)),
                    span,
                },
                span,
            }],
            span,
        };
        let stmt = Stmt::Expr(struct_lit);
        let mut module = make_test_module(vec![stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Expr(Expr::StructLit { fields, .. }) = &stmts[0] {
            assert!(
                matches!(&fields[0].value, Expr::Match { .. }),
                "field value coalesce should desugar"
            );
        } else {
            panic!("expected StructLit expression");
        }
    }

    #[test]
    fn desugar_while_condition_with_sugar() {
        let span = Span::new(0, 50);
        let while_stmt = Stmt::While {
            span,
            condition: Expr::Is {
                operand: Box::new(Expr::Ident("state".to_string(), span)),
                type_name: "Running".to_string(),
                span,
            },
            body: Block {
                span,
                stmts: vec![],
            },
        };
        let mut module = make_test_module(vec![while_stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::While { condition, .. } = &stmts[0] {
            assert!(
                matches!(condition, Expr::Match { .. }),
                "is expr in while condition should desugar"
            );
        } else {
            panic!("expected While statement");
        }
    }

    #[test]
    fn desugar_match_arm_body_with_sugar() {
        let span = Span::new(0, 50);
        let match_expr = Expr::Match {
            expr: Box::new(Expr::Ident("val".to_string(), span)),
            arms: vec![MatchArm {
                pattern: Pattern::Wildcard(span),
                body: Expr::NullCoalesce {
                    left: Box::new(Expr::Ident("opt".to_string(), span)),
                    right: Box::new(Expr::IntLit(0, span)),
                    span,
                },
                span,
            }],
            span,
        };
        let stmt = Stmt::Expr(match_expr);
        let mut module = make_test_module(vec![stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Expr(Expr::Match { arms, .. }) = &stmts[0] {
            assert!(
                matches!(&arms[0].body, Expr::Match { .. }),
                "coalesce in match arm body should desugar"
            );
        } else {
            panic!("expected Match expression");
        }
    }

    #[test]
    fn desugar_unary_op_with_sugar() {
        let span = Span::new(0, 20);
        let unary = Expr::UnaryOp {
            op: kodo_ast::UnaryOp::Not,
            operand: Box::new(Expr::Is {
                operand: Box::new(Expr::Ident("opt".to_string(), span)),
                type_name: "None".to_string(),
                span,
            }),
            span,
        };
        let stmt = Stmt::Expr(unary);
        let mut module = make_test_module(vec![stmt]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        assert_eq!(stmts.len(), 1);
        if let Stmt::Expr(Expr::UnaryOp { operand, .. }) = &stmts[0] {
            assert!(
                matches!(operand.as_ref(), Expr::Match { .. }),
                "is expr in unary operand should desugar"
            );
        } else {
            panic!("expected UnaryOp expression");
        }
    }

    #[test]
    fn desugar_string_interp_literal_only() {
        let span = Span::new(0, 10);
        let parts = vec![StringPart::Literal("hello".to_string())];
        let result = super::desugar_string_interp(parts, span);
        assert!(
            matches!(result, Expr::StringLit(ref s, _) if s == "hello"),
            "single literal part should produce StringLit, got {result:?}"
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
        let result = super::desugar_string_interp(parts, span);
        // Should be: ("hello " + name) + "!"
        assert!(matches!(result, Expr::BinaryOp { op: BinOp::Add, .. }));
    }

    #[test]
    fn desugar_string_interp_single_expr() {
        let span = Span::new(0, 10);
        let parts = vec![StringPart::Expr(Box::new(Expr::IntLit(42, span)))];
        let result = super::desugar_string_interp(parts, span);
        // Single expr part should produce the expression directly
        assert!(
            matches!(result, Expr::IntLit(42, _)),
            "single expr part should produce the expression directly, got {result:?}"
        );
    }

    #[test]
    fn desugar_string_interp_in_module() {
        let span = Span::new(0, 30);
        let mut id_gen = NodeIdGen::new();
        let mut module = Module {
            id: id_gen.next_id(),
            span,
            name: "test".to_string(),
            imports: vec![],
            meta: None,
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            functions: vec![kodo_ast::Function {
                id: id_gen.next_id(),
                span,
                name: "main".to_string(),
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![],
                return_type: kodo_ast::TypeExpr::Named("String".to_string()),
                requires: vec![],
                ensures: vec![],
                body: Block {
                    span,
                    stmts: vec![Stmt::Expr(Expr::StringInterp {
                        parts: vec![
                            StringPart::Literal("count: ".to_string()),
                            StringPart::Expr(Box::new(Expr::IntLit(42, span))),
                        ],
                        span,
                    })],
                },
            }],
        };
        desugar_module(&mut module);
        let body = &module.functions[0].body.stmts;
        assert_eq!(body.len(), 1);
        // After desugaring, the StringInterp should become a BinaryOp chain
        assert!(
            matches!(&body[0], Stmt::Expr(Expr::BinaryOp { .. })),
            "StringInterp should be desugared to BinaryOp, got {:?}",
            body[0]
        );
    }

    #[test]
    fn desugar_for_in_produces_while_loop() {
        let for_in = Stmt::ForIn {
            span: Span::new(0, 50),
            name: "x".to_string(),
            iterable: Expr::Ident("items".to_string(), Span::new(10, 15)),
            body: Block {
                span: Span::new(16, 50),
                stmts: vec![Stmt::Expr(Expr::Ident("x".to_string(), Span::new(20, 21)))],
            },
        };

        let mut block = Block {
            span: Span::new(0, 50),
            stmts: vec![for_in],
        };
        desugar_block(&mut block);

        // Should produce: let __forin_idx_x = 0, let __forin_len_x = len(items), while ...
        assert_eq!(block.stmts.len(), 3);
        assert!(matches!(&block.stmts[0], Stmt::Let { name, .. } if name == "__forin_idx_x"));
        assert!(matches!(&block.stmts[1], Stmt::Let { name, .. } if name == "__forin_len_x"));
        assert!(matches!(&block.stmts[2], Stmt::While { .. }));
    }

    #[test]
    fn desugar_for_in_while_body_has_let_and_increment() {
        let for_in = Stmt::ForIn {
            span: Span::new(0, 50),
            name: "item".to_string(),
            iterable: Expr::Ident("data".to_string(), Span::new(10, 14)),
            body: Block {
                span: Span::new(15, 50),
                stmts: vec![Stmt::Expr(Expr::IntLit(42, Span::new(20, 22)))],
            },
        };

        let mut block = Block {
            span: Span::new(0, 50),
            stmts: vec![for_in],
        };
        desugar_block(&mut block);

        if let Stmt::While { body, .. } = &block.stmts[2] {
            // First stmt: let item = get(data, __forin_idx_item)
            assert!(matches!(&body.stmts[0], Stmt::Let { name, .. } if name == "item"));
            // Middle: original body
            assert!(matches!(&body.stmts[1], Stmt::Expr(Expr::IntLit(42, _))));
            // Last: __forin_idx_item = __forin_idx_item + 1
            assert!(
                matches!(&body.stmts[2], Stmt::Assign { name, .. } if name == "__forin_idx_item")
            );
        } else {
            panic!("expected While statement");
        }
    }

    #[test]
    fn desugar_for_in_len_call() {
        let for_in = Stmt::ForIn {
            span: Span::new(0, 50),
            name: "x".to_string(),
            iterable: Expr::Ident("list".to_string(), Span::new(10, 14)),
            body: Block {
                span: Span::new(15, 50),
                stmts: vec![],
            },
        };

        let mut block = Block {
            span: Span::new(0, 50),
            stmts: vec![for_in],
        };
        desugar_block(&mut block);

        // Check the len call
        if let Stmt::Let { value, .. } = &block.stmts[1] {
            if let Expr::Call { callee, args, .. } = value {
                assert!(matches!(callee.as_ref(), Expr::Ident(n, _) if n == "len"));
                assert_eq!(args.len(), 1);
            } else {
                panic!("expected Call expression for len");
            }
        } else {
            panic!("expected Let statement for len");
        }
    }

    #[test]
    fn desugar_for_in_get_call() {
        let for_in = Stmt::ForIn {
            span: Span::new(0, 50),
            name: "x".to_string(),
            iterable: Expr::Ident("list".to_string(), Span::new(10, 14)),
            body: Block {
                span: Span::new(15, 50),
                stmts: vec![],
            },
        };

        let mut block = Block {
            span: Span::new(0, 50),
            stmts: vec![for_in],
        };
        desugar_block(&mut block);

        if let Stmt::While { body, .. } = &block.stmts[2] {
            if let Stmt::Let { value, .. } = &body.stmts[0] {
                if let Expr::Call { callee, args, .. } = value {
                    assert!(matches!(callee.as_ref(), Expr::Ident(n, _) if n == "get"));
                    assert_eq!(args.len(), 2);
                } else {
                    panic!("expected Call expression for get");
                }
            } else {
                panic!("expected Let statement for element binding");
            }
        } else {
            panic!("expected While statement");
        }
    }

    #[test]
    fn desugar_for_in_nested_in_function() {
        let mut module = Module {
            id: kodo_ast::NodeId(0),
            span: Span::new(0, 100),
            name: "test".to_string(),
            imports: vec![],
            meta: None,
            type_aliases: vec![],
            type_decls: vec![],
            enum_decls: vec![],
            trait_decls: vec![],
            impl_blocks: vec![],
            actor_decls: vec![],
            intent_decls: vec![],
            functions: vec![kodo_ast::Function {
                id: kodo_ast::NodeId(1),
                span: Span::new(0, 100),
                name: "main".to_string(),
                is_async: false,
                generic_params: vec![],
                annotations: vec![],
                params: vec![],
                return_type: kodo_ast::TypeExpr::Unit,
                requires: vec![],
                ensures: vec![],
                body: Block {
                    span: Span::new(0, 100),
                    stmts: vec![Stmt::ForIn {
                        span: Span::new(5, 90),
                        name: "x".to_string(),
                        iterable: Expr::Ident("list".to_string(), Span::new(10, 14)),
                        body: Block {
                            span: Span::new(15, 90),
                            stmts: vec![],
                        },
                    }],
                },
            }],
        };

        desugar_module(&mut module);

        // After desugaring, the function body should have the while loop pattern
        assert_eq!(module.functions[0].body.stmts.len(), 3);
        assert!(matches!(
            &module.functions[0].body.stmts[2],
            Stmt::While { .. }
        ));
    }
}
