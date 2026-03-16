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
