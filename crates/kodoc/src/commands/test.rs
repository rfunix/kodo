//! The `test` command implementation.
//!
//! Runs tests defined in a Kodo source file by desugaring `test` declarations
//! into regular functions, generating a synthetic test runner `main`, compiling,
//! executing, and reporting results.
//!
//! ## Describe block flattening
//!
//! Nested `describe` blocks are flattened into a linear sequence of tests with
//! hierarchical names (e.g. `"math > addition"`). Setup/teardown blocks are
//! merged from parent to child: setup statements are prepended and teardown
//! statements are appended to each test body.
//!
//! ## Annotation processing
//!
//! - `@skip("reason")` — test is reported as skipped without executing its body.
//! - `@todo("reason")` — same as skip but tracked separately in the summary.
//! - `@timeout(ms)` — wraps the test body with `kodo_test_set_timeout` /
//!   `kodo_test_clear_timeout` calls.

use std::path::PathBuf;

use super::common::{
    build_vtable_defs, compile_imported_module, inject_stdlib_method_functions, link_executable,
    parse_contract_mode, resolve_import_path, rewrite_map_for_in, rewrite_method_calls_in_block,
    rewrite_self_method_calls_in_block, substitute_type_expr_ast, type_to_type_expr,
};
use crate::diagnostics;

/// Parameters extracted from a `@property(...)` annotation.
struct PropertyParams {
    /// Number of iterations (default 100).
    iterations: i64,
    /// Minimum value for `Int` generators (default -1_000_000).
    int_min: i64,
    /// Maximum value for `Int` generators (default 1_000_000).
    int_max: i64,
    /// Minimum value for `Float64` generators (default -1_000_000.0).
    float_min: f64,
    /// Maximum value for `Float64` generators (default 1_000_000.0).
    float_max: f64,
    /// Maximum length for `String` generators (default 100).
    max_string_len: i64,
    /// Seed for the RNG (default 0 = random).
    seed: i64,
}

impl Default for PropertyParams {
    fn default() -> Self {
        Self {
            iterations: 100,
            int_min: -1_000_000,
            int_max: 1_000_000,
            float_min: -1_000_000.0,
            float_max: 1_000_000.0,
            max_string_len: 100,
            seed: 0,
        }
    }
}

/// Extracts a named integer argument from an annotation, handling both positive
/// and negative literals (the parser produces `UnaryOp::Neg(IntLit(n))` for `-n`).
fn extract_named_int(ann: &kodo_ast::Annotation, name: &str) -> Option<i64> {
    ann.args.iter().find_map(|arg| match arg {
        kodo_ast::AnnotationArg::Named(n, kodo_ast::Expr::IntLit(v, _)) if n == name => Some(*v),
        kodo_ast::AnnotationArg::Named(
            n,
            kodo_ast::Expr::UnaryOp {
                op: kodo_ast::UnaryOp::Neg,
                operand,
                ..
            },
        ) if n == name => {
            if let kodo_ast::Expr::IntLit(v, _) = operand.as_ref() {
                Some(-v)
            } else {
                None
            }
        }
        _ => None,
    })
}

/// Extracts a named float argument from an annotation, handling negation.
fn extract_named_float(ann: &kodo_ast::Annotation, name: &str) -> Option<f64> {
    ann.args.iter().find_map(|arg| match arg {
        kodo_ast::AnnotationArg::Named(n, kodo_ast::Expr::FloatLit(v, _)) if n == name => Some(*v),
        kodo_ast::AnnotationArg::Named(
            n,
            kodo_ast::Expr::UnaryOp {
                op: kodo_ast::UnaryOp::Neg,
                operand,
                ..
            },
        ) if n == name => {
            if let kodo_ast::Expr::FloatLit(v, _) = operand.as_ref() {
                Some(-v)
            } else {
                None
            }
        }
        _ => None,
    })
}

/// Extracts `@property(...)` parameters from a list of annotations.
///
/// Supported named arguments:
/// - `iterations: 50` — number of random inputs to generate
/// - `int_min: -100`, `int_max: 100` — range for Int generators
/// - `float_min: -1.0`, `float_max: 1.0` — range for Float64 generators
/// - `max_string_len: 50` — maximum string length
/// - `seed: 42` — deterministic seed for reproducibility
fn extract_property_params(annotations: &[kodo_ast::Annotation]) -> Option<PropertyParams> {
    let ann = annotations.iter().find(|a| a.name == "property")?;
    let mut params = PropertyParams::default();
    if let Some(v) = extract_named_int(ann, "iterations") {
        params.iterations = v;
    }
    if let Some(v) = extract_named_int(ann, "int_min") {
        params.int_min = v;
    }
    if let Some(v) = extract_named_int(ann, "int_max") {
        params.int_max = v;
    }
    if let Some(v) = extract_named_float(ann, "float_min") {
        params.float_min = v;
    }
    if let Some(v) = extract_named_float(ann, "float_max") {
        params.float_max = v;
    }
    if let Some(v) = extract_named_int(ann, "max_string_len") {
        params.max_string_len = v;
    }
    if let Some(v) = extract_named_int(ann, "seed") {
        params.seed = v;
    }
    Some(params)
}

/// Generates a call expression to a runtime function with the given arguments.
fn make_call(name: &str, args: Vec<kodo_ast::Expr>, s: kodo_ast::Span) -> kodo_ast::Expr {
    kodo_ast::Expr::Call {
        callee: Box::new(kodo_ast::Expr::Ident(name.to_string(), s)),
        args,
        span: s,
    }
}

/// Returns the appropriate generator call expression for a given type binding.
fn gen_call_for_type(
    ty: &kodo_ast::TypeExpr,
    params: &PropertyParams,
    s: kodo_ast::Span,
) -> kodo_ast::Expr {
    match ty {
        kodo_ast::TypeExpr::Named(name) => match name.as_str() {
            "Int" => make_call(
                "kodo_prop_gen_int",
                vec![
                    kodo_ast::Expr::IntLit(params.int_min, s),
                    kodo_ast::Expr::IntLit(params.int_max, s),
                ],
                s,
            ),
            "Bool" => make_call("kodo_prop_gen_bool", vec![], s),
            "Float64" => make_call(
                "kodo_prop_gen_float",
                vec![
                    kodo_ast::Expr::FloatLit(params.float_min, s),
                    kodo_ast::Expr::FloatLit(params.float_max, s),
                ],
                s,
            ),
            "String" => make_call(
                "kodo_prop_gen_string",
                vec![kodo_ast::Expr::IntLit(params.max_string_len, s)],
                s,
            ),
            // Fallback: generate an Int in [0, 100] for unknown types.
            _ => make_call(
                "kodo_prop_gen_int",
                vec![kodo_ast::Expr::IntLit(0, s), kodo_ast::Expr::IntLit(100, s)],
                s,
            ),
        },
        // For any complex types, fall back to Int generation.
        _ => make_call(
            "kodo_prop_gen_int",
            vec![kodo_ast::Expr::IntLit(0, s), kodo_ast::Expr::IntLit(100, s)],
            s,
        ),
    }
}

/// Desugars a `ForAll` statement into a `kodo_prop_start` call followed by a
/// `while` loop that generates random inputs and executes the body.
///
/// Transforms:
/// ```text
/// forall a: Int, b: Int { assert_eq(a + b, b + a) }
/// ```
/// Into:
/// ```text
/// kodo_prop_start(iterations, seed)
/// let __prop_iter: Int = 0
/// while __prop_iter < iterations {
///     let a: Int = kodo_prop_gen_int(min, max)
///     let b: Int = kodo_prop_gen_int(min, max)
///     assert_eq(a + b, b + a)
///     __prop_iter = __prop_iter + 1
/// }
/// ```
fn desugar_forall(stmt: &kodo_ast::Stmt, params: &PropertyParams) -> Vec<kodo_ast::Stmt> {
    let (span, bindings, body) = match stmt {
        kodo_ast::Stmt::ForAll {
            span,
            bindings,
            body,
        } => (*span, bindings, body),
        _ => return vec![stmt.clone()],
    };
    let s = kodo_ast::Span::new(0, 0);

    let mut result = Vec::new();

    // kodo_prop_start(iterations, seed)
    result.push(kodo_ast::Stmt::Expr(make_call(
        "kodo_prop_start",
        vec![
            kodo_ast::Expr::IntLit(params.iterations, s),
            kodo_ast::Expr::IntLit(params.seed, s),
        ],
        s,
    )));

    // let __prop_iter: Int = 0
    result.push(kodo_ast::Stmt::Let {
        span: s,
        mutable: true,
        name: "__prop_iter".to_string(),
        ty: Some(kodo_ast::TypeExpr::Named("Int".to_string())),
        value: kodo_ast::Expr::IntLit(0, s),
    });

    // Build while-loop body: generator bindings + forall body + increment.
    let mut loop_stmts = Vec::new();
    for (name, ty) in bindings {
        loop_stmts.push(kodo_ast::Stmt::Let {
            span,
            mutable: false,
            name: name.clone(),
            ty: Some(ty.clone()),
            value: gen_call_for_type(ty, params, s),
        });
    }
    loop_stmts.extend(body.stmts.iter().cloned());
    // __prop_iter = __prop_iter + 1
    loop_stmts.push(kodo_ast::Stmt::Assign {
        span: s,
        name: "__prop_iter".to_string(),
        value: kodo_ast::Expr::BinaryOp {
            left: Box::new(kodo_ast::Expr::Ident("__prop_iter".to_string(), s)),
            op: kodo_ast::BinOp::Add,
            right: Box::new(kodo_ast::Expr::IntLit(1, s)),
            span: s,
        },
    });

    // while __prop_iter < iterations { ... }
    result.push(kodo_ast::Stmt::While {
        span: s,
        condition: kodo_ast::Expr::BinaryOp {
            left: Box::new(kodo_ast::Expr::Ident("__prop_iter".to_string(), s)),
            op: kodo_ast::BinOp::Lt,
            right: Box::new(kodo_ast::Expr::IntLit(params.iterations, s)),
            span: s,
        },
        body: kodo_ast::Block {
            span: s,
            stmts: loop_stmts,
        },
    });

    result
}

/// Applies `@property` + `forall` desugaring to a test body.
///
/// Walks the body statements looking for `ForAll` nodes and replaces each one
/// with the desugared while-loop form that calls runtime generators.
fn apply_property_desugaring(body: &mut kodo_ast::Block, params: &PropertyParams) {
    let mut new_stmts = Vec::new();
    for stmt in &body.stmts {
        if matches!(stmt, kodo_ast::Stmt::ForAll { .. }) {
            new_stmts.extend(desugar_forall(stmt, params));
        } else {
            new_stmts.push(stmt.clone());
        }
    }
    body.stmts = new_stmts;
}

/// Classification of a test based on its annotations.
enum TestKind {
    /// Normal test that should be compiled and executed.
    Run,
    /// Skipped test — not compiled, reported as "skipped".
    Skip,
    /// Todo test — not compiled, reported as "todo" (tracked separately).
    Todo,
}

/// A processed test entry ready for desugaring into the synthetic main.
struct ProcessedTest {
    /// The display name for this test (hierarchical for describe tests).
    name: String,
    /// Whether this test should run, be skipped, or is a todo.
    kind: TestKind,
    /// The test body (with setup/teardown merged and timeout injected).
    /// Only meaningful for `TestKind::Run`.
    body: kodo_ast::Block,
}

/// Looks up an annotation by name in a slice of annotations.
fn get_annotation<'a>(
    annotations: &'a [kodo_ast::Annotation],
    name: &str,
) -> Option<&'a kodo_ast::Annotation> {
    annotations.iter().find(|a| a.name == name)
}

/// Extracts an integer argument from the first positional argument of an annotation.
fn get_annotation_int_arg(ann: &kodo_ast::Annotation) -> Option<i64> {
    ann.args.first().and_then(|arg| match arg {
        kodo_ast::AnnotationArg::Positional(kodo_ast::Expr::IntLit(n, _)) => Some(*n),
        _ => None,
    })
}

/// Flattens nested describe blocks into a flat list of `(name, annotations, body)` tuples.
///
/// Setup/teardown blocks from parent describes are prepended/appended to test bodies.
/// Annotations from the describe block are merged with each test's own annotations
/// (describe annotations first, then test annotations, so test-level overrides win).
fn flatten_describes(
    describes: &[kodo_ast::DescribeDecl],
    prefix: &str,
    parent_setup: &[kodo_ast::Stmt],
    parent_teardown: &[kodo_ast::Stmt],
) -> Vec<(String, Vec<kodo_ast::Annotation>, kodo_ast::Block)> {
    let mut result = Vec::new();
    for describe in describes {
        let group_name = if prefix.is_empty() {
            describe.name.clone()
        } else {
            format!("{prefix} > {}", describe.name)
        };

        // Merge setup: parent setup + this describe's setup.
        let mut merged_setup = parent_setup.to_vec();
        if let Some(ref setup) = describe.setup {
            merged_setup.extend(setup.stmts.iter().cloned());
        }

        // Merge teardown: this describe's teardown + parent teardown.
        let mut merged_teardown = Vec::new();
        if let Some(ref teardown) = describe.teardown {
            merged_teardown.extend(teardown.stmts.iter().cloned());
        }
        merged_teardown.extend(parent_teardown.iter().cloned());

        // Flatten tests in this describe block.
        for test in &describe.tests {
            let full_name = format!("{group_name} > {}", test.name);
            // Merge annotations: describe-level first, then test-level.
            let mut merged_annotations = describe.annotations.clone();
            merged_annotations.extend(test.annotations.iter().cloned());

            // Build body: setup + test body + teardown.
            let mut stmts = merged_setup.clone();
            stmts.extend(test.body.stmts.iter().cloned());
            stmts.extend(merged_teardown.clone());

            result.push((
                full_name,
                merged_annotations,
                kodo_ast::Block {
                    span: test.body.span,
                    stmts,
                },
            ));
        }

        // Recurse into nested describes.
        result.extend(flatten_describes(
            &describe.describes,
            &group_name,
            &merged_setup,
            &merged_teardown,
        ));
    }
    result
}

/// Classifies a test as run/skip/todo based on its annotations and extracts
/// the timeout value if present.
fn classify_test(annotations: &[kodo_ast::Annotation]) -> (TestKind, Option<i64>) {
    let kind = if get_annotation(annotations, "skip").is_some() {
        TestKind::Skip
    } else if get_annotation(annotations, "todo").is_some() {
        TestKind::Todo
    } else {
        TestKind::Run
    };
    let timeout_ms = get_annotation(annotations, "timeout").and_then(get_annotation_int_arg);
    (kind, timeout_ms)
}

/// Wraps a test body with timeout set/clear calls if a timeout is specified.
fn apply_timeout(body: &mut kodo_ast::Block, timeout_ms: i64) {
    let s = kodo_ast::Span::new(0, 0);
    // Prepend: kodo_test_set_timeout(ms)
    body.stmts.insert(
        0,
        kodo_ast::Stmt::Expr(kodo_ast::Expr::Call {
            callee: Box::new(kodo_ast::Expr::Ident(
                "kodo_test_set_timeout".to_string(),
                s,
            )),
            args: vec![kodo_ast::Expr::IntLit(timeout_ms, s)],
            span: s,
        }),
    );
    // Append: kodo_test_clear_timeout()
    body.stmts.push(kodo_ast::Stmt::Expr(kodo_ast::Expr::Call {
        callee: Box::new(kodo_ast::Expr::Ident(
            "kodo_test_clear_timeout".to_string(),
            s,
        )),
        args: vec![],
        span: s,
    }));
}

/// Runs tests in a Kodo source file.
///
/// Desugars `test` declarations and `describe` blocks into regular functions,
/// generates a synthetic `main` function as the test runner, compiles, executes,
/// and reports results. Handles `@skip`, `@todo`, and `@timeout` annotations.
#[allow(clippy::too_many_lines)]
pub(crate) fn run_test(
    file: &PathBuf,
    filter: Option<&str>,
    json: bool,
    contracts_mode_str: &str,
) -> i32 {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not read file `{}`: {e}", file.display());
            return 1;
        }
    };

    let filename = file.display().to_string();

    let mut module = match kodo_parser::parse(&source) {
        Ok(m) => m,
        Err(e) => {
            diagnostics::render_parse_error(&source, &filename, &e);
            return 1;
        }
    };

    // Flatten describe blocks into additional test entries.
    let flattened = flatten_describes(&module.describe_decls, "", &[], &[]);

    // Build a unified list of processed tests: regular test_decls + flattened describes.
    let mut all_tests: Vec<ProcessedTest> = Vec::new();

    for test_decl in &module.test_decls {
        let (kind, timeout_ms) = classify_test(&test_decl.annotations);
        let mut body = test_decl.body.clone();
        if matches!(kind, TestKind::Run) {
            // Desugar @property + forall before timeout wrapping.
            if let Some(ref params) = extract_property_params(&test_decl.annotations) {
                apply_property_desugaring(&mut body, params);
            }
            if let Some(ms) = timeout_ms {
                apply_timeout(&mut body, ms);
            }
        }
        all_tests.push(ProcessedTest {
            name: test_decl.name.clone(),
            kind,
            body,
        });
    }

    for (name, annotations, block) in flattened {
        let (kind, timeout_ms) = classify_test(&annotations);
        let mut body = block;
        if matches!(kind, TestKind::Run) {
            // Desugar @property + forall before timeout wrapping.
            if let Some(ref params) = extract_property_params(&annotations) {
                apply_property_desugaring(&mut body, params);
            }
            if let Some(ms) = timeout_ms {
                apply_timeout(&mut body, ms);
            }
        }
        all_tests.push(ProcessedTest { name, kind, body });
    }

    // Apply filter if provided.
    if let Some(pattern) = filter {
        all_tests.retain(|t| t.name.contains(pattern));
    }

    if all_tests.is_empty() {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "event": "summary",
                    "total": 0,
                    "passed": 0,
                    "failed": 0,
                    "skipped": 0,
                    "todo": 0,
                })
            );
        } else {
            println!("no tests found");
        }
        return 0;
    }

    // Clear the original test_decls and describe_decls — we handle them ourselves.
    module.test_decls.clear();
    module.describe_decls.clear();

    let total_count = all_tests.len();
    let s = kodo_ast::Span::new(0, 0);

    // Count skip/todo for the summary constants.
    let skip_count = all_tests
        .iter()
        .filter(|t| matches!(t.kind, TestKind::Skip))
        .count();
    let todo_count = all_tests
        .iter()
        .filter(|t| matches!(t.kind, TestKind::Todo))
        .count();

    // Desugar: convert each runnable test into a function `__test_N`.
    // Skip/todo tests don't get a function — they're handled in the synthetic main.
    let mut runnable_index = 0usize;
    let mut test_entries: Vec<(String, TestKind, Option<usize>)> = Vec::new();

    for test in all_tests {
        match test.kind {
            TestKind::Run => {
                let func = kodo_ast::Function {
                    id: kodo_ast::NodeId(0),
                    span: test.body.span,
                    name: format!("__test_{runnable_index}"),
                    visibility: kodo_ast::Visibility::Private,
                    is_async: false,
                    generic_params: vec![],
                    annotations: vec![],
                    params: vec![],
                    return_type: kodo_ast::TypeExpr::Unit,
                    requires: vec![],
                    ensures: vec![],
                    body: test.body,
                };
                module.functions.push(func);
                test_entries.push((test.name, TestKind::Run, Some(runnable_index)));
                runnable_index += 1;
            }
            TestKind::Skip => {
                test_entries.push((test.name, TestKind::Skip, None));
            }
            TestKind::Todo => {
                test_entries.push((test.name, TestKind::Todo, None));
            }
        }
    }

    // Generate synthetic `main` function as test runner.
    let mut main_stmts = vec![
        // let __total: Int = <total_count>
        kodo_ast::Stmt::Let {
            span: s,
            mutable: false,
            name: "__total".to_string(),
            ty: Some(kodo_ast::TypeExpr::Named("Int".to_string())),
            value: kodo_ast::Expr::IntLit(total_count as i64, s),
        },
        // let mut __passed: Int = 0
        kodo_ast::Stmt::Let {
            span: s,
            mutable: true,
            name: "__passed".to_string(),
            ty: Some(kodo_ast::TypeExpr::Named("Int".to_string())),
            value: kodo_ast::Expr::IntLit(0, s),
        },
        // let mut __failed: Int = 0
        kodo_ast::Stmt::Let {
            span: s,
            mutable: true,
            name: "__failed".to_string(),
            ty: Some(kodo_ast::TypeExpr::Named("Int".to_string())),
            value: kodo_ast::Expr::IntLit(0, s),
        },
        // let __skipped: Int = <skip_count>
        kodo_ast::Stmt::Let {
            span: s,
            mutable: false,
            name: "__skipped".to_string(),
            ty: Some(kodo_ast::TypeExpr::Named("Int".to_string())),
            value: kodo_ast::Expr::IntLit(skip_count as i64, s),
        },
        // let __todo: Int = <todo_count>
        kodo_ast::Stmt::Let {
            span: s,
            mutable: false,
            name: "__todo".to_string(),
            ty: Some(kodo_ast::TypeExpr::Named("Int".to_string())),
            value: kodo_ast::Expr::IntLit(todo_count as i64, s),
        },
    ];

    for (name, kind, func_idx) in &test_entries {
        // kodo_test_start("test name")
        main_stmts.push(kodo_ast::Stmt::Expr(kodo_ast::Expr::Call {
            callee: Box::new(kodo_ast::Expr::Ident("kodo_test_start".to_string(), s)),
            args: vec![kodo_ast::Expr::StringLit(name.clone(), s)],
            span: s,
        }));

        match kind {
            TestKind::Run => {
                let idx = func_idx.unwrap_or(0);
                // __test_N()
                main_stmts.push(kodo_ast::Stmt::Expr(kodo_ast::Expr::Call {
                    callee: Box::new(kodo_ast::Expr::Ident(format!("__test_{idx}"), s)),
                    args: vec![],
                    span: s,
                }));
                // let __result_N: Int = kodo_test_end()
                main_stmts.push(kodo_ast::Stmt::Let {
                    span: s,
                    mutable: false,
                    name: format!("__result_{idx}"),
                    ty: Some(kodo_ast::TypeExpr::Named("Int".to_string())),
                    value: kodo_ast::Expr::Call {
                        callee: Box::new(kodo_ast::Expr::Ident("kodo_test_end".to_string(), s)),
                        args: vec![],
                        span: s,
                    },
                });
                // if __result_N == 0 { __passed += 1 } else { __failed += 1 }
                main_stmts.push(kodo_ast::Stmt::Expr(kodo_ast::Expr::If {
                    condition: Box::new(kodo_ast::Expr::BinaryOp {
                        left: Box::new(kodo_ast::Expr::Ident(format!("__result_{idx}"), s)),
                        op: kodo_ast::BinOp::Eq,
                        right: Box::new(kodo_ast::Expr::IntLit(0, s)),
                        span: s,
                    }),
                    then_branch: kodo_ast::Block {
                        span: s,
                        stmts: vec![kodo_ast::Stmt::Assign {
                            span: s,
                            name: "__passed".to_string(),
                            value: kodo_ast::Expr::BinaryOp {
                                left: Box::new(kodo_ast::Expr::Ident("__passed".to_string(), s)),
                                op: kodo_ast::BinOp::Add,
                                right: Box::new(kodo_ast::Expr::IntLit(1, s)),
                                span: s,
                            },
                        }],
                    },
                    else_branch: Some(kodo_ast::Block {
                        span: s,
                        stmts: vec![kodo_ast::Stmt::Assign {
                            span: s,
                            name: "__failed".to_string(),
                            value: kodo_ast::Expr::BinaryOp {
                                left: Box::new(kodo_ast::Expr::Ident("__failed".to_string(), s)),
                                op: kodo_ast::BinOp::Add,
                                right: Box::new(kodo_ast::Expr::IntLit(1, s)),
                                span: s,
                            },
                        }],
                    }),
                    span: s,
                }));
            }
            TestKind::Skip | TestKind::Todo => {
                // kodo_test_skip() — prints "skipped" on the same line.
                main_stmts.push(kodo_ast::Stmt::Expr(kodo_ast::Expr::Call {
                    callee: Box::new(kodo_ast::Expr::Ident("kodo_test_skip".to_string(), s)),
                    args: vec![],
                    span: s,
                }));
            }
        }
    }

    // kodo_test_summary(__total, __passed, __failed, __skipped, __todo)
    main_stmts.push(kodo_ast::Stmt::Expr(kodo_ast::Expr::Call {
        callee: Box::new(kodo_ast::Expr::Ident("kodo_test_summary".to_string(), s)),
        args: vec![
            kodo_ast::Expr::Ident("__total".to_string(), s),
            kodo_ast::Expr::Ident("__passed".to_string(), s),
            kodo_ast::Expr::Ident("__failed".to_string(), s),
            kodo_ast::Expr::Ident("__skipped".to_string(), s),
            kodo_ast::Expr::Ident("__todo".to_string(), s),
        ],
        span: s,
    }));

    let main_fn = kodo_ast::Function {
        id: kodo_ast::NodeId(0),
        span: s,
        name: "main".to_string(),
        visibility: kodo_ast::Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![],
        return_type: kodo_ast::TypeExpr::Unit,
        requires: vec![],
        ensures: vec![],
        body: kodo_ast::Block {
            span: s,
            stmts: main_stmts,
        },
    };

    // Remove any existing `main` function (user's main is not used in test mode).
    module.functions.retain(|f| f.name != "main");
    module.functions.push(main_fn);

    // Now run the normal build pipeline.
    let base_dir = file.parent().unwrap_or_else(|| std::path::Path::new("."));
    let mut imported_modules: Vec<kodo_ast::Module> = Vec::new();

    for import in &module.imports {
        if let Some(stdlib_source) = kodo_std::resolve_stdlib_module(&import.path) {
            match kodo_parser::parse(stdlib_source) {
                Ok(m) => imported_modules.push(m),
                Err(e) => {
                    eprintln!("stdlib parse error: {e}");
                    return 1;
                }
            }
            continue;
        }
        let import_path = resolve_import_path(base_dir, &import.path);
        match compile_imported_module(&import_path, &mut Vec::new()) {
            Ok(imported_module) => imported_modules.push(imported_module),
            Err(msg) => {
                eprintln!("{msg}");
                return 1;
            }
        }
    }

    let mut prelude_modules = Vec::new();
    for (_name, source) in kodo_std::prelude_sources() {
        match kodo_parser::parse(source) {
            Ok(m) => prelude_modules.push(m),
            Err(e) => {
                eprintln!("stdlib parse error: {e}");
                return 1;
            }
        }
    }

    let mut checker = kodo_types::TypeChecker::new();
    for prelude in &prelude_modules {
        if let Err(e) = checker.check_module(prelude) {
            eprintln!("stdlib type error: {e}");
            return 1;
        }
    }
    for imported in &imported_modules {
        if let Err(e) = checker.check_module(imported) {
            eprintln!("type error in imported module `{}`: {e}", imported.name);
            return 1;
        }
        checker.register_imported_module(imported.name.clone());
        checker.register_module_visibility(imported);
    }
    let type_errors = checker.check_module_collecting(&module);
    if !type_errors.is_empty() {
        for e in &type_errors {
            diagnostics::render_type_error(&source, &filename, e);
        }
        return 1;
    }

    // Contract verification.
    let contract_mode = parse_contract_mode(contracts_mode_str);
    for func in &module.functions {
        let contracts = kodo_contracts::extract_contracts(func);
        if let Err(e) = kodo_contracts::verify_contracts(&contracts, contract_mode) {
            eprintln!("contract error: {e}");
            return 1;
        }
    }

    // Intent resolution.
    if !module.intent_decls.is_empty() {
        let resolver = kodo_resolver::Resolver::with_builtins();
        match resolver.resolve_all(&module.intent_decls) {
            Ok(resolved_intents) => {
                for resolved in resolved_intents {
                    for func in resolved.generated_functions {
                        let already_exists = module.functions.iter().any(|f| f.name == func.name);
                        if !already_exists {
                            module.functions.push(func);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("resolver error: {e}");
                return 1;
            }
        }
    }

    rewrite_map_for_in(&mut module, checker.map_for_in_spans());
    kodo_desugar::desugar_module(&mut module);

    // Impl blocks -> top-level functions.
    let default_methods = checker.trait_default_methods().clone();
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
        if let Some(ref trait_name) = impl_block.trait_name {
            if let Some(defaults) = default_methods.get(trait_name) {
                for (name, trait_method) in defaults {
                    let overridden = impl_block.methods.iter().any(|m| m.name == *name);
                    if !overridden {
                        if let Some(ref body) = trait_method.body {
                            let mut params = trait_method.params.clone();
                            for param in &mut params {
                                if param.name == "self" {
                                    param.ty =
                                        kodo_ast::TypeExpr::Named(impl_block.type_name.clone());
                                }
                            }
                            let mut body = body.clone();
                            rewrite_self_method_calls_in_block(&mut body, &impl_block.type_name);
                            let func = kodo_ast::Function {
                                id: kodo_ast::NodeId(0),
                                name: format!("{}_{name}", impl_block.type_name),
                                visibility: kodo_ast::Visibility::Private,
                                params,
                                return_type: trait_method.return_type.clone(),
                                body,
                                span: trait_method.span,
                                is_async: false,
                                annotations: Vec::new(),
                                generic_params: Vec::new(),
                                requires: Vec::new(),
                                ensures: Vec::new(),
                            };
                            module.functions.push(func);
                        }
                    }
                }
            }
        }
    }

    // Method call rewriting.
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
    }

    inject_stdlib_method_functions(&mut module);

    // Monomorphize generics.
    let mut generated_fns: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (base_name, type_args, mono_name) in checker.fn_instances() {
        if generated_fns.contains(mono_name) {
            continue;
        }
        generated_fns.insert(mono_name.clone());
        if let Some(generic_fn) = module
            .functions
            .iter()
            .find(|f| f.name == *base_name)
            .cloned()
        {
            let subst: std::collections::HashMap<String, kodo_ast::TypeExpr> = generic_fn
                .generic_params
                .iter()
                .zip(type_args)
                .map(|(param, ty)| (param.name.clone(), type_to_type_expr(ty)))
                .collect();
            let mut mono_fn = generic_fn;
            mono_fn.name = mono_name.clone();
            mono_fn.generic_params = vec![];
            for param in &mut mono_fn.params {
                param.ty = substitute_type_expr_ast(&param.ty, &subst);
            }
            mono_fn.return_type = substitute_type_expr_ast(&mono_fn.return_type, &subst);
            module.functions.push(mono_fn);
        }
    }

    // MIR lowering.
    let mut all_mir_functions = Vec::new();
    for imported in &imported_modules {
        match kodo_mir::lowering::lower_module_with_type_info(
            imported,
            checker.struct_registry(),
            checker.enum_registry(),
            checker.enum_names(),
            checker.type_alias_registry(),
            checker.trait_registry(),
        ) {
            Ok(fns) => all_mir_functions.extend(fns),
            Err(e) => {
                eprintln!("MIR lowering error in imported module: {e}");
                return 1;
            }
        }
    }
    let mir_functions = match kodo_mir::lowering::lower_module_with_type_info(
        &module,
        checker.struct_registry(),
        checker.enum_registry(),
        checker.enum_names(),
        checker.type_alias_registry(),
        checker.trait_registry(),
    ) {
        Ok(fns) => fns,
        Err(e) => {
            eprintln!("MIR lowering error: {e}");
            return 1;
        }
    };
    all_mir_functions.extend(mir_functions);
    kodo_mir::optimize::optimize_all(&mut all_mir_functions);

    // Insert green thread yield points (cooperative scheduling).
    kodo_mir::yield_insertion::insert_yield_points(&mut all_mir_functions);

    if contract_mode == kodo_contracts::ContractMode::Recoverable {
        kodo_mir::apply_recoverable_contracts(&mut all_mir_functions);
    }

    // Code generation.
    let struct_defs = checker.struct_registry().clone();
    let enum_defs = checker.enum_registry().clone();
    let options = kodo_codegen::CodegenOptions::default();
    let vtable_defs = build_vtable_defs(&checker);

    let object_bytes = match kodo_codegen::compile_module_with_vtables(
        &all_mir_functions,
        &struct_defs,
        &enum_defs,
        &vtable_defs,
        &options,
        Some("{}"),
    ) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("codegen error: {e}");
            return 1;
        }
    };

    // Write, link, and run the test binary.
    let output_path = std::env::temp_dir().join(format!("kodo_test_{}", std::process::id()));
    let obj_path = output_path.with_extension("o");
    if let Err(e) = std::fs::write(&obj_path, &object_bytes) {
        eprintln!("error: could not write object file: {e}");
        return 1;
    }
    let link_result = link_executable(&obj_path, &output_path);
    let _ = std::fs::remove_file(&obj_path);

    match link_result {
        Ok(()) => {
            let mut cmd = std::process::Command::new(&output_path);
            if json {
                cmd.env("KODO_TEST_JSON", "1");
            }
            let status = cmd.status();
            let _ = std::fs::remove_file(&output_path);
            match status {
                Ok(s) => s.code().unwrap_or(1),
                Err(e) => {
                    eprintln!("error: could not execute test binary: {e}");
                    1
                }
            }
        }
        Err(e) => {
            eprintln!("link error: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_ast::{Annotation, AnnotationArg, Expr, Span, Stmt, TypeExpr};

    fn s() -> Span {
        Span::new(0, 0)
    }

    fn make_property_ann(args: Vec<AnnotationArg>) -> Annotation {
        Annotation {
            name: "property".to_string(),
            args,
            span: s(),
        }
    }

    #[test]
    fn extract_property_params_defaults() {
        let ann = make_property_ann(vec![]);
        let params = extract_property_params(&[ann]).unwrap();
        assert_eq!(params.iterations, 100);
        assert_eq!(params.int_min, -1_000_000);
        assert_eq!(params.int_max, 1_000_000);
        assert_eq!(params.seed, 0);
        assert_eq!(params.max_string_len, 100);
    }

    #[test]
    fn extract_property_params_custom_iterations() {
        let ann = make_property_ann(vec![AnnotationArg::Named(
            "iterations".to_string(),
            Expr::IntLit(50, s()),
        )]);
        let params = extract_property_params(&[ann]).unwrap();
        assert_eq!(params.iterations, 50);
    }

    #[test]
    fn extract_property_params_custom_seed() {
        let ann = make_property_ann(vec![AnnotationArg::Named(
            "seed".to_string(),
            Expr::IntLit(42, s()),
        )]);
        let params = extract_property_params(&[ann]).unwrap();
        assert_eq!(params.seed, 42);
    }

    #[test]
    fn extract_property_params_negative_int_min() {
        let ann = make_property_ann(vec![AnnotationArg::Named(
            "int_min".to_string(),
            Expr::UnaryOp {
                op: kodo_ast::UnaryOp::Neg,
                operand: Box::new(Expr::IntLit(100, s())),
                span: s(),
            },
        )]);
        let params = extract_property_params(&[ann]).unwrap();
        assert_eq!(params.int_min, -100);
    }

    #[test]
    fn extract_property_params_returns_none_without_annotation() {
        let ann = Annotation {
            name: "skip".to_string(),
            args: vec![],
            span: s(),
        };
        assert!(extract_property_params(&[ann]).is_none());
    }

    #[test]
    fn desugar_forall_produces_while_loop() {
        let forall = Stmt::ForAll {
            span: s(),
            bindings: vec![
                ("a".to_string(), TypeExpr::Named("Int".to_string())),
                ("b".to_string(), TypeExpr::Named("Int".to_string())),
            ],
            body: kodo_ast::Block {
                span: s(),
                stmts: vec![Stmt::Expr(Expr::IntLit(1, s()))],
            },
        };
        let params = PropertyParams {
            iterations: 50,
            seed: 42,
            ..PropertyParams::default()
        };
        let result = desugar_forall(&forall, &params);
        // Should produce: kodo_prop_start call, let __prop_iter, while loop = 3 statements
        assert_eq!(result.len(), 3);
        // First is the kodo_prop_start call
        assert!(matches!(result[0], Stmt::Expr(Expr::Call { .. })));
        // Second is the let __prop_iter
        assert!(matches!(result[1], Stmt::Let { ref name, .. } if name == "__prop_iter"));
        // Third is the while loop
        assert!(matches!(result[2], Stmt::While { .. }));
    }

    #[test]
    fn desugar_forall_passthrough_non_forall() {
        let stmt = Stmt::Expr(Expr::IntLit(42, s()));
        let params = PropertyParams::default();
        let result = desugar_forall(&stmt, &params);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn apply_property_desugaring_replaces_forall() {
        let mut body = kodo_ast::Block {
            span: s(),
            stmts: vec![
                Stmt::Expr(Expr::IntLit(1, s())),
                Stmt::ForAll {
                    span: s(),
                    bindings: vec![("x".to_string(), TypeExpr::Named("Int".to_string()))],
                    body: kodo_ast::Block {
                        span: s(),
                        stmts: vec![Stmt::Expr(Expr::IntLit(2, s()))],
                    },
                },
            ],
        };
        let params = PropertyParams::default();
        apply_property_desugaring(&mut body, &params);
        // Original 1 stmt + 3 desugared stmts = 4
        assert_eq!(body.stmts.len(), 4);
    }
}
