//! Console application intent strategy.
//!
//! Generates a `kodo_main` function for console applications that prints
//! a greeting message.

use kodo_ast::{Block, Expr, Function, IntentDecl, NodeId, Stmt, TypeExpr, Visibility};

use crate::helpers::get_string_config;
use crate::{ResolvedIntent, ResolverStrategy, Result};

/// Generates a `kodo_main` function for console applications.
///
/// Config keys:
/// - `greeting` (string, optional): The message to print. Default: `"Hello from Kōdo!"`.
/// - `entry_point` (string, optional): Name of the entry point function. Default: `"kodo_main"`.
pub(crate) struct ConsoleAppStrategy;

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

#[cfg(test)]
mod tests {
    use super::*;
    use kodo_ast::{IntentConfigEntry, IntentConfigValue, NodeId, Span};

    fn dummy_span() -> Span {
        Span::new(0, 0)
    }

    fn make_intent(name: &str, config: Vec<IntentConfigEntry>) -> IntentDecl {
        IntentDecl {
            id: NodeId(0),
            span: dummy_span(),
            name: name.to_string(),
            config,
        }
    }

    fn str_entry(key: &str, val: &str) -> IntentConfigEntry {
        IntentConfigEntry {
            key: key.to_string(),
            value: IntentConfigValue::StringLit(val.to_string(), dummy_span()),
            span: dummy_span(),
        }
    }

    #[test]
    fn console_app_resolves_with_defaults() {
        let intent = make_intent("console_app", vec![]);
        let result = ConsoleAppStrategy.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(resolved.generated_functions.len(), 1);
        assert_eq!(resolved.generated_functions[0].name, "kodo_main");
        assert!(resolved.description.contains("Hello from Kōdo!"));
    }

    #[test]
    fn console_app_resolves_with_custom_greeting() {
        let intent = make_intent("console_app", vec![str_entry("greeting", "Olá Mundo!")]);
        let resolved = ConsoleAppStrategy.resolve(&intent).unwrap();
        assert!(resolved.description.contains("Olá Mundo!"));
    }

    #[test]
    fn console_app_resolves_with_custom_entry_point() {
        let intent = make_intent("console_app", vec![str_entry("entry_point", "run_app")]);
        let resolved = ConsoleAppStrategy.resolve(&intent).unwrap();
        assert_eq!(resolved.generated_functions[0].name, "run_app");
    }

    #[test]
    fn console_app_strategy_handles_console_app_intent() {
        assert!(ConsoleAppStrategy.handles().contains(&"console_app"));
    }

    #[test]
    fn console_app_strategy_valid_keys() {
        let keys = ConsoleAppStrategy.valid_keys();
        assert!(keys.contains(&"greeting"));
        assert!(keys.contains(&"entry_point"));
    }
}
