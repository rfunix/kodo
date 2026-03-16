//! Queue intent strategy.
//!
//! Generates message queue produce/consume function pairs for each topic
//! in the `queue` intent. Produce functions include a contract requiring
//! a non-empty message.

use kodo_ast::{
    Block, Expr, Function, IntentDecl, NodeId, Ownership, Param, Span, Stmt, TypeExpr, Visibility,
};

use crate::helpers::{get_string_config, get_string_list_config};
use crate::{ResolvedIntent, ResolverStrategy, Result};

/// Generates message queue produce/consume functions for each topic.
///
/// Config keys:
/// - `backend` (string): The queue backend name (e.g., `"memory"`, `"redis"`).
/// - `topics` (list): Topic names for which produce/consume function pairs are generated.
///
/// Each topic gets `produce_<topic>(message: String)` and
/// `consume_<topic>() -> String` functions. Produce functions include a contract
/// requiring a non-empty message.
pub(crate) struct QueueStrategy;

impl ResolverStrategy for QueueStrategy {
    fn handles(&self) -> &[&str] {
        &["queue"]
    }

    fn valid_keys(&self) -> &[&str] {
        &["backend", "topics"]
    }

    fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
        let span = intent.span;
        let backend = get_string_config(intent, "backend").unwrap_or("memory");
        let topics = get_string_list_config(intent, "topics");

        let mut generated = Vec::new();
        let mut descriptions = Vec::new();

        for topic in &topics {
            // produce_<topic>(message: String)
            let produce_func = generate_queue_produce(topic, span);
            descriptions.push(format!("  - `produce_{topic}(message: String)`"));
            generated.push(produce_func);

            // consume_<topic>() -> String
            let consume_func = generate_queue_consume(topic, span);
            descriptions.push(format!("  - `consume_{topic}() -> String`"));
            generated.push(consume_func);
        }

        Ok(ResolvedIntent {
            generated_functions: generated,
            generated_types: vec![],
            description: format!(
                "Generated message queue (backend: {backend}):\n{}",
                if descriptions.is_empty() {
                    "  (no topics)".to_string()
                } else {
                    descriptions.join("\n")
                }
            ),
        })
    }
}

/// Generates a `produce_<topic>` function with a non-empty message contract.
pub(crate) fn generate_queue_produce(topic: &str, span: Span) -> Function {
    let func_name = format!("produce_{topic}");
    let body_expr = Expr::Call {
        callee: Box::new(Expr::Ident("println".to_string(), span)),
        args: vec![Expr::StringLit(
            format!("producing to topic: {topic}"),
            span,
        )],
        span,
    };

    // requires { message != "" }
    let requires = vec![Expr::BinaryOp {
        left: Box::new(Expr::Ident("message".to_string(), span)),
        op: kodo_ast::BinOp::Ne,
        right: Box::new(Expr::StringLit(String::new(), span)),
        span,
    }];

    Function {
        id: NodeId(0),
        span,
        name: func_name,
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![Param {
            name: "message".to_string(),
            ty: TypeExpr::Named("String".to_string()),
            span,
            ownership: Ownership::Owned,
        }],
        return_type: TypeExpr::Unit,
        requires,
        ensures: vec![],
        body: Block {
            span,
            stmts: vec![Stmt::Expr(body_expr)],
        },
    }
}

/// Generates a `consume_<topic>` function.
pub(crate) fn generate_queue_consume(topic: &str, span: Span) -> Function {
    let func_name = format!("consume_{topic}");
    let body_expr = Expr::Call {
        callee: Box::new(Expr::Ident("println".to_string(), span)),
        args: vec![Expr::StringLit(
            format!("consuming from topic: {topic}"),
            span,
        )],
        span,
    };

    Function {
        id: NodeId(0),
        span,
        name: func_name,
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![],
        return_type: TypeExpr::Named("String".to_string()),
        requires: vec![],
        ensures: vec![],
        body: Block {
            span,
            stmts: vec![Stmt::Expr(body_expr)],
        },
    }
}
