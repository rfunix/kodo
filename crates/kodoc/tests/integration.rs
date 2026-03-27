//! Integration tests for the kodoc compiler pipeline.
//!
//! These tests exercise the full compilation pipeline from source text
//! through parsing, type checking, contract verification, and MIR lowering.

use std::path::Path;

/// Helper to read a fixture file from the tests directory.
fn read_fixture(relative_path: &str) -> String {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let fixture_path = workspace_root.join("tests/fixtures").join(relative_path);
    std::fs::read_to_string(&fixture_path)
        .unwrap_or_else(|e| panic!("could not read fixture {}: {e}", fixture_path.display()))
}

/// Runs the full pipeline (parse → type check → contracts → desugar → trait rewriting → MIR)
/// on a source string.
/// Returns Ok(()) on success, Err(String) with the error message on failure.
fn run_full_pipeline(source: &str) -> Result<(), String> {
    let module = kodo_parser::parse(source).map_err(|e| format!("parse error: {e}"))?;

    let mut checker = kodo_types::TypeChecker::new();
    // Load stdlib prelude (Option, Result).
    for (_name, prelude_source) in kodo_std::prelude_sources() {
        if let Ok(prelude_mod) = kodo_parser::parse(prelude_source) {
            let _ = checker.check_module(&prelude_mod);
        }
    }
    checker
        .check_module(&module)
        .map_err(|e| format!("type error: {e}"))?;

    for func in &module.functions {
        let contracts = kodo_contracts::extract_contracts(func);
        kodo_contracts::verify_contracts(&contracts, kodo_contracts::ContractMode::Runtime)
            .map_err(|e| format!("contract error: {e}"))?;
    }
    for impl_block in &module.impl_blocks {
        for method in &impl_block.methods {
            let contracts = kodo_contracts::extract_contracts(method);
            kodo_contracts::verify_contracts(&contracts, kodo_contracts::ContractMode::Runtime)
                .map_err(|e| format!("contract error: {e}"))?;
        }
    }

    // Desugar pass
    let mut module = module;
    kodo_desugar::desugar_module(&mut module);

    // Lift impl block methods to top-level functions with mangled names.
    for impl_block in &module.impl_blocks {
        for method in &impl_block.methods {
            let mut func = method.clone();
            func.name = format!("{}_{}", impl_block.type_name, method.name);
            for param in &mut func.params {
                if param.name == "self" {
                    param.ty = kodo_ast::TypeExpr::Named(impl_block.type_name.clone());
                }
            }
            module.functions.push(func);
        }
    }

    // Rewrite method calls using span-based resolutions from the type checker.
    let method_resolutions = checker.method_resolutions().clone();
    let static_method_calls = checker.static_method_calls().clone();
    if !method_resolutions.is_empty() {
        for func in &mut module.functions {
            rewrite_method_calls_in_block(
                &mut func.body,
                &method_resolutions,
                &static_method_calls,
            );
        }
        // Also rewrite method calls inside actor handler bodies.
        for actor_decl in &mut module.actor_decls {
            for handler in &mut actor_decl.handlers {
                rewrite_method_calls_in_block(
                    &mut handler.body,
                    &method_resolutions,
                    &static_method_calls,
                );
            }
        }
    }

    kodo_mir::lowering::lower_module(&module).map_err(|e| format!("MIR error: {e}"))?;

    Ok(())
}

/// Rewrites method calls in a block by replacing `obj.method(args)` with
/// `TypeName_method(obj, args)` where a method call was resolved during type checking.
fn rewrite_method_calls_in_block(
    block: &mut kodo_ast::Block,
    resolutions: &std::collections::HashMap<u32, String>,
    static_calls: &std::collections::HashSet<u32>,
) {
    for stmt in &mut block.stmts {
        match stmt {
            kodo_ast::Stmt::Let { value, .. } | kodo_ast::Stmt::Assign { value, .. } => {
                *value = rewrite_method_calls_in_expr(
                    std::mem::replace(value, kodo_ast::Expr::IntLit(0, kodo_ast::Span::new(0, 0))),
                    resolutions,
                    static_calls,
                );
            }
            kodo_ast::Stmt::Expr(expr) => {
                *expr = rewrite_method_calls_in_expr(
                    std::mem::replace(expr, kodo_ast::Expr::IntLit(0, kodo_ast::Span::new(0, 0))),
                    resolutions,
                    static_calls,
                );
            }
            kodo_ast::Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    *v = rewrite_method_calls_in_expr(
                        std::mem::replace(v, kodo_ast::Expr::IntLit(0, kodo_ast::Span::new(0, 0))),
                        resolutions,
                        static_calls,
                    );
                }
            }
            kodo_ast::Stmt::While {
                condition, body, ..
            } => {
                *condition = rewrite_method_calls_in_expr(
                    std::mem::replace(
                        condition,
                        kodo_ast::Expr::IntLit(0, kodo_ast::Span::new(0, 0)),
                    ),
                    resolutions,
                    static_calls,
                );
                rewrite_method_calls_in_block(body, resolutions, static_calls);
            }
            kodo_ast::Stmt::For {
                start, end, body, ..
            } => {
                *start = rewrite_method_calls_in_expr(
                    std::mem::replace(start, kodo_ast::Expr::IntLit(0, kodo_ast::Span::new(0, 0))),
                    resolutions,
                    static_calls,
                );
                *end = rewrite_method_calls_in_expr(
                    std::mem::replace(end, kodo_ast::Expr::IntLit(0, kodo_ast::Span::new(0, 0))),
                    resolutions,
                    static_calls,
                );
                rewrite_method_calls_in_block(body, resolutions, static_calls);
            }
            kodo_ast::Stmt::ForIn { iterable, body, .. } => {
                *iterable = rewrite_method_calls_in_expr(
                    std::mem::replace(
                        iterable,
                        kodo_ast::Expr::IntLit(0, kodo_ast::Span::new(0, 0)),
                    ),
                    resolutions,
                    static_calls,
                );
                rewrite_method_calls_in_block(body, resolutions, static_calls);
            }
            kodo_ast::Stmt::IfLet {
                value,
                body,
                else_body,
                ..
            } => {
                *value = rewrite_method_calls_in_expr(
                    std::mem::replace(value, kodo_ast::Expr::IntLit(0, kodo_ast::Span::new(0, 0))),
                    resolutions,
                    static_calls,
                );
                rewrite_method_calls_in_block(body, resolutions, static_calls);
                if let Some(eb) = else_body {
                    rewrite_method_calls_in_block(eb, resolutions, static_calls);
                }
            }
            kodo_ast::Stmt::LetPattern { value, .. } => {
                *value = rewrite_method_calls_in_expr(
                    std::mem::replace(value, kodo_ast::Expr::IntLit(0, kodo_ast::Span::new(0, 0))),
                    resolutions,
                    static_calls,
                );
            }
            kodo_ast::Stmt::Spawn { body, .. } => {
                rewrite_method_calls_in_block(body, resolutions, static_calls);
            }
            kodo_ast::Stmt::Parallel { body, .. } => {
                for stmt in body {
                    if let kodo_ast::Stmt::Spawn { body, .. } = stmt {
                        rewrite_method_calls_in_block(body, resolutions, static_calls);
                    }
                }
            }
            // Break and Continue have no expressions to rewrite.
            kodo_ast::Stmt::Break { .. } | kodo_ast::Stmt::Continue { .. } => {}
            // Select arms: rewrite channel exprs and body blocks.
            kodo_ast::Stmt::Select { arms, .. } => {
                for arm in arms {
                    arm.channel = rewrite_method_calls_in_expr(
                        std::mem::replace(
                            &mut arm.channel,
                            kodo_ast::Expr::IntLit(0, kodo_ast::Span::new(0, 0)),
                        ),
                        resolutions,
                        static_calls,
                    );
                    rewrite_method_calls_in_block(&mut arm.body, resolutions, static_calls);
                }
            }
            // ForAll body is recursively rewritten.
            kodo_ast::Stmt::ForAll { body, .. } => {
                rewrite_method_calls_in_block(body, resolutions, static_calls);
            }
        }
    }
}

/// Rewrites method calls in an expression using span-based resolutions.
fn rewrite_method_calls_in_expr(
    expr: kodo_ast::Expr,
    resolutions: &std::collections::HashMap<u32, String>,
    static_calls: &std::collections::HashSet<u32>,
) -> kodo_ast::Expr {
    match expr {
        kodo_ast::Expr::Call { callee, args, span } => {
            if let kodo_ast::Expr::FieldAccess {
                object,
                field,
                span: fa_span,
            } = *callee
            {
                let object = rewrite_method_calls_in_expr(*object, resolutions, static_calls);
                let args: Vec<_> = args
                    .into_iter()
                    .map(|a| rewrite_method_calls_in_expr(a, resolutions, static_calls))
                    .collect();

                if let Some(mangled) = resolutions.get(&span.start) {
                    let new_args = if static_calls.contains(&span.start) {
                        args
                    } else {
                        let mut with_self = vec![object];
                        with_self.extend(args);
                        with_self
                    };
                    return kodo_ast::Expr::Call {
                        callee: Box::new(kodo_ast::Expr::Ident(mangled.clone(), span)),
                        args: new_args,
                        span,
                    };
                }

                kodo_ast::Expr::Call {
                    callee: Box::new(kodo_ast::Expr::FieldAccess {
                        object: Box::new(object),
                        field,
                        span: fa_span,
                    }),
                    args,
                    span,
                }
            } else {
                let callee = rewrite_method_calls_in_expr(*callee, resolutions, static_calls);
                let args: Vec<_> = args
                    .into_iter()
                    .map(|a| rewrite_method_calls_in_expr(a, resolutions, static_calls))
                    .collect();
                kodo_ast::Expr::Call {
                    callee: Box::new(callee),
                    args,
                    span,
                }
            }
        }
        kodo_ast::Expr::BinaryOp {
            left,
            op,
            right,
            span,
        } => kodo_ast::Expr::BinaryOp {
            left: Box::new(rewrite_method_calls_in_expr(
                *left,
                resolutions,
                static_calls,
            )),
            op,
            right: Box::new(rewrite_method_calls_in_expr(
                *right,
                resolutions,
                static_calls,
            )),
            span,
        },
        kodo_ast::Expr::UnaryOp { op, operand, span } => kodo_ast::Expr::UnaryOp {
            op,
            operand: Box::new(rewrite_method_calls_in_expr(
                *operand,
                resolutions,
                static_calls,
            )),
            span,
        },
        kodo_ast::Expr::If {
            condition,
            mut then_branch,
            else_branch,
            span,
        } => {
            let condition = rewrite_method_calls_in_expr(*condition, resolutions, static_calls);
            rewrite_method_calls_in_block(&mut then_branch, resolutions, static_calls);
            let else_branch = else_branch.map(|mut b| {
                rewrite_method_calls_in_block(&mut b, resolutions, static_calls);
                b
            });
            kodo_ast::Expr::If {
                condition: Box::new(condition),
                then_branch,
                else_branch,
                span,
            }
        }
        kodo_ast::Expr::FieldAccess {
            object,
            field,
            span,
        } => kodo_ast::Expr::FieldAccess {
            object: Box::new(rewrite_method_calls_in_expr(
                *object,
                resolutions,
                static_calls,
            )),
            field,
            span,
        },
        kodo_ast::Expr::StructLit { name, fields, span } => kodo_ast::Expr::StructLit {
            name,
            fields: fields
                .into_iter()
                .map(|f| kodo_ast::FieldInit {
                    name: f.name,
                    value: rewrite_method_calls_in_expr(f.value, resolutions, static_calls),
                    span: f.span,
                })
                .collect(),
            span,
        },
        kodo_ast::Expr::EnumVariantExpr {
            enum_name,
            variant,
            args,
            span,
        } => kodo_ast::Expr::EnumVariantExpr {
            enum_name,
            variant,
            args: args
                .into_iter()
                .map(|a| rewrite_method_calls_in_expr(a, resolutions, static_calls))
                .collect(),
            span,
        },
        kodo_ast::Expr::Match { expr, arms, span } => kodo_ast::Expr::Match {
            expr: Box::new(rewrite_method_calls_in_expr(
                *expr,
                resolutions,
                static_calls,
            )),
            arms: arms
                .into_iter()
                .map(|arm| kodo_ast::MatchArm {
                    pattern: arm.pattern,
                    body: rewrite_method_calls_in_expr(arm.body, resolutions, static_calls),
                    span: arm.span,
                })
                .collect(),
            span,
        },
        kodo_ast::Expr::Block(mut block) => {
            rewrite_method_calls_in_block(&mut block, resolutions, static_calls);
            kodo_ast::Expr::Block(block)
        }
        kodo_ast::Expr::Closure {
            params,
            return_type,
            body,
            span,
        } => kodo_ast::Expr::Closure {
            params,
            return_type,
            body: Box::new(rewrite_method_calls_in_expr(
                *body,
                resolutions,
                static_calls,
            )),
            span,
        },
        kodo_ast::Expr::NullCoalesce { left, right, span } => kodo_ast::Expr::NullCoalesce {
            left: Box::new(rewrite_method_calls_in_expr(
                *left,
                resolutions,
                static_calls,
            )),
            right: Box::new(rewrite_method_calls_in_expr(
                *right,
                resolutions,
                static_calls,
            )),
            span,
        },
        kodo_ast::Expr::Try { operand, span } => kodo_ast::Expr::Try {
            operand: Box::new(rewrite_method_calls_in_expr(
                *operand,
                resolutions,
                static_calls,
            )),
            span,
        },
        kodo_ast::Expr::OptionalChain {
            object,
            field,
            span,
        } => kodo_ast::Expr::OptionalChain {
            object: Box::new(rewrite_method_calls_in_expr(
                *object,
                resolutions,
                static_calls,
            )),
            field,
            span,
        },
        kodo_ast::Expr::Range {
            start,
            end,
            inclusive,
            span,
        } => kodo_ast::Expr::Range {
            start: Box::new(rewrite_method_calls_in_expr(
                *start,
                resolutions,
                static_calls,
            )),
            end: Box::new(rewrite_method_calls_in_expr(
                *end,
                resolutions,
                static_calls,
            )),
            inclusive,
            span,
        },
        kodo_ast::Expr::TupleLit(elems, span) => kodo_ast::Expr::TupleLit(
            elems
                .into_iter()
                .map(|e| rewrite_method_calls_in_expr(e, resolutions, static_calls))
                .collect(),
            span,
        ),
        kodo_ast::Expr::TupleIndex { tuple, index, span } => kodo_ast::Expr::TupleIndex {
            tuple: Box::new(rewrite_method_calls_in_expr(
                *tuple,
                resolutions,
                static_calls,
            )),
            index,
            span,
        },
        e @ (kodo_ast::Expr::IntLit(_, _)
        | kodo_ast::Expr::FloatLit(_, _)
        | kodo_ast::Expr::StringLit(_, _)
        | kodo_ast::Expr::BoolLit(_, _)
        | kodo_ast::Expr::Ident(_, _)) => e,
        kodo_ast::Expr::Is {
            operand,
            type_name,
            span,
        } => kodo_ast::Expr::Is {
            operand: Box::new(rewrite_method_calls_in_expr(
                *operand,
                resolutions,
                static_calls,
            )),
            type_name,
            span,
        },
        kodo_ast::Expr::Await { operand, span } => kodo_ast::Expr::Await {
            operand: Box::new(rewrite_method_calls_in_expr(
                *operand,
                resolutions,
                static_calls,
            )),
            span,
        },
        kodo_ast::Expr::StringInterp { parts, span } => {
            let parts = parts
                .into_iter()
                .map(|p| match p {
                    kodo_ast::StringPart::Literal(s) => kodo_ast::StringPart::Literal(s),
                    kodo_ast::StringPart::Expr(e) => kodo_ast::StringPart::Expr(Box::new(
                        rewrite_method_calls_in_expr(*e, resolutions, static_calls),
                    )),
                })
                .collect();
            kodo_ast::Expr::StringInterp { parts, span }
        }
    }
}

// ========== Valid fixtures: full pipeline must succeed ==========

#[test]
fn pipeline_valid_hello() {
    let source = read_fixture("valid/hello.ko");
    run_full_pipeline(&source).unwrap();
}

#[test]
fn pipeline_valid_minimal() {
    let source = read_fixture("valid/minimal.ko");
    run_full_pipeline(&source).unwrap();
}

#[test]
fn pipeline_valid_expressions() {
    let source = read_fixture("valid/expressions.ko");
    run_full_pipeline(&source).unwrap();
}

#[test]
fn pipeline_valid_contracts() {
    let source = read_fixture("valid/contracts.ko");
    run_full_pipeline(&source).unwrap();
}

#[test]
fn pipeline_valid_string_interpolation() {
    let source = read_fixture("valid/string_interpolation.ko");
    run_full_pipeline(&source).unwrap();
}

// ========== Invalid fixtures: must fail at the expected stage ==========

#[test]
fn pipeline_type_error_return_mismatch() {
    let source = read_fixture("invalid/type_error_return.ko");
    let err = run_full_pipeline(&source).unwrap_err();
    assert!(
        err.starts_with("type error:"),
        "expected type error, got: {err}"
    );
    assert!(
        err.contains("mismatch"),
        "expected mismatch in error: {err}"
    );
}

#[test]
fn pipeline_syntax_error() {
    let source = read_fixture("invalid/syntax_error.ko");
    let err = run_full_pipeline(&source).unwrap_err();
    assert!(
        err.starts_with("parse error:"),
        "expected parse error, got: {err}"
    );
}

#[test]
fn pipeline_undefined_variable() {
    let source = read_fixture("invalid/undefined_var.ko");
    let err = run_full_pipeline(&source).unwrap_err();
    assert!(
        err.starts_with("type error:"),
        "expected type error, got: {err}"
    );
    assert!(
        err.contains("undefined") || err.contains("Undefined"),
        "expected undefined variable error: {err}"
    );
}

// ========== Contract fixtures ==========

#[test]
fn pipeline_valid_contracts_fixture() {
    let source = read_fixture("contracts/valid_contracts.ko");
    run_full_pipeline(&source).unwrap();
}

#[test]
fn pipeline_invalid_precondition() {
    let source = read_fixture("contracts/invalid_precondition.ko");
    // The string literal in the requires clause should cause a contract validation failure.
    // Note: contract verification collects failures but does not return Err for them —
    // it reports them in VerificationResult.failures. So the pipeline may succeed
    // but we should check the contract verification result directly.
    let module = kodo_parser::parse(&source).unwrap();

    let mut checker = kodo_types::TypeChecker::new();
    checker.check_module(&module).unwrap();

    for func in &module.functions {
        let contracts = kodo_contracts::extract_contracts(func);
        let result =
            kodo_contracts::verify_contracts(&contracts, kodo_contracts::ContractMode::Runtime)
                .unwrap();
        if !contracts.is_empty() {
            assert!(
                !result.failures.is_empty(),
                "expected contract validation failures for function `{}`",
                func.name
            );
        }
    }
}

// ========== Parse-only tests (preserved from original) ==========

#[test]
fn parse_valid_hello() {
    let source = read_fixture("valid/hello.ko");
    let result = kodo_parser::parse(&source);
    assert!(result.is_ok(), "failed to parse hello.ko: {result:?}");
    let module = result.unwrap();
    assert_eq!(module.name, "hello");
    assert!(module.meta.is_some());
    assert!(!module.functions.is_empty());
}

#[test]
fn parse_invalid_missing_meta_still_parses() {
    // missing_meta.ko is a module without a meta block — this should parse
    // fine since meta is optional. The error would come from a later
    // semantic analysis pass that enforces mandatory meta blocks.
    let source = read_fixture("invalid/missing_meta.ko");
    let result = kodo_parser::parse(&source);
    assert!(
        result.is_ok(),
        "failed to parse missing_meta.ko: {result:?}"
    );
    let module = result.unwrap();
    assert!(module.meta.is_none());
}

#[test]
fn lex_valid_hello() {
    let source = read_fixture("valid/hello.ko");
    let tokens = kodo_lexer::tokenize(&source);
    assert!(tokens.is_ok(), "failed to tokenize hello.ko: {tokens:?}");
    assert!(!tokens.unwrap().is_empty());
}

// ========== All examples pass check ==========

#[test]
fn all_examples_pass_check() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let examples_dir = workspace_root.join("examples");

    let mut checked = 0;
    for entry in std::fs::read_dir(&examples_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("ko") {
            continue;
        }
        let filename = path.file_name().unwrap().to_str().unwrap();

        // Files with "error" in the name are expected to fail
        if filename.contains("error") {
            let source = std::fs::read_to_string(&path).unwrap();
            let result = run_full_pipeline(&source);
            assert!(
                result.is_err(),
                "expected {filename} to fail pipeline, but it passed"
            );
        } else {
            let source = std::fs::read_to_string(&path).unwrap();
            run_full_pipeline(&source).unwrap_or_else(|e| {
                panic!("example {filename} failed pipeline: {e}");
            });
        }
        checked += 1;
    }

    assert!(
        checked >= 4,
        "expected at least 4 example files, found {checked}"
    );
}

// ========== CLI exit code tests ==========

#[test]
fn cli_check_valid_exits_zero() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let fixture = workspace_root.join("tests/fixtures/valid/hello.ko");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_kodoc"))
        .args(["check", fixture.to_str().unwrap()])
        .output()
        .expect("failed to run kodoc");

    assert!(
        output.status.success(),
        "kodoc check should exit 0 for valid file, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cli_check_invalid_exits_nonzero() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let fixture = workspace_root.join("tests/fixtures/invalid/type_error_return.ko");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_kodoc"))
        .args(["check", fixture.to_str().unwrap()])
        .output()
        .expect("failed to run kodoc");

    assert!(
        !output.status.success(),
        "kodoc check should exit non-zero for type error file"
    );
}

#[test]
fn cli_lex_valid_exits_zero() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let fixture = workspace_root.join("tests/fixtures/valid/hello.ko");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_kodoc"))
        .args(["lex", fixture.to_str().unwrap()])
        .output()
        .expect("failed to run kodoc");

    assert!(
        output.status.success(),
        "kodoc lex should exit 0 for valid file"
    );
}

#[test]
fn cli_parse_valid_exits_zero() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let fixture = workspace_root.join("tests/fixtures/valid/hello.ko");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_kodoc"))
        .args(["parse", fixture.to_str().unwrap()])
        .output()
        .expect("failed to run kodoc");

    assert!(
        output.status.success(),
        "kodoc parse should exit 0 for valid file"
    );
}

// ========== JSON error output tests ==========

#[test]
fn json_errors_parse_error() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let fixture = workspace_root.join("tests/fixtures/invalid/syntax_error.ko");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_kodoc"))
        .args(["check", fixture.to_str().unwrap(), "--json-errors"])
        .output()
        .expect("failed to run kodoc");

    assert!(
        !output.status.success(),
        "kodoc check --json-errors should exit non-zero for syntax error"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {e}\nstdout: {stdout}"));

    assert_eq!(json["status"], "failed");
    assert!(json["errors"].as_array().unwrap().len() > 0);
    let first_error = &json["errors"][0];
    assert!(first_error["code"].as_str().unwrap().starts_with("E01"));
    assert!(first_error["message"].as_str().is_some());
    assert_eq!(first_error["severity"], "error");
}

#[test]
fn json_errors_type_error() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let fixture = workspace_root.join("tests/fixtures/invalid/type_error_return.ko");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_kodoc"))
        .args(["check", fixture.to_str().unwrap(), "--json-errors"])
        .output()
        .expect("failed to run kodoc");

    assert!(
        !output.status.success(),
        "kodoc check --json-errors should exit non-zero for type error"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {e}\nstdout: {stdout}"));

    assert_eq!(json["status"], "failed");
    let first_error = &json["errors"][0];
    assert!(first_error["code"].as_str().unwrap().starts_with("E02"));
    assert!(first_error["span"].is_object());
    assert!(first_error["span"]["line"].as_u64().unwrap() > 0);
}

#[test]
fn json_errors_success() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let fixture = workspace_root.join("tests/fixtures/valid/hello.ko");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_kodoc"))
        .args(["check", fixture.to_str().unwrap(), "--json-errors"])
        .output()
        .expect("failed to run kodoc");

    assert!(
        output.status.success(),
        "kodoc check --json-errors should exit 0 for valid file, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {e}\nstdout: {stdout}"));

    assert_eq!(json["status"], "ok");
    assert_eq!(json["errors"].as_array().unwrap().len(), 0);
    assert_eq!(json["warnings"].as_array().unwrap().len(), 0);
}

// ========== Module meta validation tests ==========

#[test]
fn module_without_meta_is_error() {
    let source = read_fixture("invalid/missing_meta.ko");
    let module = kodo_parser::parse(&source).unwrap();

    let mut checker = kodo_types::TypeChecker::new();
    let err = checker.check_module(&module).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("meta"), "expected meta error, got: {msg}");
}

#[test]
fn module_with_empty_purpose_is_error() {
    let source = read_fixture("invalid/empty_purpose.ko");
    let module = kodo_parser::parse(&source).unwrap();

    let mut checker = kodo_types::TypeChecker::new();
    let err = checker.check_module(&module).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("purpose"),
        "expected purpose error, got: {msg}"
    );
}

#[test]
fn module_with_missing_purpose_is_error() {
    let source = read_fixture("invalid/missing_purpose.ko");
    let module = kodo_parser::parse(&source).unwrap();

    let mut checker = kodo_types::TypeChecker::new();
    let err = checker.check_module(&module).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("purpose"),
        "expected purpose error, got: {msg}"
    );
}

// ========== Meta in JSON output tests ==========

#[test]
fn meta_included_in_json_output() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let fixture = workspace_root.join("tests/fixtures/valid/hello.ko");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_kodoc"))
        .args(["check", fixture.to_str().unwrap(), "--json-errors"])
        .output()
        .expect("failed to run kodoc");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {e}\nstdout: {stdout}"));

    assert_eq!(json["status"], "ok");
    assert!(
        json["meta"].is_object(),
        "expected meta object in JSON output"
    );
    assert_eq!(json["meta"]["module"], "hello");
    assert!(json["meta"]["purpose"].as_str().is_some());
}

// ========== SMT contract verification tests ==========

#[test]
fn smt_static_proves_trivial_contract() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let fixture = workspace_root.join("examples/contracts_smt_demo.ko");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_kodoc"))
        .args(["check", "--contracts=static", fixture.to_str().unwrap()])
        .output()
        .expect("failed to run kodoc");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "expected check to pass, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("statically verified"),
        "expected static verification report: {stdout}"
    );
}

#[test]
fn smt_static_refutes_unprovable_contract() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let fixture = workspace_root.join("examples/contracts_verified.ko");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_kodoc"))
        .args(["check", "--contracts=static", fixture.to_str().unwrap()])
        .output()
        .expect("failed to run kodoc");

    // `b != 0` cannot be proved without call-site context — Z3 refutes it
    assert!(
        !output.status.success(),
        "expected check to fail for unprovable contract"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("refuted") || stderr.contains("counter-example"),
        "expected refutation message: {stderr}"
    );
}

#[test]
fn smt_runtime_mode_accepts_unprovable_contract() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let fixture = workspace_root.join("examples/contracts_verified.ko");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_kodoc"))
        .args(["check", "--contracts=runtime", fixture.to_str().unwrap()])
        .output()
        .expect("failed to run kodoc");

    assert!(
        output.status.success(),
        "runtime mode should accept unprovable contracts"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("runtime checks"),
        "expected runtime checks report: {stdout}"
    );
}
