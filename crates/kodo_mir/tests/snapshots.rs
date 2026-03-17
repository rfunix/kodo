//! Snapshot tests for MIR lowering output.
//!
//! Uses `insta` to capture MIR debug output for basic functions.
//! Any change to MIR structure will be flagged as a snapshot diff.

use kodo_ast::{
    BinOp, Block, Expr, Function, Module, NodeId, Ownership, Param, Span, Stmt, TypeExpr,
    Visibility,
};
use kodo_mir::lowering::lower_function;

fn span() -> Span {
    Span { start: 0, end: 0 }
}

fn make_fn(name: &str, params: Vec<Param>, body: Block, ret: TypeExpr) -> Function {
    Function {
        id: NodeId(0),
        span: span(),
        name: name.to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params,
        return_type: ret,
        requires: vec![],
        ensures: vec![],
        body,
    }
}

fn make_module(functions: Vec<Function>) -> Module {
    Module {
        test_decls: vec![],
        id: NodeId(0),
        name: "test".to_string(),
        span: span(),
        meta: None,
        imports: vec![],
        functions,
        type_aliases: vec![],
        type_decls: vec![],
        enum_decls: vec![],
        trait_decls: vec![],
        impl_blocks: vec![],
        intent_decls: vec![],
        invariants: vec![],
        actor_decls: vec![],
    }
}

#[test]
fn snapshot_mir_simple_return() {
    let func = make_fn(
        "return_42",
        vec![],
        Block {
            span: span(),
            stmts: vec![Stmt::Return {
                span: span(),
                value: Some(Expr::IntLit(42, span())),
            }],
        },
        TypeExpr::Named("Int".to_string()),
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    insta::assert_debug_snapshot!(mir);
}

#[test]
fn snapshot_mir_if_else() {
    let func = make_fn(
        "abs_value",
        vec![Param {
            name: "x".to_string(),
            ty: TypeExpr::Named("Int".to_string()),
            span: span(),
            ownership: Ownership::Owned,
        }],
        Block {
            span: span(),
            stmts: vec![Stmt::Return {
                span: span(),
                value: Some(Expr::If {
                    condition: Box::new(Expr::BinaryOp {
                        left: Box::new(Expr::Ident("x".to_string(), span())),
                        op: BinOp::Gt,
                        right: Box::new(Expr::IntLit(0, span())),
                        span: span(),
                    }),
                    then_branch: Block {
                        span: span(),
                        stmts: vec![Stmt::Expr(Expr::Ident("x".to_string(), span()))],
                    },
                    else_branch: Some(Block {
                        span: span(),
                        stmts: vec![Stmt::Expr(Expr::BinaryOp {
                            left: Box::new(Expr::IntLit(0, span())),
                            op: BinOp::Sub,
                            right: Box::new(Expr::Ident("x".to_string(), span())),
                            span: span(),
                        })],
                    }),
                    span: span(),
                }),
            }],
        },
        TypeExpr::Named("Int".to_string()),
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    insta::assert_debug_snapshot!(mir);
}

#[test]
fn snapshot_mir_while_loop() {
    let func = make_fn(
        "count_to_ten",
        vec![],
        Block {
            span: span(),
            stmts: vec![
                Stmt::Let {
                    span: span(),
                    mutable: true,
                    name: "i".to_string(),
                    ty: Some(TypeExpr::Named("Int".to_string())),
                    value: Expr::IntLit(0, span()),
                },
                Stmt::While {
                    span: span(),
                    condition: Expr::BinaryOp {
                        left: Box::new(Expr::Ident("i".to_string(), span())),
                        op: BinOp::Lt,
                        right: Box::new(Expr::IntLit(10, span())),
                        span: span(),
                    },
                    body: Block {
                        span: span(),
                        stmts: vec![Stmt::Assign {
                            span: span(),
                            name: "i".to_string(),
                            value: Expr::BinaryOp {
                                left: Box::new(Expr::Ident("i".to_string(), span())),
                                op: BinOp::Add,
                                right: Box::new(Expr::IntLit(1, span())),
                                span: span(),
                            },
                        }],
                    },
                },
                Stmt::Return {
                    span: span(),
                    value: Some(Expr::Ident("i".to_string(), span())),
                },
            ],
        },
        TypeExpr::Named("Int".to_string()),
    );
    let mir = lower_function(&func).unwrap();
    mir.validate().unwrap();
    insta::assert_debug_snapshot!(mir);
}
