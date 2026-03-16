//! HTTP serve intent strategy.
//!
//! Generates HTTP handler stubs for the `serve_http` intent, including
//! a main function that prints server startup info and optional route handlers.

use kodo_ast::{
    Block, Expr, Function, IntentConfigValue, IntentDecl, NodeId, Span, Stmt, TypeExpr, Visibility,
};

use crate::helpers::get_int_config;
use crate::{ResolvedIntent, ResolverStrategy, Result};

/// Generates HTTP handler stubs for serving HTTP requests.
///
/// Config keys:
/// - `port` (integer): The port to listen on.
/// - `routes` (list): Route definitions (currently generates handler stubs).
pub(crate) struct ServeHttpStrategy;

impl ResolverStrategy for ServeHttpStrategy {
    fn handles(&self) -> &[&str] {
        &["serve_http"]
    }

    fn valid_keys(&self) -> &[&str] {
        &["port", "routes"]
    }

    fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
        let span = intent.span;

        // Extract port if present, default to 8080
        let port = get_int_config(intent, "port").unwrap_or(8080);

        // Generate a main function that prints server startup info
        let startup_msg = format!("HTTP server starting on port {port}");
        let println_call = Expr::Call {
            callee: Box::new(Expr::Ident("println".to_string(), span)),
            args: vec![Expr::StringLit(startup_msg.clone(), span)],
            span,
        };

        let main_func = Function {
            id: NodeId(0),
            span,
            name: "kodo_main".to_string(),
            visibility: Visibility::Private,
            is_async: false,
            generic_params: vec![],
            annotations: vec![],
            params: vec![],
            return_type: TypeExpr::Unit,
            requires: vec![],
            ensures: vec![],
            body: Block {
                span,
                stmts: vec![Stmt::Expr(println_call)],
            },
        };

        // Generate handler stubs from routes
        let mut generated = vec![main_func];
        let mut route_descriptions = Vec::new();

        for entry in &intent.config {
            if entry.key == "routes" {
                if let IntentConfigValue::List(ref items, _) = entry.value {
                    for item in items {
                        if let IntentConfigValue::FnRef(ref handler_name, _) = item {
                            let handler = generate_http_handler(handler_name, span);
                            route_descriptions.push(format!("  - `{handler_name}()`"));
                            generated.push(handler);
                        }
                    }
                }
            }
        }

        let description = if route_descriptions.is_empty() {
            format!("Generated HTTP server on port {port} (no routes)")
        } else {
            format!(
                "Generated HTTP server on port {port} with handlers:\n{}",
                route_descriptions.join("\n")
            )
        };

        Ok(ResolvedIntent {
            generated_functions: generated,
            generated_types: vec![],
            description,
        })
    }
}

/// Generates an HTTP handler stub function.
pub(crate) fn generate_http_handler(name: &str, span: Span) -> Function {
    // Generate: fn handler_name() { println("Handling request: handler_name") }
    let msg = format!("Handling request: {name}");
    let println_call = Expr::Call {
        callee: Box::new(Expr::Ident("println".to_string(), span)),
        args: vec![Expr::StringLit(msg, span)],
        span,
    };

    Function {
        id: NodeId(0),
        span,
        name: name.to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![],
        return_type: TypeExpr::Unit,
        requires: vec![],
        ensures: vec![],
        body: Block {
            span,
            stmts: vec![Stmt::Expr(println_call)],
        },
    }
}
