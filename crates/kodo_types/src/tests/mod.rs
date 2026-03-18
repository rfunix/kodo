//! Tests for the Kodo type checker, organized by feature area.

use super::*;
use kodo_ast::{
    Annotation, AnnotationArg, BinOp, Block, Expr, Function, Meta, MetaEntry, Module, NodeId,
    Param, Span, Stmt, TypeExpr, UnaryOp, Visibility,
};

mod annotations;
mod basics;
mod borrow_checking;
mod control_flow;
mod generics;
mod invariants;
mod iterators;
mod methods;
mod ownership;
mod traits;
mod tuples;
mod visibility;

#[test]
fn type_display() {
    assert_eq!(Type::Int.to_string(), "Int");
    assert_eq!(Type::Unit.to_string(), "()");
    assert_eq!(
        Type::Function(vec![Type::Int, Type::Int], Box::new(Type::Bool)).to_string(),
        "(Int, Int) -> Bool"
    );
    assert_eq!(
        Type::Generic("List".to_string(), vec![Type::Int]).to_string(),
        "List<Int>"
    );
}

#[test]
fn type_env_lookup() {
    let mut env = TypeEnv::new();
    env.insert("x".to_string(), Type::Int);
    env.insert("y".to_string(), Type::Bool);
    assert_eq!(env.lookup("x"), Some(&Type::Int));
    assert_eq!(env.lookup("y"), Some(&Type::Bool));
    assert_eq!(env.lookup("z"), None);
}

#[test]
fn type_env_shadowing() {
    let mut env = TypeEnv::new();
    env.insert("x".to_string(), Type::Int);
    env.insert("x".to_string(), Type::Bool);
    assert_eq!(env.lookup("x"), Some(&Type::Bool));
}

#[test]
fn check_eq_same_types() {
    let result = TypeEnv::check_eq(&Type::Int, &Type::Int, Span::new(0, 1));
    assert!(result.is_ok());
}

#[test]
fn check_eq_different_types() {
    let result = TypeEnv::check_eq(&Type::Int, &Type::Bool, Span::new(0, 1));
    assert!(result.is_err());
}

#[test]
fn resolve_primitive_types() {
    let span = Span::new(0, 3);
    let result = resolve_type(&kodo_ast::TypeExpr::Named("Int".to_string()), span);
    assert!(result.is_ok());
    assert_eq!(result.unwrap_or(Type::Unknown), Type::Int);
}

// --- Shared test helpers ---

/// Helper to build a minimal module with one function.
pub(super) fn make_module(functions: Vec<Function>) -> Module {
    Module {
        test_decls: vec![],
        id: NodeId(0),
        span: Span::new(0, 100),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span: Span::new(0, 50),
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "unit test module".to_string(),
                span: Span::new(10, 40),
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

/// Creates a `GenericParam` with no bounds for test convenience.
pub(super) fn gp(name: &str) -> kodo_ast::GenericParam {
    kodo_ast::GenericParam {
        name: name.to_string(),
        bounds: vec![],
        span: Span::new(0, 0),
    }
}

/// Helper to build a function with the given body statements.
pub(super) fn make_function(
    name: &str,
    params: Vec<Param>,
    return_type: TypeExpr,
    stmts: Vec<Stmt>,
) -> Function {
    Function {
        id: NodeId(1),
        span: Span::new(0, 100),
        name: name.to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params,
        return_type,
        requires: vec![],
        ensures: vec![],
        body: Block {
            span: Span::new(0, 100),
            stmts,
        },
    }
}

/// Helper to build a module with a specific trust policy.
pub(super) fn make_module_with_policy(functions: Vec<Function>, policy: Option<&str>) -> Module {
    let mut entries = vec![MetaEntry {
        key: "purpose".to_string(),
        value: "test".to_string(),
        span: Span::new(10, 40),
    }];
    if let Some(p) = policy {
        entries.push(MetaEntry {
            key: "trust_policy".to_string(),
            value: p.to_string(),
            span: Span::new(10, 40),
        });
    }
    Module {
        test_decls: vec![],
        id: NodeId(0),
        span: Span::new(0, 100),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span: Span::new(0, 50),
            entries,
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

/// Helper to build a function with annotations.
pub(super) fn make_function_with_annotations(name: &str, annotations: Vec<Annotation>) -> Function {
    Function {
        id: NodeId(1),
        span: Span::new(0, 100),
        name: name.to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations,
        params: vec![],
        return_type: TypeExpr::Unit,
        requires: vec![],
        ensures: vec![],
        body: Block {
            span: Span::new(0, 100),
            stmts: vec![],
        },
    }
}

/// Helper to build a module with type and enum declarations.
pub(super) fn make_module_with_decls(
    type_decls: Vec<kodo_ast::TypeDecl>,
    enum_decls: Vec<kodo_ast::EnumDecl>,
    functions: Vec<Function>,
) -> Module {
    Module {
        test_decls: vec![],
        id: NodeId(0),
        span: Span::new(0, 100),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(99),
            span: Span::new(0, 50),
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "unit test module".to_string(),
                span: Span::new(10, 40),
            }],
        }),
        type_aliases: vec![],
        type_decls,
        enum_decls,
        trait_decls: vec![],
        impl_blocks: vec![],
        actor_decls: vec![],
        intent_decls: vec![],
        invariants: vec![],
        functions,
    }
}

/// Helper to build a minimal module for testing break/continue.
pub(super) fn make_module_with_body(stmts: Vec<Stmt>) -> Module {
    Module {
        test_decls: vec![],
        id: NodeId(0),
        span: Span::new(0, 100),
        name: "test".to_string(),
        imports: vec![],
        meta: Some(Meta {
            id: NodeId(1),
            span: Span::new(0, 10),
            entries: vec![MetaEntry {
                key: "purpose".to_string(),
                value: "test".to_string(),
                span: Span::new(0, 10),
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
        functions: vec![Function {
            id: NodeId(2),
            span: Span::new(0, 100),
            name: "test_fn".to_string(),
            visibility: Visibility::Private,
            is_async: false,
            generic_params: vec![],
            annotations: vec![],
            params: vec![],
            return_type: TypeExpr::Unit,
            requires: vec![],
            ensures: vec![],
            body: Block {
                span: Span::new(0, 100),
                stmts,
            },
        }],
    }
}
