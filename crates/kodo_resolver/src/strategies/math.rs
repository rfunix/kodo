//! Math module intent strategy.
//!
//! Generates mathematical helper functions (add, sub, mul, `safe_div`) from
//! intent declarations. Each generated function takes two `Int` parameters
//! and returns an `Int`.

use kodo_ast::{
    Block, Expr, Function, IntentConfigValue, IntentDecl, NodeId, Ownership, Param, Span, Stmt,
    TypeExpr, Visibility,
};

use crate::{ResolvedIntent, ResolverStrategy, Result};

/// Generates mathematical helper functions from intent declarations.
///
/// Config keys:
/// - `functions` (list of fn refs): Names of functions to generate wrappers for.
pub(crate) struct MathModuleStrategy;

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
pub(crate) fn generate_math_function(name: &str, span: Span) -> Option<Function> {
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

    fn fnref_list_entry(key: &str, names: &[&str]) -> IntentConfigEntry {
        let vals = names
            .iter()
            .map(|n| IntentConfigValue::FnRef(n.to_string(), dummy_span()))
            .collect();
        IntentConfigEntry {
            key: key.to_string(),
            value: IntentConfigValue::List(vals, dummy_span()),
            span: dummy_span(),
        }
    }

    #[test]
    fn generate_math_function_add() {
        let func = generate_math_function("add", dummy_span());
        assert!(func.is_some());
        let f = func.unwrap();
        assert_eq!(f.name, "add");
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.requires.len(), 0);
    }

    #[test]
    fn generate_math_function_safe_div_has_contract() {
        let func = generate_math_function("safe_div", dummy_span()).unwrap();
        assert_eq!(
            func.requires.len(),
            1,
            "safe_div should have requires {{ b != 0 }}"
        );
    }

    #[test]
    fn generate_math_function_unknown_returns_none() {
        let func = generate_math_function("unknown_op", dummy_span());
        assert!(func.is_none());
    }

    #[test]
    fn math_module_strategy_generates_named_functions() {
        let intent = make_intent(
            "math_module",
            vec![fnref_list_entry("functions", &["add", "sub", "mul"])],
        );
        let result = MathModuleStrategy.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(resolved.generated_functions.len(), 3);
        assert!(resolved.description.contains("add"));
    }

    #[test]
    fn math_module_strategy_empty_config() {
        let intent = make_intent("math_module", vec![]);
        let result = MathModuleStrategy.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert_eq!(resolved.generated_functions.len(), 0);
        assert!(resolved.description.contains("No math"));
    }

    #[test]
    fn math_module_strategy_skips_unknown_function_names() {
        let intent = make_intent(
            "math_module",
            vec![fnref_list_entry("functions", &["add", "unknown_fn"])],
        );
        let resolved = MathModuleStrategy.resolve(&intent).unwrap();
        // only "add" should be generated
        assert_eq!(resolved.generated_functions.len(), 1);
    }

    #[test]
    fn math_module_strategy_handles_math_module_intent() {
        assert!(MathModuleStrategy.handles().contains(&"math_module"));
    }
}
