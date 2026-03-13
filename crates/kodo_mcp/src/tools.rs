//! # MCP Tool Implementations
//!
//! Each function handles one MCP tool call, reusing the Kōdo compiler
//! pipeline (parse → type-check → contracts → describe/confidence).

use crate::{JsonRpcError, JsonRpcResponse};
use kodo_ast::Diagnostic;

/// Helper to build a JSON-RPC error response for missing parameters.
fn missing_param_error(id: &serde_json::Value, param: &str) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: id.clone(),
        result: None,
        error: Some(JsonRpcError {
            code: -32602,
            message: format!("Missing required parameter '{param}'"),
            data: None,
        }),
    }
}

/// Runs the standard parse + prelude type-check pipeline on source code.
///
/// Returns `Ok((module, checker))` on success, or an `Err(JsonRpcResponse)` with
/// structured error information if parsing fails.
fn compile_source(
    id: &serde_json::Value,
    source: &str,
) -> Result<(kodo_ast::Module, kodo_types::TypeChecker), Box<JsonRpcResponse>> {
    let module = match kodo_parser::parse(source) {
        Ok(m) => m,
        Err(e) => {
            let error_json = serde_json::json!({
                "status": "failed",
                "phase": "parse",
                "errors": [{
                    "message": e.to_string(),
                    "code": e.code(),
                    "span": e.span().map(|s| {
                        serde_json::json!({"start": s.start, "end": s.end})
                    }),
                }],
            });
            return Err(Box::new(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: id.clone(),
                result: Some(error_json),
                error: None,
            }));
        }
    };

    // Load stdlib prelude (Option, Result).
    let mut checker = kodo_types::TypeChecker::new();
    for (_name, prelude_source) in kodo_std::prelude_sources() {
        if let Ok(prelude_mod) = kodo_parser::parse(prelude_source) {
            let _ = checker.check_module(&prelude_mod);
        }
    }

    Ok((module, checker))
}

/// Handles `kodo.check` — type-check source code, return structured errors + repair plans.
#[must_use]
pub fn handle_check(id: &serde_json::Value, args: &serde_json::Value) -> JsonRpcResponse {
    let Some(source) = args.get("source").and_then(|v| v.as_str()) else {
        return missing_param_error(id, "source");
    };

    let (module, mut checker) = match compile_source(id, source) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };

    let type_errors = checker.check_module_collecting(&module);

    if type_errors.is_empty() {
        check_contracts(id, &module)
    } else {
        build_type_error_response(id, &type_errors)
    }
}

/// Runs contract verification and returns the appropriate response.
fn check_contracts(id: &serde_json::Value, module: &kodo_ast::Module) -> JsonRpcResponse {
    let contract_mode = kodo_contracts::ContractMode::Runtime;
    let mut contract_errors: Vec<String> = Vec::new();
    for func in &module.functions {
        let contracts = kodo_contracts::extract_contracts(func);
        if let Err(e) = kodo_contracts::verify_contracts(&contracts, contract_mode) {
            contract_errors.push(e.to_string());
        }
    }

    if contract_errors.is_empty() {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: id.clone(),
            result: Some(serde_json::json!({
                "status": "ok",
                "errors": [],
                "module": module.name,
            })),
            error: None,
        }
    } else {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: id.clone(),
            result: Some(serde_json::json!({
                "status": "failed",
                "phase": "contracts",
                "errors": contract_errors.iter().map(|e| {
                    serde_json::json!({"message": e})
                }).collect::<Vec<_>>(),
            })),
            error: None,
        }
    }
}

/// Builds a JSON-RPC response from type errors.
fn build_type_error_response(
    id: &serde_json::Value,
    type_errors: &[kodo_types::TypeError],
) -> JsonRpcResponse {
    let errors: Vec<serde_json::Value> = type_errors
        .iter()
        .map(|e| {
            let mut err = serde_json::json!({
                "code": e.code(),
                "message": e.message(),
            });
            if let Some(span) = e.span() {
                err["span"] = serde_json::json!({
                    "start": span.start,
                    "end": span.end,
                });
            }
            if let Some(suggestion) = e.suggestion() {
                err["suggestion"] = serde_json::json!(suggestion);
            }
            if let Some(patch) = e.fix_patch() {
                err["fix_patch"] = serde_json::json!({
                    "start_offset": patch.start_offset,
                    "end_offset": patch.end_offset,
                    "replacement": patch.replacement,
                    "description": patch.description,
                });
            }
            err
        })
        .collect();

    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: id.clone(),
        result: Some(serde_json::json!({
            "status": "failed",
            "phase": "types",
            "errors": errors,
        })),
        error: None,
    }
}

/// Handles `kodo.describe` — return module metadata (functions, types, contracts).
#[must_use]
pub fn handle_describe(id: &serde_json::Value, args: &serde_json::Value) -> JsonRpcResponse {
    let Some(source) = args.get("source").and_then(|v| v.as_str()) else {
        return missing_param_error(id, "source");
    };

    let (module, mut checker) = match compile_source(id, source) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };

    // Type-check to populate metadata.
    let _ = checker.check_module_collecting(&module);

    let functions = describe_functions(&module);
    let types = describe_types(&module);
    let meta = describe_meta(&module);

    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: id.clone(),
        result: Some(serde_json::json!({
            "module": module.name,
            "meta": meta,
            "functions": functions,
            "types": types,
        })),
        error: None,
    }
}

/// Builds function descriptions for the describe response.
fn describe_functions(module: &kodo_ast::Module) -> Vec<serde_json::Value> {
    module
        .functions
        .iter()
        .map(|f| {
            let params: Vec<serde_json::Value> = f
                .params
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "name": p.name,
                        "type": format!("{:?}", p.ty),
                    })
                })
                .collect();

            let contracts: Vec<serde_json::Value> = f
                .requires
                .iter()
                .map(|r| serde_json::json!({"kind": "requires", "expr": format!("{r:?}")}))
                .chain(
                    f.ensures
                        .iter()
                        .map(|e| serde_json::json!({"kind": "ensures", "expr": format!("{e:?}")})),
                )
                .collect();

            let annotations: Vec<serde_json::Value> = f
                .annotations
                .iter()
                .map(|a| {
                    let args_str: Vec<String> =
                        a.args.iter().map(|arg| format!("{arg:?}")).collect();
                    serde_json::json!({
                        "name": a.name,
                        "args": args_str,
                    })
                })
                .collect();

            serde_json::json!({
                "name": f.name,
                "params": params,
                "return_type": format!("{:?}", f.return_type),
                "contracts": contracts,
                "annotations": annotations,
                "is_async": f.is_async,
            })
        })
        .collect()
}

/// Builds type descriptions for the describe response.
fn describe_types(module: &kodo_ast::Module) -> Vec<serde_json::Value> {
    module
        .type_decls
        .iter()
        .map(|td| {
            let fields: Vec<serde_json::Value> = td
                .fields
                .iter()
                .map(|f| {
                    serde_json::json!({
                        "name": f.name,
                        "type": format!("{:?}", f.ty),
                    })
                })
                .collect();
            serde_json::json!({
                "name": td.name,
                "fields": fields,
            })
        })
        .collect()
}

/// Builds meta block description for the describe response.
fn describe_meta(module: &kodo_ast::Module) -> serde_json::Value {
    module
        .meta
        .as_ref()
        .map(|m| {
            let entries: serde_json::Map<String, serde_json::Value> = m
                .entries
                .iter()
                .map(|entry| (entry.key.clone(), serde_json::json!(entry.value)))
                .collect();
            serde_json::Value::Object(entries)
        })
        .unwrap_or(serde_json::json!(null))
}

/// Handles `kodo.explain` — explain an error code.
///
/// Provides range-based explanations for error codes. For detailed per-code
/// explanations with examples, use `kodoc explain <code>`.
#[must_use]
pub fn handle_explain(id: &serde_json::Value, args: &serde_json::Value) -> JsonRpcResponse {
    let Some(code) = args.get("code").and_then(|v| v.as_str()) else {
        return missing_param_error(id, "code");
    };

    let (title, explanation) = match code {
        c if c.starts_with("E00") => (
            "Lexer Error",
            "This error occurred during tokenization. Check for invalid characters, unterminated strings, or malformed number literals.",
        ),
        c if c.starts_with("E01") => (
            "Parser Error",
            "This error occurred during parsing. Check for syntax errors such as missing braces, invalid expressions, or unexpected tokens.",
        ),
        c if c.starts_with("E02") => (
            "Type Error",
            "This error occurred during type checking. Check for type mismatches, undefined variables, missing return statements, or invalid operations.",
        ),
        c if c.starts_with("E03") => (
            "Contract Error",
            "This error is related to contract verification (requires/ensures). Check that preconditions and postconditions are satisfiable.",
        ),
        c if c.starts_with("E04") => (
            "Resolver Error",
            "This error occurred during intent resolution. Check that intent blocks have valid configurations.",
        ),
        c if c.starts_with("E05") => (
            "MIR Error",
            "This error occurred during MIR lowering. This may indicate an internal compiler issue.",
        ),
        c if c.starts_with("E06") => (
            "Codegen Error",
            "This error occurred during code generation. This may indicate an internal compiler issue.",
        ),
        _ => (
            "Unknown Error Code",
            "The specified error code is not recognized. Valid error codes range from E0001 to E0699. Use `kodoc explain <code>` for detailed explanations.",
        ),
    };

    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: id.clone(),
        result: Some(serde_json::json!({
            "code": code,
            "title": title,
            "explanation": explanation,
            "hint": "Use `kodoc explain <code>` for detailed examples and fix suggestions.",
        })),
        error: None,
    }
}

/// Handles `kodo.confidence_report` — return confidence scores for all functions.
#[must_use]
pub fn handle_confidence_report(
    id: &serde_json::Value,
    args: &serde_json::Value,
) -> JsonRpcResponse {
    let Some(source) = args.get("source").and_then(|v| v.as_str()) else {
        return missing_param_error(id, "source");
    };

    let (module, mut checker) = match compile_source(id, source) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };

    // Type-check before computing confidence.
    if let Err(e) = checker.check_module(&module) {
        return JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: id.clone(),
            result: Some(serde_json::json!({
                "status": "failed",
                "error": e.to_string(),
            })),
            error: None,
        };
    }

    let report = checker.confidence_report(&module);

    let functions: Vec<serde_json::Value> = report
        .iter()
        .map(|(name, declared, computed, callees)| {
            serde_json::json!({
                "name": name,
                "declared_confidence": declared,
                "computed_confidence": computed,
                "callees": callees,
            })
        })
        .collect();

    let overall = report
        .iter()
        .map(|(_, _, computed, _)| *computed)
        .fold(1.0_f64, f64::min);

    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: id.clone(),
        result: Some(serde_json::json!({
            "status": "ok",
            "functions": functions,
            "overall_confidence": overall,
        })),
        error: None,
    }
}
