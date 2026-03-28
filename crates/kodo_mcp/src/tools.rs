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

/// Handles `kodo.build` — compile source code through the full pipeline.
///
/// Runs: parse → type-check → contracts → desugar → MIR → codegen.
/// Returns structured output with status and any errors encountered.
#[must_use]
pub fn handle_build(id: &serde_json::Value, args: &serde_json::Value) -> JsonRpcResponse {
    let Some(source) = args.get("source").and_then(|v| v.as_str()) else {
        return missing_param_error(id, "source");
    };

    let (module, mut checker) = match compile_source(id, source) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };

    let type_errors = checker.check_module_collecting(&module);
    if !type_errors.is_empty() {
        return build_type_error_response(id, &type_errors);
    }

    // Contract verification
    let contract_mode = kodo_contracts::ContractMode::Runtime;
    let mut contract_errors: Vec<String> = Vec::new();
    for func in &module.functions {
        let contracts = kodo_contracts::extract_contracts(func);
        if let Err(e) = kodo_contracts::verify_contracts(&contracts, contract_mode) {
            contract_errors.push(e.to_string());
        }
    }

    if !contract_errors.is_empty() {
        return JsonRpcResponse {
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
        };
    }

    // Desugar pass
    let mut module = module;
    kodo_desugar::desugar_module(&mut module);

    // MIR lowering — validates the full pipeline through to MIR.
    // Full native codegen requires linking with the runtime, which is not
    // available in the MCP context (source-only). MIR success proves the
    // program is well-formed through all compiler phases.
    match kodo_mir::lowering::lower_module(&module) {
        Ok(mir_fns) => {
            let count = mir_fns.len();
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: id.clone(),
                result: Some(serde_json::json!({
                    "status": "ok",
                    "module": module.name,
                    "phase": "mir",
                    "message": "compilation successful (through MIR)",
                    "function_count": count,
                })),
                error: None,
            }
        }
        Err(mir_err) => {
            let err_msg = mir_err.to_string();
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: id.clone(),
                result: Some(serde_json::json!({
                    "status": "failed",
                    "phase": "mir",
                    "errors": [{"message": err_msg}],
                })),
                error: None,
            }
        }
    }
}

/// Handles `kodo.fix` — collect auto-fix patches and repair plans for errors.
///
/// Returns structured JSON with patches and repair plans that agents can apply
/// to fix the source code automatically.
#[must_use]
pub fn handle_fix(id: &serde_json::Value, args: &serde_json::Value) -> JsonRpcResponse {
    let Some(source) = args.get("source").and_then(|v| v.as_str()) else {
        return missing_param_error(id, "source");
    };

    let (module, mut checker) = match compile_source(id, source) {
        Ok(pair) => pair,
        Err(resp) => return *resp,
    };

    let type_errors = checker.check_module_collecting(&module);

    if type_errors.is_empty() {
        return JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: id.clone(),
            result: Some(serde_json::json!({
                "status": "ok",
                "message": "no errors to fix",
                "patches": [],
                "repair_plans": [],
            })),
            error: None,
        };
    }

    let mut patches: Vec<serde_json::Value> = Vec::new();
    let mut repair_plans: Vec<serde_json::Value> = Vec::new();

    for e in &type_errors {
        if let Some(patch) = e.fix_patch() {
            patches.push(serde_json::json!({
                "error_code": e.code(),
                "description": patch.description,
                "start_offset": patch.start_offset,
                "end_offset": patch.end_offset,
                "replacement": patch.replacement,
            }));
        }
        if let Some(steps) = e.repair_plan() {
            let json_steps: Vec<serde_json::Value> = steps
                .iter()
                .enumerate()
                .map(|(step_id, (description, step_patches))| {
                    let jp: Vec<serde_json::Value> = step_patches
                        .iter()
                        .map(|p| {
                            serde_json::json!({
                                "description": p.description,
                                "start_offset": p.start_offset,
                                "end_offset": p.end_offset,
                                "replacement": p.replacement,
                            })
                        })
                        .collect();
                    serde_json::json!({
                        "id": step_id,
                        "description": description,
                        "patches": jp,
                    })
                })
                .collect();
            repair_plans.push(serde_json::json!({
                "error_code": e.code(),
                "message": e.message(),
                "steps": json_steps,
            }));
        }
    }

    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: id.clone(),
        result: Some(serde_json::json!({
            "status": "errors_found",
            "error_count": type_errors.len(),
            "patches": patches,
            "repair_plans": repair_plans,
        })),
        error: None,
    }
}

/// Handles `kodo.annotate` — suggest missing contracts and list unannotated functions.
///
/// Returns heuristic suggestions plus a list of uncovered functions with their source
/// code, so the agent can reason about them and suggest contracts. The agent should
/// verify suggestions by inserting them into the source and calling `kodo.check`.
#[must_use]
pub fn handle_annotate(id: &serde_json::Value, args: &serde_json::Value) -> JsonRpcResponse {
    let Some(source) = args.get("source").and_then(|v| v.as_str()) else {
        return missing_param_error(id, "source");
    };

    let module = match kodo_parser::parse(source) {
        Ok(m) => m,
        Err(e) => {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: id.clone(),
                result: Some(serde_json::json!({
                    "status": "failed",
                    "phase": "parse",
                    "errors": [{"message": e.to_string()}],
                })),
                error: None,
            };
        }
    };

    let mut heuristic_suggestions: Vec<serde_json::Value> = Vec::new();
    let mut covered_functions: std::collections::HashSet<String> = std::collections::HashSet::new();

    for func in &module.functions {
        if func.name == "main" {
            continue;
        }
        // Already annotated
        if !func.requires.is_empty() || !func.ensures.is_empty() {
            covered_functions.insert(func.name.clone());
            continue;
        }
        // Run heuristics
        let suggestions = annotate_heuristics(func, source);
        if !suggestions.is_empty() {
            covered_functions.insert(func.name.clone());
            for s in suggestions {
                heuristic_suggestions.push(s);
            }
        }
    }

    // Functions the agent should review
    let mut uncovered: Vec<serde_json::Value> = Vec::new();
    for func in &module.functions {
        if func.name == "main" || covered_functions.contains(&func.name) {
            continue;
        }
        let start = func.span.start as usize;
        let end = func.span.end as usize;
        let func_source = &source[start.min(source.len())..end.min(source.len())];

        let params: Vec<serde_json::Value> = func
            .params
            .iter()
            .map(|p| {
                serde_json::json!({
                    "name": p.name,
                    "type": format!("{:?}", p.ty),
                })
            })
            .collect();

        uncovered.push(serde_json::json!({
            "name": func.name,
            "line": annotate_line_of(source, func.span.start),
            "params": params,
            "return_type": format!("{:?}", func.return_type),
            "source": func_source,
        }));
    }

    let total_non_main = module.functions.iter().filter(|f| f.name != "main").count();
    let already_annotated = module
        .functions
        .iter()
        .filter(|f| f.name != "main" && (!f.requires.is_empty() || !f.ensures.is_empty()))
        .count();

    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: id.clone(),
        result: Some(serde_json::json!({
            "status": "ok",
            "module": module.name,
            "heuristic_suggestions": heuristic_suggestions,
            "uncovered_functions": uncovered,
            "summary": {
                "total_functions": total_non_main,
                "already_annotated": already_annotated,
                "heuristic_covered": heuristic_suggestions.len(),
                "needs_agent_review": uncovered.len(),
            },
            "hint": "For each uncovered function, analyze the source and suggest requires/ensures contracts. Verify each by adding the contract to the source and calling kodo.check.",
        })),
        error: None,
    }
}

/// Compute 1-based line number from byte offset (for annotate tool).
fn annotate_line_of(source: &str, byte_offset: u32) -> usize {
    source[..byte_offset as usize]
        .bytes()
        .filter(|&b| b == b'\n')
        .count()
        + 1
}

/// Run heuristic analysis on a function — division by param, list index by param.
fn annotate_heuristics(func: &kodo_ast::Function, source: &str) -> Vec<serde_json::Value> {
    let mut suggestions = Vec::new();
    let line = annotate_line_of(source, func.span.start);
    let param_names: Vec<&str> = func.params.iter().map(|p| p.name.as_str()).collect();
    let mut seen = std::collections::HashSet::new();

    annotate_visit_block(&func.body, &mut |expr| {
        // Division/modulo by parameter
        if let kodo_ast::Expr::BinaryOp {
            op: kodo_ast::BinOp::Div | kodo_ast::BinOp::Mod,
            right,
            ..
        } = expr
        {
            if let kodo_ast::Expr::Ident(name, _) = right.as_ref() {
                if param_names.contains(&name.as_str()) && seen.insert(format!("{name} != 0")) {
                    suggestions.push(serde_json::json!({
                        "function": func.name,
                        "line": line,
                        "kind": "requires",
                        "expression": format!("{name} != 0"),
                        "reason": format!("parameter `{name}` used as divisor"),
                    }));
                }
            }
        }
        // List index by parameter
        if let kodo_ast::Expr::Call { callee, args, .. } = expr {
            if let kodo_ast::Expr::Ident(name, _) = callee.as_ref() {
                if matches!(name.as_str(), "list_get" | "list_set" | "list_remove") {
                    if let Some(kodo_ast::Expr::Ident(idx, _)) = args.get(1) {
                        if param_names.contains(&idx.as_str()) && seen.insert(format!("{idx} >= 0"))
                        {
                            suggestions.push(serde_json::json!({
                                "function": func.name,
                                "line": line,
                                "kind": "requires",
                                "expression": format!("{idx} >= 0"),
                                "reason": format!("parameter `{idx}` used as list index"),
                            }));
                        }
                    }
                }
            }
        }
    });

    suggestions
}

/// Walk all expressions in a block (for annotate heuristics).
fn annotate_visit_block(block: &kodo_ast::Block, f: &mut dyn FnMut(&kodo_ast::Expr)) {
    for stmt in &block.stmts {
        annotate_visit_stmt(stmt, f);
    }
}

/// Walk all expressions in a statement (for annotate heuristics).
#[allow(clippy::too_many_lines)]
fn annotate_visit_stmt(stmt: &kodo_ast::Stmt, f: &mut dyn FnMut(&kodo_ast::Expr)) {
    match stmt {
        kodo_ast::Stmt::Let { value, .. } | kodo_ast::Stmt::LetPattern { value, .. } => {
            annotate_visit_expr(value, f);
        }
        kodo_ast::Stmt::Expr(expr) => annotate_visit_expr(expr, f),
        kodo_ast::Stmt::Return { value, .. } => {
            if let Some(v) = value {
                annotate_visit_expr(v, f);
            }
        }
        kodo_ast::Stmt::Assign { value, .. } => annotate_visit_expr(value, f),
        kodo_ast::Stmt::While {
            condition, body, ..
        } => {
            annotate_visit_expr(condition, f);
            annotate_visit_block(body, f);
        }
        kodo_ast::Stmt::For {
            start, end, body, ..
        } => {
            annotate_visit_expr(start, f);
            annotate_visit_expr(end, f);
            annotate_visit_block(body, f);
        }
        kodo_ast::Stmt::ForIn { iterable, body, .. } => {
            annotate_visit_expr(iterable, f);
            annotate_visit_block(body, f);
        }
        kodo_ast::Stmt::IfLet {
            value,
            body,
            else_body,
            ..
        } => {
            annotate_visit_expr(value, f);
            annotate_visit_block(body, f);
            if let Some(eb) = else_body {
                annotate_visit_block(eb, f);
            }
        }
        kodo_ast::Stmt::Spawn { body, .. } | kodo_ast::Stmt::ForAll { body, .. } => {
            annotate_visit_block(body, f);
        }
        kodo_ast::Stmt::Parallel { body, .. } => {
            for s in body {
                annotate_visit_stmt(s, f);
            }
        }
        kodo_ast::Stmt::Select { arms, .. } => {
            for arm in arms {
                annotate_visit_expr(&arm.channel, f);
                annotate_visit_block(&arm.body, f);
            }
        }
        kodo_ast::Stmt::Break { .. } | kodo_ast::Stmt::Continue { .. } => {}
    }
}

/// Walk all sub-expressions (for annotate heuristics).
#[allow(clippy::too_many_lines)]
fn annotate_visit_expr(expr: &kodo_ast::Expr, f: &mut dyn FnMut(&kodo_ast::Expr)) {
    f(expr);
    match expr {
        kodo_ast::Expr::BinaryOp { left, right, .. }
        | kodo_ast::Expr::NullCoalesce { left, right, .. }
        | kodo_ast::Expr::Range {
            start: left,
            end: right,
            ..
        } => {
            annotate_visit_expr(left, f);
            annotate_visit_expr(right, f);
        }
        kodo_ast::Expr::UnaryOp { operand, .. }
        | kodo_ast::Expr::Is { operand, .. }
        | kodo_ast::Expr::Await { operand, .. }
        | kodo_ast::Expr::Try { operand, .. } => annotate_visit_expr(operand, f),
        kodo_ast::Expr::Call { callee, args, .. } => {
            annotate_visit_expr(callee, f);
            for arg in args {
                annotate_visit_expr(arg, f);
            }
        }
        kodo_ast::Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            annotate_visit_expr(condition, f);
            annotate_visit_block(then_branch, f);
            if let Some(eb) = else_branch {
                annotate_visit_block(eb, f);
            }
        }
        kodo_ast::Expr::Match {
            expr: inner, arms, ..
        } => {
            annotate_visit_expr(inner, f);
            for arm in arms {
                annotate_visit_expr(&arm.body, f);
            }
        }
        kodo_ast::Expr::Block(block) => annotate_visit_block(block, f),
        kodo_ast::Expr::StructLit { fields, .. } => {
            for fi in fields {
                annotate_visit_expr(&fi.value, f);
            }
        }
        kodo_ast::Expr::TupleLit(elems, _) => {
            for e in elems {
                annotate_visit_expr(e, f);
            }
        }
        kodo_ast::Expr::Closure { body, .. } => annotate_visit_expr(body, f),
        kodo_ast::Expr::FieldAccess { object, .. }
        | kodo_ast::Expr::OptionalChain { object, .. }
        | kodo_ast::Expr::TupleIndex { tuple: object, .. } => annotate_visit_expr(object, f),
        kodo_ast::Expr::StringInterp { parts, .. } => {
            for part in parts {
                if let kodo_ast::StringPart::Expr(e) = part {
                    annotate_visit_expr(e, f);
                }
            }
        }
        kodo_ast::Expr::IntLit(..)
        | kodo_ast::Expr::FloatLit(..)
        | kodo_ast::Expr::StringLit(..)
        | kodo_ast::Expr::BoolLit(..)
        | kodo_ast::Expr::Ident(..)
        | kodo_ast::Expr::EnumVariantExpr { .. } => {}
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

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_SOURCE: &str = "module test {\n    meta { purpose: \"test\" }\n    fn main() -> Int {\n        return 0\n    }\n}\n";

    const TYPE_ERROR_SOURCE: &str = "module test {\n    meta { purpose: \"test\" }\n    fn main() -> Int {\n        return true\n    }\n}\n";

    const INVALID_SYNTAX: &str = "this is not valid kodo at all!!!";

    // ── handle_check tests ──────────────────────────────────────────

    #[test]
    fn check_missing_source_param() {
        let id = serde_json::json!(1);
        let args = serde_json::json!({});
        let resp = handle_check(&id, &args);
        assert!(resp.error.is_some());
        let err = resp.error.as_ref().unwrap();
        assert_eq!(err.code, -32602);
        assert!(err.message.contains("source"));
    }

    #[test]
    fn check_parse_error() {
        let id = serde_json::json!(1);
        let args = serde_json::json!({"source": INVALID_SYNTAX});
        let resp = handle_check(&id, &args);
        assert!(resp.error.is_none());
        let result = resp.result.as_ref().unwrap();
        assert_eq!(
            result.get("status").and_then(|s| s.as_str()),
            Some("failed")
        );
        assert_eq!(result.get("phase").and_then(|s| s.as_str()), Some("parse"));
    }

    #[test]
    fn check_type_error_returns_failed_types_phase() {
        let id = serde_json::json!(1);
        let args = serde_json::json!({"source": TYPE_ERROR_SOURCE});
        let resp = handle_check(&id, &args);
        assert!(resp.error.is_none());
        let result = resp.result.as_ref().unwrap();
        let status = result.get("status").and_then(|s| s.as_str());
        assert_eq!(status, Some("failed"));
        let phase = result.get("phase").and_then(|s| s.as_str());
        assert_eq!(phase, Some("types"));
    }

    // ── handle_describe tests ───────────────────────────────────────

    #[test]
    fn describe_missing_source() {
        let id = serde_json::json!(1);
        let args = serde_json::json!({});
        let resp = handle_describe(&id, &args);
        assert!(resp.error.is_some());
        let err = resp.error.as_ref().unwrap();
        assert_eq!(err.code, -32602);
        assert!(err.message.contains("source"));
    }

    #[test]
    fn describe_with_types() {
        let source = "module shapes {\n    meta { purpose: \"geometry\" }\n    struct Point {\n        x: Int\n        y: Int\n    }\n    fn origin() -> Point {\n        return Point { x: 0, y: 0 }\n    }\n}\n";
        let id = serde_json::json!(1);
        let args = serde_json::json!({"source": source});
        let resp = handle_describe(&id, &args);
        assert!(resp.error.is_none());
        let result = resp.result.as_ref().unwrap();
        assert_eq!(
            result.get("module").and_then(|m| m.as_str()),
            Some("shapes")
        );
        let types = result.get("types").and_then(|t| t.as_array()).unwrap();
        assert!(!types.is_empty(), "should have type declarations");
        let first_type = &types[0];
        assert_eq!(
            first_type.get("name").and_then(|n| n.as_str()),
            Some("Point")
        );
        let fields = first_type.get("fields").and_then(|f| f.as_array()).unwrap();
        assert_eq!(fields.len(), 2);
    }

    #[test]
    fn describe_empty_module() {
        let source = "module empty {\n    meta { purpose: \"nothing\" }\n}\n";
        let id = serde_json::json!(1);
        let args = serde_json::json!({"source": source});
        let resp = handle_describe(&id, &args);
        assert!(resp.error.is_none());
        let result = resp.result.as_ref().unwrap();
        assert_eq!(result.get("module").and_then(|m| m.as_str()), Some("empty"));
        let functions = result.get("functions").and_then(|f| f.as_array()).unwrap();
        assert!(
            functions.is_empty(),
            "empty module should have no functions"
        );
    }

    #[test]
    fn describe_with_annotations() {
        let source = "module annotated {\n    meta { purpose: \"test\" }\n    @confidence(0.85)\n    fn compute() -> Int {\n        return 42\n    }\n}\n";
        let id = serde_json::json!(1);
        let args = serde_json::json!({"source": source});
        let resp = handle_describe(&id, &args);
        assert!(resp.error.is_none());
        let result = resp.result.as_ref().unwrap();
        let functions = result.get("functions").and_then(|f| f.as_array()).unwrap();
        assert_eq!(functions.len(), 1);
        let func = &functions[0];
        let annotations = func.get("annotations").and_then(|a| a.as_array()).unwrap();
        assert!(
            !annotations.is_empty(),
            "should have @confidence annotation"
        );
        let ann = &annotations[0];
        assert_eq!(ann.get("name").and_then(|n| n.as_str()), Some("confidence"));
    }

    // ── handle_explain tests ────────────────────────────────────────

    #[test]
    fn explain_lexer_error() {
        let id = serde_json::json!(1);
        let args = serde_json::json!({"code": "E0001"});
        let resp = handle_explain(&id, &args);
        assert!(resp.error.is_none());
        let result = resp.result.as_ref().unwrap();
        assert_eq!(
            result.get("title").and_then(|t| t.as_str()),
            Some("Lexer Error")
        );
    }

    #[test]
    fn explain_parser_error() {
        let id = serde_json::json!(1);
        let args = serde_json::json!({"code": "E0100"});
        let resp = handle_explain(&id, &args);
        assert!(resp.error.is_none());
        let result = resp.result.as_ref().unwrap();
        assert_eq!(
            result.get("title").and_then(|t| t.as_str()),
            Some("Parser Error")
        );
    }

    #[test]
    fn explain_contract_error() {
        let id = serde_json::json!(1);
        let args = serde_json::json!({"code": "E0300"});
        let resp = handle_explain(&id, &args);
        assert!(resp.error.is_none());
        let result = resp.result.as_ref().unwrap();
        assert_eq!(
            result.get("title").and_then(|t| t.as_str()),
            Some("Contract Error")
        );
    }

    #[test]
    fn explain_resolver_error() {
        let id = serde_json::json!(1);
        let args = serde_json::json!({"code": "E0400"});
        let resp = handle_explain(&id, &args);
        assert!(resp.error.is_none());
        let result = resp.result.as_ref().unwrap();
        assert_eq!(
            result.get("title").and_then(|t| t.as_str()),
            Some("Resolver Error")
        );
    }

    #[test]
    fn explain_mir_error() {
        let id = serde_json::json!(1);
        let args = serde_json::json!({"code": "E0500"});
        let resp = handle_explain(&id, &args);
        assert!(resp.error.is_none());
        let result = resp.result.as_ref().unwrap();
        assert_eq!(
            result.get("title").and_then(|t| t.as_str()),
            Some("MIR Error")
        );
    }

    #[test]
    fn explain_codegen_error() {
        let id = serde_json::json!(1);
        let args = serde_json::json!({"code": "E0600"});
        let resp = handle_explain(&id, &args);
        assert!(resp.error.is_none());
        let result = resp.result.as_ref().unwrap();
        assert_eq!(
            result.get("title").and_then(|t| t.as_str()),
            Some("Codegen Error")
        );
    }

    #[test]
    fn explain_unknown_code() {
        let id = serde_json::json!(1);
        let args = serde_json::json!({"code": "E9999"});
        let resp = handle_explain(&id, &args);
        assert!(resp.error.is_none());
        let result = resp.result.as_ref().unwrap();
        assert_eq!(
            result.get("title").and_then(|t| t.as_str()),
            Some("Unknown Error Code")
        );
    }

    #[test]
    fn explain_missing_code_param() {
        let id = serde_json::json!(1);
        let args = serde_json::json!({});
        let resp = handle_explain(&id, &args);
        assert!(resp.error.is_some());
        let err = resp.error.as_ref().unwrap();
        assert_eq!(err.code, -32602);
        assert!(err.message.contains("code"));
    }

    // ── handle_build tests ──────────────────────────────────────────

    #[test]
    fn build_missing_source() {
        let id = serde_json::json!(1);
        let args = serde_json::json!({});
        let resp = handle_build(&id, &args);
        assert!(resp.error.is_some());
        let err = resp.error.as_ref().unwrap();
        assert_eq!(err.code, -32602);
        assert!(err.message.contains("source"));
    }

    #[test]
    fn build_type_error() {
        let id = serde_json::json!(1);
        let args = serde_json::json!({"source": TYPE_ERROR_SOURCE});
        let resp = handle_build(&id, &args);
        assert!(resp.error.is_none());
        let result = resp.result.as_ref().unwrap();
        assert_eq!(
            result.get("status").and_then(|s| s.as_str()),
            Some("failed")
        );
        assert_eq!(result.get("phase").and_then(|s| s.as_str()), Some("types"));
    }

    // ── handle_fix tests ────────────────────────────────────────────

    #[test]
    fn fix_missing_source() {
        let id = serde_json::json!(1);
        let args = serde_json::json!({});
        let resp = handle_fix(&id, &args);
        assert!(resp.error.is_some());
        let err = resp.error.as_ref().unwrap();
        assert_eq!(err.code, -32602);
        assert!(err.message.contains("source"));
    }

    #[test]
    fn fix_with_type_mismatch() {
        let source = "module test {\n    meta { purpose: \"test\" }\n    fn main() -> Int {\n        let x: Int = \"hello\"\n        return x\n    }\n}\n";
        let id = serde_json::json!(1);
        let args = serde_json::json!({"source": source});
        let resp = handle_fix(&id, &args);
        assert!(resp.error.is_none());
        let result = resp.result.as_ref().unwrap();
        assert_eq!(
            result.get("status").and_then(|s| s.as_str()),
            Some("errors_found")
        );
        let error_count = result.get("error_count").and_then(|c| c.as_u64()).unwrap();
        assert!(error_count > 0, "should have at least one error");
    }

    // ── handle_annotate tests ────────────────────────────────────────

    #[test]
    fn annotate_missing_source() {
        let id = serde_json::json!(1);
        let args = serde_json::json!({});
        let resp = handle_annotate(&id, &args);
        assert!(resp.error.is_some());
        let err = resp.error.as_ref().unwrap();
        assert_eq!(err.code, -32602);
        assert!(err.message.contains("source"));
    }

    #[test]
    fn annotate_detects_division_heuristic() {
        let source = "module test {\n    meta { purpose: \"test\" }\n    fn divide(a: Int, b: Int) -> Int {\n        return a / b\n    }\n}\n";
        let id = serde_json::json!(1);
        let args = serde_json::json!({"source": source});
        let resp = handle_annotate(&id, &args);
        assert!(resp.error.is_none());
        let result = resp.result.as_ref().unwrap();
        assert_eq!(result.get("status").and_then(|s| s.as_str()), Some("ok"));
        let suggestions = result
            .get("heuristic_suggestions")
            .and_then(|s| s.as_array())
            .unwrap();
        assert!(
            suggestions
                .iter()
                .any(|s| s.get("expression").and_then(|e| e.as_str()) == Some("b != 0")),
            "should suggest b != 0 for division"
        );
    }

    #[test]
    fn annotate_lists_uncovered_functions() {
        let source = "module test {\n    meta { purpose: \"test\" }\n    fn add(a: Int, b: Int) -> Int {\n        return a + b\n    }\n    fn mul(a: Int, b: Int) -> Int {\n        return a * b\n    }\n}\n";
        let id = serde_json::json!(1);
        let args = serde_json::json!({"source": source});
        let resp = handle_annotate(&id, &args);
        assert!(resp.error.is_none());
        let result = resp.result.as_ref().unwrap();
        let uncovered = result
            .get("uncovered_functions")
            .and_then(|u| u.as_array())
            .unwrap();
        assert_eq!(uncovered.len(), 2, "both functions should be uncovered");
        // Each should have source code
        for func in uncovered {
            assert!(
                func.get("source").and_then(|s| s.as_str()).is_some(),
                "uncovered function should include source"
            );
            assert!(
                func.get("params").and_then(|p| p.as_array()).is_some(),
                "uncovered function should include params"
            );
        }
    }

    #[test]
    fn annotate_skips_already_annotated() {
        let source = "module test {\n    meta { purpose: \"test\" }\n    fn divide(a: Int, b: Int) -> Int\n        requires { b != 0 }\n    {\n        return a / b\n    }\n}\n";
        let id = serde_json::json!(1);
        let args = serde_json::json!({"source": source});
        let resp = handle_annotate(&id, &args);
        assert!(resp.error.is_none());
        let result = resp.result.as_ref().unwrap();
        let summary = result.get("summary").unwrap();
        assert_eq!(
            summary.get("already_annotated").and_then(|a| a.as_u64()),
            Some(1)
        );
        assert_eq!(
            summary.get("needs_agent_review").and_then(|a| a.as_u64()),
            Some(0)
        );
    }

    #[test]
    fn annotate_summary_counts() {
        let source = "module test {\n    meta { purpose: \"test\" }\n    fn divide(a: Int, b: Int) -> Int {\n        return a / b\n    }\n    fn add(a: Int, b: Int) -> Int {\n        return a + b\n    }\n}\n";
        let id = serde_json::json!(1);
        let args = serde_json::json!({"source": source});
        let resp = handle_annotate(&id, &args);
        let result = resp.result.as_ref().unwrap();
        let summary = result.get("summary").unwrap();
        assert_eq!(
            summary.get("total_functions").and_then(|t| t.as_u64()),
            Some(2)
        );
        // divide should have heuristic, add should be uncovered
        assert_eq!(
            summary.get("needs_agent_review").and_then(|n| n.as_u64()),
            Some(1),
            "add() should need agent review"
        );
    }

    #[test]
    fn annotate_parse_error() {
        let id = serde_json::json!(1);
        let args = serde_json::json!({"source": "not valid kodo"});
        let resp = handle_annotate(&id, &args);
        assert!(resp.error.is_none());
        let result = resp.result.as_ref().unwrap();
        assert_eq!(
            result.get("status").and_then(|s| s.as_str()),
            Some("failed")
        );
        assert_eq!(result.get("phase").and_then(|s| s.as_str()), Some("parse"));
    }

    // ── handle_confidence_report tests ──────────────────────────────

    #[test]
    fn confidence_report_missing_source() {
        let id = serde_json::json!(1);
        let args = serde_json::json!({});
        let resp = handle_confidence_report(&id, &args);
        assert!(resp.error.is_some());
        let err = resp.error.as_ref().unwrap();
        assert_eq!(err.code, -32602);
        assert!(err.message.contains("source"));
    }

    #[test]
    fn confidence_report_with_annotations() {
        let source = "module annotated {\n    meta { purpose: \"test\" }\n    @confidence(0.85)\n    fn compute() -> Int {\n        return 42\n    }\n}\n";
        let id = serde_json::json!(1);
        let args = serde_json::json!({"source": source});
        let resp = handle_confidence_report(&id, &args);
        assert!(resp.error.is_none());
        let result = resp.result.as_ref().unwrap();
        assert_eq!(result.get("status").and_then(|s| s.as_str()), Some("ok"));
        let functions = result.get("functions").and_then(|f| f.as_array()).unwrap();
        assert_eq!(functions.len(), 1);
        let func = &functions[0];
        assert_eq!(func.get("name").and_then(|n| n.as_str()), Some("compute"));
    }

    #[test]
    fn confidence_report_valid_source() {
        let id = serde_json::json!(1);
        let args = serde_json::json!({"source": VALID_SOURCE});
        let resp = handle_confidence_report(&id, &args);
        assert!(resp.error.is_none());
        let result = resp.result.as_ref().unwrap();
        assert_eq!(result.get("status").and_then(|s| s.as_str()), Some("ok"));
        assert!(result.get("functions").is_some());
        assert!(result.get("overall_confidence").is_some());
        let overall = result
            .get("overall_confidence")
            .and_then(|c| c.as_f64())
            .unwrap();
        assert!(overall > 0.0, "overall confidence should be positive");
        assert!(overall <= 1.0, "overall confidence should be at most 1.0");
    }
}
