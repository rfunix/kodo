//! Property-based tests for the Kodo intent resolver.

use kodo_ast::{IntentConfigEntry, IntentConfigValue, IntentDecl, NodeId, Span};
use kodo_resolver::{Resolver, ResolverError};
use proptest::prelude::*;

fn make_intent(name: &str, config: Vec<IntentConfigEntry>) -> IntentDecl {
    IntentDecl {
        id: NodeId(0),
        span: Span::new(0, 50),
        name: name.to_string(),
        config,
    }
}

fn string_entry(key: &str, value: &str) -> IntentConfigEntry {
    IntentConfigEntry {
        key: key.to_string(),
        value: IntentConfigValue::StringLit(value.to_string(), Span::new(0, 10)),
        span: Span::new(0, 20),
    }
}

proptest! {
    /// Resolver never panics with random intent names and configs.
    #[test]
    fn resolver_never_panics_with_random_intent(
        name in "[a-z_]{1,30}",
        key in "[a-z_]{1,20}",
        val in "[a-zA-Z0-9 ]{0,50}"
    ) {
        let resolver = Resolver::with_builtins();
        let intent = make_intent(&name, vec![string_entry(&key, &val)]);
        // Must not panic — may return Ok or Err.
        let _ = resolver.resolve(&intent);
    }

    /// Resolver never panics with empty config on random intent names.
    #[test]
    fn resolver_never_panics_empty_config(name in "[a-z_]{1,30}") {
        let resolver = Resolver::with_builtins();
        let intent = make_intent(&name, vec![]);
        let _ = resolver.resolve(&intent);
    }

    /// Unknown intent names always produce a NoResolver error.
    #[test]
    fn unknown_intent_produces_no_resolver_error(
        name in "[a-z]{10,30}"  // long enough to not collide with builtins
    ) {
        // Skip known intent names.
        let known = [
            "console_app", "math_module", "serve_http", "database",
            "json_api", "cache", "queue", "cli", "http_server",
            "file_processor", "worker",
        ];
        prop_assume!(!known.contains(&name.as_str()));

        let resolver = Resolver::with_builtins();
        let intent = make_intent(&name, vec![]);
        let result = resolver.resolve(&intent);
        let is_no_resolver = matches!(result, Err(ResolverError::NoResolver { .. }));
        prop_assert!(is_no_resolver, "expected NoResolver error for intent: {}", name);
    }
}

#[test]
fn console_app_with_greeting_produces_valid_output() {
    let resolver = Resolver::with_builtins();
    let intent = make_intent("console_app", vec![string_entry("greeting", "Hi!")]);
    let result = resolver.resolve(&intent);
    assert!(result.is_ok());
    let resolved = result.unwrap();
    assert!(!resolved.generated_functions.is_empty());
    assert_eq!(resolved.generated_functions[0].name, "kodo_main");
}

#[test]
fn math_module_with_empty_config_produces_valid_output() {
    let resolver = Resolver::with_builtins();
    let intent = make_intent("math_module", vec![]);
    let result = resolver.resolve(&intent);
    assert!(result.is_ok());
    let resolved = result.unwrap();
    assert!(!resolved.description.is_empty());
}

#[test]
fn unknown_config_key_returns_error() {
    let resolver = Resolver::with_builtins();
    let intent = make_intent("console_app", vec![string_entry("invalid_key", "value")]);
    let result = resolver.resolve(&intent);
    assert!(matches!(result, Err(ResolverError::UnknownConfig { .. })));
}

#[test]
fn resolve_all_empty_list_succeeds() {
    let resolver = Resolver::with_builtins();
    let result = resolver.resolve_all(&[]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}
