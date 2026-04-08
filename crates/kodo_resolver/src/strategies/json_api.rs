//! JSON API intent strategy.
//!
//! Generates JSON API handler functions with validation contracts. Supports
//! two modes: legacy (routes/models) and endpoint-based (real HTTP server).

use kodo_ast::{
    Block, Expr, Function, IntentConfigValue, IntentDecl, NodeId, Ownership, Param, Span, Stmt,
    TypeExpr, Visibility,
};

use crate::helpers::{
    config_value_as_str, generate_http_server_main, get_int_config, get_nested_list_config,
    get_string_config, get_string_list_config,
};
use crate::{ResolvedIntent, ResolverStrategy, Result};

/// Generates JSON API handler functions with validation contracts.
///
/// Config keys:
/// - `routes` (list): Route path strings (e.g., `"/users"`, `"/posts"`).
/// - `models` (list): Model names for which struct-like accessor stubs are generated.
/// - `port` (integer, optional): Port for endpoint mode.
/// - `base_path` (string, optional): Base path prefix for endpoint mode.
/// - `endpoints` (list of lists): Endpoint definitions for real HTTP server mode.
///
/// Each route gets a handler function. Each model gets `create_<model>` and
/// `get_<model>` functions with validation contracts.
pub(crate) struct JsonApiStrategy;

impl ResolverStrategy for JsonApiStrategy {
    fn handles(&self) -> &[&str] {
        &["json_api"]
    }

    fn valid_keys(&self) -> &[&str] {
        &["routes", "models", "port", "base_path", "endpoints"]
    }

    fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
        let span = intent.span;

        // New mode: if "endpoints" is present, generate a real HTTP server.
        let endpoints = get_nested_list_config(intent, "endpoints");
        if !endpoints.is_empty() {
            let port = get_int_config(intent, "port").unwrap_or(8080);
            let base_path = get_string_config(intent, "base_path").unwrap_or("");
            return Ok(resolve_json_api_server(span, port, base_path, &endpoints));
        }

        // Legacy mode: generate stubs from routes/models.
        let routes = get_string_list_config(intent, "routes");
        let models = get_string_list_config(intent, "models");

        let mut generated = Vec::new();
        let mut descriptions = Vec::new();
        let mut generated_types = Vec::new();

        // Generate route handlers
        for route in &routes {
            let handler_name = route_to_handler_name(route);
            let func = generate_api_handler(&handler_name, route, span);
            descriptions.push(format!("  - `{handler_name}()` -> handler for {route}"));
            generated.push(func);
        }

        // Generate model CRUD stubs
        for model in &models {
            let lower = model.to_lowercase();
            generated_types.push(model.clone());

            // create_<model>(data: String) -> String
            let create_func = generate_api_create_model(&lower, span);
            descriptions.push(format!("  - `create_{lower}(data: String) -> String`"));
            generated.push(create_func);

            // get_<model>(id: Int) -> String
            let get_func = generate_api_get_model(&lower, span);
            descriptions.push(format!("  - `get_{lower}(id: Int) -> String`"));
            generated.push(get_func);
        }

        Ok(ResolvedIntent {
            generated_functions: generated,
            generated_types,
            description: format!(
                "Generated JSON API:\n{}",
                if descriptions.is_empty() {
                    "  (no routes or models)".to_string()
                } else {
                    descriptions.join("\n")
                }
            ),
        })
    }
}

/// Converts a route path like `"/users"` to a handler name like `handle_users`.
pub(crate) fn route_to_handler_name(route: &str) -> String {
    let cleaned: String = route
        .trim_matches('/')
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    if cleaned.is_empty() {
        "handle_root".to_string()
    } else {
        format!("handle_{cleaned}")
    }
}

/// Generates a JSON API route handler function.
pub(crate) fn generate_api_handler(handler_name: &str, route: &str, span: Span) -> Function {
    let body_expr = Expr::Call {
        callee: Box::new(Expr::Ident("println".to_string(), span)),
        args: vec![Expr::StringLit(
            format!("Handling API request: {route}"),
            span,
        )],
        span,
    };

    Function {
        id: NodeId(0),
        span,
        name: handler_name.to_string(),
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

/// Generates a `create_<model>` function with a non-empty data contract.
pub(crate) fn generate_api_create_model(model_lower: &str, span: Span) -> Function {
    let func_name = format!("create_{model_lower}");
    let body_expr = Expr::Call {
        callee: Box::new(Expr::Ident("println".to_string(), span)),
        args: vec![Expr::StringLit(format!("creating {model_lower}"), span)],
        span,
    };

    // requires { data != "" } — validation contract
    let requires = vec![Expr::BinaryOp {
        left: Box::new(Expr::Ident("data".to_string(), span)),
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
            name: "data".to_string(),
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

/// Generates a `get_<model>` function with a positive ID contract.
pub(crate) fn generate_api_get_model(model_lower: &str, span: Span) -> Function {
    let func_name = format!("get_{model_lower}");
    let body_expr = Expr::Call {
        callee: Box::new(Expr::Ident("println".to_string(), span)),
        args: vec![Expr::StringLit(format!("fetching {model_lower}"), span)],
        span,
    };

    // requires { id > 0 }
    let requires = vec![Expr::BinaryOp {
        left: Box::new(Expr::Ident("id".to_string(), span)),
        op: kodo_ast::BinOp::Gt,
        right: Box::new(Expr::IntLit(0, span)),
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
            name: "id".to_string(),
            ty: TypeExpr::Named("Int".to_string()),
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

/// Resolves a `json_api` intent with real HTTP server endpoints.
fn resolve_json_api_server(
    span: Span,
    port: i64,
    base_path: &str,
    endpoints: &[Vec<IntentConfigValue>],
) -> ResolvedIntent {
    let mut route_entries: Vec<(String, String, String)> = Vec::new();
    for ep in endpoints {
        if ep.len() >= 3 {
            let method = config_value_as_str(&ep[0]).unwrap_or("GET").to_string();
            let raw_path = config_value_as_str(&ep[1]).unwrap_or("/").to_string();
            let path = format!("{base_path}{raw_path}");
            let handler = config_value_as_str(&ep[2]).unwrap_or("handler").to_string();
            route_entries.push((method, path, handler));
        }
    }

    let main_func = generate_http_server_main(port, "Not Found", &route_entries, span);

    let route_desc: Vec<String> = route_entries
        .iter()
        .map(|(m, p, h)| format!("  - {m} {p} → `{h}()`"))
        .collect();

    ResolvedIntent {
        generated_functions: vec![main_func],
        generated_types: vec![],
        description: format!(
            "Generated JSON API server on port {port} with {} endpoints:\n{}",
            route_entries.len(),
            route_desc.join("\n")
        ),
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
    fn route_to_handler_name_simple() {
        assert_eq!(route_to_handler_name("/users"), "handle_users");
    }

    #[test]
    fn route_to_handler_name_nested() {
        assert_eq!(
            route_to_handler_name("/api/v1/posts"),
            "handle_api_v1_posts"
        );
    }

    #[test]
    fn route_to_handler_name_root() {
        assert_eq!(route_to_handler_name("/"), "handle_root");
    }

    #[test]
    fn route_to_handler_name_empty() {
        assert_eq!(route_to_handler_name(""), "handle_root");
    }

    #[test]
    fn api_handler_has_correct_name() {
        let func = generate_api_handler("handle_users", "/users", dummy_span());
        assert_eq!(func.name, "handle_users");
    }

    #[test]
    fn api_handler_has_no_params() {
        let func = generate_api_handler("handle_users", "/users", dummy_span());
        assert_eq!(func.params.len(), 0);
    }

    #[test]
    fn api_create_model_has_data_param_with_contract() {
        let func = generate_api_create_model("user", dummy_span());
        assert_eq!(func.name, "create_user");
        assert_eq!(func.params.len(), 1);
        assert_eq!(func.params[0].name, "data");
        assert_eq!(func.requires.len(), 1);
    }

    #[test]
    fn api_get_model_has_id_param_with_contract() {
        let func = generate_api_get_model("post", dummy_span());
        assert_eq!(func.name, "get_post");
        assert_eq!(func.params.len(), 1);
        assert_eq!(func.params[0].name, "id");
        assert_eq!(func.requires.len(), 1);
    }

    #[test]
    fn json_api_strategy_generates_route_and_model_functions() {
        let intent = make_intent(
            "json_api",
            vec![
                list_entry("routes", &["/users", "/posts"]),
                list_entry("models", &["User"]),
            ],
        );
        let result = JsonApiStrategy.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap();
        // 2 route handlers + 2 model functions (create + get)
        assert_eq!(resolved.generated_functions.len(), 4);
    }

    #[test]
    fn json_api_strategy_empty_config() {
        let intent = make_intent("json_api", vec![]);
        let result = JsonApiStrategy.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(resolved.generated_functions.len(), 0);
    }

    #[test]
    fn json_api_strategy_handles_json_api_intent() {
        assert!(JsonApiStrategy.handles().contains(&"json_api"));
    }

    #[test]
    fn json_api_strategy_valid_keys() {
        let keys = JsonApiStrategy.valid_keys();
        assert!(keys.contains(&"routes"));
        assert!(keys.contains(&"models"));
        assert!(keys.contains(&"endpoints"));
    }
}
