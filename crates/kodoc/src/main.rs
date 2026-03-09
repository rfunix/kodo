//! # kodoc — The Kōdo Compiler
//!
//! The command-line interface for the Kōdo programming language compiler.
//! Designed to be used both by AI agents (with `--emit json-errors`) and
//! humans (with beautiful terminal error messages via ariadne).

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
            output: _,
            json_errors: _,
            contracts: _,
        } => run_build(&file),
        Command::Check {
            file,
            json_errors: _,
        } => run_check(&file),
        Command::Lex { file } => run_lex(&file),
        Command::Parse { file } => run_parse(&file),
    };

    std::process::exit(exit_code);
}

fn run_build(file: &PathBuf) -> i32 {
    tracing::info!("building {}", file.display());

    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not read file `{}`: {e}", file.display());
            return 1;
        }
    };

    match kodo_parser::parse(&source) {
        Ok(module) => {
            println!("Successfully parsed module `{}`", module.name);
            println!(
                "  {} function(s), meta: {}",
                module.functions.len(),
                if module.meta.is_some() { "yes" } else { "no" }
            );
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

fn run_check(file: &PathBuf) -> i32 {
    tracing::info!("checking {}", file.display());

    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not read file `{}`: {e}", file.display());
            return 1;
        }
    };

    match kodo_parser::parse(&source) {
        Ok(module) => {
            println!("Check passed for module `{}`", module.name);
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
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
