//! The `check` and `fix` command implementations.
//!
//! `check` type-checks and verifies contracts without generating code.
//! `fix` attempts to auto-fix compiler errors using machine-applicable patches.

use std::path::PathBuf;

use super::common::{compile_imported_module, parse_contract_mode, resolve_import_path};
use crate::{certificate, diagnostics};

/// Type-checks and verifies contracts without generating code.
#[allow(clippy::too_many_lines)]
pub(crate) fn run_check(
    file: &PathBuf,
    json_errors: bool,
    sarif: bool,
    contracts_mode_str: &str,
    emit_cert: bool,
    repair_plan: bool,
) -> i32 {
    tracing::info!("checking {}", file.display());

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
            if sarif {
                let diag: &dyn kodo_ast::Diagnostic = &e;
                crate::sarif::render_sarif(&source, &filename, &[diag]);
            } else if json_errors {
                diagnostics::render_parse_error_json_envelope(&source, &filename, &e);
            } else {
                diagnostics::render_parse_error(&source, &filename, &e);
            }
            return 1;
        }
    };

    // Resolve and compile imported modules.
    let base_dir = file.parent().unwrap_or_else(|| std::path::Path::new("."));
    let mut imported_modules: Vec<kodo_ast::Module> = Vec::new();
    let mut import_visited: std::collections::HashSet<std::path::PathBuf> =
        std::collections::HashSet::new();

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
        if !import_visited.insert(import_path.clone()) {
            continue; // Skip duplicate imports.
        }
        let mut dummy_objects = Vec::new();
        match compile_imported_module(&import_path, &mut dummy_objects) {
            Ok(imported_module) => imported_modules.push(imported_module),
            Err(msg) => {
                eprintln!("{msg}");
                return 1;
            }
        }
    }

    // Load stdlib prelude for type checking.
    let mut checker = kodo_types::TypeChecker::new();
    checker.set_trust_config(crate::manifest::load_trust_config(file));
    for (_name, prelude_source) in kodo_std::prelude_sources() {
        if let Ok(prelude_mod) = kodo_parser::parse(prelude_source) {
            let _ = checker.check_module(&prelude_mod);
        }
    }

    // Type-check imported modules first, then the user module.
    for imported in &imported_modules {
        if let Err(e) = checker.check_module(imported) {
            eprintln!("type error in imported module `{}`: {e}", imported.name);
            return 1;
        }
        checker.register_imported_module(imported.name.clone());
        checker.register_module_visibility(imported);
    }

    // Type check -- collect all errors for multi-error reporting.
    let type_errors = checker.check_module_collecting(&module);
    if !type_errors.is_empty() {
        if sarif {
            let diags: Vec<&dyn kodo_ast::Diagnostic> = type_errors
                .iter()
                .map(|e| e as &dyn kodo_ast::Diagnostic)
                .collect();
            crate::sarif::render_sarif(&source, &filename, &diags);
        } else if repair_plan {
            emit_repair_plans(&type_errors);
        } else if json_errors {
            diagnostics::render_type_errors_json(&source, &filename, &type_errors);
        } else {
            for e in &type_errors {
                diagnostics::render_type_error(&source, &filename, e);
            }
        }
        return 1;
    }

    // Desugar pass -- simplify syntactic sugar for consistency.
    let mut module = module;
    kodo_desugar::desugar_module(&mut module);

    // Contract verification
    let contract_mode = parse_contract_mode(contracts_mode_str);
    let mut total_static = 0_usize;
    let mut total_runtime = 0_usize;
    for func in &module.functions {
        let contracts = kodo_contracts::extract_contracts(func);
        match kodo_contracts::verify_contracts(&contracts, contract_mode) {
            Ok(result) => {
                if !result.failures.is_empty() {
                    for failure in &result.failures {
                        eprintln!("contract error: {failure}");
                    }
                    return 1;
                }
                total_static += result.static_verified;
                total_runtime += result.runtime_checks_needed;
            }
            Err(e) => {
                eprintln!("contract error: {e}");
                return 1;
            }
        }
    }
    for impl_block in &module.impl_blocks {
        for method in &impl_block.methods {
            let contracts = kodo_contracts::extract_contracts(method);
            match kodo_contracts::verify_contracts(&contracts, contract_mode) {
                Ok(result) => {
                    if !result.failures.is_empty() {
                        for failure in &result.failures {
                            eprintln!("contract error: {failure}");
                        }
                        return 1;
                    }
                    total_static += result.static_verified;
                    total_runtime += result.runtime_checks_needed;
                }
                Err(e) => {
                    eprintln!("contract error: {e}");
                    return 1;
                }
            }
        }
    }

    if json_errors {
        diagnostics::render_success_json(&module);
    } else {
        println!("Check passed for module `{}`", module.name);
        if total_static > 0 || total_runtime > 0 {
            println!(
                "  contracts: {} statically verified, {} runtime checks",
                total_static, total_runtime
            );
        }
    }

    // Emit compilation certificate if requested (no binary hash since we're only checking).
    if emit_cert {
        let cert_path = file.with_extension("ko.cert.json");

        // Read previous certificate if it exists (for chaining).
        let parent_cert = std::fs::read_to_string(&cert_path).ok().and_then(|json| {
            serde_json::from_str::<certificate::CompilationCertificate>(&json).ok()
        });

        let verification = if total_static > 0 || total_runtime > 0 {
            Some((total_static, total_runtime, 0_usize))
        } else {
            None
        };

        let cert = certificate::CompilationCertificate::from_module(
            &module,
            &source,
            None, // no binary bytes -- check only
            parent_cert.as_ref(),
            None,
            verification,
            Some(contract_mode),
        );
        match cert.to_json() {
            Ok(json) => {
                if let Err(e) = std::fs::write(&cert_path, &json) {
                    eprintln!("warning: could not write certificate: {e}");
                } else {
                    println!("Certificate written to {}", cert_path.display());
                }
            }
            Err(e) => {
                eprintln!("warning: {e}");
            }
        }
    }

    0
}

/// Attempts to auto-fix compiler errors in a source file.
///
/// Runs the parse and type-check pipeline, collects any [`kodo_ast::FixPatch`] diagnostics
/// produced, then either applies them in-place (default) or prints them as JSON
/// (`--dry-run`).
pub(crate) fn run_fix(file: &PathBuf, dry_run: bool) -> i32 {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not read file `{}`: {e}", file.display());
            return 1;
        }
    };

    let filename = file.display().to_string();

    // Collect diagnostics by attempting to parse and type-check.
    let mut patches: Vec<kodo_ast::FixPatch> = Vec::new();

    // Try parsing first.
    let module = match kodo_parser::parse(&source) {
        Ok(m) => m,
        Err(e) => {
            use kodo_ast::Diagnostic;
            if let Some(patch) = e.fix_patch() {
                patches.push(patch);
            }
            if patches.is_empty() {
                eprintln!("parse error with no auto-fix available: {e}");
                return 1;
            }
            // Apply or show patches.
            return apply_patches(file, &source, &patches, dry_run);
        }
    };

    // Try type checking.
    let mut checker = kodo_types::TypeChecker::new();
    checker.set_trust_config(crate::manifest::load_trust_config(file));
    for (_name, prelude_source) in kodo_std::prelude_sources() {
        if let Ok(prelude_mod) = kodo_parser::parse(prelude_source) {
            let _ = checker.check_module(&prelude_mod);
        }
    }

    if let Err(e) = checker.check_module(&module) {
        use kodo_ast::Diagnostic;
        if let Some(mut patch) = e.fix_patch() {
            if patch.file.is_empty() {
                patch.file = filename;
            }
            patches.push(patch);
        }
    }

    if patches.is_empty() {
        println!("No auto-fixable errors found in `{}`", file.display());
        return 0;
    }

    apply_patches(file, &source, &patches, dry_run)
}

/// Emits repair plans as JSON for all type errors that have them.
///
/// Each error produces a JSON object with the error code, message,
/// and a list of repair steps with machine-applicable patches.
pub(crate) fn emit_repair_plans(type_errors: &[kodo_types::TypeError]) {
    use kodo_ast::Diagnostic;

    let plans: Vec<serde_json::Value> = type_errors
        .iter()
        .map(|e| {
            let mut entry = serde_json::json!({
                "code": e.code(),
                "message": e.message(),
            });
            if let Some(span) = e.span() {
                entry["span"] = serde_json::json!({
                    "start": span.start,
                    "end": span.end,
                });
            }
            if let Some(steps) = e.repair_plan() {
                let json_steps: Vec<serde_json::Value> = steps
                    .iter()
                    .enumerate()
                    .map(|(id, (description, patches))| {
                        let json_patches: Vec<serde_json::Value> = patches
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
                            "id": id,
                            "description": description,
                            "patches": json_patches,
                        })
                    })
                    .collect();
                entry["repair_plan"] = serde_json::json!(json_steps);
            }
            if let Some(patch) = e.fix_patch() {
                entry["fix_patch"] = serde_json::json!({
                    "description": patch.description,
                    "start_offset": patch.start_offset,
                    "end_offset": patch.end_offset,
                    "replacement": patch.replacement,
                });
            }
            entry
        })
        .collect();

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "repair_plans": plans,
        }))
        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
    );
}

/// In dry-run mode, prints the patches as JSON without modifying any file.
/// Otherwise, applies the patches in reverse offset order (to preserve byte
/// offsets) and writes the result back to `file`.
pub(crate) fn apply_patches(
    file: &PathBuf,
    source: &str,
    patches: &[kodo_ast::FixPatch],
    dry_run: bool,
) -> i32 {
    if dry_run {
        let json_patches: Vec<serde_json::Value> = patches
            .iter()
            .map(|p| {
                serde_json::json!({
                    "description": p.description,
                    "file": p.file,
                    "start_offset": p.start_offset,
                    "end_offset": p.end_offset,
                    "replacement": p.replacement,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "patches": json_patches,
                "applied": false,
            }))
            .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"))
        );
        return 0;
    }

    // Apply patches in reverse order (by start_offset) to preserve offsets.
    let mut result = source.to_string();
    let mut sorted_patches: Vec<&kodo_ast::FixPatch> = patches.iter().collect();
    sorted_patches.sort_by(|a, b| b.start_offset.cmp(&a.start_offset));

    for patch in &sorted_patches {
        let start = patch.start_offset.min(result.len());
        let end = patch.end_offset.min(result.len());
        result.replace_range(start..end, &patch.replacement);
        println!("Applied fix: {}", patch.description);
    }

    if let Err(e) = std::fs::write(file, &result) {
        eprintln!("error: could not write file `{}`: {e}", file.display());
        return 1;
    }

    println!("Applied {} fix(es) to `{}`", patches.len(), file.display());
    0
}
