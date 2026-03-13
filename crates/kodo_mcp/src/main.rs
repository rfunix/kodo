//! MCP server binary — reads JSON-RPC requests from stdin, writes responses to stdout.
//!
//! Usage:
//!   kodo-mcp
//!
//! The server reads newline-delimited JSON-RPC 2.0 requests from stdin
//! and writes responses to stdout, following the MCP stdio transport.

use std::io::{self, BufRead, Write};

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: kodo_mcp::JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                let error_resp = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32700,
                        "message": format!("Parse error: {e}"),
                    }
                });
                let _ = writeln!(stdout, "{error_resp}");
                let _ = stdout.flush();
                continue;
            }
        };

        let response = kodo_mcp::handle_request(&request);
        if let Ok(json) = serde_json::to_string(&response) {
            let _ = writeln!(stdout, "{json}");
            let _ = stdout.flush();
        }
    }
}
