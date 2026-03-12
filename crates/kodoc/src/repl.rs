//! Interactive REPL (Read-Eval-Print Loop) for the Kōdo compiler.
//!
//! Provides an interactive environment where users can enter Kōdo expressions
//! and function definitions, which are compiled and executed on the fly using
//! the full compiler pipeline (parse, type-check, MIR, codegen, link, run).
//!
//! Accumulated definitions persist across inputs so that functions defined in
//! one line can be called in subsequent lines.

use std::io::Write;

/// The default prompt shown to the user.
const PROMPT: &str = "kōdo> ";

/// The continuation prompt shown when multi-line input is expected.
const CONTINUATION_PROMPT: &str = "  ... ";

/// Special commands recognized by the REPL (prefixed with `:`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplCommand {
    /// Display available REPL commands.
    Help,
    /// Exit the REPL.
    Quit,
    /// Clear all accumulated state (definitions, etc.).
    Reset,
    /// Show the type of an expression without executing it.
    Type(String),
    /// Show the AST of an expression.
    Ast(String),
    /// Show the MIR of an expression.
    Mir(String),
    /// Regular Kōdo input (expression or definition).
    Input(String),
}

/// Parses a line of REPL input into a [`ReplCommand`].
pub fn parse_command(input: &str) -> ReplCommand {
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return ReplCommand::Input(String::new());
    }

    if !trimmed.starts_with(':') {
        return ReplCommand::Input(trimmed.to_string());
    }

    // Split at first whitespace to get command and argument.
    let (cmd, arg) = match trimmed.find(char::is_whitespace) {
        Some(pos) => (&trimmed[..pos], trimmed[pos..].trim()),
        None => (trimmed, ""),
    };

    match cmd {
        ":help" | ":h" => ReplCommand::Help,
        ":quit" | ":q" | ":exit" => ReplCommand::Quit,
        ":reset" | ":clear" => ReplCommand::Reset,
        ":type" | ":t" => ReplCommand::Type(arg.to_string()),
        ":ast" => ReplCommand::Ast(arg.to_string()),
        ":mir" => ReplCommand::Mir(arg.to_string()),
        _ => {
            eprintln!("unknown command: {cmd}");
            eprintln!("type :help for a list of commands");
            ReplCommand::Input(String::new())
        }
    }
}

/// Accumulated REPL state that persists between inputs.
#[derive(Debug, Default)]
pub struct ReplState {
    /// Function definitions accumulated across REPL inputs.
    pub definitions: Vec<String>,
    /// Struct/enum/type declarations accumulated across REPL inputs.
    pub type_defs: Vec<String>,
    /// Counter for generating unique wrapper function names.
    pub eval_counter: u64,
}

impl ReplState {
    /// Creates a new empty REPL state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Resets all accumulated state.
    pub fn reset(&mut self) {
        self.definitions.clear();
        self.type_defs.clear();
        self.eval_counter = 0;
    }

    /// Returns whether the input looks like a function definition.
    pub fn is_definition(input: &str) -> bool {
        let trimmed = input.trim();
        trimmed.starts_with("fn ")
    }

    /// Returns whether the input looks like a type definition (struct/enum/type).
    pub fn is_type_definition(input: &str) -> bool {
        let trimmed = input.trim();
        trimmed.starts_with("struct ")
            || trimmed.starts_with("enum ")
            || trimmed.starts_with("type ")
    }

    /// Wraps an expression in a complete Kōdo module source for compilation.
    ///
    /// The generated module includes all accumulated definitions and wraps the
    /// expression in a `main` function that prints the result.
    pub fn wrap_expression(&mut self, expr: &str) -> String {
        self.eval_counter += 1;
        let mut source = String::new();
        source.push_str("module repl {\n");
        source.push_str("    meta { purpose: \"repl\" }\n\n");

        // Include accumulated type definitions.
        for type_def in &self.type_defs {
            source.push_str("    ");
            source.push_str(type_def);
            source.push('\n');
        }

        // Include accumulated function definitions.
        for def in &self.definitions {
            source.push_str("    ");
            source.push_str(def);
            source.push('\n');
        }

        // Wrap expression in main function using print_int.
        source.push_str("\n    fn main() -> Int {\n");
        source.push_str(&format!("        let __result: Int = {expr}\n"));
        source.push_str("        print_int(__result)\n");
        source.push_str("        return 0\n");
        source.push_str("    }\n");
        source.push_str("}\n");

        source
    }

    /// Wraps a function definition in a module to verify it type-checks.
    pub fn wrap_definition(&self, def: &str) -> String {
        let mut source = String::new();
        source.push_str("module repl {\n");
        source.push_str("    meta { purpose: \"repl\" }\n\n");

        for type_def in &self.type_defs {
            source.push_str("    ");
            source.push_str(type_def);
            source.push('\n');
        }

        for existing_def in &self.definitions {
            source.push_str("    ");
            source.push_str(existing_def);
            source.push('\n');
        }

        source.push_str("    ");
        source.push_str(def);
        source.push('\n');

        // Add a dummy main so the module is complete.
        source.push_str("\n    fn main() -> Int {\n");
        source.push_str("        return 0\n");
        source.push_str("    }\n");
        source.push_str("}\n");

        source
    }

    /// Wraps input for the `:type` command — parse and type-check only.
    pub fn wrap_for_type_check(&self, expr: &str) -> String {
        let mut source = String::new();
        source.push_str("module repl {\n");
        source.push_str("    meta { purpose: \"repl\" }\n\n");

        for type_def in &self.type_defs {
            source.push_str("    ");
            source.push_str(type_def);
            source.push('\n');
        }

        for def in &self.definitions {
            source.push_str("    ");
            source.push_str(def);
            source.push('\n');
        }

        source.push_str("\n    fn __repl_type_check() -> Int {\n");
        source.push_str(&format!("        let __val: Int = {expr}\n"));
        source.push_str("        return __val\n");
        source.push_str("    }\n");
        source.push_str("}\n");

        source
    }
}

/// Checks whether the input has balanced braces (for multi-line support).
pub fn has_balanced_braces(input: &str) -> bool {
    let mut depth: i32 = 0;
    for ch in input.chars() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}

/// Displays the REPL help message.
pub fn print_help() {
    println!("Kōdo REPL Commands:");
    println!("  :help, :h       — Show this help message");
    println!("  :quit, :q       — Exit the REPL");
    println!("  :reset, :clear  — Clear all accumulated definitions");
    println!("  :type <expr>    — Show the type of an expression");
    println!("  :ast <expr>     — Show the AST of an expression");
    println!("  :mir <expr>     — Show the MIR of an expression");
    println!();
    println!("Enter expressions to evaluate or function definitions to define.");
    println!("Multi-line input is supported: open braces are auto-continued.");
}

/// Displays the REPL banner shown at startup.
pub fn print_banner() {
    println!("Kōdo REPL v{}", env!("CARGO_PKG_VERSION"));
    println!("Type :help for available commands, :quit to exit.");
    println!();
}

/// Runs the compile-and-execute pipeline for an expression.
///
/// Returns `Ok(output)` with the program's stdout on success,
/// or `Err(message)` with a description of the compilation/runtime error.
pub fn compile_and_run(source: &str) -> Result<String, String> {
    // Parse
    let module = kodo_parser::parse(source).map_err(|e| format!("parse error: {e}"))?;

    // Load stdlib prelude.
    let mut prelude_modules = Vec::new();
    for (_name, prelude_src) in kodo_std::prelude_sources() {
        if let Ok(m) = kodo_parser::parse(prelude_src) {
            prelude_modules.push(m);
        }
    }

    // Type check
    let mut checker = kodo_types::TypeChecker::new();
    for prelude in &prelude_modules {
        checker
            .check_module(prelude)
            .map_err(|e| format!("stdlib type error: {e}"))?;
    }
    checker
        .check_module(&module)
        .map_err(|e| format!("type error: {e}"))?;

    // Desugar
    let mut module = module;
    kodo_desugar::desugar_module(&mut module);

    // Transform impl block methods
    for impl_block in &module.impl_blocks.clone() {
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

    // MIR lowering
    let mut mir_functions = kodo_mir::lowering::lower_module_with_type_info(
        &module,
        checker.struct_registry(),
        checker.enum_registry(),
        checker.enum_names(),
        checker.type_alias_registry(),
    )
    .map_err(|e| format!("MIR lowering error: {e}"))?;

    // Optimize
    for func in &mut mir_functions {
        kodo_mir::optimize::optimize_function(func);
    }

    // Codegen
    let struct_defs = checker.struct_registry().clone();
    let enum_defs = checker.enum_registry().clone();
    let options = kodo_codegen::CodegenOptions::default();
    let repl_meta = r#"{"purpose":"repl"}"#;
    let object_bytes = kodo_codegen::compile_module_with_types(
        &mir_functions,
        &struct_defs,
        &enum_defs,
        &options,
        Some(repl_meta),
    )
    .map_err(|e| format!("codegen error: {e}"))?;

    // Write to temp file, link, and execute.
    let temp_dir = std::env::temp_dir().join(format!("kodo_repl_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("could not create temp directory: {e}"))?;

    let obj_path = temp_dir.join("repl.o");
    let bin_path = temp_dir.join("repl_bin");

    std::fs::write(&obj_path, &object_bytes)
        .map_err(|e| format!("could not write object file: {e}"))?;

    // Find runtime and link.
    let runtime_path = find_runtime_lib_for_repl()?;

    let mut link_cmd = std::process::Command::new("cc");
    link_cmd
        .arg(&obj_path)
        .arg(&runtime_path)
        .arg("-o")
        .arg(&bin_path);

    if cfg!(target_os = "macos") {
        link_cmd.arg("-Wl,-w");
    }

    let link_status = link_cmd
        .output()
        .map_err(|e| format!("failed to invoke linker: {e}"))?;

    // Clean up object file.
    let _ = std::fs::remove_file(&obj_path);

    if !link_status.status.success() {
        let stderr = String::from_utf8_lossy(&link_status.stderr);
        return Err(format!("link error: {stderr}"));
    }

    // Execute the compiled binary.
    let run_output = std::process::Command::new(&bin_path)
        .output()
        .map_err(|e| format!("failed to execute: {e}"))?;

    // Clean up binary.
    let _ = std::fs::remove_file(&bin_path);
    let _ = std::fs::remove_dir(&temp_dir);

    if !run_output.status.success() {
        let stderr = String::from_utf8_lossy(&run_output.stderr);
        let code = run_output.status.code().unwrap_or(-1);
        return Err(format!("runtime error (exit code {code}): {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&run_output.stdout).to_string();
    Ok(stdout)
}

/// Parses source and returns the AST as a debug string.
pub fn show_ast(source: &str) -> Result<String, String> {
    let module = kodo_parser::parse(source).map_err(|e| format!("parse error: {e}"))?;
    Ok(format!("{module:#?}"))
}

/// Parses, type-checks, and lowers source to MIR, returning a debug string.
pub fn show_mir(source: &str) -> Result<String, String> {
    let module = kodo_parser::parse(source).map_err(|e| format!("parse error: {e}"))?;

    let mut checker = kodo_types::TypeChecker::new();
    for (_name, prelude_src) in kodo_std::prelude_sources() {
        if let Ok(m) = kodo_parser::parse(prelude_src) {
            let _ = checker.check_module(&m);
        }
    }
    checker
        .check_module(&module)
        .map_err(|e| format!("type error: {e}"))?;

    let mut module = module;
    kodo_desugar::desugar_module(&mut module);

    let mir_functions = kodo_mir::lowering::lower_module_with_type_info(
        &module,
        checker.struct_registry(),
        checker.enum_registry(),
        checker.enum_names(),
        checker.type_alias_registry(),
    )
    .map_err(|e| format!("MIR lowering error: {e}"))?;

    let mut output = String::new();
    for func in &mir_functions {
        output.push_str(&format!("{func:#?}\n"));
    }
    Ok(output)
}

/// Type-checks the source and returns type information.
pub fn show_type(source: &str) -> Result<String, String> {
    let module = kodo_parser::parse(source).map_err(|e| format!("parse error: {e}"))?;

    let mut checker = kodo_types::TypeChecker::new();
    for (_name, prelude_src) in kodo_std::prelude_sources() {
        if let Ok(m) = kodo_parser::parse(prelude_src) {
            let _ = checker.check_module(&m);
        }
    }
    checker
        .check_module(&module)
        .map_err(|e| format!("type error: {e}"))?;

    // Return the return type of the __repl_type_check function.
    for func in &module.functions {
        if func.name == "__repl_type_check" {
            return Ok(format!("{:?}", func.return_type));
        }
    }

    Ok("()".to_string())
}

/// Locates `libkodo_runtime.a` for the REPL linker step.
fn find_runtime_lib_for_repl() -> Result<std::path::PathBuf, String> {
    // 1. KODO_RUNTIME_LIB env var
    if let Ok(path) = std::env::var("KODO_RUNTIME_LIB") {
        let p = std::path::PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
    }

    // 2. Relative to current executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("libkodo_runtime.a");
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    // 3. Common cargo target directories
    let candidates = [
        "target/debug/libkodo_runtime.a",
        "target/release/libkodo_runtime.a",
    ];
    for candidate in &candidates {
        let p = std::path::PathBuf::from(candidate);
        if p.exists() {
            return Ok(p);
        }
    }

    Err(
        "could not find libkodo_runtime.a — build the workspace first with `cargo build`"
            .to_string(),
    )
}

/// Runs the interactive REPL loop using rustyline for line editing.
///
/// Returns an exit code (0 for normal exit, 1 for error).
pub fn run_repl() -> i32 {
    print_banner();

    let config = rustyline::Config::builder().auto_add_history(true).build();

    let mut editor = match rustyline::DefaultEditor::with_config(config) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: could not initialize line editor: {e}");
            return 1;
        }
    };

    let mut state = ReplState::new();
    let mut multiline_buffer = String::new();

    loop {
        let prompt = if multiline_buffer.is_empty() {
            PROMPT
        } else {
            CONTINUATION_PROMPT
        };

        let line = match editor.readline(prompt) {
            Ok(line) => line,
            Err(rustyline::error::ReadlineError::Interrupted) => {
                // Ctrl-C: clear current input buffer.
                if !multiline_buffer.is_empty() {
                    multiline_buffer.clear();
                    println!("(input cleared)");
                    continue;
                }
                println!("(use :quit to exit)");
                continue;
            }
            Err(rustyline::error::ReadlineError::Eof) => {
                // Ctrl-D: exit.
                println!();
                break;
            }
            Err(e) => {
                eprintln!("readline error: {e}");
                break;
            }
        };

        // Accumulate multi-line input.
        if !multiline_buffer.is_empty() {
            multiline_buffer.push('\n');
            multiline_buffer.push_str(&line);
            if !has_balanced_braces(&multiline_buffer) {
                continue;
            }
            let input = std::mem::take(&mut multiline_buffer);
            handle_input(&input, &mut state);
            continue;
        }

        // Check if we need multi-line input.
        if !has_balanced_braces(&line) {
            multiline_buffer = line;
            continue;
        }

        handle_input(&line, &mut state);
    }

    0
}

/// Handles a single complete input (after multi-line assembly).
fn handle_input(input: &str, state: &mut ReplState) {
    let command = parse_command(input);

    match command {
        ReplCommand::Help => print_help(),
        ReplCommand::Quit => std::process::exit(0),
        ReplCommand::Reset => {
            state.reset();
            println!("(state cleared)");
        }
        ReplCommand::Input(text) => {
            if text.is_empty() {
                return;
            }
            handle_code_input(&text, state);
        }
        ReplCommand::Type(expr) => {
            if expr.is_empty() {
                eprintln!("usage: :type <expression>");
                return;
            }
            let source = state.wrap_for_type_check(&expr);
            match show_type(&source) {
                Ok(ty) => println!("{ty}"),
                Err(e) => eprintln!("{e}"),
            }
        }
        ReplCommand::Ast(expr) => {
            if expr.is_empty() {
                eprintln!("usage: :ast <expression>");
                return;
            }
            let source = state.wrap_expression(&expr);
            match show_ast(&source) {
                Ok(ast) => println!("{ast}"),
                Err(e) => eprintln!("{e}"),
            }
        }
        ReplCommand::Mir(expr) => {
            if expr.is_empty() {
                eprintln!("usage: :mir <expression>");
                return;
            }
            let source = state.wrap_expression(&expr);
            match show_mir(&source) {
                Ok(mir) => println!("{mir}"),
                Err(e) => eprintln!("{e}"),
            }
        }
    }
    // Flush stdout to ensure output appears before next prompt.
    let _ = std::io::stdout().flush();
}

/// Handles code input — either a definition or an expression.
fn handle_code_input(input: &str, state: &mut ReplState) {
    if ReplState::is_type_definition(input) {
        // Try to parse the type definition by wrapping it in a module.
        let source = state.wrap_definition(input);
        match kodo_parser::parse(&source) {
            Ok(_) => {
                state.type_defs.push(input.to_string());
                println!("(defined)");
            }
            Err(e) => eprintln!("parse error: {e}"),
        }
    } else if ReplState::is_definition(input) {
        // Try to parse and type-check the definition.
        let source = state.wrap_definition(input);
        match kodo_parser::parse(&source) {
            Ok(module) => {
                let mut checker = kodo_types::TypeChecker::new();
                for (_name, prelude_src) in kodo_std::prelude_sources() {
                    if let Ok(m) = kodo_parser::parse(prelude_src) {
                        let _ = checker.check_module(&m);
                    }
                }
                match checker.check_module(&module) {
                    Ok(()) => {
                        state.definitions.push(input.to_string());
                        println!("(defined)");
                    }
                    Err(e) => eprintln!("type error: {e}"),
                }
            }
            Err(e) => eprintln!("parse error: {e}"),
        }
    } else {
        // Expression — compile and run.
        let source = state.wrap_expression(input);
        match compile_and_run(&source) {
            Ok(output) => {
                let trimmed = output.trim();
                if !trimmed.is_empty() {
                    println!("{trimmed}");
                }
            }
            Err(e) => eprintln!("{e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_command_help() {
        assert_eq!(parse_command(":help"), ReplCommand::Help);
        assert_eq!(parse_command(":h"), ReplCommand::Help);
    }

    #[test]
    fn test_parse_command_quit() {
        assert_eq!(parse_command(":quit"), ReplCommand::Quit);
        assert_eq!(parse_command(":q"), ReplCommand::Quit);
        assert_eq!(parse_command(":exit"), ReplCommand::Quit);
    }

    #[test]
    fn test_parse_command_reset() {
        assert_eq!(parse_command(":reset"), ReplCommand::Reset);
        assert_eq!(parse_command(":clear"), ReplCommand::Reset);
    }

    #[test]
    fn test_parse_command_type() {
        assert_eq!(
            parse_command(":type 42"),
            ReplCommand::Type("42".to_string())
        );
        assert_eq!(
            parse_command(":t 2 + 3"),
            ReplCommand::Type("2 + 3".to_string())
        );
    }

    #[test]
    fn test_parse_command_ast() {
        assert_eq!(
            parse_command(":ast 1 + 2"),
            ReplCommand::Ast("1 + 2".to_string())
        );
    }

    #[test]
    fn test_parse_command_mir() {
        assert_eq!(parse_command(":mir 42"), ReplCommand::Mir("42".to_string()));
    }

    #[test]
    fn test_parse_command_input() {
        assert_eq!(
            parse_command("2 + 3"),
            ReplCommand::Input("2 + 3".to_string())
        );
        assert_eq!(
            parse_command("fn foo() -> Int { return 1 }"),
            ReplCommand::Input("fn foo() -> Int { return 1 }".to_string())
        );
    }

    #[test]
    fn test_parse_command_empty() {
        assert_eq!(parse_command(""), ReplCommand::Input(String::new()));
        assert_eq!(parse_command("   "), ReplCommand::Input(String::new()));
    }

    #[test]
    fn test_balanced_braces() {
        assert!(has_balanced_braces(""));
        assert!(has_balanced_braces("2 + 3"));
        assert!(has_balanced_braces("fn foo() { return 1 }"));
        assert!(!has_balanced_braces("fn foo() {"));
        assert!(!has_balanced_braces("fn foo() { { }"));
        assert!(has_balanced_braces("fn foo() { { } }"));
    }

    #[test]
    fn test_is_definition() {
        assert!(ReplState::is_definition("fn foo() -> Int { return 1 }"));
        assert!(ReplState::is_definition(
            "  fn bar(x: Int) -> Int { return x }"
        ));
        assert!(!ReplState::is_definition("2 + 3"));
        assert!(!ReplState::is_definition("let x: Int = 42"));
    }

    #[test]
    fn test_is_type_definition() {
        assert!(ReplState::is_type_definition(
            "struct Point { x: Int, y: Int }"
        ));
        assert!(ReplState::is_type_definition("enum Color { Red, Blue }"));
        assert!(ReplState::is_type_definition("type Age = Int"));
        assert!(!ReplState::is_type_definition(
            "fn foo() -> Int { return 1 }"
        ));
        assert!(!ReplState::is_type_definition("2 + 3"));
    }

    #[test]
    fn test_repl_state_new() {
        let state = ReplState::new();
        assert!(state.definitions.is_empty());
        assert!(state.type_defs.is_empty());
        assert_eq!(state.eval_counter, 0);
    }

    #[test]
    fn test_repl_state_reset() {
        let mut state = ReplState::new();
        state
            .definitions
            .push("fn foo() -> Int { return 1 }".to_string());
        state.type_defs.push("struct Pt { x: Int }".to_string());
        state.eval_counter = 5;

        state.reset();

        assert!(state.definitions.is_empty());
        assert!(state.type_defs.is_empty());
        assert_eq!(state.eval_counter, 0);
    }

    #[test]
    fn test_wrap_expression() {
        let mut state = ReplState::new();
        let source = state.wrap_expression("2 + 3");

        assert!(source.contains("module repl"));
        assert!(source.contains("meta { purpose: \"repl\" }"));
        assert!(source.contains("fn main() -> Int"));
        assert!(source.contains("2 + 3"));
        assert_eq!(state.eval_counter, 1);
    }

    #[test]
    fn test_wrap_expression_with_definitions() {
        let mut state = ReplState::new();
        state
            .definitions
            .push("fn double(x: Int) -> Int { return x * 2 }".to_string());

        let source = state.wrap_expression("double(21)");

        assert!(source.contains("fn double(x: Int) -> Int"));
        assert!(source.contains("double(21)"));
    }

    #[test]
    fn test_wrap_expression_with_type_defs() {
        let mut state = ReplState::new();
        state
            .type_defs
            .push("struct Point { x: Int, y: Int }".to_string());

        let source = state.wrap_expression("42");

        assert!(source.contains("struct Point { x: Int, y: Int }"));
        assert!(source.contains("42"));
    }

    #[test]
    fn test_wrap_definition() {
        let state = ReplState::new();
        let source = state.wrap_definition("fn add(a: Int, b: Int) -> Int { return a + b }");

        assert!(source.contains("module repl"));
        assert!(source.contains("fn add(a: Int, b: Int) -> Int"));
        assert!(source.contains("fn main() -> Int"));
    }

    #[test]
    fn test_wrap_for_type_check() {
        let state = ReplState::new();
        let source = state.wrap_for_type_check("42");

        assert!(source.contains("module repl"));
        assert!(source.contains("fn __repl_type_check() -> Int"));
        assert!(source.contains("42"));
    }

    #[test]
    fn test_eval_counter_increments() {
        let mut state = ReplState::new();
        let _ = state.wrap_expression("1");
        assert_eq!(state.eval_counter, 1);
        let _ = state.wrap_expression("2");
        assert_eq!(state.eval_counter, 2);
    }

    #[test]
    fn test_show_ast_valid() {
        let source = "module test {\n    meta { purpose: \"test\" }\n    fn main() -> Int {\n        return 42\n    }\n}\n";
        let result = show_ast(source);
        assert!(result.is_ok());
        let ast = result.unwrap();
        assert!(ast.contains("Module"));
    }

    #[test]
    fn test_show_ast_invalid() {
        let result = show_ast("this is not valid kodo");
        assert!(result.is_err());
    }
}
