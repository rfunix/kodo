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
//! ## Current Status
//!
//! Stub implementation — intent declarations are parsed but not yet resolved.
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

use kodo_ast::Span;
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
}

/// Alias for results in this crate.
pub type Result<T> = std::result::Result<T, ResolverError>;

/// Represents a parsed intent declaration.
#[derive(Debug, Clone)]
pub struct IntentDecl {
    /// The intent name (e.g., `serve_http`, `database`).
    pub name: String,
    /// Configuration key-value pairs.
    pub config: Vec<(String, String)>,
    /// Source span.
    pub span: Span,
}

/// A resolver strategy that can handle a specific kind of intent.
pub trait ResolverStrategy {
    /// Returns the intent names this strategy can handle.
    fn handles(&self) -> &[&str];

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
    pub generated_functions: Vec<kodo_ast::Function>,
    /// Any additional type definitions.
    pub generated_types: Vec<String>,
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

    /// Registers a resolver strategy.
    pub fn register(&mut self, strategy: Box<dyn ResolverStrategy>) {
        self.strategies.push(strategy);
    }

    /// Resolves an intent using registered strategies.
    ///
    /// # Errors
    ///
    /// Returns [`ResolverError::NoResolver`] if no strategy handles the intent.
    pub fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
        for strategy in &self.strategies {
            if strategy.handles().contains(&intent.name.as_str()) {
                return strategy.resolve(intent);
            }
        }
        Err(ResolverError::NoResolver {
            intent: intent.name.clone(),
            span: intent.span,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_resolver_returns_no_resolver_error() {
        let resolver = Resolver::new();
        let intent = IntentDecl {
            name: "serve_http".to_string(),
            config: vec![],
            span: Span::new(0, 10),
        };
        let result = resolver.resolve(&intent);
        assert!(result.is_err());
        assert!(matches!(result, Err(ResolverError::NoResolver { .. })));
    }

    #[test]
    fn resolver_new_creates_empty() {
        let resolver = Resolver::new();
        let intent = IntentDecl {
            name: "test".to_string(),
            config: vec![],
            span: Span::new(0, 4),
        };
        // No strategies registered, so resolve should fail.
        assert!(resolver.resolve(&intent).is_err());
    }

    #[test]
    fn resolver_register_and_resolve() {
        struct MockStrategy;
        impl ResolverStrategy for MockStrategy {
            fn handles(&self) -> &[&str] {
                &["mock"]
            }
            fn resolve(&self, _intent: &IntentDecl) -> Result<ResolvedIntent> {
                Ok(ResolvedIntent {
                    generated_functions: vec![],
                    generated_types: vec![],
                })
            }
        }
        let mut resolver = Resolver::new();
        resolver.register(Box::new(MockStrategy));
        let intent = IntentDecl {
            name: "mock".to_string(),
            config: vec![],
            span: Span::new(0, 4),
        };
        assert!(resolver.resolve(&intent).is_ok());
    }

    #[test]
    fn resolver_wrong_intent_returns_no_resolver() {
        struct FooStrategy;
        impl ResolverStrategy for FooStrategy {
            fn handles(&self) -> &[&str] {
                &["foo"]
            }
            fn resolve(&self, _intent: &IntentDecl) -> Result<ResolvedIntent> {
                Ok(ResolvedIntent {
                    generated_functions: vec![],
                    generated_types: vec![],
                })
            }
        }
        let mut resolver = Resolver::new();
        resolver.register(Box::new(FooStrategy));
        let intent = IntentDecl {
            name: "bar".to_string(),
            config: vec![],
            span: Span::new(0, 3),
        };
        let result = resolver.resolve(&intent);
        assert!(matches!(result, Err(ResolverError::NoResolver { .. })));
    }

    #[test]
    fn resolver_first_matching_strategy_wins() {
        struct StrategyA;
        impl ResolverStrategy for StrategyA {
            fn handles(&self) -> &[&str] {
                &["shared"]
            }
            fn resolve(&self, _intent: &IntentDecl) -> Result<ResolvedIntent> {
                Ok(ResolvedIntent {
                    generated_functions: vec![],
                    generated_types: vec!["from_a".to_string()],
                })
            }
        }
        struct StrategyB;
        impl ResolverStrategy for StrategyB {
            fn handles(&self) -> &[&str] {
                &["shared"]
            }
            fn resolve(&self, _intent: &IntentDecl) -> Result<ResolvedIntent> {
                Ok(ResolvedIntent {
                    generated_functions: vec![],
                    generated_types: vec!["from_b".to_string()],
                })
            }
        }
        let mut resolver = Resolver::new();
        resolver.register(Box::new(StrategyA));
        resolver.register(Box::new(StrategyB));
        let intent = IntentDecl {
            name: "shared".to_string(),
            config: vec![],
            span: Span::new(0, 6),
        };
        let result = resolver.resolve(&intent).unwrap();
        assert_eq!(result.generated_types, vec!["from_a".to_string()]);
    }

    #[test]
    fn intent_decl_with_config() {
        let intent = IntentDecl {
            name: "serve_http".to_string(),
            config: vec![
                ("port".to_string(), "8080".to_string()),
                ("host".to_string(), "localhost".to_string()),
            ],
            span: Span::new(0, 50),
        };
        assert_eq!(intent.name, "serve_http");
        assert_eq!(intent.config.len(), 2);
        assert_eq!(intent.config[0].0, "port");
        assert_eq!(intent.config[0].1, "8080");
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
