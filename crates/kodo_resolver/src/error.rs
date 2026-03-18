//! Error types for the intent resolver.
//!
//! Defines [`ResolverError`], its error codes, diagnostic trait implementation,
//! and the [`valid_config_keys`] helper used for error suggestions.

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

/// Returns valid config keys for a known intent name.
pub(crate) fn valid_config_keys(intent: &str) -> Vec<&'static str> {
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
