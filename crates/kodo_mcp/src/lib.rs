//! # `kodo_mcp` — MCP Server for the Kōdo Compiler
//!
//! Exposes the Kōdo compiler as a set of MCP (Model Context Protocol) tools
//! that AI agents can invoke natively via JSON-RPC over stdio.
//!
//! ## Available Tools
//!
//! - `kodo.check` — Type-check a source file, returning structured errors + repair plans
//! - `kodo.build` — Compile a source file to a binary
//! - `kodo.explain` — Explain an error code
//! - `kodo.describe` — Return module metadata (functions, types, contracts)
//! - `kodo.confidence_report` — Return confidence scores for all functions

#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![warn(clippy::pedantic)]

pub mod tools;

use serde::{Deserialize, Serialize};

/// A JSON-RPC 2.0 request.
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    /// JSON-RPC version (must be "2.0").
    pub jsonrpc: String,
    /// Request ID.
    pub id: serde_json::Value,
    /// Method name.
    pub method: String,
    /// Parameters.
    #[serde(default)]
    pub params: serde_json::Value,
}

/// A JSON-RPC 2.0 response.
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    /// JSON-RPC version.
    pub jsonrpc: String,
    /// Request ID (echoed from request).
    pub id: serde_json::Value,
    /// Result on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Error on failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    /// Error code.
    pub code: i32,
    /// Human-readable message.
    pub message: String,
    /// Optional structured data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// MCP tool definition for the `tools/list` response.
#[derive(Debug, Serialize)]
pub struct ToolDefinition {
    /// Tool name.
    pub name: String,
    /// Tool description.
    pub description: String,
    /// Input schema (JSON Schema).
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// Returns the list of MCP tools exposed by the Kōdo compiler.
#[must_use]
pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "kodo.check".to_string(),
            description:
                "Type-check a Kōdo source file and return structured errors with repair plans"
                    .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "Kōdo source code to check" },
                    "filename": { "type": "string", "description": "Optional filename for error reporting" }
                },
                "required": ["source"]
            }),
        },
        ToolDefinition {
            name: "kodo.describe".to_string(),
            description: "Describe a Kōdo module: functions, types, contracts, metadata"
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "Kōdo source code to describe" }
                },
                "required": ["source"]
            }),
        },
        ToolDefinition {
            name: "kodo.explain".to_string(),
            description: "Explain a Kōdo compiler error code".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "code": { "type": "string", "description": "Error code (e.g. E0200)" }
                },
                "required": ["code"]
            }),
        },
        ToolDefinition {
            name: "kodo.confidence_report".to_string(),
            description: "Generate a confidence report for all functions in a Kōdo module"
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "Kōdo source code to analyze" }
                },
                "required": ["source"]
            }),
        },
    ]
}

/// Handles an MCP JSON-RPC request and returns a response.
///
/// # Errors
///
/// Returns a JSON-RPC error response if the method is unknown or
/// parameters are invalid.
#[must_use]
pub fn handle_request(request: &JsonRpcRequest) -> JsonRpcResponse {
    match request.method.as_str() {
        "initialize" => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id.clone(),
            result: Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "kodo-mcp",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
            error: None,
        },
        "tools/list" => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id.clone(),
            result: Some(serde_json::json!({ "tools": tool_definitions() })),
            error: None,
        },
        "tools/call" => {
            let tool_name = request.params.get("name").and_then(|v| v.as_str());
            let arguments = request
                .params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

            match tool_name {
                Some("kodo.check") => tools::handle_check(&request.id, &arguments),
                Some("kodo.describe") => tools::handle_describe(&request.id, &arguments),
                Some("kodo.explain") => tools::handle_explain(&request.id, &arguments),
                Some("kodo.confidence_report") => {
                    tools::handle_confidence_report(&request.id, &arguments)
                }
                Some(name) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id.clone(),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32601,
                        message: format!("Unknown tool: {name}"),
                        data: None,
                    }),
                },
                None => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id.clone(),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32602,
                        message: "Missing 'name' in tools/call params".to_string(),
                        data: None,
                    }),
                },
            }
        }
        _ => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id.clone(),
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: format!("Method not found: {}", request.method),
                data: None,
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_definitions_are_valid() {
        let tools = tool_definitions();
        assert_eq!(tools.len(), 4);
        assert_eq!(tools[0].name, "kodo.check");
        assert_eq!(tools[1].name, "kodo.describe");
        assert_eq!(tools[2].name, "kodo.explain");
        assert_eq!(tools[3].name, "kodo.confidence_report");
    }

    #[test]
    fn handle_initialize() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(1),
            method: "initialize".to_string(),
            params: serde_json::Value::Null,
        };
        let resp = handle_request(&req);
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn handle_tools_list() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(2),
            method: "tools/list".to_string(),
            params: serde_json::Value::Null,
        };
        let resp = handle_request(&req);
        assert!(resp.result.is_some());
        let tools = resp.result.as_ref().and_then(|r| r.get("tools"));
        assert!(tools.is_some());
    }

    #[test]
    fn handle_unknown_method() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(3),
            method: "nonexistent".to_string(),
            params: serde_json::Value::Null,
        };
        let resp = handle_request(&req);
        assert!(resp.error.is_some());
        assert_eq!(resp.error.as_ref().map(|e| e.code), Some(-32601));
    }

    #[test]
    fn handle_kodo_check_valid_source() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(4),
            method: "tools/call".to_string(),
            params: serde_json::json!({
                "name": "kodo.check",
                "arguments": {
                    "source": "module test {\n    meta { purpose: \"test\" }\n    fn main() -> Int {\n        return 0\n    }\n}\n"
                }
            }),
        };
        let resp = handle_request(&req);
        assert!(
            resp.error.is_none(),
            "check should succeed: {:?}",
            resp.error
        );
        let result = resp.result.as_ref();
        assert!(result.is_some());
        let status = result
            .and_then(|r| r.get("status"))
            .and_then(|s| s.as_str());
        assert_eq!(status, Some("ok"));
    }

    #[test]
    fn handle_kodo_check_with_errors() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(5),
            method: "tools/call".to_string(),
            params: serde_json::json!({
                "name": "kodo.check",
                "arguments": {
                    "source": "module test {\n    fn main() -> Int {\n        return 0\n    }\n}\n"
                }
            }),
        };
        let resp = handle_request(&req);
        assert!(resp.error.is_none());
        let result = resp.result.as_ref();
        let status = result
            .and_then(|r| r.get("status"))
            .and_then(|s| s.as_str());
        assert_eq!(status, Some("failed"));
    }

    #[test]
    fn handle_kodo_describe() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(6),
            method: "tools/call".to_string(),
            params: serde_json::json!({
                "name": "kodo.describe",
                "arguments": {
                    "source": "module test {\n    meta { purpose: \"test\" }\n    fn add(a: Int, b: Int) -> Int {\n        return a + b\n    }\n}\n"
                }
            }),
        };
        let resp = handle_request(&req);
        assert!(
            resp.error.is_none(),
            "describe should succeed: {:?}",
            resp.error
        );
        let result = resp.result.as_ref();
        assert!(result.is_some());
    }

    #[test]
    fn handle_kodo_explain() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(7),
            method: "tools/call".to_string(),
            params: serde_json::json!({
                "name": "kodo.explain",
                "arguments": {
                    "code": "E0200"
                }
            }),
        };
        let resp = handle_request(&req);
        assert!(resp.error.is_none());
        assert!(resp.result.is_some());
    }
}
