//! Shared helper functions for intent config extraction and AST construction.
//!
//! These utilities are used by multiple resolver strategies to extract
//! configuration values from intent declarations and to build common
//! AST patterns (function declarations, if-chains, let bindings, etc.).

use kodo_ast::{
    Block, Expr, Function, IntentConfigValue, IntentDecl, NodeId, Span, Stmt, TypeExpr, Visibility,
};

// ===== Config extraction helpers =====

/// Extracts a string value from an intent config entry.
pub(crate) fn get_string_config<'a>(intent: &'a IntentDecl, key: &str) -> Option<&'a str> {
    for entry in &intent.config {
        if entry.key == key {
            if let IntentConfigValue::StringLit(ref s, _) = entry.value {
                return Some(s.as_str());
            }
        }
    }
    None
}

/// Extracts an integer value from an intent config entry.
pub(crate) fn get_int_config(intent: &IntentDecl, key: &str) -> Option<i64> {
    for entry in &intent.config {
        if entry.key == key {
            if let IntentConfigValue::IntLit(n, _) = entry.value {
                return Some(n);
            }
        }
    }
    None
}

/// Extracts a list of string values from an intent config entry.
///
/// Handles both `StringLit` and `FnRef` list items, treating `FnRef` names as strings.
pub(crate) fn get_string_list_config(intent: &IntentDecl, key: &str) -> Vec<String> {
    for entry in &intent.config {
        if entry.key == key {
            if let IntentConfigValue::List(ref items, _) = entry.value {
                return items
                    .iter()
                    .filter_map(|item| match item {
                        IntentConfigValue::StringLit(s, _) | IntentConfigValue::FnRef(s, _) => {
                            Some(s.clone())
                        }
                        _ => None,
                    })
                    .collect();
            }
        }
    }
    Vec::new()
}

/// Extracts a function reference name from an intent config entry.
pub(crate) fn get_fn_ref_config<'a>(intent: &'a IntentDecl, key: &str) -> Option<&'a str> {
    for entry in &intent.config {
        if entry.key == key {
            if let IntentConfigValue::FnRef(ref s, _) = entry.value {
                return Some(s.as_str());
            }
        }
    }
    None
}

/// Extracts nested lists (list of lists) from an intent config entry.
///
/// Each inner list is a `Vec<IntentConfigValue>`. Non-list items are skipped.
pub(crate) fn get_nested_list_config(
    intent: &IntentDecl,
    key: &str,
) -> Vec<Vec<IntentConfigValue>> {
    for entry in &intent.config {
        if entry.key == key {
            if let IntentConfigValue::List(ref items, _) = entry.value {
                return items
                    .iter()
                    .filter_map(|item| {
                        if let IntentConfigValue::List(ref inner, _) = item {
                            Some(inner.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
            }
        }
    }
    Vec::new()
}

/// Extracts a string from an `IntentConfigValue`.
pub(crate) fn config_value_as_str(v: &IntentConfigValue) -> Option<&str> {
    match v {
        IntentConfigValue::StringLit(s, _) | IntentConfigValue::FnRef(s, _) => Some(s.as_str()),
        _ => None,
    }
}

// ===== AST construction helpers =====

/// Builds a `let name: type = expr` statement.
pub(crate) fn make_let(name: &str, ty: TypeExpr, value: Expr, span: Span) -> Stmt {
    Stmt::Let {
        span,
        mutable: false,
        name: name.to_string(),
        ty: Some(ty),
        value,
    }
}

/// Builds a function call expression.
pub(crate) fn make_call(callee: &str, args: Vec<Expr>, span: Span) -> Expr {
    Expr::Call {
        callee: Box::new(Expr::Ident(callee.to_string(), span)),
        args,
        span,
    }
}

/// Builds `println(msg)` as a statement.
pub(crate) fn make_println(msg: &str, span: Span) -> Stmt {
    Stmt::Expr(make_call(
        "println",
        vec![Expr::StringLit(msg.to_string(), span)],
        span,
    ))
}

/// Builds a simple function with given body statements and return type.
pub(crate) fn make_function(
    name: &str,
    return_type: TypeExpr,
    body: Vec<Stmt>,
    span: Span,
) -> Function {
    Function {
        id: NodeId(0),
        span,
        name: name.to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![],
        return_type,
        requires: vec![],
        ensures: vec![],
        body: Block { span, stmts: body },
    }
}

/// Builds an if-else chain from a list of (condition, body) pairs and an else body.
pub(crate) fn make_if_chain(
    branches: Vec<(Expr, Vec<Stmt>)>,
    else_body: &[Stmt],
    span: Span,
) -> Expr {
    let mut result: Option<Expr> = None;

    // Build from the end backwards
    for (condition, body) in branches.into_iter().rev() {
        let else_branch = if let Some(inner) = result.take() {
            Some(Block {
                span,
                stmts: vec![Stmt::Expr(inner)],
            })
        } else if else_body.is_empty() {
            None
        } else {
            Some(Block {
                span,
                stmts: else_body.to_vec(),
            })
        };

        result = Some(Expr::If {
            condition: Box::new(condition),
            then_branch: Block { span, stmts: body },
            else_branch,
            span,
        });
    }

    result.unwrap_or(Expr::IntLit(0, span))
}

/// Emits output statements based on the output mode.
pub(crate) fn emit_output(stmts: &mut Vec<Stmt>, output_mode: &str, span: Span) {
    match output_mode {
        "file" => {
            // file_write("output.txt", result)
            stmts.push(Stmt::Expr(make_call(
                "file_write",
                vec![
                    Expr::StringLit("output.txt".to_string(), span),
                    Expr::Ident("result".to_string(), span),
                ],
                span,
            )));
        }
        _ => {
            // println(result)
            stmts.push(Stmt::Expr(make_call(
                "println",
                vec![Expr::Ident("result".to_string(), span)],
                span,
            )));
        }
    }
}

/// Builds a single route branch: `if method == M && path == P { resp = handler(); respond(req, 200, resp) }`.
pub(crate) fn make_route_branch(
    method: &str,
    path: &str,
    handler: &str,
    span: Span,
) -> (Expr, Vec<Stmt>) {
    let method_cmp = Expr::BinaryOp {
        op: kodo_ast::BinOp::Eq,
        left: Box::new(Expr::Ident("method".to_string(), span)),
        right: Box::new(Expr::StringLit(method.to_string(), span)),
        span,
    };
    let path_cmp = Expr::BinaryOp {
        op: kodo_ast::BinOp::Eq,
        left: Box::new(Expr::Ident("path".to_string(), span)),
        right: Box::new(Expr::StringLit(path.to_string(), span)),
        span,
    };
    let condition = Expr::BinaryOp {
        op: kodo_ast::BinOp::And,
        left: Box::new(method_cmp),
        right: Box::new(path_cmp),
        span,
    };
    let body = vec![
        make_let(
            "resp",
            TypeExpr::Named("String".to_string()),
            make_call(handler, vec![], span),
            span,
        ),
        Stmt::Expr(make_call(
            "http_respond",
            vec![
                Expr::Ident("req".to_string(), span),
                Expr::IntLit(200, span),
                Expr::Ident("resp".to_string(), span),
            ],
            span,
        )),
    ];
    (condition, body)
}

/// Generates `kodo_main()` with HTTP server loop and route dispatch.
///
/// Shared between `HttpServerStrategy` and `JsonApiStrategy` (endpoint mode).
pub(crate) fn generate_http_server_main(
    port: i64,
    not_found: &str,
    routes: &[(String, String, String)],
    span: Span,
) -> Function {
    let mut stmts = Vec::new();

    // let server: Int = http_server_new(port)
    stmts.push(make_let(
        "server",
        TypeExpr::Named("Int".to_string()),
        make_call("http_server_new", vec![Expr::IntLit(port, span)], span),
        span,
    ));

    // println("HTTP server listening on port {port}")
    stmts.push(make_println(
        &format!("HTTP server listening on port {port}"),
        span,
    ));

    // while true { ... }
    let mut loop_stmts = Vec::new();

    // let req: Int = http_server_recv(server)
    loop_stmts.push(make_let(
        "req",
        TypeExpr::Named("Int".to_string()),
        make_call(
            "http_server_recv",
            vec![Expr::Ident("server".to_string(), span)],
            span,
        ),
        span,
    ));

    // let method: String = http_request_method(req)
    loop_stmts.push(make_let(
        "method",
        TypeExpr::Named("String".to_string()),
        make_call(
            "http_request_method",
            vec![Expr::Ident("req".to_string(), span)],
            span,
        ),
        span,
    ));

    // let path: String = http_request_path(req)
    loop_stmts.push(make_let(
        "path",
        TypeExpr::Named("String".to_string()),
        make_call(
            "http_request_path",
            vec![Expr::Ident("req".to_string(), span)],
            span,
        ),
        span,
    ));

    // Build if-else chain for routes
    let branches: Vec<(Expr, Vec<Stmt>)> = routes
        .iter()
        .map(|(method, path, handler)| make_route_branch(method, path, handler, span))
        .collect();

    // Else: 404
    let else_body = vec![Stmt::Expr(make_call(
        "http_respond",
        vec![
            Expr::Ident("req".to_string(), span),
            Expr::IntLit(404, span),
            Expr::StringLit(not_found.to_string(), span),
        ],
        span,
    ))];

    if branches.is_empty() {
        // No routes: respond 404 to everything
        loop_stmts.extend(else_body);
    } else {
        loop_stmts.push(Stmt::Expr(make_if_chain(branches, &else_body, span)));
    }

    stmts.push(Stmt::While {
        span,
        condition: Expr::BoolLit(true, span),
        body: Block {
            span,
            stmts: loop_stmts,
        },
    });

    make_function("kodo_main", TypeExpr::Named("Int".to_string()), stmts, span)
}
