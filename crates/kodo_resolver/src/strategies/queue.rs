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

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_ast::{IntentConfigEntry, IntentConfigValue, NodeId, Span};

    fn dummy_span() -> Span {
        Span::new(0, 0)
    }

    fn make_intent(name: &str, config: Vec<IntentConfigEntry>) -> IntentDecl {
        IntentDecl {
            id: NodeId(0),
            span: dummy_span(),
            name: name.to_string(),
            config,
        }
    }

    fn str_entry(key: &str, val: &str) -> IntentConfigEntry {
        IntentConfigEntry {
            key: key.to_string(),
            value: IntentConfigValue::StringLit(val.to_string(), dummy_span()),
            span: dummy_span(),
        }
    }

    fn list_entry(key: &str, items: &[&str]) -> IntentConfigEntry {
        let vals = items
            .iter()
            .map(|s| IntentConfigValue::StringLit(s.to_string(), dummy_span()))
            .collect();
        IntentConfigEntry {
            key: key.to_string(),
            value: IntentConfigValue::List(vals, dummy_span()),
            span: dummy_span(),
        }
    }

    #[test]
    fn produce_func_has_correct_name() {
        let func = generate_queue_produce("orders", dummy_span());
        assert_eq!(func.name, "produce_orders");
    }

    #[test]
    fn produce_func_has_message_param() {
        let func = generate_queue_produce("events", dummy_span());
        assert_eq!(func.params.len(), 1);
        assert_eq!(func.params[0].name, "message");
    }

    #[test]
    fn produce_func_has_requires_clause() {
        let func = generate_queue_produce("jobs", dummy_span());
        assert_eq!(
            func.requires.len(),
            1,
            "produce should require non-empty message"
        );
    }

    #[test]
    fn consume_func_has_correct_name() {
        let func = generate_queue_consume("orders", dummy_span());
        assert_eq!(func.name, "consume_orders");
    }

    #[test]
    fn consume_func_has_no_params() {
        let func = generate_queue_consume("events", dummy_span());
        assert_eq!(func.params.len(), 0);
    }

    #[test]
    fn consume_func_returns_string() {
        let func = generate_queue_consume("jobs", dummy_span());
        assert!(matches!(func.return_type, kodo_ast::TypeExpr::Named(ref n) if n == "String"));
    }

    #[test]
    fn queue_strategy_generates_produce_and_consume_per_topic() {
        let intent = make_intent(
            "queue",
            vec![
                str_entry("backend", "redis"),
                list_entry("topics", &["orders", "events"]),
            ],
        );
        let result = QueueStrategy.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap();
        // 2 topics × 2 functions each = 4
        assert_eq!(resolved.generated_functions.len(), 4);
        assert!(resolved.description.contains("redis"));
    }

    #[test]
    fn queue_strategy_empty_topics() {
        let intent = make_intent("queue", vec![]);
        let result = QueueStrategy.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(resolved.generated_functions.len(), 0);
        assert!(resolved.description.contains("(no topics)"));
    }

    #[test]
    fn queue_strategy_handles_queue_intent() {
        assert!(QueueStrategy.handles().contains(&"queue"));
    }

    #[test]
    fn queue_strategy_valid_keys() {
        let keys = QueueStrategy.valid_keys();
        assert!(keys.contains(&"backend"));
        assert!(keys.contains(&"topics"));
    }
}
