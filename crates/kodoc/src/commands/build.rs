//! The `build` command implementation.
//!
//! Compiles a Kodo source file through the full pipeline: parsing, type checking,
//! contract verification, intent resolution, desugaring, MIR lowering, optimization,
//! code generation, and linking to produce a native executable.
//!
//! Supports both single-file and directory compilation. When a directory is passed,
//! all `.ko` files are discovered, the entry point (`fn main()`) is located, and
//! files are compiled in topological dependency order.

use std::path::PathBuf;

use super::common::{
    build_module_metadata, build_vtable_defs, check_import_cycles, compile_imported_module,
    find_entry_point, find_ko_files, inject_stdlib_method_functions, link_executable,
    parse_contract_mode, resolve_import_path, rewrite_map_for_in, rewrite_method_calls_in_block,
    rewrite_self_method_calls_in_block, rewrite_set_for_in, substitute_type_expr_ast,
    topological_sort, type_to_type_expr,
};
use crate::{certificate, diagnostics};

/// Compiles a Kodo source file or directory to a native executable.
///
/// When `file` is a directory, discovers all `.ko` files, finds the entry point
/// containing `fn main()`, builds a dependency graph, and compiles in
/// topological order.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub(crate) fn run_build(
    file: &PathBuf,
    output: Option<&std::path::Path>,
    json_errors: bool,
    contracts_mode_str: &str,
    emit_mir: bool,
    green_threads: bool,
    backend: &str,
    emit_llvm: bool,
    release: bool,
) -> i32 {
    // If the argument is a directory, delegate to directory compilation.
    if file.is_dir() {
        return run_build_dir(
            file,
            output,
            json_errors,
            contracts_mode_str,
            emit_mir,
            green_threads,
            backend,
            emit_llvm,
        );
    }

    tracing::info!("building {}", file.display());

    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not read file `{}`: {e}", file.display());
            return 1;
        }
    };

    let filename = file.display().to_string();

    let module = match kodo_parser::parse(&source) {
        Ok(m) => m,
        Err(e) => {
            if json_errors {
                diagnostics::render_parse_error_json_envelope(&source, &filename, &e);
            } else {
                diagnostics::render_parse_error(&source, &filename, &e);
            }
            return 1;
        }
    };

    // Process imports -- compile imported modules and collect their types/functions.
    let base_dir = file.parent().unwrap_or_else(|| std::path::Path::new("."));

    // If a kodo.toml exists, resolve dependencies and collect extra import dirs.
    let dep_import_dirs = resolve_manifest_deps(base_dir);

    let mut imported_object_files: Vec<std::path::PathBuf> = Vec::new();
    let mut imported_modules: Vec<kodo_ast::Module> = Vec::new();

    // Detect import cycles.
    let mut visited = std::collections::HashSet::new();
    if let Ok(canonical) = file.canonicalize() {
        visited.insert(canonical);
    }
    if let Err(msg) = check_import_cycles(base_dir, &module, &mut visited, file) {
        eprintln!("{msg}");
        return 1;
    }

    for import in &module.imports {
        // Check stdlib first for `std::*` imports.
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
        let import_path = resolve_import_with_deps(base_dir, &import.path, &dep_import_dirs);
        match compile_imported_module(&import_path, &mut imported_object_files) {
            Ok(imported_module) => imported_modules.push(imported_module),
            Err(msg) => {
                eprintln!("{msg}");
                return 1;
            }
        }
    }

    // Load stdlib prelude modules (Option, Result).
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

    // Type check -- register prelude, imports, then user module.
    let mut checker = kodo_types::TypeChecker::new();
    for prelude in &prelude_modules {
        if let Err(e) = checker.check_module(prelude) {
            eprintln!("stdlib type error: {e}");
            return 1;
        }
    }
    for (idx, imported) in imported_modules.iter().enumerate() {
        if let Err(e) = checker.check_module(imported) {
            eprintln!("type error in imported module `{}`: {e}", imported.name);
            return 1;
        }
        // Register the module name for qualified access (e.g., math.add()).
        checker.register_imported_module(imported.name.clone());
        // Track visibility of imported module symbols for enforcement.
        checker.register_module_visibility(imported);
        // For selective imports, the names are already in scope via check_module.
        // The import declaration's `names` field is informational at this stage.
        let _ = &module.imports.get(idx);
    }
    let type_errors = checker.check_module_collecting(&module);
    if !type_errors.is_empty() {
        if json_errors {
            diagnostics::render_type_errors_json(&source, &filename, &type_errors);
        } else {
            for e in &type_errors {
                diagnostics::render_type_error(&source, &filename, e);
            }
        }
        return 1;
    }

    // Contract verification
    let contract_mode = parse_contract_mode(contracts_mode_str);
    for func in &module.functions {
        let contracts = kodo_contracts::extract_contracts(func);
        if let Err(e) = kodo_contracts::verify_contracts(&contracts, contract_mode) {
            eprintln!("contract error: {e}");
            return 1;
        }
    }
    for impl_block in &module.impl_blocks {
        for method in &impl_block.methods {
            let contracts = kodo_contracts::extract_contracts(method);
            if let Err(e) = kodo_contracts::verify_contracts(&contracts, contract_mode) {
                eprintln!("contract error: {e}");
                return 1;
            }
        }
    }

    // Intent resolution -- resolve intent blocks into concrete code.
    let mut module = module;
    if !module.intent_decls.is_empty() {
        let resolver = kodo_resolver::Resolver::with_builtins();
        match resolver.resolve_all(&module.intent_decls) {
            Ok(resolved_intents) => {
                for resolved in resolved_intents {
                    // Skip generated functions that already exist in the module
                    // (the intent serves as a declaration of intent, not a replacement).
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

    // Rewrite for-in over Maps to use Map_keys() before desugaring.
    rewrite_map_for_in(&mut module, checker.map_for_in_spans());
    // Rewrite for-in over Sets to use set_to_list() before desugaring.
    rewrite_set_for_in(&mut module, checker.set_for_in_spans());

    // Desugar pass -- simplify syntactic sugar (e.g. for loops) before MIR lowering.
    kodo_desugar::desugar_module(&mut module);

    // Transform impl block methods into top-level functions with mangled names.
    // Also inject default method bodies from traits when not overridden.
    let default_methods = checker.trait_default_methods().clone();
    for impl_block in &module.impl_blocks {
        for method in &impl_block.methods {
            let mut func = method.clone();
            func.name = format!("{}_{}", impl_block.type_name, method.name);
            // Ensure self param has the correct type
            for param in &mut func.params {
                if param.name == "self" {
                    param.ty = kodo_ast::TypeExpr::Named(impl_block.type_name.clone());
                }
            }
            module.functions.push(func);
        }
        // Inject default methods from the trait that are not overridden.
        if let Some(ref trait_name) = impl_block.trait_name {
            if let Some(defaults) = default_methods.get(trait_name) {
                for (name, trait_method) in defaults {
                    let overridden = impl_block.methods.iter().any(|m| m.name == *name);
                    if !overridden {
                        if let Some(ref body) = trait_method.body {
                            let mut params = trait_method.params.clone();
                            // Fix self param type to concrete type.
                            for param in &mut params {
                                if param.name == "self" {
                                    param.ty =
                                        kodo_ast::TypeExpr::Named(impl_block.type_name.clone());
                                }
                            }
                            // Rewrite method calls on self in the body.
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

    // Rewrite method calls in the AST: `obj.method(args)` -> `TypeName_method(obj, args)`
    // Uses span-based resolutions from the type checker to precisely identify method calls.
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
        // Also rewrite method calls inside actor handler bodies so that
        // handler-to-handler calls and self-calls are properly mangled.
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

    // Inject synthetic stdlib method implementations (Option/Result methods).
    // These are added after type checking since the type checker only registers
    // their signatures in method_lookup; the bodies are generated here.
    inject_stdlib_method_functions(&mut module);

    // Generate monomorphized function instances from generic functions.
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

    // MIR lowering -- combine all modules' functions.
    let mut all_mir_functions = Vec::new();

    // Lower imported modules first.
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

    // Lower the main module.
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

    // Run MIR optimization passes (inlining, constant folding, DCE, copy propagation).
    kodo_mir::optimize::optimize_all(&mut all_mir_functions);

    // Insert green thread yield points (cooperative scheduling).
    if green_threads {
        kodo_mir::yield_insertion::insert_yield_points(&mut all_mir_functions);
    }

    // Print MIR to stdout if --emit-mir was requested.
    if emit_mir {
        println!("--- MIR ({} functions) ---", all_mir_functions.len());
        for func in &all_mir_functions {
            println!("{func:#?}");
            println!();
        }
        println!("--- end MIR ---");
    }

    // Build module metadata for embedding in the binary.
    let metadata_json = build_module_metadata(&module);

    // Use type checker registries for codegen (includes monomorphized generics).
    let struct_defs: std::collections::HashMap<String, Vec<(String, kodo_types::Type)>> =
        checker.struct_registry().clone();
    let enum_defs: std::collections::HashMap<String, Vec<(String, Vec<kodo_types::Type>)>> =
        checker.enum_registry().clone();

    // Apply recoverable contract transformation if requested.
    let contract_mode = parse_contract_mode(contracts_mode_str);
    if contract_mode == kodo_contracts::ContractMode::Recoverable {
        kodo_mir::apply_recoverable_contracts(&mut all_mir_functions);
    }

    // Code generation — dispatch based on backend.
    let vtable_defs = build_vtable_defs(&checker);

    // Determine output path
    let output_path = output
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| file.with_extension(""));

    // Inkwell LLVM backend (feature-gated, uses LLVM C API for optimized codegen)
    #[cfg(feature = "inkwell")]
    if backend == "inkwell" {
        let opt = if release { 3 } else { 2 };
        return match kodo_codegen_llvm::inkwell_backend::compile_module(
            &all_mir_functions,
            &struct_defs,
            &enum_defs,
            opt,
            &output_path,
        ) {
            Ok(()) => {
                let obj_path = output_path.with_extension("o");
                let result = link_executable(&obj_path, &output_path);
                let _ = std::fs::remove_file(&obj_path);
                match result {
                    Ok(()) => {
                        println!("Successfully compiled → {}", output_path.display());
                        0
                    }
                    Err(e) => {
                        eprintln!("link error: {e}");
                        1
                    }
                }
            }
            Err(e) => {
                eprintln!("inkwell codegen error: {e}");
                1
            }
        };
    }

    let link_result = if backend == "llvm" {
        // LLVM backend: generate textual IR, optionally compile with llc.
        let llvm_opts = kodo_codegen_llvm::LLVMCodegenOptions::default();
        let llvm_vtable_defs: std::collections::HashMap<(String, String), Vec<String>> =
            vtable_defs
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
        let ir = match kodo_codegen_llvm::compile_module_to_llvm_ir(
            &all_mir_functions,
            &struct_defs,
            &enum_defs,
            &llvm_vtable_defs,
            &llvm_opts,
            Some(&metadata_json),
        ) {
            Ok(ir) => ir,
            Err(e) => {
                eprintln!("codegen-llvm error: {e}");
                return 1;
            }
        };

        let ll_path = output_path.with_extension("ll");
        if let Err(e) = std::fs::write(&ll_path, &ir) {
            eprintln!("error: could not write LLVM IR file: {e}");
            return 1;
        }

        if emit_llvm {
            println!("Emitted LLVM IR → {}", ll_path.display());
            return 0;
        }

        // Compile .ll → .o using llc, then link.
        let obj_path = output_path.with_extension("o");
        let opt_flag = if release { "-O3" } else { "-O2" };
        let llc_status = std::process::Command::new("llc")
            .args([
                "-filetype=obj",
                opt_flag,
                ll_path.to_str().unwrap_or("output.ll"),
                "-o",
                obj_path.to_str().unwrap_or("output.o"),
            ])
            .status();

        // Clean up the .ll file after compilation.
        let _ = std::fs::remove_file(&ll_path);

        match llc_status {
            Ok(status) if status.success() => {
                let result = link_executable(&obj_path, &output_path);
                let _ = std::fs::remove_file(&obj_path);
                result
            }
            Ok(status) => {
                let _ = std::fs::remove_file(&obj_path);
                Err(format!("llc exited with status {status}"))
            }
            Err(e) => {
                let _ = std::fs::remove_file(&obj_path);
                Err(format!(
                    "LLVM backend requires `llc` in PATH. Install LLVM or use default Cranelift backend. ({e})"
                ))
            }
        }
    } else {
        // Cranelift backend (default).
        let options = kodo_codegen::CodegenOptions::default();
        let object_bytes = match kodo_codegen::compile_module_with_vtables(
            &all_mir_functions,
            &struct_defs,
            &enum_defs,
            &vtable_defs,
            &options,
            Some(&metadata_json),
        ) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("codegen error: {e}");
                return 1;
            }
        };

        let obj_path = output_path.with_extension("o");
        if let Err(e) = std::fs::write(&obj_path, &object_bytes) {
            eprintln!("error: could not write object file: {e}");
            return 1;
        }

        let result = link_executable(&obj_path, &output_path);
        let _ = std::fs::remove_file(&obj_path);
        result
    };

    match link_result {
        Ok(()) => {
            // Read the binary for hashing.
            let binary_bytes = std::fs::read(&output_path).ok();

            // Read previous certificate if it exists (for chaining).
            let cert_path = output_path.with_extension("ko.cert.json");
            let parent_cert = std::fs::read_to_string(&cert_path).ok().and_then(|json| {
                serde_json::from_str::<certificate::CompilationCertificate>(&json).ok()
            });

            // Emit compilation certificate.
            let cert = certificate::CompilationCertificate::from_module(
                &module,
                &source,
                binary_bytes.as_deref(),
                parent_cert.as_ref(),
                None,
                None,
                Some(contract_mode),
            );
            match cert.to_json() {
                Ok(json) => {
                    if let Err(e) = std::fs::write(&cert_path, &json) {
                        eprintln!("warning: could not write certificate: {e}");
                    }
                }
                Err(e) => {
                    eprintln!("warning: {e}");
                }
            }

            println!(
                "Successfully compiled `{}` → {}",
                module.name,
                output_path.display()
            );
            0
        }
        Err(e) => {
            eprintln!("link error: {e}");
            1
        }
    }
}

/// Resolves dependency import directories from `kodo.toml` if it exists.
///
/// Returns a list of additional directories to search for imports.
/// If no manifest exists or resolution fails, returns an empty list
/// (with a warning printed).
fn resolve_manifest_deps(project_dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let manifest_path = project_dir.join("kodo.toml");
    if !manifest_path.exists() {
        return Vec::new();
    }

    let manifest = match crate::manifest::read_manifest(project_dir) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("warning: {e}");
            return Vec::new();
        }
    };

    if manifest.deps.is_empty() {
        return Vec::new();
    }

    // Try to use the lock file for reproducible resolution.
    let lockfile = crate::lockfile::read_lockfile(project_dir).unwrap_or_default();

    let (resolved, new_lockfile) = if lockfile.package.is_empty() {
        match crate::dep_resolver::resolve_deps(&manifest, project_dir) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("warning: dependency resolution failed: {e}");
                return Vec::new();
            }
        }
    } else {
        match crate::dep_resolver::resolve_deps_from_lock(&manifest, project_dir, &lockfile) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("warning: dependency resolution failed: {e}");
                return Vec::new();
            }
        }
    };

    // Update lock file if needed.
    if let Err(e) = crate::lockfile::write_lockfile(project_dir, &new_lockfile) {
        eprintln!("warning: could not update kodo.lock: {e}");
    }

    resolved.into_iter().map(|d| d.source_dir).collect()
}

/// Resolves an import path, also searching in dependency directories.
///
/// First tries the standard resolution relative to `base_dir`. If not found,
/// searches each dependency directory for a matching `.ko` file.
fn resolve_import_with_deps(
    base_dir: &std::path::Path,
    segments: &[String],
    dep_dirs: &[std::path::PathBuf],
) -> std::path::PathBuf {
    // Standard resolution first.
    let standard = resolve_import_path(base_dir, segments);
    if standard.exists() {
        return standard;
    }

    // Search in dependency directories.
    for dep_dir in dep_dirs {
        let dep_path = resolve_import_path(dep_dir, segments);
        if dep_path.exists() {
            return dep_path;
        }
        // Also try just the last segment (module name) in the dep dir.
        if let Some(module_name) = segments.last() {
            let direct = dep_dir.join(format!("{module_name}.ko"));
            if direct.exists() {
                return direct;
            }
            // Try lib.ko in the dep dir itself.
            let lib_ko = dep_dir.join("lib.ko");
            if lib_ko.exists() {
                return lib_ko;
            }
        }
    }

    // Fall back to the standard (non-existent) path.
    standard
}

/// Compiles a directory of `.ko` files to a native executable.
///
/// 1. Discovers all `.ko` files recursively.
/// 2. Finds the entry point (the file with `fn main()`).
/// 3. Builds the dependency graph via imports.
/// 4. Compiles the entry point, with all other project files available as imports.
#[allow(clippy::too_many_arguments)]
fn run_build_dir(
    dir: &std::path::Path,
    output: Option<&std::path::Path>,
    json_errors: bool,
    contracts_mode_str: &str,
    emit_mir: bool,
    green_threads: bool,
    backend: &str,
    emit_llvm: bool,
) -> i32 {
    let all_files = match find_ko_files(dir) {
        Ok(files) => files,
        Err(msg) => {
            eprintln!("{msg}");
            return 1;
        }
    };

    if all_files.is_empty() {
        eprintln!("error: no `.ko` files found in `{}`", dir.display());
        return 1;
    }

    let entry = match find_entry_point(&all_files) {
        Ok(ep) => ep,
        Err(msg) => {
            eprintln!("{msg}");
            return 1;
        }
    };

    tracing::info!(
        "directory build: entry point `{}`, {} files",
        entry.display(),
        all_files.len()
    );

    // Verify topological ordering is possible (no cycles, all imports resolve).
    if let Err(msg) = topological_sort(&entry, &all_files) {
        eprintln!("{msg}");
        return 1;
    }

    // Determine output path: use provided output, or dir_name/main (strip .ko extension).
    let output_path = output.map(std::path::PathBuf::from).unwrap_or_else(|| {
        let stem = entry.file_stem().unwrap_or_default();
        dir.join(stem)
    });

    // Delegate to the single-file build with the entry point.
    // The import resolver will find other .ko files in the same directory.
    run_build(
        &entry,
        Some(&output_path),
        json_errors,
        contracts_mode_str,
        emit_mir,
        green_threads,
        backend,
        emit_llvm,
        backend == "llvm",
    )
}
