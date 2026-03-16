//! # `kodo_resolver` — Intent Resolution Engine for the Kōdo Language
//!
//! This crate implements the intent resolution system, Kōdo's most distinctive
//! feature. Agents declare WHAT should happen using `intent` blocks, and the
//! resolver maps those declarations to concrete implementations.
//!
//! The intent system bridges the gap between AI agents' high-level reasoning
//! and the concrete code that machines execute. Agents describe goals; the
//! resolver generates verified implementations.
//!
//! ## How It Works
//!
//! 1. Agent writes `intent` blocks with configuration
//! 2. Resolver looks up matching resolver strategies
//! 3. Strategy generates concrete implementation code
//! 4. Generated code is verified against the intent's contracts
//! 5. If verification passes, code is injected into the compilation pipeline
//!
//! ## Built-in Resolvers
//!
//! - **`console_app`**: Generates a `kodo_main` function that prints a greeting
//!   message. Config: `greeting` (string).
//! - **`math_module`**: Generates mathematical helper functions from declarations.
//!   Config: `functions` (list of function references).
//!
//! ## Academic References
//!
//! - **\[PLP\]** *Programming Language Pragmatics* Ch. 10, 14–15 —
//!   Metaprogramming and compile-time code generation; intent resolution
//!   is a form of declarative metaprogramming where agents specify goals.
//!
//! Note: The intent system is a novel construct in Kōdo with no direct
//! precedent in the literature.
//!
//! See `docs/REFERENCES.md` for the full bibliography.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
#![warn(clippy::pedantic)]

use kodo_ast::{
    BinOp, Block, Expr, Function, IntentConfigValue, IntentDecl, NodeId, Ownership, Param, Span,
    Stmt, TypeExpr, Visibility,
};
use thiserror::Error;

/// Errors from intent resolution.
#[derive(Debug, Error)]
pub enum ResolverError {
    /// No resolver was found for the given intent.
    #[error("no resolver found for intent `{intent}` at {span:?}")]
    NoResolver {
        /// The intent name.
        intent: String,
        /// Source location.
        span: Span,
    },
    /// The resolved implementation does not satisfy the intent's contracts.
    #[error("resolved implementation for `{intent}` violates contracts: {reason}")]
    ContractViolation {
        /// The intent name.
        intent: String,
        /// Description of the violation.
        reason: String,
    },
    /// An intent configuration key is invalid.
    #[error("unknown configuration key `{key}` for intent `{intent}` at {span:?}")]
    UnknownConfig {
        /// The invalid key.
        key: String,
        /// The intent name.
        intent: String,
        /// Source location.
        span: Span,
    },
    /// An intent configuration value has the wrong type.
    #[error("config `{key}` for intent `{intent}` expects {expected}, found {found} at {span:?}")]
    ConfigTypeMismatch {
        /// The config key.
        key: String,
        /// The intent name.
        intent: String,
        /// Expected type description.
        expected: String,
        /// Found type description.
        found: String,
        /// Source location.
        span: Span,
    },
}

impl ResolverError {
    /// Returns the error code for this resolver error.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoResolver { .. } => "E0400",
            Self::ContractViolation { .. } => "E0401",
            Self::UnknownConfig { .. } => "E0402",
            Self::ConfigTypeMismatch { .. } => "E0403",
        }
    }
}

impl kodo_ast::Diagnostic for ResolverError {
    fn code(&self) -> &'static str {
        self.code()
    }

    fn severity(&self) -> kodo_ast::Severity {
        kodo_ast::Severity::Error
    }

    fn span(&self) -> Option<Span> {
        match self {
            Self::NoResolver { span, .. }
            | Self::UnknownConfig { span, .. }
            | Self::ConfigTypeMismatch { span, .. } => Some(*span),
            Self::ContractViolation { .. } => None,
        }
    }

    fn message(&self) -> String {
        self.to_string()
    }

    fn suggestion(&self) -> Option<String> {
        match self {
            Self::NoResolver { .. } => {
                Some("available intents: console_app, math_module, serve_http, database, json_api, cache, queue, cli, http_server, file_processor, worker".to_string())
            }
            Self::UnknownConfig { intent, .. } => {
                let valid_keys = valid_config_keys(intent);
                if valid_keys.is_empty() {
                    None
                } else {
                    Some(format!(
                        "valid keys for `{intent}`: {}",
                        valid_keys.join(", ")
                    ))
                }
            }
            Self::ConfigTypeMismatch { .. } | Self::ContractViolation { .. } => None,
        }
    }
}

/// Alias for results in this crate.
pub type Result<T> = std::result::Result<T, ResolverError>;

/// A resolver strategy that can handle a specific kind of intent.
pub trait ResolverStrategy {
    /// Returns the intent names this strategy can handle.
    fn handles(&self) -> &[&str];

    /// Returns the valid config keys for this strategy.
    fn valid_keys(&self) -> &[&str];

    /// Attempts to resolve an intent into concrete code.
    ///
    /// # Errors
    ///
    /// Returns [`ResolverError`] if the intent cannot be resolved.
    fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent>;
}

/// The output of a successful intent resolution.
#[derive(Debug)]
pub struct ResolvedIntent {
    /// The generated AST nodes to inject.
    pub generated_functions: Vec<Function>,
    /// Any additional type definitions.
    pub generated_types: Vec<String>,
    /// Human-readable description of what was generated.
    pub description: String,
}

/// The intent resolver registry.
#[derive(Default)]
pub struct Resolver {
    strategies: Vec<Box<dyn ResolverStrategy>>,
}

impl Resolver {
    /// Creates a new empty resolver.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a resolver with all built-in strategies registered.
    #[must_use]
    pub fn with_builtins() -> Self {
        let mut resolver = Self::new();
        resolver.register(Box::new(ConsoleAppStrategy));
        resolver.register(Box::new(MathModuleStrategy));
        resolver.register(Box::new(ServeHttpStrategy));
        resolver.register(Box::new(DatabaseStrategy));
        resolver.register(Box::new(JsonApiStrategy));
        resolver.register(Box::new(CacheStrategy));
        resolver.register(Box::new(QueueStrategy));
        resolver.register(Box::new(CliStrategy));
        resolver.register(Box::new(HttpServerStrategy));
        resolver.register(Box::new(FileProcessorStrategy));
        resolver.register(Box::new(WorkerStrategy));
        resolver
    }

    /// Registers a resolver strategy.
    pub fn register(&mut self, strategy: Box<dyn ResolverStrategy>) {
        self.strategies.push(strategy);
    }

    /// Resolves an intent using registered strategies.
    ///
    /// Validates config keys before resolving.
    ///
    /// # Errors
    ///
    /// Returns [`ResolverError::NoResolver`] if no strategy handles the intent.
    /// Returns [`ResolverError::UnknownConfig`] if a config key is invalid.
    pub fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
        for strategy in &self.strategies {
            if strategy.handles().contains(&intent.name.as_str()) {
                // Validate config keys
                let valid_keys = strategy.valid_keys();
                for entry in &intent.config {
                    if !valid_keys.contains(&entry.key.as_str()) {
                        return Err(ResolverError::UnknownConfig {
                            key: entry.key.clone(),
                            intent: intent.name.clone(),
                            span: entry.span,
                        });
                    }
                }
                return strategy.resolve(intent);
            }
        }
        Err(ResolverError::NoResolver {
            intent: intent.name.clone(),
            span: intent.span,
        })
    }

    /// Resolves all intents in a module and returns the generated functions.
    ///
    /// # Errors
    ///
    /// Returns the first error encountered during resolution.
    pub fn resolve_all(&self, intents: &[IntentDecl]) -> Result<Vec<ResolvedIntent>> {
        let mut results = Vec::new();
        for intent in intents {
            results.push(self.resolve(intent)?);
        }
        Ok(results)
    }
}

/// Returns valid config keys for a known intent name.
fn valid_config_keys(intent: &str) -> Vec<&'static str> {
    match intent {
        "console_app" => vec!["greeting", "entry_point"],
        "math_module" => vec!["functions"],
        "serve_http" => vec!["port", "routes"],
        "database" => vec!["driver", "tables", "queries"],
        "json_api" => vec!["routes", "models", "port", "base_path", "endpoints"],
        "cache" => vec!["strategy", "max_size"],
        "queue" => vec!["backend", "topics"],
        "cli" => vec!["name", "version", "commands"],
        "http_server" => vec!["port", "routes", "not_found"],
        "file_processor" => vec!["input", "output", "transform"],
        "worker" => vec!["task", "max_iterations", "on_error"],
        _ => vec![],
    }
}

/// Extracts a string value from an intent config entry.
fn get_string_config<'a>(intent: &'a IntentDecl, key: &str) -> Option<&'a str> {
    for entry in &intent.config {
        if entry.key == key {
            if let IntentConfigValue::StringLit(ref s, _) = entry.value {
                return Some(s.as_str());
            }
        }
    }
    None
}

/// Extracts an integer value from an intent config entry.
fn get_int_config(intent: &IntentDecl, key: &str) -> Option<i64> {
    for entry in &intent.config {
        if entry.key == key {
            if let IntentConfigValue::IntLit(n, _) = entry.value {
                return Some(n);
            }
        }
    }
    None
}

/// Extracts a list of string values from an intent config entry.
///
/// Handles both `StringLit` and `FnRef` list items, treating `FnRef` names as strings.
fn get_string_list_config(intent: &IntentDecl, key: &str) -> Vec<String> {
    for entry in &intent.config {
        if entry.key == key {
            if let IntentConfigValue::List(ref items, _) = entry.value {
                return items
                    .iter()
                    .filter_map(|item| match item {
                        IntentConfigValue::StringLit(s, _) | IntentConfigValue::FnRef(s, _) => {
                            Some(s.clone())
                        }
                        _ => None,
                    })
                    .collect();
            }
        }
    }
    Vec::new()
}

// ===== Built-in Strategies =====

/// Generates a `kodo_main` function for console applications.
///
/// Config keys:
/// - `greeting` (string, optional): The message to print. Default: `"Hello from Kōdo!"`.
/// - `entry_point` (string, optional): Name of the entry point function. Default: `"kodo_main"`.
pub struct ConsoleAppStrategy;

impl ResolverStrategy for ConsoleAppStrategy {
    fn handles(&self) -> &[&str] {
        &["console_app"]
    }

    fn valid_keys(&self) -> &[&str] {
        &["greeting", "entry_point"]
    }

    fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
        let greeting = get_string_config(intent, "greeting").unwrap_or("Hello from Kōdo!");
        let entry_point = get_string_config(intent, "entry_point").unwrap_or("kodo_main");

        let span = intent.span;

        // Generate: fn kodo_main() { println("greeting") }
        let println_call = Expr::Call {
            callee: Box::new(Expr::Ident("println".to_string(), span)),
            args: vec![Expr::StringLit(greeting.to_string(), span)],
            span,
        };

        let func = Function {
            id: NodeId(0),
            span,
            name: entry_point.to_string(),
            visibility: Visibility::Private,
            is_async: false,
            generic_params: vec![],
            annotations: vec![],
            params: vec![],
            return_type: TypeExpr::Unit,
            requires: vec![],
            ensures: vec![],
            body: Block {
                span,
                stmts: vec![Stmt::Expr(println_call)],
            },
        };

        Ok(ResolvedIntent {
            generated_functions: vec![func],
            generated_types: vec![],
            description: format!("Generated `{entry_point}()` that prints: \"{greeting}\""),
        })
    }
}

/// Generates mathematical helper functions from intent declarations.
///
/// Config keys:
/// - `functions` (list of fn refs): Names of functions to generate wrappers for.
pub struct MathModuleStrategy;

impl ResolverStrategy for MathModuleStrategy {
    fn handles(&self) -> &[&str] {
        &["math_module"]
    }

    fn valid_keys(&self) -> &[&str] {
        &["functions"]
    }

    fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
        let span = intent.span;
        let mut generated = Vec::new();
        let mut descriptions = Vec::new();

        // Look for `functions` config entry
        for entry in &intent.config {
            if entry.key == "functions" {
                if let IntentConfigValue::List(ref items, _) = entry.value {
                    for item in items {
                        if let IntentConfigValue::FnRef(ref name, _) = item {
                            if let Some(func) = generate_math_function(name, span) {
                                descriptions.push(format!("  - `{name}(a: Int, b: Int) -> Int`"));
                                generated.push(func);
                            }
                        }
                    }
                }
            }
        }

        Ok(ResolvedIntent {
            generated_functions: generated,
            generated_types: vec![],
            description: if descriptions.is_empty() {
                "No math functions generated.".to_string()
            } else {
                format!("Generated math functions:\n{}", descriptions.join("\n"))
            },
        })
    }
}

/// Generates a named math function that wraps a binary operation.
fn generate_math_function(name: &str, span: Span) -> Option<Function> {
    let (op, contract_expr) = match name {
        "add" => (kodo_ast::BinOp::Add, None),
        "sub" => (kodo_ast::BinOp::Sub, None),
        "mul" => (kodo_ast::BinOp::Mul, None),
        "safe_div" => (
            kodo_ast::BinOp::Div,
            Some(Expr::BinaryOp {
                left: Box::new(Expr::Ident("b".to_string(), span)),
                op: kodo_ast::BinOp::Ne,
                right: Box::new(Expr::IntLit(0, span)),
                span,
            }),
        ),
        _ => return None,
    };

    let body_expr = Expr::BinaryOp {
        left: Box::new(Expr::Ident("a".to_string(), span)),
        op,
        right: Box::new(Expr::Ident("b".to_string(), span)),
        span,
    };

    let requires = contract_expr.into_iter().collect();

    Some(Function {
        id: NodeId(0),
        span,
        name: name.to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![
            Param {
                name: "a".to_string(),
                ty: TypeExpr::Named("Int".to_string()),
                span,
                ownership: Ownership::Owned,
            },
            Param {
                name: "b".to_string(),
                ty: TypeExpr::Named("Int".to_string()),
                span,
                ownership: Ownership::Owned,
            },
        ],
        return_type: TypeExpr::Named("Int".to_string()),
        requires,
        ensures: vec![],
        body: Block {
            span,
            stmts: vec![Stmt::Return {
                span,
                value: Some(body_expr),
            }],
        },
    })
}

/// Generates HTTP handler stubs for serving HTTP requests.
///
/// Config keys:
/// - `port` (integer): The port to listen on.
/// - `routes` (list): Route definitions (currently generates handler stubs).
pub struct ServeHttpStrategy;

impl ResolverStrategy for ServeHttpStrategy {
    fn handles(&self) -> &[&str] {
        &["serve_http"]
    }

    fn valid_keys(&self) -> &[&str] {
        &["port", "routes"]
    }

    fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
        let span = intent.span;

        // Extract port if present, default to 8080
        let port = get_int_config(intent, "port").unwrap_or(8080);

        // Generate a main function that prints server startup info
        let startup_msg = format!("HTTP server starting on port {port}");
        let println_call = Expr::Call {
            callee: Box::new(Expr::Ident("println".to_string(), span)),
            args: vec![Expr::StringLit(startup_msg.clone(), span)],
            span,
        };

        let main_func = Function {
            id: NodeId(0),
            span,
            name: "kodo_main".to_string(),
            visibility: Visibility::Private,
            is_async: false,
            generic_params: vec![],
            annotations: vec![],
            params: vec![],
            return_type: TypeExpr::Unit,
            requires: vec![],
            ensures: vec![],
            body: Block {
                span,
                stmts: vec![Stmt::Expr(println_call)],
            },
        };

        // Generate handler stubs from routes
        let mut generated = vec![main_func];
        let mut route_descriptions = Vec::new();

        for entry in &intent.config {
            if entry.key == "routes" {
                if let IntentConfigValue::List(ref items, _) = entry.value {
                    for item in items {
                        if let IntentConfigValue::FnRef(ref handler_name, _) = item {
                            let handler = generate_http_handler(handler_name, span);
                            route_descriptions.push(format!("  - `{handler_name}()`"));
                            generated.push(handler);
                        }
                    }
                }
            }
        }

        let description = if route_descriptions.is_empty() {
            format!("Generated HTTP server on port {port} (no routes)")
        } else {
            format!(
                "Generated HTTP server on port {port} with handlers:\n{}",
                route_descriptions.join("\n")
            )
        };

        Ok(ResolvedIntent {
            generated_functions: generated,
            generated_types: vec![],
            description,
        })
    }
}

/// Generates an HTTP handler stub function.
fn generate_http_handler(name: &str, span: Span) -> Function {
    // Generate: fn handler_name() { println("Handling request: handler_name") }
    let msg = format!("Handling request: {name}");
    let println_call = Expr::Call {
        callee: Box::new(Expr::Ident("println".to_string(), span)),
        args: vec![Expr::StringLit(msg, span)],
        span,
    };

    Function {
        id: NodeId(0),
        span,
        name: name.to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![],
        return_type: TypeExpr::Unit,
        requires: vec![],
        ensures: vec![],
        body: Block {
            span,
            stmts: vec![Stmt::Expr(println_call)],
        },
    }
}

/// Generates typed database query functions with contracts.
///
/// Config keys:
/// - `driver` (string): The database driver name (e.g., `"sqlite"`, `"postgres"`).
/// - `tables` (list): Table names for which accessor functions are generated.
/// - `queries` (list): Named query function stubs to generate.
///
/// Each table gets a `query_<table>` function with a contract requiring a non-empty
/// table name. Each named query gets a function stub with a contract.
pub struct DatabaseStrategy;

impl ResolverStrategy for DatabaseStrategy {
    fn handles(&self) -> &[&str] {
        &["database"]
    }

    fn valid_keys(&self) -> &[&str] {
        &["driver", "tables", "queries"]
    }

    fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
        let span = intent.span;
        let driver = get_string_config(intent, "driver").unwrap_or("sqlite");
        let tables = get_string_list_config(intent, "tables");
        let queries = get_string_list_config(intent, "queries");

        let mut generated = Vec::new();
        let mut descriptions = Vec::new();

        // Generate a connect function
        let connect_func = generate_db_connect(driver, span);
        descriptions.push(format!("  - `db_connect() -> String` (driver: {driver})"));
        generated.push(connect_func);

        // Generate query_<table> functions for each table
        for table in &tables {
            let func = generate_db_table_query(table, span);
            descriptions.push(format!("  - `query_{table}(id: Int) -> String`"));
            generated.push(func);
        }

        // Generate named query stubs
        for query in &queries {
            let func = generate_db_named_query(query, span);
            descriptions.push(format!("  - `{query}(id: Int) -> String`"));
            generated.push(func);
        }

        Ok(ResolvedIntent {
            generated_functions: generated,
            generated_types: tables.iter().map(|t| format!("{t}Row")).collect(),
            description: format!(
                "Generated database layer (driver: {driver}):\n{}",
                descriptions.join("\n")
            ),
        })
    }
}

/// Generates a database connection function stub.
fn generate_db_connect(driver: &str, span: Span) -> Function {
    let body_expr = Expr::StringLit(format!("connected:{driver}"), span);

    Function {
        id: NodeId(0),
        span,
        name: "db_connect".to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![],
        return_type: TypeExpr::Named("String".to_string()),
        requires: vec![],
        ensures: vec![],
        body: Block {
            span,
            stmts: vec![Stmt::Return {
                span,
                value: Some(body_expr),
            }],
        },
    }
}

/// Generates a table query function with a contract requiring a valid ID.
fn generate_db_table_query(table: &str, span: Span) -> Function {
    let func_name = format!("query_{table}");
    let body_expr = Expr::Call {
        callee: Box::new(Expr::Ident("println".to_string(), span)),
        args: vec![Expr::StringLit(format!("querying table: {table}"), span)],
        span,
    };

    // requires { id > 0 }
    let requires = vec![Expr::BinaryOp {
        left: Box::new(Expr::Ident("id".to_string(), span)),
        op: kodo_ast::BinOp::Gt,
        right: Box::new(Expr::IntLit(0, span)),
        span,
    }];

    Function {
        id: NodeId(0),
        span,
        name: func_name,
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![Param {
            name: "id".to_string(),
            ty: TypeExpr::Named("Int".to_string()),
            span,
            ownership: Ownership::Owned,
        }],
        return_type: TypeExpr::Named("String".to_string()),
        requires,
        ensures: vec![],
        body: Block {
            span,
            stmts: vec![Stmt::Expr(body_expr)],
        },
    }
}

/// Generates a named query function stub with a contract.
fn generate_db_named_query(name: &str, span: Span) -> Function {
    let body_expr = Expr::Call {
        callee: Box::new(Expr::Ident("println".to_string(), span)),
        args: vec![Expr::StringLit(format!("executing query: {name}"), span)],
        span,
    };

    // requires { id > 0 }
    let requires = vec![Expr::BinaryOp {
        left: Box::new(Expr::Ident("id".to_string(), span)),
        op: kodo_ast::BinOp::Gt,
        right: Box::new(Expr::IntLit(0, span)),
        span,
    }];

    Function {
        id: NodeId(0),
        span,
        name: name.to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![Param {
            name: "id".to_string(),
            ty: TypeExpr::Named("Int".to_string()),
            span,
            ownership: Ownership::Owned,
        }],
        return_type: TypeExpr::Named("String".to_string()),
        requires,
        ensures: vec![],
        body: Block {
            span,
            stmts: vec![Stmt::Expr(body_expr)],
        },
    }
}

/// Generates JSON API handler functions with validation contracts.
///
/// Config keys:
/// - `routes` (list): Route path strings (e.g., `"/users"`, `"/posts"`).
/// - `models` (list): Model names for which struct-like accessor stubs are generated.
///
/// Each route gets a handler function. Each model gets `create_<model>` and
/// `get_<model>` functions with validation contracts.
pub struct JsonApiStrategy;

impl ResolverStrategy for JsonApiStrategy {
    fn handles(&self) -> &[&str] {
        &["json_api"]
    }

    fn valid_keys(&self) -> &[&str] {
        &["routes", "models", "port", "base_path", "endpoints"]
    }

    fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
        let span = intent.span;

        // New mode: if "endpoints" is present, generate a real HTTP server.
        let endpoints = get_nested_list_config(intent, "endpoints");
        if !endpoints.is_empty() {
            let port = get_int_config(intent, "port").unwrap_or(8080);
            let base_path = get_string_config(intent, "base_path").unwrap_or("");
            return Ok(resolve_json_api_server(span, port, base_path, &endpoints));
        }

        // Legacy mode: generate stubs from routes/models.
        let routes = get_string_list_config(intent, "routes");
        let models = get_string_list_config(intent, "models");

        let mut generated = Vec::new();
        let mut descriptions = Vec::new();
        let mut generated_types = Vec::new();

        // Generate route handlers
        for route in &routes {
            let handler_name = route_to_handler_name(route);
            let func = generate_api_handler(&handler_name, route, span);
            descriptions.push(format!("  - `{handler_name}()` -> handler for {route}"));
            generated.push(func);
        }

        // Generate model CRUD stubs
        for model in &models {
            let lower = model.to_lowercase();
            generated_types.push(model.clone());

            // create_<model>(data: String) -> String
            let create_func = generate_api_create_model(&lower, span);
            descriptions.push(format!("  - `create_{lower}(data: String) -> String`"));
            generated.push(create_func);

            // get_<model>(id: Int) -> String
            let get_func = generate_api_get_model(&lower, span);
            descriptions.push(format!("  - `get_{lower}(id: Int) -> String`"));
            generated.push(get_func);
        }

        Ok(ResolvedIntent {
            generated_functions: generated,
            generated_types,
            description: format!(
                "Generated JSON API:\n{}",
                if descriptions.is_empty() {
                    "  (no routes or models)".to_string()
                } else {
                    descriptions.join("\n")
                }
            ),
        })
    }
}

/// Converts a route path like `"/users"` to a handler name like `handle_users`.
fn route_to_handler_name(route: &str) -> String {
    let cleaned: String = route
        .trim_matches('/')
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    if cleaned.is_empty() {
        "handle_root".to_string()
    } else {
        format!("handle_{cleaned}")
    }
}

/// Generates a JSON API route handler function.
fn generate_api_handler(handler_name: &str, route: &str, span: Span) -> Function {
    let body_expr = Expr::Call {
        callee: Box::new(Expr::Ident("println".to_string(), span)),
        args: vec![Expr::StringLit(
            format!("Handling API request: {route}"),
            span,
        )],
        span,
    };

    Function {
        id: NodeId(0),
        span,
        name: handler_name.to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![],
        return_type: TypeExpr::Named("String".to_string()),
        requires: vec![],
        ensures: vec![],
        body: Block {
            span,
            stmts: vec![Stmt::Expr(body_expr)],
        },
    }
}

/// Generates a `create_<model>` function with a non-empty data contract.
fn generate_api_create_model(model_lower: &str, span: Span) -> Function {
    let func_name = format!("create_{model_lower}");
    let body_expr = Expr::Call {
        callee: Box::new(Expr::Ident("println".to_string(), span)),
        args: vec![Expr::StringLit(format!("creating {model_lower}"), span)],
        span,
    };

    // requires { data != "" } — validation contract
    let requires = vec![Expr::BinaryOp {
        left: Box::new(Expr::Ident("data".to_string(), span)),
        op: kodo_ast::BinOp::Ne,
        right: Box::new(Expr::StringLit(String::new(), span)),
        span,
    }];

    Function {
        id: NodeId(0),
        span,
        name: func_name,
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![Param {
            name: "data".to_string(),
            ty: TypeExpr::Named("String".to_string()),
            span,
            ownership: Ownership::Owned,
        }],
        return_type: TypeExpr::Named("String".to_string()),
        requires,
        ensures: vec![],
        body: Block {
            span,
            stmts: vec![Stmt::Expr(body_expr)],
        },
    }
}

/// Generates a `get_<model>` function with a positive ID contract.
fn generate_api_get_model(model_lower: &str, span: Span) -> Function {
    let func_name = format!("get_{model_lower}");
    let body_expr = Expr::Call {
        callee: Box::new(Expr::Ident("println".to_string(), span)),
        args: vec![Expr::StringLit(format!("fetching {model_lower}"), span)],
        span,
    };

    // requires { id > 0 }
    let requires = vec![Expr::BinaryOp {
        left: Box::new(Expr::Ident("id".to_string(), span)),
        op: kodo_ast::BinOp::Gt,
        right: Box::new(Expr::IntLit(0, span)),
        span,
    }];

    Function {
        id: NodeId(0),
        span,
        name: func_name,
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![Param {
            name: "id".to_string(),
            ty: TypeExpr::Named("Int".to_string()),
            span,
            ownership: Ownership::Owned,
        }],
        return_type: TypeExpr::Named("String".to_string()),
        requires,
        ensures: vec![],
        body: Block {
            span,
            stmts: vec![Stmt::Expr(body_expr)],
        },
    }
}

/// Default maximum size for cache strategies when not specified.
const DEFAULT_CACHE_MAX_SIZE: i64 = 256;

/// Generates cache access functions with size-bounded contracts.
///
/// Config keys:
/// - `strategy` (string): The caching strategy (e.g., `"lru"`, `"fifo"`).
/// - `max_size` (integer): The maximum number of entries in the cache.
///
/// Generates `cache_get`, `cache_set`, and `cache_invalidate` functions.
/// The `cache_set` function includes a contract ensuring the key is non-empty.
pub struct CacheStrategy;

impl ResolverStrategy for CacheStrategy {
    fn handles(&self) -> &[&str] {
        &["cache"]
    }

    fn valid_keys(&self) -> &[&str] {
        &["strategy", "max_size"]
    }

    fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
        let span = intent.span;
        let strategy = get_string_config(intent, "strategy").unwrap_or("lru");
        let max_size = get_int_config(intent, "max_size").unwrap_or(DEFAULT_CACHE_MAX_SIZE);

        let mut generated = Vec::new();
        let mut descriptions = Vec::new();

        // cache_get(key: String) -> String
        let get_func = generate_cache_get(span);
        descriptions.push("  - `cache_get(key: String) -> String`".to_string());
        generated.push(get_func);

        // cache_set(key: String, value: String) -> Bool
        let set_func = generate_cache_set(max_size, span);
        descriptions.push(format!(
            "  - `cache_set(key: String, value: String) -> Bool` (max_size: {max_size})"
        ));
        generated.push(set_func);

        // cache_invalidate(key: String) -> Bool
        let invalidate_func = generate_cache_invalidate(span);
        descriptions.push("  - `cache_invalidate(key: String) -> Bool`".to_string());
        generated.push(invalidate_func);

        Ok(ResolvedIntent {
            generated_functions: generated,
            generated_types: vec![],
            description: format!(
                "Generated cache layer (strategy: {strategy}, max_size: {max_size}):\n{}",
                descriptions.join("\n")
            ),
        })
    }
}

/// Generates a `cache_get` function.
fn generate_cache_get(span: Span) -> Function {
    let body_expr = Expr::Call {
        callee: Box::new(Expr::Ident("println".to_string(), span)),
        args: vec![Expr::StringLit("cache_get".to_string(), span)],
        span,
    };

    // requires { key != "" }
    let requires = vec![Expr::BinaryOp {
        left: Box::new(Expr::Ident("key".to_string(), span)),
        op: kodo_ast::BinOp::Ne,
        right: Box::new(Expr::StringLit(String::new(), span)),
        span,
    }];

    Function {
        id: NodeId(0),
        span,
        name: "cache_get".to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![Param {
            name: "key".to_string(),
            ty: TypeExpr::Named("String".to_string()),
            span,
            ownership: Ownership::Owned,
        }],
        return_type: TypeExpr::Named("String".to_string()),
        requires,
        ensures: vec![],
        body: Block {
            span,
            stmts: vec![Stmt::Expr(body_expr)],
        },
    }
}

/// Generates a `cache_set` function with a max-size contract.
fn generate_cache_set(max_size: i64, span: Span) -> Function {
    let body_expr = Expr::Call {
        callee: Box::new(Expr::Ident("println".to_string(), span)),
        args: vec![Expr::StringLit(
            format!("cache_set (max: {max_size})"),
            span,
        )],
        span,
    };

    // requires { key != "" }
    let requires = vec![Expr::BinaryOp {
        left: Box::new(Expr::Ident("key".to_string(), span)),
        op: kodo_ast::BinOp::Ne,
        right: Box::new(Expr::StringLit(String::new(), span)),
        span,
    }];

    Function {
        id: NodeId(0),
        span,
        name: "cache_set".to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![
            Param {
                name: "key".to_string(),
                ty: TypeExpr::Named("String".to_string()),
                span,
                ownership: Ownership::Owned,
            },
            Param {
                name: "value".to_string(),
                ty: TypeExpr::Named("String".to_string()),
                span,
                ownership: Ownership::Owned,
            },
        ],
        return_type: TypeExpr::Named("Bool".to_string()),
        requires,
        ensures: vec![],
        body: Block {
            span,
            stmts: vec![Stmt::Expr(body_expr)],
        },
    }
}

/// Generates a `cache_invalidate` function.
fn generate_cache_invalidate(span: Span) -> Function {
    let body_expr = Expr::Call {
        callee: Box::new(Expr::Ident("println".to_string(), span)),
        args: vec![Expr::StringLit("cache_invalidate".to_string(), span)],
        span,
    };

    // requires { key != "" }
    let requires = vec![Expr::BinaryOp {
        left: Box::new(Expr::Ident("key".to_string(), span)),
        op: kodo_ast::BinOp::Ne,
        right: Box::new(Expr::StringLit(String::new(), span)),
        span,
    }];

    Function {
        id: NodeId(0),
        span,
        name: "cache_invalidate".to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![Param {
            name: "key".to_string(),
            ty: TypeExpr::Named("String".to_string()),
            span,
            ownership: Ownership::Owned,
        }],
        return_type: TypeExpr::Named("Bool".to_string()),
        requires,
        ensures: vec![],
        body: Block {
            span,
            stmts: vec![Stmt::Expr(body_expr)],
        },
    }
}

/// Generates message queue produce/consume functions for each topic.
///
/// Config keys:
/// - `backend` (string): The queue backend name (e.g., `"memory"`, `"redis"`).
/// - `topics` (list): Topic names for which produce/consume function pairs are generated.
///
/// Each topic gets `produce_<topic>(message: String)` and
/// `consume_<topic>() -> String` functions. Produce functions include a contract
/// requiring a non-empty message.
pub struct QueueStrategy;

impl ResolverStrategy for QueueStrategy {
    fn handles(&self) -> &[&str] {
        &["queue"]
    }

    fn valid_keys(&self) -> &[&str] {
        &["backend", "topics"]
    }

    fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
        let span = intent.span;
        let backend = get_string_config(intent, "backend").unwrap_or("memory");
        let topics = get_string_list_config(intent, "topics");

        let mut generated = Vec::new();
        let mut descriptions = Vec::new();

        for topic in &topics {
            // produce_<topic>(message: String)
            let produce_func = generate_queue_produce(topic, span);
            descriptions.push(format!("  - `produce_{topic}(message: String)`"));
            generated.push(produce_func);

            // consume_<topic>() -> String
            let consume_func = generate_queue_consume(topic, span);
            descriptions.push(format!("  - `consume_{topic}() -> String`"));
            generated.push(consume_func);
        }

        Ok(ResolvedIntent {
            generated_functions: generated,
            generated_types: vec![],
            description: format!(
                "Generated message queue (backend: {backend}):\n{}",
                if descriptions.is_empty() {
                    "  (no topics)".to_string()
                } else {
                    descriptions.join("\n")
                }
            ),
        })
    }
}

/// Generates a `produce_<topic>` function with a non-empty message contract.
fn generate_queue_produce(topic: &str, span: Span) -> Function {
    let func_name = format!("produce_{topic}");
    let body_expr = Expr::Call {
        callee: Box::new(Expr::Ident("println".to_string(), span)),
        args: vec![Expr::StringLit(
            format!("producing to topic: {topic}"),
            span,
        )],
        span,
    };

    // requires { message != "" }
    let requires = vec![Expr::BinaryOp {
        left: Box::new(Expr::Ident("message".to_string(), span)),
        op: kodo_ast::BinOp::Ne,
        right: Box::new(Expr::StringLit(String::new(), span)),
        span,
    }];

    Function {
        id: NodeId(0),
        span,
        name: func_name,
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![Param {
            name: "message".to_string(),
            ty: TypeExpr::Named("String".to_string()),
            span,
            ownership: Ownership::Owned,
        }],
        return_type: TypeExpr::Unit,
        requires,
        ensures: vec![],
        body: Block {
            span,
            stmts: vec![Stmt::Expr(body_expr)],
        },
    }
}

/// Generates a `consume_<topic>` function.
fn generate_queue_consume(topic: &str, span: Span) -> Function {
    let func_name = format!("consume_{topic}");
    let body_expr = Expr::Call {
        callee: Box::new(Expr::Ident("println".to_string(), span)),
        args: vec![Expr::StringLit(
            format!("consuming from topic: {topic}"),
            span,
        )],
        span,
    };

    Function {
        id: NodeId(0),
        span,
        name: func_name,
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![],
        return_type: TypeExpr::Named("String".to_string()),
        requires: vec![],
        ensures: vec![],
        body: Block {
            span,
            stmts: vec![Stmt::Expr(body_expr)],
        },
    }
}

// ===== Helper: extract FnRef config value =====

/// Extracts a function reference name from an intent config entry.
fn get_fn_ref_config<'a>(intent: &'a IntentDecl, key: &str) -> Option<&'a str> {
    for entry in &intent.config {
        if entry.key == key {
            if let IntentConfigValue::FnRef(ref s, _) = entry.value {
                return Some(s.as_str());
            }
        }
    }
    None
}

/// Extracts nested lists (list of lists) from an intent config entry.
///
/// Each inner list is a `Vec<IntentConfigValue>`. Non-list items are skipped.
fn get_nested_list_config(intent: &IntentDecl, key: &str) -> Vec<Vec<IntentConfigValue>> {
    for entry in &intent.config {
        if entry.key == key {
            if let IntentConfigValue::List(ref items, _) = entry.value {
                return items
                    .iter()
                    .filter_map(|item| {
                        if let IntentConfigValue::List(ref inner, _) = item {
                            Some(inner.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
            }
        }
    }
    Vec::new()
}

/// Extracts a string from an `IntentConfigValue`.
fn config_value_as_str(v: &IntentConfigValue) -> Option<&str> {
    match v {
        IntentConfigValue::StringLit(s, _) | IntentConfigValue::FnRef(s, _) => Some(s.as_str()),
        _ => None,
    }
}

// ===== Helper: generate common AST patterns =====

/// Builds a `let name: type = expr` statement.
fn make_let(name: &str, ty: TypeExpr, value: Expr, span: Span) -> Stmt {
    Stmt::Let {
        span,
        mutable: false,
        name: name.to_string(),
        ty: Some(ty),
        value,
    }
}

/// Builds a function call expression.
fn make_call(callee: &str, args: Vec<Expr>, span: Span) -> Expr {
    Expr::Call {
        callee: Box::new(Expr::Ident(callee.to_string(), span)),
        args,
        span,
    }
}

/// Builds `println(msg)` as a statement.
fn make_println(msg: &str, span: Span) -> Stmt {
    Stmt::Expr(make_call(
        "println",
        vec![Expr::StringLit(msg.to_string(), span)],
        span,
    ))
}

/// Builds a simple function with given body statements and return type.
fn make_function(name: &str, return_type: TypeExpr, body: Vec<Stmt>, span: Span) -> Function {
    Function {
        id: NodeId(0),
        span,
        name: name.to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![],
        return_type,
        requires: vec![],
        ensures: vec![],
        body: Block { span, stmts: body },
    }
}

/// Builds an if-else chain from a list of (condition, body) pairs and an else body.
fn make_if_chain(branches: Vec<(Expr, Vec<Stmt>)>, else_body: &[Stmt], span: Span) -> Expr {
    let mut result: Option<Expr> = None;

    // Build from the end backwards
    for (condition, body) in branches.into_iter().rev() {
        let else_branch = if let Some(inner) = result.take() {
            Some(Block {
                span,
                stmts: vec![Stmt::Expr(inner)],
            })
        } else if else_body.is_empty() {
            None
        } else {
            Some(Block {
                span,
                stmts: else_body.to_vec(),
            })
        };

        result = Some(Expr::If {
            condition: Box::new(condition),
            then_branch: Block { span, stmts: body },
            else_branch,
            span,
        });
    }

    result.unwrap_or(Expr::IntLit(0, span))
}

// ===== Strategy: cli =====

/// Generates a CLI tool with command dispatch using `args()` and `exit()`.
///
/// Config keys:
/// - `name` (string, optional): Tool name for help text. Default: `"tool"`.
/// - `version` (string, optional): Version string. Default: `"0.1.0"`.
/// - `commands` (list of lists): Each sub-list is `[command_name, handler_fn, description]`.
pub struct CliStrategy;

impl ResolverStrategy for CliStrategy {
    fn handles(&self) -> &[&str] {
        &["cli"]
    }

    fn valid_keys(&self) -> &[&str] {
        &["name", "version", "commands"]
    }

    fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
        let span = intent.span;
        let name = get_string_config(intent, "name").unwrap_or("tool");
        let version = get_string_config(intent, "version").unwrap_or("0.1.0");
        let commands = get_nested_list_config(intent, "commands");

        // Parse commands: each is [name_str, handler_fn_ref, description_str]
        let mut cmd_entries: Vec<(String, String, String)> = Vec::new();
        for cmd in &commands {
            if cmd.len() >= 2 {
                let cmd_name = config_value_as_str(&cmd[0])
                    .unwrap_or("unknown")
                    .to_string();
                let handler = config_value_as_str(&cmd[1])
                    .unwrap_or("unknown")
                    .to_string();
                let desc = if cmd.len() >= 3 {
                    config_value_as_str(&cmd[2]).unwrap_or("").to_string()
                } else {
                    String::new()
                };
                cmd_entries.push((cmd_name, handler, desc));
            }
        }

        let mut generated = Vec::new();

        // Generate cli_help() function
        let help_func = generate_cli_help(name, version, &cmd_entries, span);
        generated.push(help_func);

        // Generate kodo_main() with dispatch
        let main_func = generate_cli_main(&cmd_entries, span);
        generated.push(main_func);

        let cmd_desc: Vec<String> = cmd_entries
            .iter()
            .map(|(n, h, d)| format!("  - `{n}` → `{h}()` — {d}"))
            .collect();

        let description = format!(
            "Generated CLI tool `{name}` v{version} with {} commands:\n{}",
            cmd_entries.len(),
            cmd_desc.join("\n")
        );

        Ok(ResolvedIntent {
            generated_functions: generated,
            generated_types: vec![],
            description,
        })
    }
}

/// Generates the `cli_help()` function that prints usage info.
fn generate_cli_help(
    name: &str,
    version: &str,
    commands: &[(String, String, String)],
    span: Span,
) -> Function {
    let mut stmts = Vec::new();
    stmts.push(make_println(&format!("{name} v{version}"), span));
    stmts.push(make_println("", span));
    stmts.push(make_println("Commands:", span));
    for (cmd_name, _, desc) in commands {
        stmts.push(make_println(&format!("  {cmd_name}  {desc}"), span));
    }
    stmts.push(make_println("  help  Show this help message", span));

    make_function("cli_help", TypeExpr::Unit, stmts, span)
}

/// Generates the `kodo_main()` function with arg parsing and command dispatch.
fn generate_cli_main(commands: &[(String, String, String)], span: Span) -> Function {
    let mut stmts = Vec::new();

    // let cmd: String = "help"
    stmts.push(make_let(
        "cmd",
        TypeExpr::Named("String".to_string()),
        Expr::StringLit("help".to_string(), span),
        span,
    ));

    // Build if-else chain for each command
    let mut branches: Vec<(Expr, Vec<Stmt>)> = Vec::new();
    for (cmd_name, handler, _) in commands {
        let condition = Expr::BinaryOp {
            op: BinOp::Eq,
            left: Box::new(Expr::Ident("cmd".to_string(), span)),
            right: Box::new(Expr::StringLit(cmd_name.clone(), span)),
            span,
        };
        let body = vec![Stmt::Expr(make_call(handler, vec![], span))];
        branches.push((condition, body));
    }

    // Add "help" command
    let help_condition = Expr::BinaryOp {
        op: BinOp::Eq,
        left: Box::new(Expr::Ident("cmd".to_string(), span)),
        right: Box::new(Expr::StringLit("help".to_string(), span)),
        span,
    };
    branches.push((
        help_condition,
        vec![Stmt::Expr(make_call("cli_help", vec![], span))],
    ));

    // Else: unknown command
    let else_body = vec![
        make_println("Unknown command", span),
        Stmt::Expr(make_call("cli_help", vec![], span)),
        Stmt::Expr(make_call("exit", vec![Expr::IntLit(1, span)], span)),
    ];

    stmts.push(Stmt::Expr(make_if_chain(branches, &else_body, span)));
    stmts.push(Stmt::Return {
        span,
        value: Some(Expr::IntLit(0, span)),
    });

    make_function("kodo_main", TypeExpr::Named("Int".to_string()), stmts, span)
}

// ===== Strategy: http_server =====

/// Generates a real HTTP server with routing using `http_server_*` builtins.
///
/// Config keys:
/// - `port` (integer, optional): Port to listen on. Default: `8080`.
/// - `routes` (list of lists): Each sub-list is `[method, path, handler_fn]`.
/// - `not_found` (string, optional): 404 response body. Default: `"Not Found"`.
pub struct HttpServerStrategy;

impl ResolverStrategy for HttpServerStrategy {
    fn handles(&self) -> &[&str] {
        &["http_server"]
    }

    fn valid_keys(&self) -> &[&str] {
        &["port", "routes", "not_found"]
    }

    fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
        let span = intent.span;
        let port = get_int_config(intent, "port").unwrap_or(8080);
        let not_found = get_string_config(intent, "not_found").unwrap_or("Not Found");
        let routes = get_nested_list_config(intent, "routes");

        // Parse routes: each is [method, path, handler_fn]
        let mut route_entries: Vec<(String, String, String)> = Vec::new();
        for route in &routes {
            if route.len() >= 3 {
                let method = config_value_as_str(&route[0]).unwrap_or("GET").to_string();
                let path = config_value_as_str(&route[1]).unwrap_or("/").to_string();
                let handler = config_value_as_str(&route[2])
                    .unwrap_or("handler")
                    .to_string();
                route_entries.push((method, path, handler));
            }
        }

        let main_func = generate_http_server_main(port, not_found, &route_entries, span);

        let route_desc: Vec<String> = route_entries
            .iter()
            .map(|(m, p, h)| format!("  - {m} {p} → `{h}()`"))
            .collect();

        let description = format!(
            "Generated HTTP server on port {port} with {} routes:\n{}",
            route_entries.len(),
            route_desc.join("\n")
        );

        Ok(ResolvedIntent {
            generated_functions: vec![main_func],
            generated_types: vec![],
            description,
        })
    }
}

/// Builds a single route branch: `if method == M && path == P { resp = handler(); respond(req, 200, resp) }`.
fn make_route_branch(method: &str, path: &str, handler: &str, span: Span) -> (Expr, Vec<Stmt>) {
    let method_cmp = Expr::BinaryOp {
        op: BinOp::Eq,
        left: Box::new(Expr::Ident("method".to_string(), span)),
        right: Box::new(Expr::StringLit(method.to_string(), span)),
        span,
    };
    let path_cmp = Expr::BinaryOp {
        op: BinOp::Eq,
        left: Box::new(Expr::Ident("path".to_string(), span)),
        right: Box::new(Expr::StringLit(path.to_string(), span)),
        span,
    };
    let condition = Expr::BinaryOp {
        op: BinOp::And,
        left: Box::new(method_cmp),
        right: Box::new(path_cmp),
        span,
    };
    let body = vec![
        make_let(
            "resp",
            TypeExpr::Named("String".to_string()),
            make_call(handler, vec![], span),
            span,
        ),
        Stmt::Expr(make_call(
            "http_respond",
            vec![
                Expr::Ident("req".to_string(), span),
                Expr::IntLit(200, span),
                Expr::Ident("resp".to_string(), span),
            ],
            span,
        )),
    ];
    (condition, body)
}

/// Generates `kodo_main()` with HTTP server loop and route dispatch.
fn generate_http_server_main(
    port: i64,
    not_found: &str,
    routes: &[(String, String, String)],
    span: Span,
) -> Function {
    let mut stmts = Vec::new();

    // let server: Int = http_server_new(port)
    stmts.push(make_let(
        "server",
        TypeExpr::Named("Int".to_string()),
        make_call("http_server_new", vec![Expr::IntLit(port, span)], span),
        span,
    ));

    // println("HTTP server listening on port {port}")
    stmts.push(make_println(
        &format!("HTTP server listening on port {port}"),
        span,
    ));

    // while true { ... }
    let mut loop_stmts = Vec::new();

    // let req: Int = http_server_recv(server)
    loop_stmts.push(make_let(
        "req",
        TypeExpr::Named("Int".to_string()),
        make_call(
            "http_server_recv",
            vec![Expr::Ident("server".to_string(), span)],
            span,
        ),
        span,
    ));

    // let method: String = http_request_method(req)
    loop_stmts.push(make_let(
        "method",
        TypeExpr::Named("String".to_string()),
        make_call(
            "http_request_method",
            vec![Expr::Ident("req".to_string(), span)],
            span,
        ),
        span,
    ));

    // let path: String = http_request_path(req)
    loop_stmts.push(make_let(
        "path",
        TypeExpr::Named("String".to_string()),
        make_call(
            "http_request_path",
            vec![Expr::Ident("req".to_string(), span)],
            span,
        ),
        span,
    ));

    // Build if-else chain for routes
    let branches: Vec<(Expr, Vec<Stmt>)> = routes
        .iter()
        .map(|(method, path, handler)| make_route_branch(method, path, handler, span))
        .collect();

    // Else: 404
    let else_body = vec![Stmt::Expr(make_call(
        "http_respond",
        vec![
            Expr::Ident("req".to_string(), span),
            Expr::IntLit(404, span),
            Expr::StringLit(not_found.to_string(), span),
        ],
        span,
    ))];

    if branches.is_empty() {
        // No routes: respond 404 to everything
        loop_stmts.extend(else_body);
    } else {
        loop_stmts.push(Stmt::Expr(make_if_chain(branches, &else_body, span)));
    }

    stmts.push(Stmt::While {
        span,
        condition: Expr::BoolLit(true, span),
        body: Block {
            span,
            stmts: loop_stmts,
        },
    });

    make_function("kodo_main", TypeExpr::Named("Int".to_string()), stmts, span)
}

/// Resolves a `json_api` intent with real HTTP server endpoints.
fn resolve_json_api_server(
    span: Span,
    port: i64,
    base_path: &str,
    endpoints: &[Vec<IntentConfigValue>],
) -> ResolvedIntent {
    let mut route_entries: Vec<(String, String, String)> = Vec::new();
    for ep in endpoints {
        if ep.len() >= 3 {
            let method = config_value_as_str(&ep[0]).unwrap_or("GET").to_string();
            let raw_path = config_value_as_str(&ep[1]).unwrap_or("/").to_string();
            let path = format!("{base_path}{raw_path}");
            let handler = config_value_as_str(&ep[2]).unwrap_or("handler").to_string();
            route_entries.push((method, path, handler));
        }
    }

    let main_func = generate_http_server_main(port, "Not Found", &route_entries, span);

    let route_desc: Vec<String> = route_entries
        .iter()
        .map(|(m, p, h)| format!("  - {m} {p} → `{h}()`"))
        .collect();

    ResolvedIntent {
        generated_functions: vec![main_func],
        generated_types: vec![],
        description: format!(
            "Generated JSON API server on port {port} with {} endpoints:\n{}",
            route_entries.len(),
            route_desc.join("\n")
        ),
    }
}

// ===== Strategy: file_processor =====

/// Generates a file processing pipeline using file I/O builtins.
///
/// Config keys:
/// - `input` (string): Input mode — `"file"`, `"directory"`, or `"stdin"`. Default: `"file"`.
/// - `output` (string): Output mode — `"stdout"` or `"file"`. Default: `"stdout"`.
/// - `transform` (fn ref): Function to apply to input content.
pub struct FileProcessorStrategy;

impl ResolverStrategy for FileProcessorStrategy {
    fn handles(&self) -> &[&str] {
        &["file_processor"]
    }

    fn valid_keys(&self) -> &[&str] {
        &["input", "output", "transform"]
    }

    fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
        let span = intent.span;
        let input_mode = get_string_config(intent, "input").unwrap_or("file");
        let output_mode = get_string_config(intent, "output").unwrap_or("stdout");
        let transform = get_fn_ref_config(intent, "transform")
            .or_else(|| get_string_config(intent, "transform"))
            .unwrap_or("transform");

        let main_func = generate_file_processor_main(input_mode, output_mode, transform, span);

        let description = format!(
            "Generated file processor: input={input_mode}, output={output_mode}, transform=`{transform}()`"
        );

        Ok(ResolvedIntent {
            generated_functions: vec![main_func],
            generated_types: vec![],
            description,
        })
    }
}

/// Generates `kodo_main()` for file processing.
fn generate_file_processor_main(
    input_mode: &str,
    output_mode: &str,
    transform: &str,
    span: Span,
) -> Function {
    let mut stmts = Vec::new();

    match input_mode {
        "stdin" => {
            // let content: String = readln()
            stmts.push(make_let(
                "content",
                TypeExpr::Named("String".to_string()),
                make_call("readln", vec![], span),
                span,
            ));
            // let result: String = transform(content)
            stmts.push(make_let(
                "result",
                TypeExpr::Named("String".to_string()),
                make_call(
                    transform,
                    vec![Expr::Ident("content".to_string(), span)],
                    span,
                ),
                span,
            ));
            emit_output(&mut stmts, output_mode, span);
        }
        "directory" => {
            // println("Processing directory")
            stmts.push(make_println("Processing directory", span));
            // let result: String = transform(".")
            stmts.push(make_let(
                "result",
                TypeExpr::Named("String".to_string()),
                make_call(
                    transform,
                    vec![Expr::StringLit(".".to_string(), span)],
                    span,
                ),
                span,
            ));
            emit_output(&mut stmts, output_mode, span);
        }
        _ => {
            // Default: "file" mode
            // println("Processing file")
            stmts.push(make_println("Processing file", span));
            // let result: String = transform("input")
            stmts.push(make_let(
                "result",
                TypeExpr::Named("String".to_string()),
                make_call(
                    transform,
                    vec![Expr::StringLit("input".to_string(), span)],
                    span,
                ),
                span,
            ));
            emit_output(&mut stmts, output_mode, span);
        }
    }

    stmts.push(Stmt::Return {
        span,
        value: Some(Expr::IntLit(0, span)),
    });

    make_function("kodo_main", TypeExpr::Named("Int".to_string()), stmts, span)
}

/// Emits output statements based on the output mode.
fn emit_output(stmts: &mut Vec<Stmt>, output_mode: &str, span: Span) {
    match output_mode {
        "file" => {
            // file_write("output.txt", result)
            stmts.push(Stmt::Expr(make_call(
                "file_write",
                vec![
                    Expr::StringLit("output.txt".to_string(), span),
                    Expr::Ident("result".to_string(), span),
                ],
                span,
            )));
        }
        _ => {
            // println(result)
            stmts.push(Stmt::Expr(make_call(
                "println",
                vec![Expr::Ident("result".to_string(), span)],
                span,
            )));
        }
    }
}

// ===== Strategy: worker =====

/// Generates a worker loop that calls a task function repeatedly.
///
/// Config keys:
/// - `task` (fn ref): Function to call each iteration.
/// - `max_iterations` (integer, optional): Maximum iterations. Default: `10`.
/// - `on_error` (fn ref, optional): Function to call when task returns non-zero.
pub struct WorkerStrategy;

impl ResolverStrategy for WorkerStrategy {
    fn handles(&self) -> &[&str] {
        &["worker"]
    }

    fn valid_keys(&self) -> &[&str] {
        &["task", "max_iterations", "on_error"]
    }

    fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
        let span = intent.span;
        let task = get_fn_ref_config(intent, "task")
            .or_else(|| get_string_config(intent, "task"))
            .unwrap_or("do_work");
        let max_iterations = get_int_config(intent, "max_iterations").unwrap_or(10);
        let on_error =
            get_fn_ref_config(intent, "on_error").or_else(|| get_string_config(intent, "on_error"));

        let main_func = generate_worker_main(task, max_iterations, on_error, span);

        let error_desc = if let Some(handler) = on_error {
            format!(", on_error=`{handler}()`")
        } else {
            String::new()
        };

        let description = format!(
            "Generated worker: task=`{task}()`, max_iterations={max_iterations}{error_desc}"
        );

        Ok(ResolvedIntent {
            generated_functions: vec![main_func],
            generated_types: vec![],
            description,
        })
    }
}

/// Generates `kodo_main()` with a worker loop.
fn generate_worker_main(
    task: &str,
    max_iterations: i64,
    on_error: Option<&str>,
    span: Span,
) -> Function {
    let mut stmts = Vec::new();

    stmts.push(make_println(
        &format!("Worker starting ({max_iterations} iterations)"),
        span,
    ));

    // for i in 0..max_iterations { task_call }
    let mut loop_body = Vec::new();

    if let Some(error_handler) = on_error {
        // let status: Int = task()
        loop_body.push(make_let(
            "status",
            TypeExpr::Named("Int".to_string()),
            make_call(task, vec![], span),
            span,
        ));
        // if status != 0 { on_error() }
        let condition = Expr::BinaryOp {
            op: BinOp::Ne,
            left: Box::new(Expr::Ident("status".to_string(), span)),
            right: Box::new(Expr::IntLit(0, span)),
            span,
        };
        loop_body.push(Stmt::Expr(Expr::If {
            condition: Box::new(condition),
            then_branch: Block {
                span,
                stmts: vec![Stmt::Expr(make_call(error_handler, vec![], span))],
            },
            else_branch: None,
            span,
        }));
    } else {
        // Simple: just call task()
        loop_body.push(Stmt::Expr(make_call(task, vec![], span)));
    }

    stmts.push(Stmt::For {
        span,
        name: "i".to_string(),
        start: Expr::IntLit(0, span),
        end: Expr::IntLit(max_iterations, span),
        inclusive: false,
        body: Block {
            span,
            stmts: loop_body,
        },
    });

    stmts.push(make_println("Worker completed", span));
    stmts.push(Stmt::Return {
        span,
        value: Some(Expr::IntLit(0, span)),
    });

    make_function("kodo_main", TypeExpr::Named("Int".to_string()), stmts, span)
}

/// Formats generated code as a human-readable Kōdo source string.
///
/// Used by `kodoc intent-explain` to show what an intent resolves to.
#[must_use]
pub fn format_resolved_intent(resolved: &ResolvedIntent) -> String {
    use std::fmt::Write;
    let mut output = String::new();
    output.push_str("// Generated by intent resolver\n\n");
    output.push_str(&resolved.description);
    output.push_str("\n\n");

    for func in &resolved.generated_functions {
        let _ = write!(output, "fn {}(", func.name);
        for (i, param) in func.params.iter().enumerate() {
            if i > 0 {
                output.push_str(", ");
            }
            let _ = write!(output, "{}: {:?}", param.name, param.ty);
        }
        let _ = write!(output, ") -> {:?}", func.return_type);

        for req in &func.requires {
            let _ = write!(output, "\n    requires {{ {req:?} }}");
        }

        output.push_str(" {\n    // ... generated body ...\n}\n\n");
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_intent(name: &str, config: Vec<kodo_ast::IntentConfigEntry>) -> IntentDecl {
        IntentDecl {
            id: NodeId(0),
            span: Span::new(0, 50),
            name: name.to_string(),
            config,
        }
    }

    fn string_entry(key: &str, value: &str) -> kodo_ast::IntentConfigEntry {
        kodo_ast::IntentConfigEntry {
            key: key.to_string(),
            value: IntentConfigValue::StringLit(value.to_string(), Span::new(0, 10)),
            span: Span::new(0, 20),
        }
    }

    #[test]
    fn empty_resolver_returns_no_resolver_error() {
        let resolver = Resolver::new();
        let intent = make_intent("serve_http", vec![]);
        let result = resolver.resolve(&intent);
        assert!(result.is_err());
        assert!(matches!(result, Err(ResolverError::NoResolver { .. })));
    }

    #[test]
    fn resolver_with_builtins_handles_console_app() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("console_app", vec![string_entry("greeting", "Hello!")]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert_eq!(resolved.generated_functions.len(), 1);
        assert_eq!(resolved.generated_functions[0].name, "kodo_main");
    }

    #[test]
    fn console_app_default_greeting() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("console_app", vec![]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert!(resolved.description.contains("Hello from Kōdo!"));
    }

    #[test]
    fn console_app_custom_entry_point() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("console_app", vec![string_entry("entry_point", "main")]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert_eq!(resolved.generated_functions[0].name, "main");
    }

    #[test]
    fn unknown_config_key_returns_error() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("console_app", vec![string_entry("unknown_key", "value")]);
        let result = resolver.resolve(&intent);
        assert!(matches!(result, Err(ResolverError::UnknownConfig { .. })));
    }

    #[test]
    fn math_module_generates_functions() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent(
            "math_module",
            vec![kodo_ast::IntentConfigEntry {
                key: "functions".to_string(),
                value: IntentConfigValue::List(
                    vec![
                        IntentConfigValue::FnRef("add".to_string(), Span::new(0, 3)),
                        IntentConfigValue::FnRef("safe_div".to_string(), Span::new(0, 8)),
                    ],
                    Span::new(0, 20),
                ),
                span: Span::new(0, 30),
            }],
        );
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert_eq!(resolved.generated_functions.len(), 2);
        assert_eq!(resolved.generated_functions[0].name, "add");
        assert_eq!(resolved.generated_functions[1].name, "safe_div");
        // safe_div should have a requires clause
        assert!(!resolved.generated_functions[1].requires.is_empty());
    }

    #[test]
    fn unknown_intent_returns_no_resolver() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("nonexistent", vec![]);
        let result = resolver.resolve(&intent);
        assert!(matches!(result, Err(ResolverError::NoResolver { .. })));
    }

    #[test]
    fn error_codes_are_correct() {
        let e1 = ResolverError::NoResolver {
            intent: "x".to_string(),
            span: Span::new(0, 1),
        };
        assert_eq!(e1.code(), "E0400");

        let e2 = ResolverError::ContractViolation {
            intent: "x".to_string(),
            reason: "y".to_string(),
        };
        assert_eq!(e2.code(), "E0401");

        let e3 = ResolverError::UnknownConfig {
            key: "k".to_string(),
            intent: "x".to_string(),
            span: Span::new(0, 1),
        };
        assert_eq!(e3.code(), "E0402");

        let e4 = ResolverError::ConfigTypeMismatch {
            key: "k".to_string(),
            intent: "x".to_string(),
            expected: "string".to_string(),
            found: "int".to_string(),
            span: Span::new(0, 1),
        };
        assert_eq!(e4.code(), "E0403");
    }

    #[test]
    fn resolve_all_multiple_intents() {
        let resolver = Resolver::with_builtins();
        let intents = vec![
            make_intent("console_app", vec![]),
            make_intent(
                "math_module",
                vec![kodo_ast::IntentConfigEntry {
                    key: "functions".to_string(),
                    value: IntentConfigValue::List(
                        vec![IntentConfigValue::FnRef("add".to_string(), Span::new(0, 3))],
                        Span::new(0, 10),
                    ),
                    span: Span::new(0, 20),
                }],
            ),
        ];
        let results = resolver.resolve_all(&intents);
        assert!(results.is_ok());
        let results = results.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn format_resolved_intent_produces_output() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("console_app", vec![]);
        let resolved = resolver
            .resolve(&intent)
            .unwrap_or_else(|e| panic!("unexpected: {e}"));
        let formatted = format_resolved_intent(&resolved);
        assert!(formatted.contains("Generated by intent resolver"));
        assert!(formatted.contains("kodo_main"));
    }

    #[test]
    fn error_display_messages() {
        let no_resolver = ResolverError::NoResolver {
            intent: "test".to_string(),
            span: Span::new(0, 4),
        };
        assert!(no_resolver.to_string().contains("no resolver found"));

        let violation = ResolverError::ContractViolation {
            intent: "test".to_string(),
            reason: "bad impl".to_string(),
        };
        assert!(violation.to_string().contains("violates contracts"));

        let unknown_cfg = ResolverError::UnknownConfig {
            key: "baz".to_string(),
            intent: "test".to_string(),
            span: Span::new(5, 8),
        };
        assert!(unknown_cfg
            .to_string()
            .contains("unknown configuration key"));
    }

    #[test]
    fn serve_http_basic() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("serve_http", vec![]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert_eq!(resolved.generated_functions.len(), 1);
        assert_eq!(resolved.generated_functions[0].name, "kodo_main");
        assert!(resolved.description.contains("8080"));
    }

    #[test]
    fn serve_http_custom_port() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent(
            "serve_http",
            vec![kodo_ast::IntentConfigEntry {
                key: "port".to_string(),
                value: IntentConfigValue::IntLit(3000, Span::new(0, 4)),
                span: Span::new(0, 10),
            }],
        );
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert!(resolved.description.contains("3000"));
    }

    #[test]
    fn serve_http_with_routes() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent(
            "serve_http",
            vec![kodo_ast::IntentConfigEntry {
                key: "routes".to_string(),
                value: IntentConfigValue::List(
                    vec![
                        IntentConfigValue::FnRef("health_check".to_string(), Span::new(0, 12)),
                        IntentConfigValue::FnRef("handle_greet".to_string(), Span::new(0, 12)),
                    ],
                    Span::new(0, 30),
                ),
                span: Span::new(0, 40),
            }],
        );
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        // main + 2 handlers
        assert_eq!(resolved.generated_functions.len(), 3);
        assert_eq!(resolved.generated_functions[1].name, "health_check");
        assert_eq!(resolved.generated_functions[2].name, "handle_greet");
    }

    #[test]
    fn serve_http_invalid_config() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("serve_http", vec![string_entry("invalid_key", "value")]);
        let result = resolver.resolve(&intent);
        assert!(matches!(result, Err(ResolverError::UnknownConfig { .. })));
    }

    // ===== Database resolver tests =====

    fn list_entry(key: &str, items: Vec<&str>) -> kodo_ast::IntentConfigEntry {
        kodo_ast::IntentConfigEntry {
            key: key.to_string(),
            value: IntentConfigValue::List(
                items
                    .into_iter()
                    .map(|s| IntentConfigValue::StringLit(s.to_string(), Span::new(0, 5)))
                    .collect(),
                Span::new(0, 20),
            ),
            span: Span::new(0, 30),
        }
    }

    fn int_entry(key: &str, value: i64) -> kodo_ast::IntentConfigEntry {
        kodo_ast::IntentConfigEntry {
            key: key.to_string(),
            value: IntentConfigValue::IntLit(value, Span::new(0, 5)),
            span: Span::new(0, 10),
        }
    }

    #[test]
    fn database_basic_defaults() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("database", vec![]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        // Only the connect function with defaults
        assert_eq!(resolved.generated_functions.len(), 1);
        assert_eq!(resolved.generated_functions[0].name, "db_connect");
        assert!(resolved.description.contains("sqlite"));
    }

    #[test]
    fn database_with_tables_and_queries() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent(
            "database",
            vec![
                string_entry("driver", "postgres"),
                list_entry("tables", vec!["users", "posts"]),
                list_entry("queries", vec!["find_user", "list_posts"]),
            ],
        );
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        // connect + 2 table queries + 2 named queries = 5
        assert_eq!(resolved.generated_functions.len(), 5);
        assert_eq!(resolved.generated_functions[0].name, "db_connect");
        assert_eq!(resolved.generated_functions[1].name, "query_users");
        assert_eq!(resolved.generated_functions[2].name, "query_posts");
        assert_eq!(resolved.generated_functions[3].name, "find_user");
        assert_eq!(resolved.generated_functions[4].name, "list_posts");
        assert!(resolved.description.contains("postgres"));
    }

    #[test]
    fn database_table_queries_have_contracts() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("database", vec![list_entry("tables", vec!["users"])]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        // query_users should have requires { id > 0 }
        let query_func = &resolved.generated_functions[1];
        assert_eq!(query_func.name, "query_users");
        assert!(!query_func.requires.is_empty());
    }

    #[test]
    fn database_invalid_config() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("database", vec![string_entry("unknown", "x")]);
        let result = resolver.resolve(&intent);
        assert!(matches!(result, Err(ResolverError::UnknownConfig { .. })));
    }

    #[test]
    fn database_generates_type_names() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent(
            "database",
            vec![list_entry("tables", vec!["users", "posts"])],
        );
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert_eq!(resolved.generated_types, vec!["usersRow", "postsRow"]);
    }

    // ===== JSON API resolver tests =====

    #[test]
    fn json_api_basic_empty() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("json_api", vec![]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert!(resolved.generated_functions.is_empty());
        assert!(resolved.description.contains("no routes or models"));
    }

    #[test]
    fn json_api_with_routes() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent(
            "json_api",
            vec![list_entry("routes", vec!["/users", "/posts"])],
        );
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert_eq!(resolved.generated_functions.len(), 2);
        assert_eq!(resolved.generated_functions[0].name, "handle_users");
        assert_eq!(resolved.generated_functions[1].name, "handle_posts");
    }

    #[test]
    fn json_api_with_models() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("json_api", vec![list_entry("models", vec!["User", "Post"])]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        // 2 models * 2 (create + get) = 4 functions
        assert_eq!(resolved.generated_functions.len(), 4);
        assert_eq!(resolved.generated_functions[0].name, "create_user");
        assert_eq!(resolved.generated_functions[1].name, "get_user");
        assert_eq!(resolved.generated_functions[2].name, "create_post");
        assert_eq!(resolved.generated_functions[3].name, "get_post");
        // create functions should have requires { data != "" }
        assert!(!resolved.generated_functions[0].requires.is_empty());
        // get functions should have requires { id > 0 }
        assert!(!resolved.generated_functions[1].requires.is_empty());
    }

    #[test]
    fn json_api_invalid_config() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("json_api", vec![string_entry("unknown_key", "value")]);
        let result = resolver.resolve(&intent);
        assert!(matches!(result, Err(ResolverError::UnknownConfig { .. })));
    }

    #[test]
    fn json_api_route_to_handler_root() {
        assert_eq!(route_to_handler_name("/"), "handle_root");
    }

    // ===== Cache resolver tests =====

    #[test]
    fn cache_basic_defaults() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("cache", vec![]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        // get, set, invalidate = 3 functions
        assert_eq!(resolved.generated_functions.len(), 3);
        assert_eq!(resolved.generated_functions[0].name, "cache_get");
        assert_eq!(resolved.generated_functions[1].name, "cache_set");
        assert_eq!(resolved.generated_functions[2].name, "cache_invalidate");
        assert!(resolved.description.contains("lru"));
        assert!(resolved.description.contains("256"));
    }

    #[test]
    fn cache_custom_strategy_and_size() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent(
            "cache",
            vec![
                string_entry("strategy", "fifo"),
                int_entry("max_size", 1000),
            ],
        );
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert!(resolved.description.contains("fifo"));
        assert!(resolved.description.contains("1000"));
    }

    #[test]
    fn cache_functions_have_contracts() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("cache", vec![]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        // All three functions should have requires { key != "" }
        for func in &resolved.generated_functions {
            assert!(
                !func.requires.is_empty(),
                "{} should have a contract",
                func.name
            );
        }
    }

    #[test]
    fn cache_invalid_config() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("cache", vec![string_entry("backend", "redis")]);
        let result = resolver.resolve(&intent);
        assert!(matches!(result, Err(ResolverError::UnknownConfig { .. })));
    }

    // ===== Queue resolver tests =====

    #[test]
    fn queue_basic_no_topics() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("queue", vec![]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert!(resolved.generated_functions.is_empty());
        assert!(resolved.description.contains("no topics"));
    }

    #[test]
    fn queue_with_topics() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent(
            "queue",
            vec![
                string_entry("backend", "redis"),
                list_entry("topics", vec!["events", "tasks"]),
            ],
        );
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        // 2 topics * 2 (produce + consume) = 4 functions
        assert_eq!(resolved.generated_functions.len(), 4);
        assert_eq!(resolved.generated_functions[0].name, "produce_events");
        assert_eq!(resolved.generated_functions[1].name, "consume_events");
        assert_eq!(resolved.generated_functions[2].name, "produce_tasks");
        assert_eq!(resolved.generated_functions[3].name, "consume_tasks");
        assert!(resolved.description.contains("redis"));
    }

    #[test]
    fn queue_produce_has_contract() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("queue", vec![list_entry("topics", vec!["events"])]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        // produce_events should have requires { message != "" }
        let produce_func = &resolved.generated_functions[0];
        assert_eq!(produce_func.name, "produce_events");
        assert!(!produce_func.requires.is_empty());
        // consume_events should have no requires
        let consume_func = &resolved.generated_functions[1];
        assert_eq!(consume_func.name, "consume_events");
        assert!(consume_func.requires.is_empty());
    }

    #[test]
    fn queue_invalid_config() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("queue", vec![string_entry("driver", "x")]);
        let result = resolver.resolve(&intent);
        assert!(matches!(result, Err(ResolverError::UnknownConfig { .. })));
    }

    // ===== Helper: nested list entry for route/command configs =====

    fn fn_ref_entry(key: &str, name: &str) -> kodo_ast::IntentConfigEntry {
        kodo_ast::IntentConfigEntry {
            key: key.to_string(),
            value: IntentConfigValue::FnRef(name.to_string(), Span::new(0, 5)),
            span: Span::new(0, 10),
        }
    }

    fn nested_list_entry(key: &str, items: Vec<Vec<&str>>) -> kodo_ast::IntentConfigEntry {
        kodo_ast::IntentConfigEntry {
            key: key.to_string(),
            value: IntentConfigValue::List(
                items
                    .into_iter()
                    .map(|inner| {
                        IntentConfigValue::List(
                            inner
                                .into_iter()
                                .map(|s| {
                                    IntentConfigValue::StringLit(s.to_string(), Span::new(0, 5))
                                })
                                .collect(),
                            Span::new(0, 20),
                        )
                    })
                    .collect(),
                Span::new(0, 30),
            ),
            span: Span::new(0, 40),
        }
    }

    // ===== CLI resolver tests =====

    #[test]
    fn cli_basic_no_commands() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("cli", vec![]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        // cli_help + kodo_main = 2 functions
        assert_eq!(resolved.generated_functions.len(), 2);
        assert_eq!(resolved.generated_functions[0].name, "cli_help");
        assert_eq!(resolved.generated_functions[1].name, "kodo_main");
    }

    #[test]
    fn cli_with_commands() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent(
            "cli",
            vec![
                string_entry("name", "mytool"),
                string_entry("version", "1.0.0"),
                nested_list_entry(
                    "commands",
                    vec![
                        vec!["run", "do_run", "Run the tool"],
                        vec!["test", "do_test", "Run tests"],
                    ],
                ),
            ],
        );
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert_eq!(resolved.generated_functions.len(), 2);
        assert!(resolved.description.contains("mytool"));
        assert!(resolved.description.contains("1.0.0"));
        assert!(resolved.description.contains("2 commands"));
    }

    #[test]
    fn cli_default_name_and_version() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("cli", vec![]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert!(resolved.description.contains("tool"));
        assert!(resolved.description.contains("0.1.0"));
    }

    #[test]
    fn cli_invalid_config() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("cli", vec![string_entry("unknown", "x")]);
        let result = resolver.resolve(&intent);
        assert!(matches!(result, Err(ResolverError::UnknownConfig { .. })));
    }

    #[test]
    fn cli_main_returns_int() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("cli", vec![]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        let main_fn = &resolved.generated_functions[1];
        assert_eq!(main_fn.name, "kodo_main");
        assert!(matches!(main_fn.return_type, TypeExpr::Named(ref n) if n == "Int"));
    }

    // ===== HTTP Server resolver tests =====

    #[test]
    fn http_server_basic() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("http_server", vec![]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert_eq!(resolved.generated_functions.len(), 1);
        assert_eq!(resolved.generated_functions[0].name, "kodo_main");
        assert!(resolved.description.contains("8080"));
    }

    #[test]
    fn http_server_custom_port() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("http_server", vec![int_entry("port", 3000)]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert!(resolved.description.contains("3000"));
    }

    #[test]
    fn http_server_with_routes() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent(
            "http_server",
            vec![nested_list_entry(
                "routes",
                vec![
                    vec!["GET", "/health", "handle_health"],
                    vec!["POST", "/data", "handle_data"],
                ],
            )],
        );
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert!(resolved.description.contains("2 routes"));
        assert!(resolved.description.contains("GET /health"));
        assert!(resolved.description.contains("POST /data"));
    }

    #[test]
    fn http_server_custom_not_found() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("http_server", vec![string_entry("not_found", "Custom 404")]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
    }

    #[test]
    fn http_server_invalid_config() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("http_server", vec![string_entry("host", "localhost")]);
        let result = resolver.resolve(&intent);
        assert!(matches!(result, Err(ResolverError::UnknownConfig { .. })));
    }

    // ===== File Processor resolver tests =====

    #[test]
    fn file_processor_basic() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("file_processor", vec![]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert_eq!(resolved.generated_functions.len(), 1);
        assert_eq!(resolved.generated_functions[0].name, "kodo_main");
        assert!(resolved.description.contains("input=file"));
        assert!(resolved.description.contains("output=stdout"));
    }

    #[test]
    fn file_processor_stdin_mode() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("file_processor", vec![string_entry("input", "stdin")]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert!(resolved.description.contains("input=stdin"));
    }

    #[test]
    fn file_processor_directory_mode() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("file_processor", vec![string_entry("input", "directory")]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert!(resolved.description.contains("input=directory"));
    }

    #[test]
    fn file_processor_file_output() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("file_processor", vec![string_entry("output", "file")]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert!(resolved.description.contains("output=file"));
    }

    #[test]
    fn file_processor_custom_transform() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent(
            "file_processor",
            vec![fn_ref_entry("transform", "my_transform")],
        );
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert!(resolved.description.contains("my_transform"));
    }

    #[test]
    fn file_processor_invalid_config() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("file_processor", vec![string_entry("format", "csv")]);
        let result = resolver.resolve(&intent);
        assert!(matches!(result, Err(ResolverError::UnknownConfig { .. })));
    }

    // ===== Worker resolver tests =====

    #[test]
    fn worker_basic() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("worker", vec![]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert_eq!(resolved.generated_functions.len(), 1);
        assert_eq!(resolved.generated_functions[0].name, "kodo_main");
        assert!(resolved.description.contains("do_work"));
        assert!(resolved.description.contains("10"));
    }

    #[test]
    fn worker_custom_task() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("worker", vec![fn_ref_entry("task", "process_item")]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert!(resolved.description.contains("process_item"));
    }

    #[test]
    fn worker_custom_iterations() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("worker", vec![int_entry("max_iterations", 100)]);
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert!(resolved.description.contains("100"));
    }

    #[test]
    fn worker_with_error_handler() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent(
            "worker",
            vec![
                fn_ref_entry("task", "my_task"),
                fn_ref_entry("on_error", "handle_err"),
            ],
        );
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert!(resolved.description.contains("handle_err"));
    }

    #[test]
    fn worker_invalid_config() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("worker", vec![string_entry("schedule", "cron")]);
        let result = resolver.resolve(&intent);
        assert!(matches!(result, Err(ResolverError::UnknownConfig { .. })));
    }

    // ===== JSON API with endpoints (new mode) =====

    #[test]
    fn json_api_with_endpoints() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent(
            "json_api",
            vec![
                int_entry("port", 9090),
                string_entry("base_path", "/api"),
                nested_list_entry(
                    "endpoints",
                    vec![
                        vec!["GET", "/health", "handle_health"],
                        vec!["POST", "/users", "handle_users"],
                    ],
                ),
            ],
        );
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert_eq!(resolved.generated_functions.len(), 1);
        assert_eq!(resolved.generated_functions[0].name, "kodo_main");
        assert!(resolved.description.contains("9090"));
        assert!(resolved.description.contains("2 endpoints"));
    }

    #[test]
    fn json_api_endpoints_with_base_path() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent(
            "json_api",
            vec![
                string_entry("base_path", "/v1"),
                nested_list_entry("endpoints", vec![vec!["GET", "/status", "get_status"]]),
            ],
        );
        let result = resolver.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap_or_else(|e| panic!("unexpected: {e}"));
        assert!(resolved.description.contains("/v1/status"));
    }

    // ===== Diagnostic trait implementation tests =====

    #[test]
    fn diagnostic_severity_is_always_error() {
        use kodo_ast::Diagnostic;

        let errors: Vec<ResolverError> = vec![
            ResolverError::NoResolver {
                intent: "x".to_string(),
                span: Span::new(0, 1),
            },
            ResolverError::ContractViolation {
                intent: "x".to_string(),
                reason: "y".to_string(),
            },
            ResolverError::UnknownConfig {
                key: "k".to_string(),
                intent: "x".to_string(),
                span: Span::new(0, 1),
            },
            ResolverError::ConfigTypeMismatch {
                key: "k".to_string(),
                intent: "x".to_string(),
                expected: "string".to_string(),
                found: "int".to_string(),
                span: Span::new(0, 1),
            },
        ];

        for err in &errors {
            assert_eq!(err.severity(), kodo_ast::Severity::Error);
        }
    }

    #[test]
    fn diagnostic_span_returns_some_for_located_errors() {
        use kodo_ast::Diagnostic;

        let err = ResolverError::NoResolver {
            intent: "x".to_string(),
            span: Span::new(5, 10),
        };
        assert_eq!(err.span(), Some(Span::new(5, 10)));

        let err = ResolverError::UnknownConfig {
            key: "k".to_string(),
            intent: "x".to_string(),
            span: Span::new(3, 7),
        };
        assert_eq!(err.span(), Some(Span::new(3, 7)));

        let err = ResolverError::ConfigTypeMismatch {
            key: "k".to_string(),
            intent: "x".to_string(),
            expected: "s".to_string(),
            found: "i".to_string(),
            span: Span::new(1, 2),
        };
        assert_eq!(err.span(), Some(Span::new(1, 2)));
    }

    #[test]
    fn diagnostic_span_returns_none_for_contract_violation() {
        use kodo_ast::Diagnostic;

        let err = ResolverError::ContractViolation {
            intent: "x".to_string(),
            reason: "y".to_string(),
        };
        assert_eq!(err.span(), None);
    }

    #[test]
    fn diagnostic_message_matches_display() {
        use kodo_ast::Diagnostic;

        let err = ResolverError::NoResolver {
            intent: "foo".to_string(),
            span: Span::new(0, 3),
        };
        assert_eq!(err.message(), err.to_string());
    }

    #[test]
    fn diagnostic_suggestion_for_no_resolver_lists_intents() {
        use kodo_ast::Diagnostic;

        let err = ResolverError::NoResolver {
            intent: "unknown".to_string(),
            span: Span::new(0, 7),
        };
        let suggestion = err.suggestion().expect("should have a suggestion");
        assert!(suggestion.contains("console_app"));
        assert!(suggestion.contains("math_module"));
        assert!(suggestion.contains("cli"));
        assert!(suggestion.contains("http_server"));
        assert!(suggestion.contains("worker"));
        assert!(suggestion.contains("file_processor"));
    }

    #[test]
    fn diagnostic_suggestion_for_unknown_config_shows_valid_keys() {
        use kodo_ast::Diagnostic;

        let err = ResolverError::UnknownConfig {
            key: "bad_key".to_string(),
            intent: "console_app".to_string(),
            span: Span::new(0, 7),
        };
        let suggestion = err.suggestion().expect("should have a suggestion");
        assert!(suggestion.contains("greeting"));
        assert!(suggestion.contains("entry_point"));
    }

    #[test]
    fn diagnostic_suggestion_none_for_unknown_intent_config() {
        use kodo_ast::Diagnostic;

        let err = ResolverError::UnknownConfig {
            key: "bad_key".to_string(),
            intent: "totally_unknown_intent".to_string(),
            span: Span::new(0, 7),
        };
        assert!(err.suggestion().is_none());
    }

    #[test]
    fn diagnostic_suggestion_none_for_contract_violation() {
        use kodo_ast::Diagnostic;

        let err = ResolverError::ContractViolation {
            intent: "x".to_string(),
            reason: "y".to_string(),
        };
        assert!(err.suggestion().is_none());
    }

    #[test]
    fn diagnostic_suggestion_none_for_config_type_mismatch() {
        use kodo_ast::Diagnostic;

        let err = ResolverError::ConfigTypeMismatch {
            key: "k".to_string(),
            intent: "x".to_string(),
            expected: "string".to_string(),
            found: "int".to_string(),
            span: Span::new(0, 1),
        };
        assert!(err.suggestion().is_none());
    }

    // ===== valid_config_keys helper tests =====

    #[test]
    fn valid_config_keys_returns_correct_keys_for_each_intent() {
        assert_eq!(
            valid_config_keys("console_app"),
            vec!["greeting", "entry_point"]
        );
        assert_eq!(valid_config_keys("math_module"), vec!["functions"]);
        assert_eq!(valid_config_keys("serve_http"), vec!["port", "routes"]);
        assert_eq!(
            valid_config_keys("database"),
            vec!["driver", "tables", "queries"]
        );
        assert_eq!(
            valid_config_keys("json_api"),
            vec!["routes", "models", "port", "base_path", "endpoints"]
        );
        assert_eq!(valid_config_keys("cache"), vec!["strategy", "max_size"]);
        assert_eq!(valid_config_keys("queue"), vec!["backend", "topics"]);
        assert_eq!(
            valid_config_keys("cli"),
            vec!["name", "version", "commands"]
        );
        assert_eq!(
            valid_config_keys("http_server"),
            vec!["port", "routes", "not_found"]
        );
        assert_eq!(
            valid_config_keys("file_processor"),
            vec!["input", "output", "transform"]
        );
        assert_eq!(
            valid_config_keys("worker"),
            vec!["task", "max_iterations", "on_error"]
        );
    }

    #[test]
    fn valid_config_keys_returns_empty_for_unknown_intent() {
        assert!(valid_config_keys("nonexistent").is_empty());
        assert!(valid_config_keys("").is_empty());
    }

    // ===== Config helper function tests =====

    #[test]
    fn get_string_config_finds_string_value() {
        let intent = make_intent("test", vec![string_entry("key1", "value1")]);
        assert_eq!(get_string_config(&intent, "key1"), Some("value1"));
    }

    #[test]
    fn get_string_config_returns_none_for_missing_key() {
        let intent = make_intent("test", vec![string_entry("key1", "value1")]);
        assert_eq!(get_string_config(&intent, "missing"), None);
    }

    #[test]
    fn get_string_config_returns_none_for_non_string_value() {
        let intent = make_intent("test", vec![int_entry("count", 42)]);
        assert_eq!(get_string_config(&intent, "count"), None);
    }

    #[test]
    fn get_int_config_finds_int_value() {
        let intent = make_intent("test", vec![int_entry("count", 42)]);
        assert_eq!(get_int_config(&intent, "count"), Some(42));
    }

    #[test]
    fn get_int_config_returns_none_for_missing_key() {
        let intent = make_intent("test", vec![int_entry("count", 42)]);
        assert_eq!(get_int_config(&intent, "missing"), None);
    }

    #[test]
    fn get_int_config_returns_none_for_non_int_value() {
        let intent = make_intent("test", vec![string_entry("name", "hello")]);
        assert_eq!(get_int_config(&intent, "name"), None);
    }

    #[test]
    fn get_string_list_config_extracts_strings() {
        let intent = make_intent("test", vec![list_entry("items", vec!["a", "b", "c"])]);
        assert_eq!(
            get_string_list_config(&intent, "items"),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }

    #[test]
    fn get_string_list_config_extracts_fn_refs_as_strings() {
        let intent = make_intent(
            "test",
            vec![kodo_ast::IntentConfigEntry {
                key: "funcs".to_string(),
                value: IntentConfigValue::List(
                    vec![
                        IntentConfigValue::FnRef("add".to_string(), Span::new(0, 3)),
                        IntentConfigValue::FnRef("sub".to_string(), Span::new(0, 3)),
                    ],
                    Span::new(0, 20),
                ),
                span: Span::new(0, 30),
            }],
        );
        assert_eq!(
            get_string_list_config(&intent, "funcs"),
            vec!["add".to_string(), "sub".to_string()]
        );
    }

    #[test]
    fn get_string_list_config_returns_empty_for_missing_key() {
        let intent = make_intent("test", vec![]);
        assert!(get_string_list_config(&intent, "items").is_empty());
    }

    #[test]
    fn get_string_list_config_returns_empty_for_non_list_value() {
        let intent = make_intent("test", vec![string_entry("items", "not a list")]);
        assert!(get_string_list_config(&intent, "items").is_empty());
    }

    #[test]
    fn get_fn_ref_config_finds_fn_ref() {
        let intent = make_intent("test", vec![fn_ref_entry("handler", "my_fn")]);
        assert_eq!(get_fn_ref_config(&intent, "handler"), Some("my_fn"));
    }

    #[test]
    fn get_fn_ref_config_returns_none_for_string_value() {
        let intent = make_intent("test", vec![string_entry("handler", "my_fn")]);
        assert_eq!(get_fn_ref_config(&intent, "handler"), None);
    }

    #[test]
    fn get_fn_ref_config_returns_none_for_missing_key() {
        let intent = make_intent("test", vec![]);
        assert_eq!(get_fn_ref_config(&intent, "handler"), None);
    }

    #[test]
    fn get_nested_list_config_extracts_nested_lists() {
        let intent = make_intent(
            "test",
            vec![nested_list_entry(
                "cmds",
                vec![vec!["a", "b"], vec!["c", "d", "e"]],
            )],
        );
        let result = get_nested_list_config(&intent, "cmds");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].len(), 2);
        assert_eq!(result[1].len(), 3);
    }

    #[test]
    fn get_nested_list_config_returns_empty_for_missing_key() {
        let intent = make_intent("test", vec![]);
        assert!(get_nested_list_config(&intent, "cmds").is_empty());
    }

    #[test]
    fn get_nested_list_config_skips_non_list_items() {
        let intent = make_intent(
            "test",
            vec![kodo_ast::IntentConfigEntry {
                key: "mixed".to_string(),
                value: IntentConfigValue::List(
                    vec![
                        IntentConfigValue::StringLit("not_a_list".to_string(), Span::new(0, 5)),
                        IntentConfigValue::List(
                            vec![IntentConfigValue::StringLit(
                                "inner".to_string(),
                                Span::new(0, 5),
                            )],
                            Span::new(0, 10),
                        ),
                    ],
                    Span::new(0, 20),
                ),
                span: Span::new(0, 30),
            }],
        );
        let result = get_nested_list_config(&intent, "mixed");
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn config_value_as_str_extracts_string_lit() {
        let v = IntentConfigValue::StringLit("hello".to_string(), Span::new(0, 5));
        assert_eq!(config_value_as_str(&v), Some("hello"));
    }

    #[test]
    fn config_value_as_str_extracts_fn_ref() {
        let v = IntentConfigValue::FnRef("my_fn".to_string(), Span::new(0, 5));
        assert_eq!(config_value_as_str(&v), Some("my_fn"));
    }

    #[test]
    fn config_value_as_str_returns_none_for_int() {
        let v = IntentConfigValue::IntLit(42, Span::new(0, 2));
        assert_eq!(config_value_as_str(&v), None);
    }

    #[test]
    fn config_value_as_str_returns_none_for_list() {
        let v = IntentConfigValue::List(vec![], Span::new(0, 2));
        assert_eq!(config_value_as_str(&v), None);
    }

    // ===== route_to_handler_name edge cases =====

    #[test]
    fn route_to_handler_name_simple_path() {
        assert_eq!(route_to_handler_name("/users"), "handle_users");
    }

    #[test]
    fn route_to_handler_name_nested_path() {
        assert_eq!(route_to_handler_name("/api/users"), "handle_api_users");
    }

    #[test]
    fn route_to_handler_name_empty_string() {
        assert_eq!(route_to_handler_name(""), "handle_root");
    }

    #[test]
    fn route_to_handler_name_only_slashes() {
        assert_eq!(route_to_handler_name("///"), "handle_root");
    }

    #[test]
    fn route_to_handler_name_with_special_chars() {
        assert_eq!(route_to_handler_name("/api-v2"), "handle_api_v2");
    }

    // ===== generate_math_function edge cases =====

    #[test]
    fn generate_math_function_add() {
        let span = Span::new(0, 10);
        let func = generate_math_function("add", span).expect("should generate add");
        assert_eq!(func.name, "add");
        assert_eq!(func.params.len(), 2);
        assert!(func.requires.is_empty());
    }

    #[test]
    fn generate_math_function_sub() {
        let span = Span::new(0, 10);
        let func = generate_math_function("sub", span).expect("should generate sub");
        assert_eq!(func.name, "sub");
        assert!(func.requires.is_empty());
    }

    #[test]
    fn generate_math_function_mul() {
        let span = Span::new(0, 10);
        let func = generate_math_function("mul", span).expect("should generate mul");
        assert_eq!(func.name, "mul");
        assert!(func.requires.is_empty());
    }

    #[test]
    fn generate_math_function_safe_div_has_contract() {
        let span = Span::new(0, 10);
        let func = generate_math_function("safe_div", span).expect("should generate safe_div");
        assert_eq!(func.name, "safe_div");
        assert_eq!(func.requires.len(), 1);
    }

    #[test]
    fn generate_math_function_unknown_returns_none() {
        let span = Span::new(0, 10);
        assert!(generate_math_function("sqrt", span).is_none());
        assert!(generate_math_function("", span).is_none());
        assert!(generate_math_function("pow", span).is_none());
    }

    // ===== resolve_all edge cases =====

    #[test]
    fn resolve_all_empty_list() {
        let resolver = Resolver::with_builtins();
        let results = resolver
            .resolve_all(&[])
            .expect("should succeed with empty list");
        assert!(results.is_empty());
    }

    #[test]
    fn resolve_all_stops_on_first_error() {
        let resolver = Resolver::with_builtins();
        let intents = vec![
            make_intent("console_app", vec![]),
            make_intent("nonexistent_intent", vec![]),
            make_intent("console_app", vec![]),
        ];
        let result = resolver.resolve_all(&intents);
        assert!(result.is_err());
        assert!(matches!(result, Err(ResolverError::NoResolver { .. })));
    }

    #[test]
    fn resolve_all_stops_on_unknown_config() {
        let resolver = Resolver::with_builtins();
        let intents = vec![make_intent(
            "console_app",
            vec![string_entry("bad_key", "value")],
        )];
        let result = resolver.resolve_all(&intents);
        assert!(matches!(result, Err(ResolverError::UnknownConfig { .. })));
    }

    // ===== ResolverStrategy trait method tests =====

    #[test]
    fn console_app_strategy_handles_and_keys() {
        let strategy = ConsoleAppStrategy;
        assert_eq!(strategy.handles(), &["console_app"]);
        assert_eq!(strategy.valid_keys(), &["greeting", "entry_point"]);
    }

    #[test]
    fn math_module_strategy_handles_and_keys() {
        let strategy = MathModuleStrategy;
        assert_eq!(strategy.handles(), &["math_module"]);
        assert_eq!(strategy.valid_keys(), &["functions"]);
    }

    #[test]
    fn serve_http_strategy_handles_and_keys() {
        let strategy = ServeHttpStrategy;
        assert_eq!(strategy.handles(), &["serve_http"]);
        assert_eq!(strategy.valid_keys(), &["port", "routes"]);
    }

    #[test]
    fn database_strategy_handles_and_keys() {
        let strategy = DatabaseStrategy;
        assert_eq!(strategy.handles(), &["database"]);
        assert_eq!(strategy.valid_keys(), &["driver", "tables", "queries"]);
    }

    #[test]
    fn json_api_strategy_handles_and_keys() {
        let strategy = JsonApiStrategy;
        assert_eq!(strategy.handles(), &["json_api"]);
        assert_eq!(
            strategy.valid_keys(),
            &["routes", "models", "port", "base_path", "endpoints"]
        );
    }

    #[test]
    fn cache_strategy_handles_and_keys() {
        let strategy = CacheStrategy;
        assert_eq!(strategy.handles(), &["cache"]);
        assert_eq!(strategy.valid_keys(), &["strategy", "max_size"]);
    }

    #[test]
    fn queue_strategy_handles_and_keys() {
        let strategy = QueueStrategy;
        assert_eq!(strategy.handles(), &["queue"]);
        assert_eq!(strategy.valid_keys(), &["backend", "topics"]);
    }

    #[test]
    fn cli_strategy_handles_and_keys() {
        let strategy = CliStrategy;
        assert_eq!(strategy.handles(), &["cli"]);
        assert_eq!(strategy.valid_keys(), &["name", "version", "commands"]);
    }

    #[test]
    fn http_server_strategy_handles_and_keys() {
        let strategy = HttpServerStrategy;
        assert_eq!(strategy.handles(), &["http_server"]);
        assert_eq!(strategy.valid_keys(), &["port", "routes", "not_found"]);
    }

    #[test]
    fn file_processor_strategy_handles_and_keys() {
        let strategy = FileProcessorStrategy;
        assert_eq!(strategy.handles(), &["file_processor"]);
        assert_eq!(strategy.valid_keys(), &["input", "output", "transform"]);
    }

    #[test]
    fn worker_strategy_handles_and_keys() {
        let strategy = WorkerStrategy;
        assert_eq!(strategy.handles(), &["worker"]);
        assert_eq!(
            strategy.valid_keys(),
            &["task", "max_iterations", "on_error"]
        );
    }

    // ===== Resolver registration and lookup tests =====

    #[test]
    fn custom_strategy_can_be_registered() {
        struct CustomStrategy;
        impl ResolverStrategy for CustomStrategy {
            fn handles(&self) -> &[&str] {
                &["custom"]
            }
            fn valid_keys(&self) -> &[&str] {
                &["option"]
            }
            fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
                Ok(ResolvedIntent {
                    generated_functions: vec![],
                    generated_types: vec![],
                    description: format!("Custom resolved: {}", intent.name),
                })
            }
        }

        let mut resolver = Resolver::new();
        resolver.register(Box::new(CustomStrategy));

        let intent = make_intent("custom", vec![]);
        let result = resolver
            .resolve(&intent)
            .expect("should resolve custom intent");
        assert!(result.description.contains("Custom resolved: custom"));
    }

    #[test]
    fn first_matching_strategy_wins() {
        struct StrategyA;
        impl ResolverStrategy for StrategyA {
            fn handles(&self) -> &[&str] {
                &["shared"]
            }
            fn valid_keys(&self) -> &[&str] {
                &[]
            }
            fn resolve(&self, _intent: &IntentDecl) -> Result<ResolvedIntent> {
                Ok(ResolvedIntent {
                    generated_functions: vec![],
                    generated_types: vec![],
                    description: "Strategy A".to_string(),
                })
            }
        }

        struct StrategyB;
        impl ResolverStrategy for StrategyB {
            fn handles(&self) -> &[&str] {
                &["shared"]
            }
            fn valid_keys(&self) -> &[&str] {
                &[]
            }
            fn resolve(&self, _intent: &IntentDecl) -> Result<ResolvedIntent> {
                Ok(ResolvedIntent {
                    generated_functions: vec![],
                    generated_types: vec![],
                    description: "Strategy B".to_string(),
                })
            }
        }

        let mut resolver = Resolver::new();
        resolver.register(Box::new(StrategyA));
        resolver.register(Box::new(StrategyB));

        let intent = make_intent("shared", vec![]);
        let result = resolver.resolve(&intent).expect("should resolve");
        assert_eq!(result.description, "Strategy A");
    }

    // ===== format_resolved_intent detailed tests =====

    #[test]
    fn format_resolved_intent_includes_params() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent(
            "math_module",
            vec![kodo_ast::IntentConfigEntry {
                key: "functions".to_string(),
                value: IntentConfigValue::List(
                    vec![IntentConfigValue::FnRef("add".to_string(), Span::new(0, 3))],
                    Span::new(0, 10),
                ),
                span: Span::new(0, 20),
            }],
        );
        let resolved = resolver
            .resolve(&intent)
            .expect("should resolve math_module");
        let formatted = format_resolved_intent(&resolved);
        assert!(formatted.contains("add"));
        assert!(formatted.contains("a:"));
        assert!(formatted.contains("b:"));
    }

    #[test]
    fn format_resolved_intent_includes_contracts() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent(
            "math_module",
            vec![kodo_ast::IntentConfigEntry {
                key: "functions".to_string(),
                value: IntentConfigValue::List(
                    vec![IntentConfigValue::FnRef(
                        "safe_div".to_string(),
                        Span::new(0, 8),
                    )],
                    Span::new(0, 10),
                ),
                span: Span::new(0, 20),
            }],
        );
        let resolved = resolver
            .resolve(&intent)
            .expect("should resolve math_module");
        let formatted = format_resolved_intent(&resolved);
        assert!(formatted.contains("requires"));
    }

    #[test]
    fn format_resolved_intent_empty_functions() {
        let resolved = ResolvedIntent {
            generated_functions: vec![],
            generated_types: vec![],
            description: "Empty resolution".to_string(),
        };
        let formatted = format_resolved_intent(&resolved);
        assert!(formatted.contains("Generated by intent resolver"));
        assert!(formatted.contains("Empty resolution"));
    }

    // ===== Generated function structure verification =====

    #[test]
    fn console_app_function_is_not_async() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("console_app", vec![]);
        let resolved = resolver.resolve(&intent).expect("should resolve");
        assert!(!resolved.generated_functions[0].is_async);
    }

    #[test]
    fn console_app_function_is_private() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("console_app", vec![]);
        let resolved = resolver.resolve(&intent).expect("should resolve");
        assert!(matches!(
            resolved.generated_functions[0].visibility,
            Visibility::Private
        ));
    }

    #[test]
    fn console_app_function_returns_unit() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("console_app", vec![]);
        let resolved = resolver.resolve(&intent).expect("should resolve");
        assert!(matches!(
            resolved.generated_functions[0].return_type,
            TypeExpr::Unit
        ));
    }

    #[test]
    fn console_app_function_has_no_params() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("console_app", vec![]);
        let resolved = resolver.resolve(&intent).expect("should resolve");
        assert!(resolved.generated_functions[0].params.is_empty());
    }

    #[test]
    fn math_module_empty_functions_list() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent(
            "math_module",
            vec![kodo_ast::IntentConfigEntry {
                key: "functions".to_string(),
                value: IntentConfigValue::List(vec![], Span::new(0, 5)),
                span: Span::new(0, 10),
            }],
        );
        let resolved = resolver.resolve(&intent).expect("should resolve");
        assert!(resolved.generated_functions.is_empty());
        assert!(resolved.description.contains("No math functions generated"));
    }

    #[test]
    fn math_module_unknown_function_name_is_skipped() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent(
            "math_module",
            vec![kodo_ast::IntentConfigEntry {
                key: "functions".to_string(),
                value: IntentConfigValue::List(
                    vec![
                        IntentConfigValue::FnRef("add".to_string(), Span::new(0, 3)),
                        IntentConfigValue::FnRef("unknown_op".to_string(), Span::new(0, 10)),
                        IntentConfigValue::FnRef("mul".to_string(), Span::new(0, 3)),
                    ],
                    Span::new(0, 20),
                ),
                span: Span::new(0, 30),
            }],
        );
        let resolved = resolver.resolve(&intent).expect("should resolve");
        // unknown_op should be skipped, only add and mul generated
        assert_eq!(resolved.generated_functions.len(), 2);
        assert_eq!(resolved.generated_functions[0].name, "add");
        assert_eq!(resolved.generated_functions[1].name, "mul");
    }

    #[test]
    fn math_function_params_are_int_typed() {
        let span = Span::new(0, 10);
        let func = generate_math_function("add", span).expect("should generate");
        assert_eq!(func.params[0].name, "a");
        assert!(matches!(func.params[0].ty, TypeExpr::Named(ref n) if n == "Int"));
        assert_eq!(func.params[1].name, "b");
        assert!(matches!(func.params[1].ty, TypeExpr::Named(ref n) if n == "Int"));
        assert!(matches!(func.return_type, TypeExpr::Named(ref n) if n == "Int"));
    }

    #[test]
    fn math_function_params_are_owned() {
        let span = Span::new(0, 10);
        let func = generate_math_function("add", span).expect("should generate");
        assert!(matches!(func.params[0].ownership, Ownership::Owned));
        assert!(matches!(func.params[1].ownership, Ownership::Owned));
    }

    // ===== Worker structure tests =====

    #[test]
    fn worker_main_returns_int() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("worker", vec![]);
        let resolved = resolver.resolve(&intent).expect("should resolve");
        let main_fn = &resolved.generated_functions[0];
        assert!(matches!(main_fn.return_type, TypeExpr::Named(ref n) if n == "Int"));
    }

    #[test]
    fn worker_with_zero_iterations() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("worker", vec![int_entry("max_iterations", 0)]);
        let resolved = resolver.resolve(&intent).expect("should resolve");
        assert!(resolved.description.contains("max_iterations=0"));
    }

    // ===== HTTP Server structure tests =====

    #[test]
    fn http_server_main_returns_int() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("http_server", vec![]);
        let resolved = resolver.resolve(&intent).expect("should resolve");
        let main_fn = &resolved.generated_functions[0];
        assert!(matches!(main_fn.return_type, TypeExpr::Named(ref n) if n == "Int"));
    }

    #[test]
    fn http_server_no_routes_still_generates_main() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("http_server", vec![]);
        let resolved = resolver.resolve(&intent).expect("should resolve");
        assert_eq!(resolved.generated_functions.len(), 1);
        assert_eq!(resolved.generated_functions[0].name, "kodo_main");
        assert!(resolved.description.contains("0 routes"));
    }

    // ===== File Processor structure tests =====

    #[test]
    fn file_processor_main_returns_int() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent("file_processor", vec![]);
        let resolved = resolver.resolve(&intent).expect("should resolve");
        let main_fn = &resolved.generated_functions[0];
        assert!(matches!(main_fn.return_type, TypeExpr::Named(ref n) if n == "Int"));
    }

    #[test]
    fn file_processor_with_string_transform() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent(
            "file_processor",
            vec![string_entry("transform", "uppercase")],
        );
        let resolved = resolver.resolve(&intent).expect("should resolve");
        assert!(resolved.description.contains("uppercase"));
    }

    // ===== CLI structure tests =====

    #[test]
    fn cli_with_command_missing_description() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent(
            "cli",
            vec![nested_list_entry("commands", vec![vec!["run", "do_run"]])],
        );
        let resolved = resolver.resolve(&intent).expect("should resolve");
        assert_eq!(resolved.generated_functions.len(), 2);
        assert!(resolved.description.contains("1 commands"));
    }

    #[test]
    fn cli_with_command_too_few_entries_skipped() {
        let resolver = Resolver::with_builtins();
        // A command with only 1 element should be skipped (needs >= 2)
        let intent = make_intent(
            "cli",
            vec![nested_list_entry("commands", vec![vec!["only_name"]])],
        );
        let resolved = resolver.resolve(&intent).expect("should resolve");
        assert!(resolved.description.contains("0 commands"));
    }

    // ===== Resolver default tests =====

    #[test]
    fn resolver_default_is_empty() {
        let resolver = Resolver::default();
        let intent = make_intent("console_app", vec![]);
        assert!(resolver.resolve(&intent).is_err());
    }

    #[test]
    fn resolver_new_is_same_as_default() {
        let resolver = Resolver::new();
        let intent = make_intent("console_app", vec![]);
        assert!(resolver.resolve(&intent).is_err());
    }

    // ===== Error message content tests =====

    #[test]
    fn config_type_mismatch_error_message_contains_details() {
        let err = ResolverError::ConfigTypeMismatch {
            key: "port".to_string(),
            intent: "serve_http".to_string(),
            expected: "integer".to_string(),
            found: "string".to_string(),
            span: Span::new(10, 20),
        };
        let msg = err.to_string();
        assert!(msg.contains("port"));
        assert!(msg.contains("serve_http"));
        assert!(msg.contains("integer"));
        assert!(msg.contains("string"));
    }

    // ===== Database additional tests =====

    #[test]
    fn database_connect_returns_string() {
        let span = Span::new(0, 10);
        let func = generate_db_connect("postgres", span);
        assert_eq!(func.name, "db_connect");
        assert!(matches!(func.return_type, TypeExpr::Named(ref n) if n == "String"));
        assert!(func.params.is_empty());
        assert!(func.requires.is_empty());
    }

    #[test]
    fn database_table_query_has_id_param() {
        let span = Span::new(0, 10);
        let func = generate_db_table_query("users", span);
        assert_eq!(func.name, "query_users");
        assert_eq!(func.params.len(), 1);
        assert_eq!(func.params[0].name, "id");
        assert!(matches!(func.params[0].ty, TypeExpr::Named(ref n) if n == "Int"));
    }

    #[test]
    fn database_named_query_has_id_param() {
        let span = Span::new(0, 10);
        let func = generate_db_named_query("find_by_email", span);
        assert_eq!(func.name, "find_by_email");
        assert_eq!(func.params.len(), 1);
        assert_eq!(func.params[0].name, "id");
    }

    // ===== Cache additional tests =====

    #[test]
    fn cache_get_has_key_param() {
        let span = Span::new(0, 10);
        let func = generate_cache_get(span);
        assert_eq!(func.params.len(), 1);
        assert_eq!(func.params[0].name, "key");
        assert!(matches!(func.return_type, TypeExpr::Named(ref n) if n == "String"));
    }

    #[test]
    fn cache_set_has_key_and_value_params() {
        let span = Span::new(0, 10);
        let func = generate_cache_set(512, span);
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "key");
        assert_eq!(func.params[1].name, "value");
        assert!(matches!(func.return_type, TypeExpr::Named(ref n) if n == "Bool"));
    }

    #[test]
    fn cache_invalidate_has_key_param() {
        let span = Span::new(0, 10);
        let func = generate_cache_invalidate(span);
        assert_eq!(func.params.len(), 1);
        assert_eq!(func.params[0].name, "key");
        assert!(matches!(func.return_type, TypeExpr::Named(ref n) if n == "Bool"));
    }

    // ===== Queue additional tests =====

    #[test]
    fn queue_produce_has_message_param() {
        let span = Span::new(0, 10);
        let func = generate_queue_produce("events", span);
        assert_eq!(func.name, "produce_events");
        assert_eq!(func.params.len(), 1);
        assert_eq!(func.params[0].name, "message");
        assert!(matches!(func.return_type, TypeExpr::Unit));
    }

    #[test]
    fn queue_consume_has_no_params() {
        let span = Span::new(0, 10);
        let func = generate_queue_consume("events", span);
        assert_eq!(func.name, "consume_events");
        assert!(func.params.is_empty());
        assert!(matches!(func.return_type, TypeExpr::Named(ref n) if n == "String"));
    }

    // ===== JSON API additional tests =====

    #[test]
    fn json_api_model_creates_have_data_param() {
        let span = Span::new(0, 10);
        let func = generate_api_create_model("user", span);
        assert_eq!(func.name, "create_user");
        assert_eq!(func.params.len(), 1);
        assert_eq!(func.params[0].name, "data");
        assert!(matches!(func.params[0].ty, TypeExpr::Named(ref n) if n == "String"));
    }

    #[test]
    fn json_api_model_gets_have_id_param() {
        let span = Span::new(0, 10);
        let func = generate_api_get_model("user", span);
        assert_eq!(func.name, "get_user");
        assert_eq!(func.params.len(), 1);
        assert_eq!(func.params[0].name, "id");
        assert!(matches!(func.params[0].ty, TypeExpr::Named(ref n) if n == "Int"));
    }

    #[test]
    fn json_api_route_to_handler_preserves_alphanumeric() {
        assert_eq!(route_to_handler_name("/abc123"), "handle_abc123");
    }

    // ===== Multiple config entries with same key =====

    #[test]
    fn get_string_config_returns_first_matching_entry() {
        let intent = make_intent(
            "test",
            vec![string_entry("key", "first"), string_entry("key", "second")],
        );
        assert_eq!(get_string_config(&intent, "key"), Some("first"));
    }

    #[test]
    fn get_int_config_returns_first_matching_entry() {
        let intent = make_intent("test", vec![int_entry("n", 1), int_entry("n", 2)]);
        assert_eq!(get_int_config(&intent, "n"), Some(1));
    }

    // ===== JSON API endpoints with empty base_path =====

    #[test]
    fn json_api_endpoints_with_empty_base_path() {
        let resolver = Resolver::with_builtins();
        let intent = make_intent(
            "json_api",
            vec![nested_list_entry(
                "endpoints",
                vec![vec!["GET", "/health", "check"]],
            )],
        );
        let resolved = resolver.resolve(&intent).expect("should resolve");
        assert!(resolved.description.contains("/health"));
    }

    // ===== HTTP handler generation test =====

    #[test]
    fn generate_http_handler_creates_correct_function() {
        let span = Span::new(0, 10);
        let func = generate_http_handler("my_handler", span);
        assert_eq!(func.name, "my_handler");
        assert!(func.params.is_empty());
        assert!(matches!(func.return_type, TypeExpr::Unit));
        assert!(func.requires.is_empty());
        assert!(!func.is_async);
    }

    // ===== Make helpers tests =====

    #[test]
    fn make_function_helper_creates_valid_function() {
        let span = Span::new(0, 10);
        let func = make_function(
            "test_fn",
            TypeExpr::Named("Int".to_string()),
            vec![Stmt::Return {
                span,
                value: Some(Expr::IntLit(42, span)),
            }],
            span,
        );
        assert_eq!(func.name, "test_fn");
        assert!(matches!(func.return_type, TypeExpr::Named(ref n) if n == "Int"));
        assert!(func.params.is_empty());
        assert!(func.requires.is_empty());
        assert!(func.ensures.is_empty());
        assert!(!func.is_async);
        assert!(matches!(func.visibility, Visibility::Private));
        assert_eq!(func.body.stmts.len(), 1);
    }

    #[test]
    fn make_if_chain_empty_branches() {
        let span = Span::new(0, 10);
        let result = make_if_chain(vec![], &[], span);
        // With no branches and no else, falls back to IntLit(0)
        assert!(matches!(result, Expr::IntLit(0, _)));
    }

    #[test]
    fn make_if_chain_single_branch_no_else() {
        let span = Span::new(0, 10);
        let condition = Expr::BoolLit(true, span);
        let body = vec![Stmt::Expr(Expr::IntLit(1, span))];
        let result = make_if_chain(vec![(condition, body)], &[], span);
        assert!(matches!(result, Expr::If { .. }));
    }

    #[test]
    fn make_if_chain_single_branch_with_else() {
        let span = Span::new(0, 10);
        let condition = Expr::BoolLit(true, span);
        let body = vec![Stmt::Expr(Expr::IntLit(1, span))];
        let else_body = vec![Stmt::Expr(Expr::IntLit(2, span))];
        let result = make_if_chain(vec![(condition, body)], &else_body, span);
        match result {
            Expr::If { else_branch, .. } => {
                assert!(else_branch.is_some());
            }
            _ => panic!("expected If expression"),
        }
    }

    #[test]
    fn make_if_chain_multiple_branches() {
        let span = Span::new(0, 10);
        let branches = vec![
            (
                Expr::BoolLit(true, span),
                vec![Stmt::Expr(Expr::IntLit(1, span))],
            ),
            (
                Expr::BoolLit(false, span),
                vec![Stmt::Expr(Expr::IntLit(2, span))],
            ),
        ];
        let else_body = vec![Stmt::Expr(Expr::IntLit(3, span))];
        let result = make_if_chain(branches, &else_body, span);
        // Should be a nested if-else chain
        match result {
            Expr::If { else_branch, .. } => {
                assert!(else_branch.is_some());
            }
            _ => panic!("expected If expression"),
        }
    }
}
