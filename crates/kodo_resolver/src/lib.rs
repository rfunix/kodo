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
                Some("available intents: console_app, math_module".to_string())
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
}
