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

/// The type of a REPL expression, used to choose the correct print wrapper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprType {
    /// Integer expression — printed via `print_int`.
    Int,
    /// Float64 expression — printed via `println_float`.
    Float64,
    /// String expression — printed via `println`.
    String,
    /// Boolean expression — printed via conditional `println`.
    Bool,
    /// Unit/void expression (e.g. `println(...)`) — no result to print.
    Unit,
    /// Unknown or composite type — executed as a statement.
    Other(std::string::String),
}

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
    /// Variable bindings (`let` statements) accumulated across REPL inputs.
    pub bindings: Vec<String>,
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
        self.bindings.clear();
        self.eval_counter = 0;
    }

    /// Returns whether the input looks like a function definition.
    pub fn is_definition(input: &str) -> bool {
        let trimmed = input.trim();
        trimmed.starts_with("fn ")
    }

    /// Returns whether the input looks like a variable binding (`let`).
    pub fn is_let_binding(input: &str) -> bool {
        input.trim().starts_with("let ")
    }

    /// Returns whether the input looks like a type definition (struct/enum/type).
    pub fn is_type_definition(input: &str) -> bool {
        let trimmed = input.trim();
        trimmed.starts_with("struct ")
            || trimmed.starts_with("enum ")
            || trimmed.starts_with("type ")
    }

    /// Extracts the name from a function definition (`fn name(...)`).
    fn extract_fn_name(input: &str) -> Option<&str> {
        let trimmed = input.trim().strip_prefix("fn ")?;
        trimmed
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .next()
    }

    /// Extracts the name from a type definition (`struct Name`, `enum Name`, `type Name`).
    fn extract_type_name(input: &str) -> Option<&str> {
        let trimmed = input.trim();
        let rest = trimmed
            .strip_prefix("struct ")
            .or_else(|| trimmed.strip_prefix("enum "))
            .or_else(|| trimmed.strip_prefix("type "))?;
        rest.split(|c: char| !c.is_alphanumeric() && c != '_')
            .next()
    }

    /// Extracts the variable name from a `let` binding (`let name: Type = ...`).
    fn extract_let_name(input: &str) -> Option<&str> {
        let trimmed = input.trim().strip_prefix("let ")?;
        trimmed
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .next()
    }

    /// Adds or replaces a function definition (by name).
    pub fn upsert_definition(&mut self, input: &str) {
        if let Some(name) = Self::extract_fn_name(input) {
            self.definitions
                .retain(|d| Self::extract_fn_name(d) != Some(name));
        }
        self.definitions.push(input.to_string());
    }

    /// Adds or replaces a type definition (by name).
    pub fn upsert_type_def(&mut self, input: &str) {
        if let Some(name) = Self::extract_type_name(input) {
            self.type_defs
                .retain(|d| Self::extract_type_name(d) != Some(name));
        }
        self.type_defs.push(input.to_string());
    }

    /// Adds or replaces a variable binding (by name).
    pub fn upsert_binding(&mut self, input: &str) {
        if let Some(name) = Self::extract_let_name(input) {
            self.bindings
                .retain(|b| Self::extract_let_name(b) != Some(name));
        }
        self.bindings.push(input.to_string());
    }

    /// Builds the module preamble with accumulated type and function definitions.
    fn build_preamble(&self) -> String {
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

        source
    }

    /// Emits accumulated `let` bindings into a function body.
    fn emit_bindings(&self, source: &mut String) {
        for binding in &self.bindings {
            source.push_str("        ");
            source.push_str(binding);
            source.push('\n');
        }
    }

    /// Infers the type of a REPL expression using a type-check probe.
    ///
    /// Wraps the expression with `let __probe: Int = {expr}` and type-checks.
    /// If it succeeds, the type is `Int`. If it fails, the error message
    /// reveals the actual type (e.g. `found \`String\``).
    pub fn infer_expression_type(&self, expr: &str) -> ExprType {
        let mut source = self.build_preamble();
        source.push_str("\n    fn __repl_probe() -> Int {\n");
        self.emit_bindings(&mut source);
        source.push_str(&format!("        let __probe: Int = {expr}\n"));
        source.push_str("        return __probe\n");
        source.push_str("    }\n");
        source.push_str("}\n");

        let module = match kodo_parser::parse(&source) {
            Ok(m) => m,
            Err(_) => return ExprType::Other("parse error".to_string()),
        };

        let mut checker = kodo_types::TypeChecker::new();
        for (_name, prelude_src) in kodo_std::prelude_sources() {
            if let Ok(m) = kodo_parser::parse(prelude_src) {
                let _ = checker.check_module(&m);
            }
        }

        match checker.check_module(&module) {
            Ok(()) => ExprType::Int,
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("found `()`") {
                    ExprType::Unit
                } else if msg.contains("found `String`") {
                    ExprType::String
                } else if msg.contains("found `Float64`") {
                    ExprType::Float64
                } else if msg.contains("found `Bool`") {
                    ExprType::Bool
                } else {
                    ExprType::Other(msg)
                }
            }
        }
    }

    /// Wraps an expression in a complete Kōdo module source for compilation.
    ///
    /// The generated module includes all accumulated definitions and wraps the
    /// expression in a `main` function that prints the result using the
    /// appropriate builtin for the expression's type.
    pub fn wrap_expression(&mut self, expr: &str) -> String {
        let expr_type = self.infer_expression_type(expr);
        self.wrap_expression_typed(expr, &expr_type)
    }

    /// Wraps an expression with a known type in a compilable module.
    pub fn wrap_expression_typed(&mut self, expr: &str, expr_type: &ExprType) -> String {
        self.eval_counter += 1;
        let mut source = self.build_preamble();

        source.push_str("\n    fn main() -> Int {\n");
        self.emit_bindings(&mut source);

        match expr_type {
            ExprType::Int => {
                source.push_str(&format!("        let __result: Int = {expr}\n"));
                source.push_str("        print_int(__result)\n");
            }
            ExprType::Float64 => {
                source.push_str(&format!("        let __result: Float64 = {expr}\n"));
                source.push_str("        println_float(__result)\n");
            }
            ExprType::String => {
                source.push_str(&format!("        let __result: String = {expr}\n"));
                source.push_str("        println(__result)\n");
            }
            ExprType::Bool => {
                source.push_str(&format!("        let __result: Bool = {expr}\n"));
                source.push_str(
                    "        if __result { println(\"true\") } else { println(\"false\") }\n",
                );
            }
            ExprType::Unit | ExprType::Other(_) => {
                // Execute as a statement — no result capture.
                source.push_str(&format!("        {expr}\n"));
            }
        }

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

/// Returns the inferred type of a REPL expression as a display string.
pub fn show_type_for_expr(state: &ReplState, expr: &str) -> String {
    match state.infer_expression_type(expr) {
        ExprType::Int => "Int".to_string(),
        ExprType::Float64 => "Float64".to_string(),
        ExprType::String => "String".to_string(),
        ExprType::Bool => "Bool".to_string(),
        ExprType::Unit => "()".to_string(),
        ExprType::Other(msg) => format!("unknown ({msg})"),
    }
}

/// Locates `libkodo_runtime.a` for the REPL linker step.
fn find_runtime_lib_for_repl() -> Result<std::path::PathBuf, String> {
    crate::embedded_runtime::find_runtime_lib()
}

/// Runs the interactive REPL loop using rustyline for line editing.
///
/// Returns an exit code (0 for normal exit, 1 for error).
pub fn run_repl() -> i32 {
    run_repl_with_mode(false)
}

/// Runs the REPL in JSON mode: all output is structured JSON to stdout.
///
/// Each response is a single JSON object with fields:
/// - `"status"`: `"ok"` or `"error"`
/// - `"kind"`: `"result"`, `"defined"`, `"bound"`, `"type"`, `"ast"`, `"mir"`, `"help"`, `"reset"`
/// - `"value"`: the result value (for expressions)
/// - `"type"`: the inferred type (for expressions and `:type`)
/// - `"error"`: the error message (when status is `"error"`)
///
/// Returns an exit code (0 for normal exit, 1 for error).
pub fn run_repl_json() -> i32 {
    run_repl_with_mode(true)
}

/// Default history file name, stored in the user's home directory.
const HISTORY_FILE: &str = ".kodo_history";

/// Returns the path to the REPL history file (`~/.kodo_history`).
///
/// Falls back to a local `.kodo_history` in the current directory if the
/// `HOME` environment variable is not set.
fn history_path() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(|home| std::path::PathBuf::from(home).join(HISTORY_FILE))
        .unwrap_or_else(|_| std::path::PathBuf::from(HISTORY_FILE))
}

/// Internal REPL loop shared between normal and JSON modes.
fn run_repl_with_mode(json_mode: bool) -> i32 {
    if !json_mode {
        print_banner();
    }

    let config = rustyline::Config::builder().auto_add_history(true).build();

    let mut editor = match rustyline::DefaultEditor::with_config(config) {
        Ok(e) => e,
        Err(e) => {
            if json_mode {
                emit_json_error(&format!("could not initialize line editor: {e}"));
            } else {
                eprintln!("error: could not initialize line editor: {e}");
            }
            return 1;
        }
    };

    // Load previous history (ignore errors — file may not exist on first run).
    let hist_path = history_path();
    let _ = editor.load_history(&hist_path);

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
                    if !json_mode {
                        println!("(input cleared)");
                    }
                    continue;
                }
                if !json_mode {
                    println!("(use :quit to exit)");
                }
                continue;
            }
            Err(rustyline::error::ReadlineError::Eof) => {
                // Ctrl-D: exit.
                if !json_mode {
                    println!();
                }
                break;
            }
            Err(e) => {
                if json_mode {
                    emit_json_error(&format!("readline error: {e}"));
                } else {
                    eprintln!("readline error: {e}");
                }
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
            if !handle_input_with_mode(&input, &mut state, json_mode) {
                break;
            }
            continue;
        }

        // Check if we need multi-line input.
        if !has_balanced_braces(&line) {
            multiline_buffer = line;
            continue;
        }

        if !handle_input_with_mode(&line, &mut state, json_mode) {
            break;
        }
    }

    // Save history on exit (ignore errors — best-effort persistence).
    let _ = editor.save_history(&hist_path);

    0
}

/// Handles a single complete input with optional JSON output mode.
///
/// Returns `true` if the REPL should continue, `false` if the user requested to quit.
fn handle_input_with_mode(input: &str, state: &mut ReplState, json_mode: bool) -> bool {
    let command = parse_command(input);

    match command {
        ReplCommand::Help => {
            if json_mode {
                emit_json_ok("help", None, None);
            } else {
                print_help();
            }
        }
        ReplCommand::Quit => return false,
        ReplCommand::Reset => {
            state.reset();
            if json_mode {
                emit_json_ok("reset", None, None);
            } else {
                println!("(state cleared)");
            }
        }
        ReplCommand::Input(text) => {
            if text.is_empty() {
                return true;
            }
            handle_code_input_with_mode(&text, state, json_mode);
        }
        ReplCommand::Type(expr) => {
            if expr.is_empty() {
                if json_mode {
                    emit_json_error("usage: :type <expression>");
                } else {
                    eprintln!("usage: :type <expression>");
                }
                return true;
            }
            let ty = show_type_for_expr(state, &expr);
            if json_mode {
                emit_json_ok("type", None, Some(&ty));
            } else {
                println!("{ty}");
            }
        }
        ReplCommand::Ast(expr) => {
            if expr.is_empty() {
                if json_mode {
                    emit_json_error("usage: :ast <expression>");
                } else {
                    eprintln!("usage: :ast <expression>");
                }
                return true;
            }
            let source = state.wrap_expression(&expr);
            match show_ast(&source) {
                Ok(ast) => {
                    if json_mode {
                        emit_json_ok("ast", Some(&ast), None);
                    } else {
                        println!("{ast}");
                    }
                }
                Err(e) => {
                    if json_mode {
                        emit_json_error(&e);
                    } else {
                        eprintln!("{e}");
                    }
                }
            }
        }
        ReplCommand::Mir(expr) => {
            if expr.is_empty() {
                if json_mode {
                    emit_json_error("usage: :mir <expression>");
                } else {
                    eprintln!("usage: :mir <expression>");
                }
                return true;
            }
            let source = state.wrap_expression(&expr);
            match show_mir(&source) {
                Ok(mir) => {
                    if json_mode {
                        emit_json_ok("mir", Some(&mir), None);
                    } else {
                        println!("{mir}");
                    }
                }
                Err(e) => {
                    if json_mode {
                        emit_json_error(&e);
                    } else {
                        eprintln!("{e}");
                    }
                }
            }
        }
    }
    // Flush stdout to ensure output appears before next prompt.
    let _ = std::io::stdout().flush();
    true
}

/// Handles code input with optional JSON output mode.
fn handle_code_input_with_mode(input: &str, state: &mut ReplState, json_mode: bool) {
    if ReplState::is_type_definition(input) {
        // Try to parse the type definition by wrapping it in a module.
        let source = state.wrap_definition(input);
        match kodo_parser::parse(&source) {
            Ok(_) => {
                state.upsert_type_def(input);
                if json_mode {
                    emit_json_ok("defined", None, None);
                } else {
                    println!("(defined)");
                }
            }
            Err(e) => {
                if json_mode {
                    emit_json_error(&format!("parse error: {e}"));
                } else {
                    eprintln!("parse error: {e}");
                }
            }
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
                        state.upsert_definition(input);
                        if json_mode {
                            emit_json_ok("defined", None, None);
                        } else {
                            println!("(defined)");
                        }
                    }
                    Err(e) => {
                        if json_mode {
                            emit_json_error(&format!("type error: {e}"));
                        } else {
                            eprintln!("type error: {e}");
                        }
                    }
                }
            }
            Err(e) => {
                if json_mode {
                    emit_json_error(&format!("parse error: {e}"));
                } else {
                    eprintln!("parse error: {e}");
                }
            }
        }
    } else if ReplState::is_let_binding(input) {
        // Validate the binding by wrapping in a module and type-checking.
        let mut probe = state.build_preamble();
        probe.push_str("\n    fn main() -> Int {\n");
        state.emit_bindings(&mut probe);
        probe.push_str(&format!("        {input}\n"));
        probe.push_str("        return 0\n");
        probe.push_str("    }\n");
        probe.push_str("}\n");

        match kodo_parser::parse(&probe) {
            Ok(module) => {
                let mut checker = kodo_types::TypeChecker::new();
                for (_name, prelude_src) in kodo_std::prelude_sources() {
                    if let Ok(m) = kodo_parser::parse(prelude_src) {
                        let _ = checker.check_module(&m);
                    }
                }
                match checker.check_module(&module) {
                    Ok(()) => {
                        state.upsert_binding(input);
                        if json_mode {
                            emit_json_ok("bound", None, None);
                        } else {
                            println!("(bound)");
                        }
                    }
                    Err(e) => {
                        if json_mode {
                            emit_json_error(&format!("type error: {e}"));
                        } else {
                            eprintln!("type error: {e}");
                        }
                    }
                }
            }
            Err(e) => {
                if json_mode {
                    emit_json_error(&format!("parse error: {e}"));
                } else {
                    eprintln!("parse error: {e}");
                }
            }
        }
    } else {
        // Expression — compile and run.
        let expr_type = state.infer_expression_type(input);
        let type_str = match &expr_type {
            ExprType::Int => "Int",
            ExprType::Float64 => "Float64",
            ExprType::String => "String",
            ExprType::Bool => "Bool",
            ExprType::Unit => "()",
            ExprType::Other(_) => "unknown",
        };
        let source = state.wrap_expression_typed(input, &expr_type);
        match compile_and_run(&source) {
            Ok(output) => {
                let trimmed = output.trim();
                if json_mode {
                    let value = if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed)
                    };
                    emit_json_ok("result", value, Some(type_str));
                } else if !trimmed.is_empty() {
                    println!("{trimmed}");
                }
            }
            Err(e) => {
                if json_mode {
                    emit_json_error(&e);
                } else {
                    eprintln!("{e}");
                }
            }
        }
    }
}

/// Emits a JSON success response to stdout.
///
/// Used in JSON REPL mode to provide structured output for AI agents.
fn emit_json_ok(kind: &str, value: Option<&str>, type_name: Option<&str>) {
    let mut obj = serde_json::Map::new();
    obj.insert(
        "status".to_string(),
        serde_json::Value::String("ok".to_string()),
    );
    obj.insert(
        "kind".to_string(),
        serde_json::Value::String(kind.to_string()),
    );
    if let Some(v) = value {
        obj.insert(
            "value".to_string(),
            serde_json::Value::String(v.to_string()),
        );
    }
    if let Some(t) = type_name {
        obj.insert("type".to_string(), serde_json::Value::String(t.to_string()));
    }
    let json = serde_json::Value::Object(obj);
    println!(
        "{}",
        serde_json::to_string(&json)
            .unwrap_or_else(|e| format!("{{\"error\": \"json serialization failed: {e}\"}}"))
    );
}

/// Emits a JSON error response to stdout.
///
/// Used in JSON REPL mode to provide structured error output for AI agents.
fn emit_json_error(message: &str) {
    let mut obj = serde_json::Map::new();
    obj.insert(
        "status".to_string(),
        serde_json::Value::String("error".to_string()),
    );
    obj.insert(
        "error".to_string(),
        serde_json::Value::String(message.to_string()),
    );
    let json = serde_json::Value::Object(obj);
    println!(
        "{}",
        serde_json::to_string(&json)
            .unwrap_or_else(|e| format!("{{\"error\": \"json serialization failed: {e}\"}}"))
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── parse_command ──────────────────────────────────────────

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
    fn test_parse_command_unknown() {
        // Unknown commands return empty Input.
        assert_eq!(parse_command(":foobar"), ReplCommand::Input(String::new()));
    }

    #[test]
    fn test_parse_command_with_extra_whitespace() {
        assert_eq!(
            parse_command("  :type   1 + 2  "),
            ReplCommand::Type("1 + 2".to_string())
        );
        assert_eq!(parse_command("  :help  "), ReplCommand::Help);
    }

    // ─── has_balanced_braces ────────────────────────────────────

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
    fn test_balanced_braces_deeply_nested() {
        assert!(has_balanced_braces("{ { { } } }"));
        assert!(!has_balanced_braces("{ { { } }"));
        assert!(!has_balanced_braces("{ { } } }"));
    }

    #[test]
    fn test_balanced_braces_only_closing() {
        assert!(!has_balanced_braces("}"));
        assert!(!has_balanced_braces("} }"));
    }

    #[test]
    fn test_balanced_braces_empty_pairs() {
        assert!(has_balanced_braces("{}{}{}"));
    }

    // ─── is_let_binding / is_definition / is_type_definition ──────

    #[test]
    fn test_is_let_binding() {
        assert!(ReplState::is_let_binding("let x: Int = 42"));
        assert!(ReplState::is_let_binding("  let name: String = \"hi\""));
        assert!(!ReplState::is_let_binding("letter"));
        assert!(!ReplState::is_let_binding("fn foo() -> Int { return 1 }"));
        assert!(!ReplState::is_let_binding("2 + 3"));
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
    fn test_is_definition_not_fn_prefix() {
        // "fnord" starts with "fn" but not "fn " — should NOT match.
        assert!(!ReplState::is_definition("fnord"));
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
    fn test_is_type_definition_not_prefix() {
        assert!(!ReplState::is_type_definition("structural"));
        assert!(!ReplState::is_type_definition("enumerate"));
        assert!(!ReplState::is_type_definition("typeof"));
    }

    // ─── ReplState basics ───────────────────────────────────────

    #[test]
    fn test_repl_state_new() {
        let state = ReplState::new();
        assert!(state.definitions.is_empty());
        assert!(state.type_defs.is_empty());
        assert!(state.bindings.is_empty());
        assert_eq!(state.eval_counter, 0);
    }

    #[test]
    fn test_repl_state_reset() {
        let mut state = ReplState::new();
        state
            .definitions
            .push("fn foo() -> Int { return 1 }".to_string());
        state.type_defs.push("struct Pt { x: Int }".to_string());
        state.bindings.push("let x: Int = 42".to_string());
        state.eval_counter = 5;

        state.reset();

        assert!(state.definitions.is_empty());
        assert!(state.type_defs.is_empty());
        assert!(state.bindings.is_empty());
        assert_eq!(state.eval_counter, 0);
    }

    #[test]
    fn test_eval_counter_increments() {
        let mut state = ReplState::new();
        let _ = state.wrap_expression("1");
        assert_eq!(state.eval_counter, 1);
        let _ = state.wrap_expression("2");
        assert_eq!(state.eval_counter, 2);
    }

    // ─── infer_expression_type ──────────────────────────────────

    #[test]
    fn test_infer_int_literal() {
        let state = ReplState::new();
        assert_eq!(state.infer_expression_type("42"), ExprType::Int);
    }

    #[test]
    fn test_infer_int_arithmetic() {
        let state = ReplState::new();
        assert_eq!(state.infer_expression_type("2 + 3"), ExprType::Int);
    }

    #[test]
    fn test_infer_string_literal() {
        let state = ReplState::new();
        assert_eq!(state.infer_expression_type("\"hello\""), ExprType::String);
    }

    #[test]
    fn test_infer_bool_literal() {
        let state = ReplState::new();
        assert_eq!(state.infer_expression_type("true"), ExprType::Bool);
        assert_eq!(state.infer_expression_type("false"), ExprType::Bool);
    }

    #[test]
    fn test_infer_bool_comparison() {
        let state = ReplState::new();
        assert_eq!(state.infer_expression_type("1 > 2"), ExprType::Bool);
        assert_eq!(state.infer_expression_type("1 == 1"), ExprType::Bool);
    }

    #[test]
    fn test_infer_unit_println() {
        let state = ReplState::new();
        assert_eq!(
            state.infer_expression_type("println(\"hello\")"),
            ExprType::Unit
        );
    }

    #[test]
    fn test_infer_unit_print_int() {
        let state = ReplState::new();
        assert_eq!(state.infer_expression_type("print_int(42)"), ExprType::Unit);
    }

    #[test]
    fn test_infer_float64_literal() {
        let state = ReplState::new();
        assert_eq!(state.infer_expression_type("3.14"), ExprType::Float64);
    }

    #[test]
    fn test_infer_float64_arithmetic() {
        let state = ReplState::new();
        assert_eq!(state.infer_expression_type("1.0 + 2.5"), ExprType::Float64);
    }

    #[test]
    fn test_infer_parse_error() {
        let state = ReplState::new();
        // Completely invalid syntax should return Other.
        let result = state.infer_expression_type("@@@invalid!!!");
        assert!(matches!(result, ExprType::Other(_)));
    }

    // ─── wrap_expression (typed) ────────────────────────────────

    #[test]
    fn test_wrap_expression_int() {
        let mut state = ReplState::new();
        let source = state.wrap_expression("2 + 3");

        assert!(source.contains("module repl"));
        assert!(source.contains("meta { purpose: \"repl\" }"));
        assert!(source.contains("fn main() -> Int"));
        assert!(source.contains("let __result: Int = 2 + 3"));
        assert!(source.contains("print_int(__result)"));
        assert_eq!(state.eval_counter, 1);
    }

    #[test]
    fn test_wrap_expression_string() {
        let mut state = ReplState::new();
        let source = state.wrap_expression("\"hello\"");

        assert!(source.contains("let __result: String = \"hello\""));
        assert!(source.contains("println(__result)"));
    }

    #[test]
    fn test_wrap_expression_bool() {
        let mut state = ReplState::new();
        let source = state.wrap_expression("true");

        assert!(source.contains("let __result: Bool = true"));
        assert!(source.contains("println(\"true\")"));
        assert!(source.contains("println(\"false\")"));
    }

    #[test]
    fn test_wrap_expression_float64() {
        let mut state = ReplState::new();
        let source = state.wrap_expression("3.14");

        assert!(source.contains("let __result: Float64 = 3.14"));
        assert!(source.contains("println_float(__result)"));
    }

    #[test]
    fn test_wrap_expression_unit_println() {
        let mut state = ReplState::new();
        let source = state.wrap_expression("println(\"Hello from the REPL!\")");

        // Unit expressions are executed as statements — no `let __result`.
        assert!(!source.contains("let __result"));
        assert!(source.contains("println(\"Hello from the REPL!\")"));
        assert!(source.contains("return 0"));
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
    fn test_wrap_expression_with_bindings() {
        let mut state = ReplState::new();
        state.bindings.push("let x: Int = 2 + 3".to_string());

        let source = state.wrap_expression("x");

        // Binding should appear inside main body before the expression.
        assert!(source.contains("let x: Int = 2 + 3"));
        assert!(source.contains("let __result: Int = x"));
        assert!(source.contains("print_int(__result)"));
    }

    #[test]
    fn test_wrap_expression_multiple_bindings() {
        let mut state = ReplState::new();
        state.bindings.push("let a: Int = 10".to_string());
        state.bindings.push("let b: Int = 20".to_string());

        let source = state.wrap_expression("a + b");

        assert!(source.contains("let a: Int = 10"));
        assert!(source.contains("let b: Int = 20"));
        assert!(source.contains("a + b"));
    }

    #[test]
    fn test_infer_with_bindings() {
        let mut state = ReplState::new();
        state.bindings.push("let x: Int = 42".to_string());

        // `x` should be recognized as Int since binding is in scope.
        assert_eq!(state.infer_expression_type("x"), ExprType::Int);
    }

    #[test]
    fn test_infer_string_binding() {
        let mut state = ReplState::new();
        state.bindings.push("let s: String = \"hello\"".to_string());

        assert_eq!(state.infer_expression_type("s"), ExprType::String);
    }

    // ─── wrap_definition ────────────────────────────────────────

    #[test]
    fn test_wrap_definition() {
        let state = ReplState::new();
        let source = state.wrap_definition("fn add(a: Int, b: Int) -> Int { return a + b }");

        assert!(source.contains("module repl"));
        assert!(source.contains("fn add(a: Int, b: Int) -> Int"));
        assert!(source.contains("fn main() -> Int"));
    }

    #[test]
    fn test_wrap_definition_includes_accumulated() {
        let mut state = ReplState::new();
        state
            .definitions
            .push("fn helper() -> Int { return 1 }".to_string());
        state.type_defs.push("struct Foo { val: Int }".to_string());

        let source = state.wrap_definition("fn bar(x: Int) -> Int { return helper() + x }");

        assert!(source.contains("fn helper() -> Int"));
        assert!(source.contains("struct Foo { val: Int }"));
        assert!(source.contains("fn bar(x: Int) -> Int"));
    }

    // ─── show_type_for_expr ─────────────────────────────────────

    #[test]
    fn test_show_type_int() {
        let state = ReplState::new();
        assert_eq!(show_type_for_expr(&state, "42"), "Int");
    }

    #[test]
    fn test_show_type_string() {
        let state = ReplState::new();
        assert_eq!(show_type_for_expr(&state, "\"hello\""), "String");
    }

    #[test]
    fn test_show_type_bool() {
        let state = ReplState::new();
        assert_eq!(show_type_for_expr(&state, "true"), "Bool");
    }

    #[test]
    fn test_show_type_float64() {
        let state = ReplState::new();
        assert_eq!(show_type_for_expr(&state, "3.14"), "Float64");
    }

    #[test]
    fn test_show_type_unit() {
        let state = ReplState::new();
        assert_eq!(show_type_for_expr(&state, "println(\"x\")"), "()");
    }

    // ─── show_ast ───────────────────────────────────────────────

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

    // ─── build_preamble ─────────────────────────────────────────

    #[test]
    fn test_build_preamble_empty_state() {
        let state = ReplState::new();
        let preamble = state.build_preamble();

        assert!(preamble.contains("module repl"));
        assert!(preamble.contains("meta { purpose: \"repl\" }"));
        // No definitions or type defs.
        assert!(!preamble.contains("fn "));
        assert!(!preamble.contains("struct "));
    }

    #[test]
    fn test_build_preamble_with_state() {
        let mut state = ReplState::new();
        state.type_defs.push("struct Pt { x: Int }".to_string());
        state
            .definitions
            .push("fn id(x: Int) -> Int { return x }".to_string());

        let preamble = state.build_preamble();

        assert!(preamble.contains("struct Pt { x: Int }"));
        assert!(preamble.contains("fn id(x: Int) -> Int"));
    }

    // ─── extract names ──────────────────────────────────────────

    #[test]
    fn test_extract_fn_name() {
        assert_eq!(
            ReplState::extract_fn_name("fn double(x: Int) -> Int { return x * 2 }"),
            Some("double")
        );
        assert_eq!(
            ReplState::extract_fn_name("fn foo() -> Int { return 1 }"),
            Some("foo")
        );
        assert_eq!(ReplState::extract_fn_name("not a function"), None);
    }

    #[test]
    fn test_extract_type_name() {
        assert_eq!(
            ReplState::extract_type_name("struct Point { x: Int, y: Int }"),
            Some("Point")
        );
        assert_eq!(
            ReplState::extract_type_name("enum Color { Red, Blue }"),
            Some("Color")
        );
        assert_eq!(ReplState::extract_type_name("type Age = Int"), Some("Age"));
        assert_eq!(ReplState::extract_type_name("let x: Int = 42"), None);
    }

    #[test]
    fn test_extract_let_name() {
        assert_eq!(ReplState::extract_let_name("let x: Int = 42"), Some("x"));
        assert_eq!(
            ReplState::extract_let_name("let my_var: String = \"hello\""),
            Some("my_var")
        );
        assert_eq!(ReplState::extract_let_name("fn foo() -> Int { 1 }"), None);
    }

    // ─── upsert (no duplicates) ─────────────────────────────────

    #[test]
    fn test_upsert_definition_replaces() {
        let mut state = ReplState::new();
        state.upsert_definition("fn foo() -> Int { return 1 }");
        state.upsert_definition("fn foo() -> Int { return 2 }");

        assert_eq!(state.definitions.len(), 1);
        assert!(state.definitions[0].contains("return 2"));
    }

    #[test]
    fn test_upsert_definition_keeps_different() {
        let mut state = ReplState::new();
        state.upsert_definition("fn foo() -> Int { return 1 }");
        state.upsert_definition("fn bar() -> Int { return 2 }");

        assert_eq!(state.definitions.len(), 2);
    }

    #[test]
    fn test_upsert_type_def_replaces() {
        let mut state = ReplState::new();
        state.upsert_type_def("struct Point { x: Int }");
        state.upsert_type_def("struct Point { x: Int, y: Int }");

        assert_eq!(state.type_defs.len(), 1);
        assert!(state.type_defs[0].contains("y: Int"));
    }

    #[test]
    fn test_upsert_binding_replaces() {
        let mut state = ReplState::new();
        state.upsert_binding("let x: Int = 10");
        state.upsert_binding("let x: Int = 20");

        assert_eq!(state.bindings.len(), 1);
        assert!(state.bindings[0].contains("20"));
    }

    #[test]
    fn test_upsert_binding_keeps_different() {
        let mut state = ReplState::new();
        state.upsert_binding("let x: Int = 10");
        state.upsert_binding("let y: Int = 20");

        assert_eq!(state.bindings.len(), 2);
    }
}
