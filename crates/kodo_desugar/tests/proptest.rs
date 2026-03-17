//! Property-based tests for the Kodo desugaring pass.

use kodo_ast::{
    BinOp, Block, Expr, Function, Meta, MetaEntry, Module, NodeId, NodeIdGen, Span, Stmt, TypeExpr,
    Visibility,
};
use kodo_desugar::desugar_module;
use proptest::prelude::*;

fn make_test_module(functions: Vec<Function>) -> Module {
    let mut id_gen = NodeIdGen::new();
    Module {
        test_decls: vec![],
        id: id_gen.next_id(),
        span: Span::new(0, 100),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: id_gen.next_id(),
            span: Span::new(0, 50),
            entries: vec![MetaEntry {
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
        invariants: vec![],
        functions,
    }
}

fn make_function(name: &str, stmts: Vec<Stmt>) -> Function {
    Function {
        id: NodeId(42),
        span: Span::new(0, 100),
        name: name.to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![],
        return_type: TypeExpr::Named("Int".to_string()),
        requires: vec![],
        ensures: vec![],
        body: Block {
            span: Span::new(0, 100),
            stmts,
        },
    }
}

/// Strategy for generating simple valid statements.
fn simple_stmt_strategy() -> impl Strategy<Value = Stmt> {
    prop_oneof![
        // Let binding
        (-1000i64..1000).prop_map(|n| Stmt::Let {
            span: Span::new(0, 20),
            mutable: false,
            name: "x".to_string(),
            ty: None,
            value: Expr::IntLit(n, Span::new(5, 10)),
        }),
        // Return statement
        (-1000i64..1000).prop_map(|n| Stmt::Return {
            span: Span::new(0, 15),
            value: Some(Expr::IntLit(n, Span::new(7, 10))),
        }),
        // Expression statement
        (-1000i64..1000).prop_map(|n| Stmt::Expr(Expr::IntLit(n, Span::new(0, 5)))),
    ]
}

proptest! {
    /// Desugar never panics on valid AST with simple statements.
    #[test]
    fn desugar_never_panics_on_simple_stmts(
        stmts in prop::collection::vec(simple_stmt_strategy(), 0..10)
    ) {
        let func = make_function("test_fn", stmts);
        let mut module = make_test_module(vec![func]);
        // Must not panic.
        desugar_module(&mut module);
    }

    /// Desugar preserves function count — desugaring does not add or remove functions.
    #[test]
    fn desugar_preserves_function_count(count in 1usize..5) {
        let functions: Vec<Function> = (0..count)
            .map(|i| make_function(
                &format!("fn_{i}"),
                vec![Stmt::Return {
                    span: Span::new(0, 10),
                    value: Some(Expr::IntLit(i as i64, Span::new(0, 5))),
                }],
            ))
            .collect();
        let mut module = make_test_module(functions);
        desugar_module(&mut module);
        prop_assert_eq!(module.functions.len(), count);
    }

    /// For-loop desugaring always produces a let + while pair.
    #[test]
    fn for_loop_desugars_to_let_and_while(
        start in 0i64..100,
        end in 0i64..100,
        inclusive in prop::bool::ANY
    ) {
        let for_stmt = Stmt::For {
            span: Span::new(0, 50),
            name: "i".to_string(),
            start: Expr::IntLit(start, Span::new(10, 15)),
            end: Expr::IntLit(end, Span::new(20, 25)),
            inclusive,
            body: Block {
                span: Span::new(30, 50),
                stmts: vec![],
            },
        };
        let func = make_function("test_fn", vec![for_stmt]);
        let mut module = make_test_module(vec![func]);
        desugar_module(&mut module);

        let stmts = &module.functions[0].body.stmts;
        prop_assert_eq!(stmts.len(), 2, "for should desugar into let + while");
        let is_let = matches!(stmts[0], Stmt::Let { mutable: true, .. });
        prop_assert!(is_let, "expected Let statement");
        let is_while = matches!(stmts[1], Stmt::While { .. });
        prop_assert!(is_while, "expected While statement");

        // Check inclusive vs exclusive operator.
        if let Stmt::While { condition, .. } = &stmts[1] {
            if let Expr::BinaryOp { op, .. } = condition {
                if inclusive {
                    prop_assert_eq!(*op, BinOp::Le);
                } else {
                    prop_assert_eq!(*op, BinOp::Lt);
                }
            }
        }
    }

    /// Desugaring is idempotent — applying it twice produces the same result.
    #[test]
    fn desugar_is_idempotent(
        n in -100i64..100
    ) {
        let for_stmt = Stmt::For {
            span: Span::new(0, 50),
            name: "i".to_string(),
            start: Expr::IntLit(0, Span::new(10, 11)),
            end: Expr::IntLit(n.abs(), Span::new(14, 16)),
            inclusive: false,
            body: Block {
                span: Span::new(20, 50),
                stmts: vec![],
            },
        };
        let func = make_function("test_fn", vec![for_stmt]);
        let mut module = make_test_module(vec![func]);
        desugar_module(&mut module);
        let count_after_first = module.functions[0].body.stmts.len();
        desugar_module(&mut module);
        let count_after_second = module.functions[0].body.stmts.len();
        prop_assert_eq!(count_after_first, count_after_second);
    }
}
