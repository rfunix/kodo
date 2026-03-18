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

mod error;
mod format;
mod helpers;
mod strategies;

#[cfg(test)]
mod tests;

pub use error::ResolverError;
pub use format::format_resolved_intent;

use kodo_ast::{Function, IntentDecl};

use strategies::{
    CacheStrategy, CliStrategy, ConsoleAppStrategy, DatabaseStrategy, FileProcessorStrategy,
    HttpServerStrategy, JsonApiStrategy, MathModuleStrategy, QueueStrategy, ServeHttpStrategy,
    WorkerStrategy,
};

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
