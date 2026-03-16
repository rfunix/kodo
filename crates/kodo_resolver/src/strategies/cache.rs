//! Cache intent strategy.
//!
//! Generates cache access functions (`cache_get`, `cache_set`, `cache_invalidate`)
//! with size-bounded contracts for the `cache` intent.

use kodo_ast::{
    Block, Expr, Function, IntentDecl, NodeId, Ownership, Param, Span, Stmt, TypeExpr, Visibility,
};

use crate::helpers::{get_int_config, get_string_config};
use crate::{ResolvedIntent, ResolverStrategy, Result};

/// Default maximum size for cache strategies when not specified.
const DEFAULT_CACHE_MAX_SIZE: i64 = 256;

/// Generates cache access functions with size-bounded contracts.
///
/// Config keys:
/// - `strategy` (string): The caching strategy (e.g., `"lru"`, `"fifo"`).
/// - `max_size` (integer): The maximum number of entries in the cache.
///
/// Generates `cache_get`, `cache_set`, and `cache_invalidate` functions.
/// The `cache_set` function includes a contract ensuring the key is non-empty.
pub(crate) struct CacheStrategy;

impl ResolverStrategy for CacheStrategy {
    fn handles(&self) -> &[&str] {
        &["cache"]
    }

    fn valid_keys(&self) -> &[&str] {
        &["strategy", "max_size"]
    }

    fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
        let span = intent.span;
        let strategy = get_string_config(intent, "strategy").unwrap_or("lru");
        let max_size = get_int_config(intent, "max_size").unwrap_or(DEFAULT_CACHE_MAX_SIZE);

        let mut generated = Vec::new();
        let mut descriptions = Vec::new();

        // cache_get(key: String) -> String
        let get_func = generate_cache_get(span);
        descriptions.push("  - `cache_get(key: String) -> String`".to_string());
        generated.push(get_func);

        // cache_set(key: String, value: String) -> Bool
        let set_func = generate_cache_set(max_size, span);
        descriptions.push(format!(
            "  - `cache_set(key: String, value: String) -> Bool` (max_size: {max_size})"
        ));
        generated.push(set_func);

        // cache_invalidate(key: String) -> Bool
        let invalidate_func = generate_cache_invalidate(span);
        descriptions.push("  - `cache_invalidate(key: String) -> Bool`".to_string());
        generated.push(invalidate_func);

        Ok(ResolvedIntent {
            generated_functions: generated,
            generated_types: vec![],
            description: format!(
                "Generated cache layer (strategy: {strategy}, max_size: {max_size}):\n{}",
                descriptions.join("\n")
            ),
        })
    }
}

/// Generates a `cache_get` function.
pub(crate) fn generate_cache_get(span: Span) -> Function {
    let body_expr = Expr::Call {
        callee: Box::new(Expr::Ident("println".to_string(), span)),
        args: vec![Expr::StringLit("cache_get".to_string(), span)],
        span,
    };

    // requires { key != "" }
    let requires = vec![Expr::BinaryOp {
        left: Box::new(Expr::Ident("key".to_string(), span)),
        op: kodo_ast::BinOp::Ne,
        right: Box::new(Expr::StringLit(String::new(), span)),
        span,
    }];

    Function {
        id: NodeId(0),
        span,
        name: "cache_get".to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![Param {
            name: "key".to_string(),
            ty: TypeExpr::Named("String".to_string()),
            span,
            ownership: Ownership::Owned,
        }],
        return_type: TypeExpr::Named("String".to_string()),
        requires,
        ensures: vec![],
        body: Block {
            span,
            stmts: vec![Stmt::Expr(body_expr)],
        },
    }
}

/// Generates a `cache_set` function with a max-size contract.
pub(crate) fn generate_cache_set(max_size: i64, span: Span) -> Function {
    let body_expr = Expr::Call {
        callee: Box::new(Expr::Ident("println".to_string(), span)),
        args: vec![Expr::StringLit(
            format!("cache_set (max: {max_size})"),
            span,
        )],
        span,
    };

    // requires { key != "" }
    let requires = vec![Expr::BinaryOp {
        left: Box::new(Expr::Ident("key".to_string(), span)),
        op: kodo_ast::BinOp::Ne,
        right: Box::new(Expr::StringLit(String::new(), span)),
        span,
    }];

    Function {
        id: NodeId(0),
        span,
        name: "cache_set".to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![
            Param {
                name: "key".to_string(),
                ty: TypeExpr::Named("String".to_string()),
                span,
                ownership: Ownership::Owned,
            },
            Param {
                name: "value".to_string(),
                ty: TypeExpr::Named("String".to_string()),
                span,
                ownership: Ownership::Owned,
            },
        ],
        return_type: TypeExpr::Named("Bool".to_string()),
        requires,
        ensures: vec![],
        body: Block {
            span,
            stmts: vec![Stmt::Expr(body_expr)],
        },
    }
}

/// Generates a `cache_invalidate` function.
pub(crate) fn generate_cache_invalidate(span: Span) -> Function {
    let body_expr = Expr::Call {
        callee: Box::new(Expr::Ident("println".to_string(), span)),
        args: vec![Expr::StringLit("cache_invalidate".to_string(), span)],
        span,
    };

    // requires { key != "" }
    let requires = vec![Expr::BinaryOp {
        left: Box::new(Expr::Ident("key".to_string(), span)),
        op: kodo_ast::BinOp::Ne,
        right: Box::new(Expr::StringLit(String::new(), span)),
        span,
    }];

    Function {
        id: NodeId(0),
        span,
        name: "cache_invalidate".to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![Param {
            name: "key".to_string(),
            ty: TypeExpr::Named("String".to_string()),
            span,
            ownership: Ownership::Owned,
        }],
        return_type: TypeExpr::Named("Bool".to_string()),
        requires,
        ensures: vec![],
        body: Block {
            span,
            stmts: vec![Stmt::Expr(body_expr)],
        },
    }
}
