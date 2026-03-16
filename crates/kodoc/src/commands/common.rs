//! Shared helper functions used across multiple command implementations.
//!
//! This module contains utilities for import resolution, linking, method call
//! rewriting, stdlib injection, vtable construction, and other common operations
//! needed by the build, test, check, and MIR commands.

use std::path::PathBuf;

/// Parses a `--contracts` flag value into a [`kodo_contracts::ContractMode`].
pub(crate) fn parse_contract_mode(value: &str) -> kodo_contracts::ContractMode {
    match value.to_lowercase().as_str() {
        "static" => kodo_contracts::ContractMode::Static,
        "runtime" => kodo_contracts::ContractMode::Runtime,
        "both" => kodo_contracts::ContractMode::Both,
        "none" => kodo_contracts::ContractMode::None,
        "recoverable" => kodo_contracts::ContractMode::Recoverable,
        _ => {
            eprintln!("warning: unknown contract mode `{value}`, falling back to `runtime`");
            kodo_contracts::ContractMode::Runtime
        }
    }
}

/// Resolves an import path to a `.ko` file path.
///
/// Tries `<base_dir>/<segments>.ko` first, then `<base_dir>/<segments>/lib.ko`.
///
/// `import math.utils` resolves to `<base_dir>/math/utils.ko`
/// or `<base_dir>/math/utils/lib.ko`.
pub(crate) fn resolve_import_path(
    base_dir: &std::path::Path,
    segments: &[String],
) -> std::path::PathBuf {
    let mut path = base_dir.to_path_buf();
    for segment in segments {
        path.push(segment);
    }
    path.set_extension("ko");

    // Try lib.ko fallback if the direct path doesn't exist.
    if !path.exists() {
        let mut alt_path = base_dir.to_path_buf();
        for segment in segments {
            alt_path.push(segment);
        }
        alt_path.push("lib.ko");
        if alt_path.exists() {
            return alt_path;
        }
    }

    path
}

/// Compiles an imported module and returns its parsed AST.
///
/// Checks for import cycles using the `visited` set. Returns an error
/// if a cycle is detected.
pub(crate) fn compile_imported_module(
    path: &std::path::Path,
    _object_files: &mut Vec<std::path::PathBuf>,
) -> std::result::Result<kodo_ast::Module, String> {
    let source = std::fs::read_to_string(path).map_err(|_| {
        let mut msg = format!("error: unresolved import `{}`", path.display());
        // Suggest similar .ko files in the same directory.
        if let Some(parent) = path.parent() {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if let Ok(entries) = std::fs::read_dir(parent) {
                    let candidates: Vec<String> = entries
                        .filter_map(|e| e.ok())
                        .filter_map(|e| {
                            let name = e.file_name();
                            let name_str = name.to_str()?;
                            if name_str.ends_with(".ko") {
                                Some(name_str.trim_end_matches(".ko").to_string())
                            } else {
                                None
                            }
                        })
                        .collect();
                    if let Some(suggestion) = find_similar_import(stem, &candidates) {
                        msg.push_str(&format!("\n  hint: did you mean `{suggestion}`?"));
                    }
                }
            }
        }
        msg
    })?;
    let module = kodo_parser::parse(&source)
        .map_err(|e| format!("parse error in `{}`: {e}", path.display()))?;
    Ok(module)
}

/// Finds the most similar import name using Levenshtein distance.
pub(crate) fn find_similar_import(name: &str, candidates: &[String]) -> Option<String> {
    let threshold = std::cmp::max(name.len() / 2, 3);
    let mut best: Option<(usize, String)> = None;
    for candidate in candidates {
        let dist = strsim::levenshtein(name, candidate);
        if dist > 0 && dist <= threshold && best.as_ref().is_none_or(|(d, _)| dist < *d) {
            best = Some((dist, candidate.clone()));
        }
    }
    best.map(|(_, n)| n)
}

/// Checks for import cycles by detecting if a file imports itself transitively.
pub(crate) fn check_import_cycles(
    base_dir: &std::path::Path,
    module: &kodo_ast::Module,
    visited: &mut std::collections::HashSet<std::path::PathBuf>,
    current_path: &std::path::Path,
) -> std::result::Result<(), String> {
    for import in &module.imports {
        let import_path = resolve_import_path(base_dir, &import.path);
        let canonical = import_path
            .canonicalize()
            .unwrap_or_else(|_| import_path.clone());
        if !visited.insert(canonical.clone()) {
            return Err(format!(
                "error: import cycle detected: `{}` is imported transitively from `{}`",
                import_path.display(),
                current_path.display()
            ));
        }
        // Recursively check the imported module for cycles.
        if let Ok(source) = std::fs::read_to_string(&import_path) {
            if let Ok(imported) = kodo_parser::parse(&source) {
                let imported_base = import_path
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."));
                check_import_cycles(imported_base, &imported, visited, &import_path)?;
            }
        }
    }
    Ok(())
}

/// Links an object file with the Kodo runtime to produce an executable.
pub(crate) fn link_executable(
    obj_path: &std::path::Path,
    output_path: &std::path::Path,
) -> std::result::Result<(), String> {
    // Find the runtime library.
    // Strategy: look relative to the kodoc binary, then in the workspace target dir.
    let runtime_path = find_runtime_lib()?;

    let mut cmd = std::process::Command::new("cc");
    cmd.arg(obj_path)
        .arg(&runtime_path)
        .arg("-o")
        .arg(output_path);

    // On macOS, Cranelift object files lack LC_BUILD_VERSION metadata,
    // producing harmless linker warnings. Suppress them.
    if cfg!(target_os = "macos") {
        cmd.arg("-Wl,-w");
    }

    let status = cmd
        .status()
        .map_err(|e| format!("failed to invoke linker `cc`: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "linker failed with exit code {}",
            status.code().unwrap_or(-1)
        ))
    }
}

/// Locates `libkodo_runtime.a` -- checks env, exe dir, cargo targets,
/// and falls back to the embedded copy.
pub(crate) fn find_runtime_lib() -> std::result::Result<PathBuf, String> {
    crate::embedded_runtime::find_runtime_lib()
}

/// Builds vtable definitions from the type checker's trait registries.
///
/// For each `(concrete_type, trait_name)` pair found in the trait impl set,
/// produces an ordered list of mangled function names matching the trait's
/// method declaration order.
pub(crate) fn build_vtable_defs(
    checker: &kodo_types::TypeChecker,
) -> std::collections::HashMap<(String, String), kodo_codegen::VtableDef> {
    let mut vtable_defs = std::collections::HashMap::new();
    let trait_impl_set = checker.trait_impl_set();
    let trait_registry = checker.trait_registry();
    let method_lookup = checker.method_lookup();

    for (type_name, traits) in trait_impl_set {
        for trait_name in traits {
            if let Some(trait_methods) = trait_registry.get(trait_name) {
                let mut method_names = Vec::with_capacity(trait_methods.len());
                for (method_name, _, _) in trait_methods {
                    // Look up the mangled name for this (type, method) pair.
                    let key = (type_name.clone(), method_name.clone());
                    if let Some((mangled_name, _, _)) = method_lookup.get(&key) {
                        method_names.push(mangled_name.clone());
                    } else {
                        // Fallback: use a simple mangling scheme.
                        method_names.push(format!("{type_name}_{method_name}"));
                    }
                }
                vtable_defs.insert((type_name.clone(), trait_name.clone()), method_names);
            }
        }
    }
    vtable_defs
}

/// Builds a JSON string with module metadata for embedding in the binary.
pub(crate) fn build_module_metadata(module: &kodo_ast::Module) -> String {
    let meta = module.meta.as_ref();
    let purpose = meta
        .and_then(|m| m.entries.iter().find(|e| e.key == "purpose"))
        .map_or_else(String::new, |e| e.value.clone());
    let version = meta
        .and_then(|m| m.entries.iter().find(|e| e.key == "version"))
        .map_or_else(String::new, |e| e.value.clone());

    let mut functions = Vec::new();
    let mut validators = Vec::new();
    for func in &module.functions {
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

        let requires: Vec<String> = func
            .requires
            .iter()
            .enumerate()
            .map(|(i, _)| format!("requires clause {}", i + 1))
            .collect();

        let ensures: Vec<String> = func
            .ensures
            .iter()
            .enumerate()
            .map(|(i, _)| format!("ensures clause {}", i + 1))
            .collect();

        let mut annotations = serde_json::Map::new();
        for ann in &func.annotations {
            let value = match ann.args.first() {
                Some(kodo_ast::AnnotationArg::Positional(kodo_ast::Expr::IntLit(n, _))) => {
                    serde_json::json!(n)
                }
                Some(kodo_ast::AnnotationArg::Positional(kodo_ast::Expr::StringLit(s, _))) => {
                    serde_json::json!(s)
                }
                Some(kodo_ast::AnnotationArg::Named(_, kodo_ast::Expr::StringLit(s, _))) => {
                    serde_json::json!(s)
                }
                _ => serde_json::json!(true),
            };
            annotations.insert(ann.name.clone(), value);
        }

        functions.push(serde_json::json!({
            "name": func.name,
            "params": params,
            "return_type": format!("{:?}", func.return_type),
            "requires": requires,
            "ensures": ensures,
            "annotations": annotations,
        }));

        if !func.requires.is_empty() {
            validators.push(format!("validate_{}", func.name));
        }
    }

    let metadata = serde_json::json!({
        "module": module.name,
        "purpose": purpose,
        "version": version,
        "compiler_version": env!("CARGO_PKG_VERSION"),
        "functions": functions,
        "validators": validators,
    });

    // This can only fail on non-UTF-8 data which we don't have.
    serde_json::to_string_pretty(&metadata).unwrap_or_default()
}

/// Injects synthetic AST functions for stdlib enum methods (Option/Result).
///
/// These are generated after type checking because the type checker registers
/// their signatures in `method_lookup` but cannot type-check bodies that use
/// bare generic enum names. The generated functions use match expressions to
/// inspect the enum discriminant.
pub(crate) fn inject_stdlib_method_functions(module: &mut kodo_ast::Module) {
    let s = kodo_ast::Span::new(0, 0);

    // Helper: build a match-based bool-returning method for an enum.
    // `positive_variant` is the variant for which the method returns `true`.
    let make_bool_method =
        |name: &str, enum_name: &str, positive_variant: &str, _negative_variant: &str| {
            kodo_ast::Function {
                id: kodo_ast::NodeId(0),
                name: name.to_string(),
                visibility: kodo_ast::Visibility::Private,
                params: vec![kodo_ast::Param {
                    name: "self".to_string(),
                    ty: kodo_ast::TypeExpr::Named(enum_name.to_string()),
                    span: s,
                    ownership: kodo_ast::Ownership::Owned,
                }],
                return_type: kodo_ast::TypeExpr::Named("Bool".to_string()),
                body: kodo_ast::Block {
                    span: s,
                    stmts: vec![kodo_ast::Stmt::Return {
                        span: s,
                        value: Some(kodo_ast::Expr::Match {
                            span: s,
                            expr: Box::new(kodo_ast::Expr::Ident("self".to_string(), s)),
                            arms: vec![
                                kodo_ast::MatchArm {
                                    pattern: kodo_ast::Pattern::Variant {
                                        enum_name: Some(enum_name.to_string()),
                                        variant: positive_variant.to_string(),
                                        bindings: vec!["_v".to_string()],
                                        span: s,
                                    },
                                    body: kodo_ast::Expr::BoolLit(true, s),
                                    span: s,
                                },
                                kodo_ast::MatchArm {
                                    pattern: kodo_ast::Pattern::Wildcard(s),
                                    body: kodo_ast::Expr::BoolLit(false, s),
                                    span: s,
                                },
                            ],
                        }),
                    }],
                },
                span: s,
                is_async: false,
                annotations: Vec::new(),
                generic_params: Vec::new(),
                requires: Vec::new(),
                ensures: Vec::new(),
            }
        };

    // Helper: build unwrap_or method that returns the payload or a default.
    let make_unwrap_or = |name: &str, enum_name: &str, success_variant: &str| kodo_ast::Function {
        id: kodo_ast::NodeId(0),
        name: name.to_string(),
        visibility: kodo_ast::Visibility::Private,
        params: vec![
            kodo_ast::Param {
                name: "self".to_string(),
                ty: kodo_ast::TypeExpr::Named(enum_name.to_string()),
                span: s,
                ownership: kodo_ast::Ownership::Owned,
            },
            kodo_ast::Param {
                name: "default".to_string(),
                ty: kodo_ast::TypeExpr::Named("Int".to_string()),
                span: s,
                ownership: kodo_ast::Ownership::Owned,
            },
        ],
        return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
        body: kodo_ast::Block {
            span: s,
            stmts: vec![kodo_ast::Stmt::Return {
                span: s,
                value: Some(kodo_ast::Expr::Match {
                    span: s,
                    expr: Box::new(kodo_ast::Expr::Ident("self".to_string(), s)),
                    arms: vec![
                        kodo_ast::MatchArm {
                            pattern: kodo_ast::Pattern::Variant {
                                enum_name: Some(enum_name.to_string()),
                                variant: success_variant.to_string(),
                                bindings: vec!["_v".to_string()],
                                span: s,
                            },
                            body: kodo_ast::Expr::Ident("_v".to_string(), s),
                            span: s,
                        },
                        kodo_ast::MatchArm {
                            pattern: kodo_ast::Pattern::Wildcard(s),
                            body: kodo_ast::Expr::Ident("default".to_string(), s),
                            span: s,
                        },
                    ],
                }),
            }],
        },
        span: s,
        is_async: false,
        annotations: Vec::new(),
        generic_params: Vec::new(),
        requires: Vec::new(),
        ensures: Vec::new(),
    };

    // Option methods: is_some matches Some(v) -> true, _ -> false
    module
        .functions
        .push(make_bool_method("Option_is_some", "Option", "Some", "None"));
    // is_none: match Some(v) -> false, _ -> true (note: positive_variant returns true)
    // For is_none, we want None->true, Some->false. Use Wildcard for simplicity.
    module.functions.push({
        let mut f = make_bool_method("Option_is_none", "Option", "Some", "None");
        // Swap the bool values: Some->false, _->true
        if let Some(kodo_ast::Stmt::Return {
            value: Some(kodo_ast::Expr::Match { arms, .. }),
            ..
        }) = f.body.stmts.first_mut()
        {
            arms[0].body = kodo_ast::Expr::BoolLit(false, s);
            arms[1].body = kodo_ast::Expr::BoolLit(true, s);
        }
        f
    });
    module
        .functions
        .push(make_unwrap_or("Option_unwrap_or", "Option", "Some"));

    // Result methods
    module
        .functions
        .push(make_bool_method("Result_is_ok", "Result", "Ok", "Err"));
    module.functions.push({
        let mut f = make_bool_method("Result_is_err", "Result", "Ok", "Err");
        if let Some(kodo_ast::Stmt::Return {
            value: Some(kodo_ast::Expr::Match { arms, .. }),
            ..
        }) = f.body.stmts.first_mut()
        {
            arms[0].body = kodo_ast::Expr::BoolLit(false, s);
            arms[1].body = kodo_ast::Expr::BoolLit(true, s);
        }
        f
    });
    module
        .functions
        .push(make_unwrap_or("Result_unwrap_or", "Result", "Ok"));

    // -- Phase 48: Functional combinators on List<Int> --

    let list_param = kodo_ast::Param {
        name: "self".to_string(),
        ty: kodo_ast::TypeExpr::Generic(
            "List".to_string(),
            vec![kodo_ast::TypeExpr::Named("Int".to_string())],
        ),
        span: s,
        ownership: kodo_ast::Ownership::Owned,
    };

    let fn_int_int_ty = kodo_ast::TypeExpr::Function(
        vec![kodo_ast::TypeExpr::Named("Int".to_string())],
        Box::new(kodo_ast::TypeExpr::Named("Int".to_string())),
    );
    let fn_int_bool_ty = kodo_ast::TypeExpr::Function(
        vec![kodo_ast::TypeExpr::Named("Int".to_string())],
        Box::new(kodo_ast::TypeExpr::Named("Bool".to_string())),
    );

    // Helper: creates an iterator-based loop body with setup and teardown.
    // Returns (iter_var_name, setup_stmts_before_while, while_condition).
    let iter_var = "__comb_iter";
    let elem_var = "__comb_elem";

    let make_iter_setup = || -> Vec<kodo_ast::Stmt> {
        vec![kodo_ast::Stmt::Let {
            span: s,
            mutable: false,
            name: iter_var.to_string(),
            ty: None,
            value: kodo_ast::Expr::Call {
                callee: Box::new(kodo_ast::Expr::Ident("list_iter".to_string(), s)),
                args: vec![kodo_ast::Expr::Ident("self".to_string(), s)],
                span: s,
            },
        }]
    };

    let make_iter_condition = || -> kodo_ast::Expr {
        kodo_ast::Expr::BinaryOp {
            left: Box::new(kodo_ast::Expr::Call {
                callee: Box::new(kodo_ast::Expr::Ident(
                    "list_iterator_advance".to_string(),
                    s,
                )),
                args: vec![kodo_ast::Expr::Ident(iter_var.to_string(), s)],
                span: s,
            }),
            op: kodo_ast::BinOp::Gt,
            right: Box::new(kodo_ast::Expr::IntLit(0, s)),
            span: s,
        }
    };

    let make_elem_let = || -> kodo_ast::Stmt {
        kodo_ast::Stmt::Let {
            span: s,
            mutable: false,
            name: elem_var.to_string(),
            ty: None,
            value: kodo_ast::Expr::Call {
                callee: Box::new(kodo_ast::Expr::Ident("list_iterator_value".to_string(), s)),
                args: vec![kodo_ast::Expr::Ident(iter_var.to_string(), s)],
                span: s,
            },
        }
    };

    let make_iter_free = || -> kodo_ast::Stmt {
        kodo_ast::Stmt::Expr(kodo_ast::Expr::Call {
            callee: Box::new(kodo_ast::Expr::Ident("list_iterator_free".to_string(), s)),
            args: vec![kodo_ast::Expr::Ident(iter_var.to_string(), s)],
            span: s,
        })
    };

    let list_ret = kodo_ast::TypeExpr::Generic(
        "List".to_string(),
        vec![kodo_ast::TypeExpr::Named("Int".to_string())],
    );

    // List_map(self: List<Int>, f: (Int) -> Int) -> List<Int>
    {
        let result_var = "__map_result";
        let val_var = "__map_val";
        let mut body_stmts = make_iter_setup();
        // let __map_result = list_new()
        body_stmts.insert(
            0,
            kodo_ast::Stmt::Let {
                span: s,
                mutable: false,
                name: result_var.to_string(),
                ty: None,
                value: kodo_ast::Expr::Call {
                    callee: Box::new(kodo_ast::Expr::Ident("list_new".to_string(), s)),
                    args: vec![],
                    span: s,
                },
            },
        );
        let while_body = kodo_ast::Block {
            span: s,
            stmts: vec![
                make_elem_let(),
                // let __map_val = f(__comb_elem)
                kodo_ast::Stmt::Let {
                    span: s,
                    mutable: false,
                    name: val_var.to_string(),
                    ty: None,
                    value: kodo_ast::Expr::Call {
                        callee: Box::new(kodo_ast::Expr::Ident("f".to_string(), s)),
                        args: vec![kodo_ast::Expr::Ident(elem_var.to_string(), s)],
                        span: s,
                    },
                },
                // list_push(__map_result, __map_val)
                kodo_ast::Stmt::Expr(kodo_ast::Expr::Call {
                    callee: Box::new(kodo_ast::Expr::Ident("list_push".to_string(), s)),
                    args: vec![
                        kodo_ast::Expr::Ident(result_var.to_string(), s),
                        kodo_ast::Expr::Ident(val_var.to_string(), s),
                    ],
                    span: s,
                }),
            ],
        };
        body_stmts.push(kodo_ast::Stmt::While {
            span: s,
            condition: make_iter_condition(),
            body: while_body,
        });
        body_stmts.push(make_iter_free());
        body_stmts.push(kodo_ast::Stmt::Return {
            span: s,
            value: Some(kodo_ast::Expr::Ident(result_var.to_string(), s)),
        });

        module.functions.push(kodo_ast::Function {
            id: kodo_ast::NodeId(0),
            name: "List_map".to_string(),
            visibility: kodo_ast::Visibility::Private,
            params: vec![
                list_param.clone(),
                kodo_ast::Param {
                    name: "f".to_string(),
                    ty: fn_int_int_ty.clone(),
                    span: s,
                    ownership: kodo_ast::Ownership::Owned,
                },
            ],
            return_type: list_ret.clone(),
            body: kodo_ast::Block {
                span: s,
                stmts: body_stmts,
            },
            span: s,
            is_async: false,
            annotations: Vec::new(),
            generic_params: Vec::new(),
            requires: Vec::new(),
            ensures: Vec::new(),
        });
    }

    // List_filter(self: List<Int>, f: (Int) -> Bool) -> List<Int>
    {
        let result_var = "__filter_result";
        let mut body_stmts = vec![kodo_ast::Stmt::Let {
            span: s,
            mutable: false,
            name: result_var.to_string(),
            ty: None,
            value: kodo_ast::Expr::Call {
                callee: Box::new(kodo_ast::Expr::Ident("list_new".to_string(), s)),
                args: vec![],
                span: s,
            },
        }];
        body_stmts.extend(make_iter_setup());
        let while_body = kodo_ast::Block {
            span: s,
            stmts: vec![
                make_elem_let(),
                // if f(__comb_elem) { list_push(__filter_result, __comb_elem) }
                kodo_ast::Stmt::Expr(kodo_ast::Expr::If {
                    span: s,
                    condition: Box::new(kodo_ast::Expr::Call {
                        callee: Box::new(kodo_ast::Expr::Ident("f".to_string(), s)),
                        args: vec![kodo_ast::Expr::Ident(elem_var.to_string(), s)],
                        span: s,
                    }),
                    then_branch: kodo_ast::Block {
                        span: s,
                        stmts: vec![kodo_ast::Stmt::Expr(kodo_ast::Expr::Call {
                            callee: Box::new(kodo_ast::Expr::Ident("list_push".to_string(), s)),
                            args: vec![
                                kodo_ast::Expr::Ident(result_var.to_string(), s),
                                kodo_ast::Expr::Ident(elem_var.to_string(), s),
                            ],
                            span: s,
                        })],
                    },
                    else_branch: None,
                }),
            ],
        };
        body_stmts.push(kodo_ast::Stmt::While {
            span: s,
            condition: make_iter_condition(),
            body: while_body,
        });
        body_stmts.push(make_iter_free());
        body_stmts.push(kodo_ast::Stmt::Return {
            span: s,
            value: Some(kodo_ast::Expr::Ident(result_var.to_string(), s)),
        });

        module.functions.push(kodo_ast::Function {
            id: kodo_ast::NodeId(0),
            name: "List_filter".to_string(),
            visibility: kodo_ast::Visibility::Private,
            params: vec![
                list_param.clone(),
                kodo_ast::Param {
                    name: "f".to_string(),
                    ty: fn_int_bool_ty.clone(),
                    span: s,
                    ownership: kodo_ast::Ownership::Owned,
                },
            ],
            return_type: list_ret.clone(),
            body: kodo_ast::Block {
                span: s,
                stmts: body_stmts,
            },
            span: s,
            is_async: false,
            annotations: Vec::new(),
            generic_params: Vec::new(),
            requires: Vec::new(),
            ensures: Vec::new(),
        });
    }

    // List_fold(self: List<Int>, init: Int, f: (Int, Int) -> Int) -> Int
    {
        let acc_var = "__fold_acc";
        let mut body_stmts = vec![kodo_ast::Stmt::Let {
            span: s,
            mutable: true,
            name: acc_var.to_string(),
            ty: None,
            value: kodo_ast::Expr::Ident("init".to_string(), s),
        }];
        body_stmts.extend(make_iter_setup());
        let while_body = kodo_ast::Block {
            span: s,
            stmts: vec![
                make_elem_let(),
                // __fold_acc = f(__fold_acc, __comb_elem)
                kodo_ast::Stmt::Assign {
                    span: s,
                    name: acc_var.to_string(),
                    value: kodo_ast::Expr::Call {
                        callee: Box::new(kodo_ast::Expr::Ident("f".to_string(), s)),
                        args: vec![
                            kodo_ast::Expr::Ident(acc_var.to_string(), s),
                            kodo_ast::Expr::Ident(elem_var.to_string(), s),
                        ],
                        span: s,
                    },
                },
            ],
        };
        body_stmts.push(kodo_ast::Stmt::While {
            span: s,
            condition: make_iter_condition(),
            body: while_body,
        });
        body_stmts.push(make_iter_free());
        body_stmts.push(kodo_ast::Stmt::Return {
            span: s,
            value: Some(kodo_ast::Expr::Ident(acc_var.to_string(), s)),
        });

        module.functions.push(kodo_ast::Function {
            id: kodo_ast::NodeId(0),
            name: "List_fold".to_string(),
            visibility: kodo_ast::Visibility::Private,
            params: vec![
                list_param.clone(),
                kodo_ast::Param {
                    name: "init".to_string(),
                    ty: kodo_ast::TypeExpr::Named("Int".to_string()),
                    span: s,
                    ownership: kodo_ast::Ownership::Owned,
                },
                kodo_ast::Param {
                    name: "f".to_string(),
                    ty: kodo_ast::TypeExpr::Function(
                        vec![
                            kodo_ast::TypeExpr::Named("Int".to_string()),
                            kodo_ast::TypeExpr::Named("Int".to_string()),
                        ],
                        Box::new(kodo_ast::TypeExpr::Named("Int".to_string())),
                    ),
                    span: s,
                    ownership: kodo_ast::Ownership::Owned,
                },
            ],
            return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
            body: kodo_ast::Block {
                span: s,
                stmts: body_stmts,
            },
            span: s,
            is_async: false,
            annotations: Vec::new(),
            generic_params: Vec::new(),
            requires: Vec::new(),
            ensures: Vec::new(),
        });
    }

    // List_count(self: List<Int>) -> Int
    {
        // Simply calls list_length(self)
        module.functions.push(kodo_ast::Function {
            id: kodo_ast::NodeId(0),
            name: "List_count".to_string(),
            visibility: kodo_ast::Visibility::Private,
            params: vec![list_param.clone()],
            return_type: kodo_ast::TypeExpr::Named("Int".to_string()),
            body: kodo_ast::Block {
                span: s,
                stmts: vec![kodo_ast::Stmt::Return {
                    span: s,
                    value: Some(kodo_ast::Expr::Call {
                        callee: Box::new(kodo_ast::Expr::Ident("list_length".to_string(), s)),
                        args: vec![kodo_ast::Expr::Ident("self".to_string(), s)],
                        span: s,
                    }),
                }],
            },
            span: s,
            is_async: false,
            annotations: Vec::new(),
            generic_params: Vec::new(),
            requires: Vec::new(),
            ensures: Vec::new(),
        });
    }

    // List_any(self: List<Int>, f: (Int) -> Bool) -> Bool
    {
        let result_var = "__any_result";
        let mut body_stmts = vec![kodo_ast::Stmt::Let {
            span: s,
            mutable: true,
            name: result_var.to_string(),
            ty: None,
            value: kodo_ast::Expr::BoolLit(false, s),
        }];
        body_stmts.extend(make_iter_setup());
        let while_body = kodo_ast::Block {
            span: s,
            stmts: vec![
                make_elem_let(),
                // if f(__comb_elem) { __any_result = true; break }
                kodo_ast::Stmt::Expr(kodo_ast::Expr::If {
                    span: s,
                    condition: Box::new(kodo_ast::Expr::Call {
                        callee: Box::new(kodo_ast::Expr::Ident("f".to_string(), s)),
                        args: vec![kodo_ast::Expr::Ident(elem_var.to_string(), s)],
                        span: s,
                    }),
                    then_branch: kodo_ast::Block {
                        span: s,
                        stmts: vec![
                            kodo_ast::Stmt::Assign {
                                span: s,
                                name: result_var.to_string(),
                                value: kodo_ast::Expr::BoolLit(true, s),
                            },
                            kodo_ast::Stmt::Break { span: s },
                        ],
                    },
                    else_branch: None,
                }),
            ],
        };
        body_stmts.push(kodo_ast::Stmt::While {
            span: s,
            condition: make_iter_condition(),
            body: while_body,
        });
        body_stmts.push(make_iter_free());
        body_stmts.push(kodo_ast::Stmt::Return {
            span: s,
            value: Some(kodo_ast::Expr::Ident(result_var.to_string(), s)),
        });

        module.functions.push(kodo_ast::Function {
            id: kodo_ast::NodeId(0),
            name: "List_any".to_string(),
            visibility: kodo_ast::Visibility::Private,
            params: vec![
                list_param.clone(),
                kodo_ast::Param {
                    name: "f".to_string(),
                    ty: fn_int_bool_ty.clone(),
                    span: s,
                    ownership: kodo_ast::Ownership::Owned,
                },
            ],
            return_type: kodo_ast::TypeExpr::Named("Bool".to_string()),
            body: kodo_ast::Block {
                span: s,
                stmts: body_stmts,
            },
            span: s,
            is_async: false,
            annotations: Vec::new(),
            generic_params: Vec::new(),
            requires: Vec::new(),
            ensures: Vec::new(),
        });
    }

    // List_all(self: List<Int>, f: (Int) -> Bool) -> Bool
    {
        let result_var = "__all_result";
        let mut body_stmts = vec![kodo_ast::Stmt::Let {
            span: s,
            mutable: true,
            name: result_var.to_string(),
            ty: None,
            value: kodo_ast::Expr::BoolLit(true, s),
        }];
        body_stmts.extend(make_iter_setup());
        let while_body = kodo_ast::Block {
            span: s,
            stmts: vec![
                make_elem_let(),
                // if !f(__comb_elem) { __all_result = false; break }
                kodo_ast::Stmt::Expr(kodo_ast::Expr::If {
                    span: s,
                    condition: Box::new(kodo_ast::Expr::UnaryOp {
                        op: kodo_ast::UnaryOp::Not,
                        operand: Box::new(kodo_ast::Expr::Call {
                            callee: Box::new(kodo_ast::Expr::Ident("f".to_string(), s)),
                            args: vec![kodo_ast::Expr::Ident(elem_var.to_string(), s)],
                            span: s,
                        }),
                        span: s,
                    }),
                    then_branch: kodo_ast::Block {
                        span: s,
                        stmts: vec![
                            kodo_ast::Stmt::Assign {
                                span: s,
                                name: result_var.to_string(),
                                value: kodo_ast::Expr::BoolLit(false, s),
                            },
                            kodo_ast::Stmt::Break { span: s },
                        ],
                    },
                    else_branch: None,
                }),
            ],
        };
        body_stmts.push(kodo_ast::Stmt::While {
            span: s,
            condition: make_iter_condition(),
            body: while_body,
        });
        body_stmts.push(make_iter_free());
        body_stmts.push(kodo_ast::Stmt::Return {
            span: s,
            value: Some(kodo_ast::Expr::Ident(result_var.to_string(), s)),
        });

        module.functions.push(kodo_ast::Function {
            id: kodo_ast::NodeId(0),
            name: "List_all".to_string(),
            visibility: kodo_ast::Visibility::Private,
            params: vec![
                list_param,
                kodo_ast::Param {
                    name: "f".to_string(),
                    ty: fn_int_bool_ty,
                    span: s,
                    ownership: kodo_ast::Ownership::Owned,
                },
            ],
            return_type: kodo_ast::TypeExpr::Named("Bool".to_string()),
            body: kodo_ast::Block {
                span: s,
                stmts: body_stmts,
            },
            span: s,
            is_async: false,
            annotations: Vec::new(),
            generic_params: Vec::new(),
            requires: Vec::new(),
            ensures: Vec::new(),
        });
    }
}

/// Rewrites method calls in a block by replacing `obj.method(args)` with
/// `TypeName_method(obj, args)` where a method call was resolved during type checking.
pub(crate) fn rewrite_method_calls_in_block(
    block: &mut kodo_ast::Block,
    resolutions: &std::collections::HashMap<u32, String>,
) {
    for stmt in &mut block.stmts {
        match stmt {
            kodo_ast::Stmt::Let { value, .. } | kodo_ast::Stmt::Assign { value, .. } => {
                *value = rewrite_method_calls_in_expr(
                    std::mem::replace(value, kodo_ast::Expr::IntLit(0, kodo_ast::Span::new(0, 0))),
                    resolutions,
                );
            }
            kodo_ast::Stmt::Expr(expr) => {
                *expr = rewrite_method_calls_in_expr(
                    std::mem::replace(expr, kodo_ast::Expr::IntLit(0, kodo_ast::Span::new(0, 0))),
                    resolutions,
                );
            }
            kodo_ast::Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    *v = rewrite_method_calls_in_expr(
                        std::mem::replace(v, kodo_ast::Expr::IntLit(0, kodo_ast::Span::new(0, 0))),
                        resolutions,
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
                );
                rewrite_method_calls_in_block(body, resolutions);
            }
            kodo_ast::Stmt::For {
                start, end, body, ..
            } => {
                *start = rewrite_method_calls_in_expr(
                    std::mem::replace(start, kodo_ast::Expr::IntLit(0, kodo_ast::Span::new(0, 0))),
                    resolutions,
                );
                *end = rewrite_method_calls_in_expr(
                    std::mem::replace(end, kodo_ast::Expr::IntLit(0, kodo_ast::Span::new(0, 0))),
                    resolutions,
                );
                rewrite_method_calls_in_block(body, resolutions);
            }
            kodo_ast::Stmt::ForIn { iterable, body, .. } => {
                *iterable = rewrite_method_calls_in_expr(
                    std::mem::replace(
                        iterable,
                        kodo_ast::Expr::IntLit(0, kodo_ast::Span::new(0, 0)),
                    ),
                    resolutions,
                );
                rewrite_method_calls_in_block(body, resolutions);
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
                );
                rewrite_method_calls_in_block(body, resolutions);
                if let Some(eb) = else_body {
                    rewrite_method_calls_in_block(eb, resolutions);
                }
            }
            kodo_ast::Stmt::LetPattern { value, .. } => {
                *value = rewrite_method_calls_in_expr(
                    std::mem::replace(value, kodo_ast::Expr::IntLit(0, kodo_ast::Span::new(0, 0))),
                    resolutions,
                );
            }
            kodo_ast::Stmt::Spawn { body, .. } => {
                rewrite_method_calls_in_block(body, resolutions);
            }
            kodo_ast::Stmt::Parallel { body, .. } => {
                for stmt in body {
                    if let kodo_ast::Stmt::Spawn { body, .. } = stmt {
                        rewrite_method_calls_in_block(body, resolutions);
                    }
                }
            }
            // Break and Continue have no expressions to rewrite.
            kodo_ast::Stmt::Break { .. } | kodo_ast::Stmt::Continue { .. } => {}
        }
    }
}

/// Rewrites method calls in an expression. Uses span-based resolutions from the
/// type checker to detect `Call { callee: FieldAccess { object, field }, args }`
/// and rewrites to `Call { callee: Ident(mangled), args: [object, ...args] }`.
pub(crate) fn rewrite_method_calls_in_expr(
    expr: kodo_ast::Expr,
    resolutions: &std::collections::HashMap<u32, String>,
) -> kodo_ast::Expr {
    match expr {
        kodo_ast::Expr::Call { callee, args, span } => {
            if let kodo_ast::Expr::FieldAccess {
                object,
                field,
                span: fa_span,
            } = *callee
            {
                // Rewrite sub-expressions first
                let object = rewrite_method_calls_in_expr(*object, resolutions);
                let args: Vec<_> = args
                    .into_iter()
                    .map(|a| rewrite_method_calls_in_expr(a, resolutions))
                    .collect();

                // Check if this call span was resolved as a method call
                if let Some(mangled) = resolutions.get(&span.start) {
                    let mut new_args = vec![object];
                    new_args.extend(args);
                    return kodo_ast::Expr::Call {
                        callee: Box::new(kodo_ast::Expr::Ident(mangled.clone(), span)),
                        args: new_args,
                        span,
                    };
                }

                // Not a method call -- reconstruct as FieldAccess + Call
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
                let callee = rewrite_method_calls_in_expr(*callee, resolutions);
                let args: Vec<_> = args
                    .into_iter()
                    .map(|a| rewrite_method_calls_in_expr(a, resolutions))
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
            left: Box::new(rewrite_method_calls_in_expr(*left, resolutions)),
            op,
            right: Box::new(rewrite_method_calls_in_expr(*right, resolutions)),
            span,
        },
        kodo_ast::Expr::UnaryOp { op, operand, span } => kodo_ast::Expr::UnaryOp {
            op,
            operand: Box::new(rewrite_method_calls_in_expr(*operand, resolutions)),
            span,
        },
        kodo_ast::Expr::If {
            condition,
            mut then_branch,
            else_branch,
            span,
        } => {
            let condition = rewrite_method_calls_in_expr(*condition, resolutions);
            rewrite_method_calls_in_block(&mut then_branch, resolutions);
            let else_branch = else_branch.map(|mut b| {
                rewrite_method_calls_in_block(&mut b, resolutions);
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
            object: Box::new(rewrite_method_calls_in_expr(*object, resolutions)),
            field,
            span,
        },
        kodo_ast::Expr::StructLit { name, fields, span } => kodo_ast::Expr::StructLit {
            name,
            fields: fields
                .into_iter()
                .map(|f| kodo_ast::FieldInit {
                    name: f.name,
                    value: rewrite_method_calls_in_expr(f.value, resolutions),
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
                .map(|a| rewrite_method_calls_in_expr(a, resolutions))
                .collect(),
            span,
        },
        kodo_ast::Expr::Match { expr, arms, span } => kodo_ast::Expr::Match {
            expr: Box::new(rewrite_method_calls_in_expr(*expr, resolutions)),
            arms: arms
                .into_iter()
                .map(|arm| kodo_ast::MatchArm {
                    pattern: arm.pattern,
                    body: rewrite_method_calls_in_expr(arm.body, resolutions),
                    span: arm.span,
                })
                .collect(),
            span,
        },
        kodo_ast::Expr::Block(mut block) => {
            rewrite_method_calls_in_block(&mut block, resolutions);
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
            body: Box::new(rewrite_method_calls_in_expr(*body, resolutions)),
            span,
        },
        kodo_ast::Expr::NullCoalesce { left, right, span } => kodo_ast::Expr::NullCoalesce {
            left: Box::new(rewrite_method_calls_in_expr(*left, resolutions)),
            right: Box::new(rewrite_method_calls_in_expr(*right, resolutions)),
            span,
        },
        kodo_ast::Expr::Try { operand, span } => kodo_ast::Expr::Try {
            operand: Box::new(rewrite_method_calls_in_expr(*operand, resolutions)),
            span,
        },
        kodo_ast::Expr::OptionalChain {
            object,
            field,
            span,
        } => kodo_ast::Expr::OptionalChain {
            object: Box::new(rewrite_method_calls_in_expr(*object, resolutions)),
            field,
            span,
        },
        kodo_ast::Expr::Range {
            start,
            end,
            inclusive,
            span,
        } => kodo_ast::Expr::Range {
            start: Box::new(rewrite_method_calls_in_expr(*start, resolutions)),
            end: Box::new(rewrite_method_calls_in_expr(*end, resolutions)),
            inclusive,
            span,
        },
        kodo_ast::Expr::Is {
            operand,
            type_name,
            span,
        } => kodo_ast::Expr::Is {
            operand: Box::new(rewrite_method_calls_in_expr(*operand, resolutions)),
            type_name,
            span,
        },
        kodo_ast::Expr::Await { operand, span } => kodo_ast::Expr::Await {
            operand: Box::new(rewrite_method_calls_in_expr(*operand, resolutions)),
            span,
        },
        kodo_ast::Expr::StringInterp { parts, span } => {
            let parts = parts
                .into_iter()
                .map(|p| match p {
                    kodo_ast::StringPart::Literal(s) => kodo_ast::StringPart::Literal(s),
                    kodo_ast::StringPart::Expr(e) => kodo_ast::StringPart::Expr(Box::new(
                        rewrite_method_calls_in_expr(*e, resolutions),
                    )),
                })
                .collect();
            kodo_ast::Expr::StringInterp { parts, span }
        }
        kodo_ast::Expr::TupleLit(elems, span) => kodo_ast::Expr::TupleLit(
            elems
                .into_iter()
                .map(|e| rewrite_method_calls_in_expr(e, resolutions))
                .collect(),
            span,
        ),
        kodo_ast::Expr::TupleIndex { tuple, index, span } => kodo_ast::Expr::TupleIndex {
            tuple: Box::new(rewrite_method_calls_in_expr(*tuple, resolutions)),
            index,
            span,
        },
        // Leaf expressions -- no sub-expressions to rewrite
        e @ (kodo_ast::Expr::IntLit(_, _)
        | kodo_ast::Expr::FloatLit(_, _)
        | kodo_ast::Expr::StringLit(_, _)
        | kodo_ast::Expr::BoolLit(_, _)
        | kodo_ast::Expr::Ident(_, _)) => e,
    }
}

/// Rewrites `self.method(args)` calls in a block to `TypeName_method(self, args)`.
///
/// Used for default trait method bodies, where the span-based method resolution
/// from the type checker is not available.
pub(crate) fn rewrite_self_method_calls_in_block(block: &mut kodo_ast::Block, type_name: &str) {
    for stmt in &mut block.stmts {
        match stmt {
            kodo_ast::Stmt::Let { value, .. } | kodo_ast::Stmt::Assign { value, .. } => {
                *value = rewrite_self_method_calls_in_expr(
                    std::mem::replace(value, kodo_ast::Expr::IntLit(0, kodo_ast::Span::new(0, 0))),
                    type_name,
                );
            }
            kodo_ast::Stmt::Expr(expr) => {
                *expr = rewrite_self_method_calls_in_expr(
                    std::mem::replace(expr, kodo_ast::Expr::IntLit(0, kodo_ast::Span::new(0, 0))),
                    type_name,
                );
            }
            kodo_ast::Stmt::Return { value: Some(v), .. } => {
                *v = rewrite_self_method_calls_in_expr(
                    std::mem::replace(v, kodo_ast::Expr::IntLit(0, kodo_ast::Span::new(0, 0))),
                    type_name,
                );
            }
            _ => {}
        }
    }
}

/// Rewrites `self.method(args)` calls to `TypeName_method(self, args)` in expressions.
pub(crate) fn rewrite_self_method_calls_in_expr(
    expr: kodo_ast::Expr,
    type_name: &str,
) -> kodo_ast::Expr {
    match expr {
        kodo_ast::Expr::Call { callee, args, span } => {
            if let kodo_ast::Expr::FieldAccess {
                object,
                field,
                span: _fa_span,
            } = *callee
            {
                let object = rewrite_self_method_calls_in_expr(*object, type_name);
                let args: Vec<_> = args
                    .into_iter()
                    .map(|a| rewrite_self_method_calls_in_expr(a, type_name))
                    .collect();
                // If the receiver is `self`, rewrite to a flat call.
                if matches!(&object, kodo_ast::Expr::Ident(name, _) if name == "self") {
                    let mangled = format!("{type_name}_{field}");
                    let mut new_args = vec![object];
                    new_args.extend(args);
                    return kodo_ast::Expr::Call {
                        callee: Box::new(kodo_ast::Expr::Ident(mangled, span)),
                        args: new_args,
                        span,
                    };
                }
                kodo_ast::Expr::Call {
                    callee: Box::new(kodo_ast::Expr::FieldAccess {
                        object: Box::new(object),
                        field,
                        span: _fa_span,
                    }),
                    args,
                    span,
                }
            } else {
                let callee = rewrite_self_method_calls_in_expr(*callee, type_name);
                let args: Vec<_> = args
                    .into_iter()
                    .map(|a| rewrite_self_method_calls_in_expr(a, type_name))
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
            left: Box::new(rewrite_self_method_calls_in_expr(*left, type_name)),
            op,
            right: Box::new(rewrite_self_method_calls_in_expr(*right, type_name)),
            span,
        },
        kodo_ast::Expr::FieldAccess {
            object,
            field,
            span,
        } => kodo_ast::Expr::FieldAccess {
            object: Box::new(rewrite_self_method_calls_in_expr(*object, type_name)),
            field,
            span,
        },
        // Leaf expressions -- no sub-expressions to rewrite.
        other => other,
    }
}

/// Converts a [`kodo_types::Type`] to a [`kodo_ast::TypeExpr`].
pub(crate) fn type_to_type_expr(ty: &kodo_types::Type) -> kodo_ast::TypeExpr {
    match ty {
        kodo_types::Type::Int => kodo_ast::TypeExpr::Named("Int".to_string()),
        kodo_types::Type::Bool => kodo_ast::TypeExpr::Named("Bool".to_string()),
        kodo_types::Type::String => kodo_ast::TypeExpr::Named("String".to_string()),
        kodo_types::Type::Unit => kodo_ast::TypeExpr::Unit,
        kodo_types::Type::Struct(name) | kodo_types::Type::Enum(name) => {
            kodo_ast::TypeExpr::Named(name.clone())
        }
        _ => kodo_ast::TypeExpr::Named("Unknown".to_string()),
    }
}

/// Substitutes type parameters in a [`kodo_ast::TypeExpr`].
pub(crate) fn substitute_type_expr_ast(
    expr: &kodo_ast::TypeExpr,
    subst: &std::collections::HashMap<String, kodo_ast::TypeExpr>,
) -> kodo_ast::TypeExpr {
    match expr {
        kodo_ast::TypeExpr::Named(name) => {
            if let Some(replacement) = subst.get(name) {
                replacement.clone()
            } else {
                expr.clone()
            }
        }
        kodo_ast::TypeExpr::Generic(name, args) => kodo_ast::TypeExpr::Generic(
            name.clone(),
            args.iter()
                .map(|a| substitute_type_expr_ast(a, subst))
                .collect(),
        ),
        kodo_ast::TypeExpr::Function(params, ret) => kodo_ast::TypeExpr::Function(
            params
                .iter()
                .map(|p| substitute_type_expr_ast(p, subst))
                .collect(),
            Box::new(substitute_type_expr_ast(ret, subst)),
        ),
        kodo_ast::TypeExpr::Unit => kodo_ast::TypeExpr::Unit,
        kodo_ast::TypeExpr::Optional(inner) => {
            kodo_ast::TypeExpr::Optional(Box::new(substitute_type_expr_ast(inner, subst)))
        }
        kodo_ast::TypeExpr::Tuple(elems) => kodo_ast::TypeExpr::Tuple(
            elems
                .iter()
                .map(|e| substitute_type_expr_ast(e, subst))
                .collect(),
        ),
        kodo_ast::TypeExpr::DynTrait(name) => kodo_ast::TypeExpr::DynTrait(name.clone()),
    }
}
