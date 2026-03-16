//! The `test` command implementation.
//!
//! Runs tests defined in a Kodo source file by desugaring `test` declarations
//! into regular functions, generating a synthetic test runner `main`, compiling,
//! executing, and reporting results.

use std::path::PathBuf;

use super::common::{
    build_vtable_defs, compile_imported_module, inject_stdlib_method_functions, link_executable,
    parse_contract_mode, resolve_import_path, rewrite_method_calls_in_block,
    rewrite_self_method_calls_in_block, substitute_type_expr_ast, type_to_type_expr,
};
use crate::diagnostics;

/// Runs tests in a Kodo source file.
///
/// Desugars `test` declarations into regular functions, generates a synthetic
/// `main` function as the test runner, compiles, executes, and reports results.
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

    // Filter tests by name substring if --filter provided.
    if let Some(pattern) = filter {
        module.test_decls.retain(|t| t.name.contains(pattern));
    }

    if module.test_decls.is_empty() {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "event": "summary",
                    "total": 0,
                    "passed": 0,
                    "failed": 0,
                })
            );
        } else {
            println!("no tests found");
        }
        return 0;
    }

    let test_count = module.test_decls.len();
    let test_names: Vec<String> = module.test_decls.iter().map(|t| t.name.clone()).collect();
    let s = kodo_ast::Span::new(0, 0);

    // Desugar: convert each TestDecl into a function `__test_N`.
    for (i, test_decl) in module.test_decls.drain(..).enumerate() {
        let func = kodo_ast::Function {
            id: kodo_ast::NodeId(0),
            span: test_decl.span,
            name: format!("__test_{i}"),
            visibility: kodo_ast::Visibility::Private,
            is_async: false,
            generic_params: vec![],
            annotations: test_decl.annotations,
            params: vec![],
            return_type: kodo_ast::TypeExpr::Unit,
            requires: vec![],
            ensures: vec![],
            body: test_decl.body,
        };
        module.functions.push(func);
    }

    // Generate synthetic `main` function as test runner.
    // main() calls kodo_test_start, __test_N, kodo_test_end for each test,
    // then kodo_test_summary at the end.
    let mut main_stmts = Vec::new();

    // let total: Int = <test_count>
    main_stmts.push(kodo_ast::Stmt::Let {
        span: s,
        mutable: false,
        name: "__total".to_string(),
        ty: Some(kodo_ast::TypeExpr::Named("Int".to_string())),
        value: kodo_ast::Expr::IntLit(test_count as i64, s),
    });
    // let mut passed: Int = 0
    main_stmts.push(kodo_ast::Stmt::Let {
        span: s,
        mutable: true,
        name: "__passed".to_string(),
        ty: Some(kodo_ast::TypeExpr::Named("Int".to_string())),
        value: kodo_ast::Expr::IntLit(0, s),
    });
    // let mut failed: Int = 0
    main_stmts.push(kodo_ast::Stmt::Let {
        span: s,
        mutable: true,
        name: "__failed".to_string(),
        ty: Some(kodo_ast::TypeExpr::Named("Int".to_string())),
        value: kodo_ast::Expr::IntLit(0, s),
    });

    for (i, name) in test_names.iter().enumerate() {
        // kodo_test_start("test name")
        main_stmts.push(kodo_ast::Stmt::Expr(kodo_ast::Expr::Call {
            callee: Box::new(kodo_ast::Expr::Ident("kodo_test_start".to_string(), s)),
            args: vec![kodo_ast::Expr::StringLit(name.clone(), s)],
            span: s,
        }));
        // __test_N()
        main_stmts.push(kodo_ast::Stmt::Expr(kodo_ast::Expr::Call {
            callee: Box::new(kodo_ast::Expr::Ident(format!("__test_{i}"), s)),
            args: vec![],
            span: s,
        }));
        // let __result_N: Int = kodo_test_end()
        main_stmts.push(kodo_ast::Stmt::Let {
            span: s,
            mutable: false,
            name: format!("__result_{i}"),
            ty: Some(kodo_ast::TypeExpr::Named("Int".to_string())),
            value: kodo_ast::Expr::Call {
                callee: Box::new(kodo_ast::Expr::Ident("kodo_test_end".to_string(), s)),
                args: vec![],
                span: s,
            },
        });
        // if __result_N == 0 { __passed = __passed + 1 } else { __failed = __failed + 1 }
        main_stmts.push(kodo_ast::Stmt::Expr(kodo_ast::Expr::If {
            condition: Box::new(kodo_ast::Expr::BinaryOp {
                left: Box::new(kodo_ast::Expr::Ident(format!("__result_{i}"), s)),
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

    // kodo_test_summary(__total, __passed, __failed)
    main_stmts.push(kodo_ast::Stmt::Expr(kodo_ast::Expr::Call {
        callee: Box::new(kodo_ast::Expr::Ident("kodo_test_summary".to_string(), s)),
        args: vec![
            kodo_ast::Expr::Ident("__total".to_string(), s),
            kodo_ast::Expr::Ident("__passed".to_string(), s),
            kodo_ast::Expr::Ident("__failed".to_string(), s),
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
    if !method_resolutions.is_empty() {
        for func in &mut module.functions {
            rewrite_method_calls_in_block(&mut func.body, &method_resolutions);
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
    ) {
        Ok(fns) => fns,
        Err(e) => {
            eprintln!("MIR lowering error: {e}");
            return 1;
        }
    };
    all_mir_functions.extend(mir_functions);
    kodo_mir::optimize::optimize_all(&mut all_mir_functions);

    if contract_mode == kodo_contracts::ContractMode::Recoverable {
        kodo_mir::apply_recoverable_contracts(&mut all_mir_functions);
    }

    // Code generation.
    let struct_defs = checker.struct_registry().clone();
    let enum_defs = checker.enum_registry().clone();
    let is_recoverable = contract_mode == kodo_contracts::ContractMode::Recoverable;
    let options = kodo_codegen::CodegenOptions {
        recoverable_contracts: is_recoverable,
        ..kodo_codegen::CodegenOptions::default()
    };
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
