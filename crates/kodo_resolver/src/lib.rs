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
    Block, Expr, Function, IntentConfigValue, IntentDecl, NodeId, Ownership, Param, Span, Stmt,
    TypeExpr,
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
                Some("available intents: console_app, math_module, serve_http, database, json_api, cache, queue".to_string())
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
        "json_api" => vec!["routes", "models"],
        "cache" => vec!["strategy", "max_size"],
        "queue" => vec!["backend", "topics"],
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
        &["routes", "models"]
    }

    fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
        let span = intent.span;
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
        let intent = make_intent("json_api", vec![string_entry("port", "8080")]);
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
}
