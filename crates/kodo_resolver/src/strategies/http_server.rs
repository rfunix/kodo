//! HTTP server intent strategy.
//!
//! Generates a real HTTP server with routing using `http_server_*` builtins
//! for the `http_server` intent.

use kodo_ast::IntentDecl;

use crate::helpers::{
    config_value_as_str, generate_http_server_main, get_int_config, get_nested_list_config,
    get_string_config,
};
use crate::{ResolvedIntent, ResolverStrategy, Result};

/// Generates a real HTTP server with routing using `http_server_*` builtins.
///
/// Config keys:
/// - `port` (integer, optional): Port to listen on. Default: `8080`.
/// - `routes` (list of lists): Each sub-list is `[method, path, handler_fn]`.
/// - `not_found` (string, optional): 404 response body. Default: `"Not Found"`.
pub(crate) struct HttpServerStrategy;

impl ResolverStrategy for HttpServerStrategy {
    fn handles(&self) -> &[&str] {
        &["http_server"]
    }

    fn valid_keys(&self) -> &[&str] {
        &["port", "routes", "not_found"]
    }

    fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
        let span = intent.span;
        let port = get_int_config(intent, "port").unwrap_or(8080);
        let not_found = get_string_config(intent, "not_found").unwrap_or("Not Found");
        let routes = get_nested_list_config(intent, "routes");

        // Parse routes: each is [method, path, handler_fn]
        let mut route_entries: Vec<(String, String, String)> = Vec::new();
        for route in &routes {
            if route.len() >= 3 {
                let method = config_value_as_str(&route[0]).unwrap_or("GET").to_string();
                let path = config_value_as_str(&route[1]).unwrap_or("/").to_string();
                let handler = config_value_as_str(&route[2])
                    .unwrap_or("handler")
                    .to_string();
                route_entries.push((method, path, handler));
            }
        }

        let main_func = generate_http_server_main(port, not_found, &route_entries, span);

        let route_desc: Vec<String> = route_entries
            .iter()
            .map(|(m, p, h)| format!("  - {m} {p} → `{h}()`"))
            .collect();

        let description = format!(
            "Generated HTTP server on port {port} with {} routes:\n{}",
            route_entries.len(),
            route_desc.join("\n")
        );

        Ok(ResolvedIntent {
            generated_functions: vec![main_func],
            generated_types: vec![],
            description,
        })
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

    fn int_entry(key: &str, val: i64) -> IntentConfigEntry {
        IntentConfigEntry {
            key: key.to_string(),
            value: IntentConfigValue::IntLit(val, dummy_span()),
            span: dummy_span(),
        }
    }

    fn nested_list_entry(key: &str, rows: &[(&str, &str, &str)]) -> IntentConfigEntry {
        let vals = rows
            .iter()
            .map(|(method, path, handler)| {
                IntentConfigValue::List(
                    vec![
                        IntentConfigValue::StringLit(method.to_string(), dummy_span()),
                        IntentConfigValue::StringLit(path.to_string(), dummy_span()),
                        IntentConfigValue::StringLit(handler.to_string(), dummy_span()),
                    ],
                    dummy_span(),
                )
            })
            .collect();
        IntentConfigEntry {
            key: key.to_string(),
            value: IntentConfigValue::List(vals, dummy_span()),
            span: dummy_span(),
        }
    }

    #[test]
    fn http_server_resolves_with_no_routes() {
        let intent = make_intent("http_server", vec![]);
        let result = HttpServerStrategy.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(resolved.generated_functions.len(), 1);
        assert!(resolved.description.contains("8080"));
        assert!(resolved.description.contains("0 routes"));
    }

    #[test]
    fn http_server_resolves_with_custom_port() {
        let intent = make_intent("http_server", vec![int_entry("port", 3000)]);
        let result = HttpServerStrategy.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert!(resolved.description.contains("3000"));
    }

    #[test]
    fn http_server_resolves_with_routes() {
        let intent = make_intent(
            "http_server",
            vec![nested_list_entry(
                "routes",
                &[
                    ("GET", "/health", "health_check"),
                    ("POST", "/api/users", "create_user"),
                ],
            )],
        );
        let result = HttpServerStrategy.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert!(resolved.description.contains("2 routes"));
        assert!(resolved.description.contains("health_check"));
        assert!(resolved.description.contains("create_user"));
    }

    #[test]
    fn http_server_strategy_handles_http_server_intent() {
        assert!(HttpServerStrategy.handles().contains(&"http_server"));
    }

    #[test]
    fn http_server_strategy_valid_keys() {
        let keys = HttpServerStrategy.valid_keys();
        assert!(keys.contains(&"port"));
        assert!(keys.contains(&"routes"));
        assert!(keys.contains(&"not_found"));
    }
}
