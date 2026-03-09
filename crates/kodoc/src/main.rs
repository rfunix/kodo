//! # kodoc — The Kōdo Compiler
//!
//! The command-line interface for the Kōdo programming language compiler.
//! Designed to be used both by AI agents (with `--emit json-errors`) and
//! humans (with beautiful terminal error messages via ariadne).

mod certificate;
mod diagnostics;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// The Kōdo compiler — a language built for AI agents, transparent for humans.
#[derive(Parser)]
#[command(name = "kodoc", version, about, long_about = None)]
struct Cli {
    /// Subcommand to execute.
    #[command(subcommand)]
    command: Command,
}

/// Available compiler commands.
#[derive(Subcommand)]
enum Command {
    /// Compile a Kōdo source file to a binary.
    Build {
        /// The source file to compile.
        #[arg()]
        file: PathBuf,

        /// Output file path.
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Emit errors as JSON (for AI agent consumption).
        #[arg(long, default_value_t = false)]
        json_errors: bool,

        /// Contract checking mode: static, runtime, both, none.
        #[arg(long, default_value = "runtime")]
        contracts: String,
    },
    /// Type-check and verify contracts without generating code.
    Check {
        /// The source file to check.
        #[arg()]
        file: PathBuf,

        /// Emit errors as JSON.
        #[arg(long, default_value_t = false)]
        json_errors: bool,
    },
    /// Tokenize a source file and print the token stream.
    Lex {
        /// The source file to tokenize.
        #[arg()]
        file: PathBuf,
    },
    /// Parse a source file and print the AST.
    Parse {
        /// The source file to parse.
        #[arg()]
        file: PathBuf,
    },
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();

    let exit_code = match cli.command {
        Command::Build {
            file,
            output,
            json_errors,
            contracts: _,
        } => run_build(&file, output.as_deref(), json_errors),
        Command::Check { file, json_errors } => run_check(&file, json_errors),
        Command::Lex { file } => run_lex(&file),
        Command::Parse { file } => run_parse(&file),
    };

    std::process::exit(exit_code);
}

fn run_build(file: &PathBuf, output: Option<&std::path::Path>, json_errors: bool) -> i32 {
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
                diagnostics::render_parse_error_json(&source, &filename, &e);
            } else {
                diagnostics::render_parse_error(&source, &filename, &e);
            }
            return 1;
        }
    };

    // Type check
    let mut checker = kodo_types::TypeChecker::new();
    if let Err(e) = checker.check_module(&module) {
        if json_errors {
            diagnostics::render_type_error_json(&source, &filename, &e);
        } else {
            diagnostics::render_type_error(&source, &filename, &e);
        }
        return 1;
    }

    // Contract verification
    for func in &module.functions {
        let contracts = kodo_contracts::extract_contracts(func);
        if let Err(e) =
            kodo_contracts::verify_contracts(&contracts, kodo_contracts::ContractMode::Runtime)
        {
            eprintln!("contract error: {e}");
            return 1;
        }
    }

    // MIR lowering
    let mir_functions = match kodo_mir::lowering::lower_module(&module) {
        Ok(fns) => fns,
        Err(e) => {
            eprintln!("MIR lowering error: {e}");
            return 1;
        }
    };

    // Build module metadata for embedding in the binary.
    let metadata_json = build_module_metadata(&module);

    // Code generation
    let options = kodo_codegen::CodegenOptions::default();
    let object_bytes =
        match kodo_codegen::compile_module(&mir_functions, &options, Some(&metadata_json)) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("codegen error: {e}");
                return 1;
            }
        };

    // Determine output path
    let output_path = output
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| file.with_extension(""));

    // Write object file to a temporary location
    let obj_path = output_path.with_extension("o");
    if let Err(e) = std::fs::write(&obj_path, &object_bytes) {
        eprintln!("error: could not write object file: {e}");
        return 1;
    }

    // Link with the runtime
    let link_result = link_executable(&obj_path, &output_path);

    // Clean up the .o file
    let _ = std::fs::remove_file(&obj_path);

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

/// Links an object file with the Kōdo runtime to produce an executable.
fn link_executable(
    obj_path: &std::path::Path,
    output_path: &std::path::Path,
) -> std::result::Result<(), String> {
    // Find the runtime library.
    // Strategy: look relative to the kodoc binary, then in the workspace target dir.
    let runtime_path = find_runtime_lib()?;

    let status = std::process::Command::new("cc")
        .arg(obj_path)
        .arg(&runtime_path)
        .arg("-o")
        .arg(output_path)
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

/// Locates `libkodo_runtime.a` by searching common paths.
fn find_runtime_lib() -> std::result::Result<PathBuf, String> {
    // 1. Check KODO_RUNTIME_LIB env var
    if let Ok(path) = std::env::var("KODO_RUNTIME_LIB") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
    }

    // 2. Check relative to the current executable (workspace target/debug/)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("libkodo_runtime.a");
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    // 3. Check common cargo target directories
    let candidates = [
        "target/debug/libkodo_runtime.a",
        "target/release/libkodo_runtime.a",
    ];
    for candidate in &candidates {
        let p = PathBuf::from(candidate);
        if p.exists() {
            return Ok(p);
        }
    }

    Err(
        "could not find libkodo_runtime.a — build the workspace first with `cargo build`"
            .to_string(),
    )
}

/// Builds a JSON string with module metadata for embedding in the binary.
fn build_module_metadata(module: &kodo_ast::Module) -> String {
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

fn run_check(file: &PathBuf, json_errors: bool) -> i32 {
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
            if json_errors {
                diagnostics::render_parse_error_json(&source, &filename, &e);
            } else {
                diagnostics::render_parse_error(&source, &filename, &e);
            }
            return 1;
        }
    };

    // Type check
    let mut checker = kodo_types::TypeChecker::new();
    if let Err(e) = checker.check_module(&module) {
        if json_errors {
            diagnostics::render_type_error_json(&source, &filename, &e);
        } else {
            diagnostics::render_type_error(&source, &filename, &e);
        }
        return 1;
    }

    // Contract verification
    for func in &module.functions {
        let contracts = kodo_contracts::extract_contracts(func);
        if let Err(e) =
            kodo_contracts::verify_contracts(&contracts, kodo_contracts::ContractMode::Runtime)
        {
            eprintln!("contract error: {e}");
            return 1;
        }
    }

    if json_errors {
        diagnostics::render_success_json(&module);
    } else {
        println!("Check passed for module `{}`", module.name);
    }
    0
}

fn run_lex(file: &PathBuf) -> i32 {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not read file `{}`: {e}", file.display());
            return 1;
        }
    };

    match kodo_lexer::tokenize(&source) {
        Ok(tokens) => {
            for token in &tokens {
                println!(
                    "{:?} @ {}..{}",
                    token.kind, token.span.start, token.span.end
                );
            }
            println!("\n{} token(s)", tokens.len());
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn run_parse(file: &PathBuf) -> i32 {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not read file `{}`: {e}", file.display());
            return 1;
        }
    };

    match kodo_parser::parse(&source) {
        Ok(module) => {
            println!("{module:#?}");
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}
