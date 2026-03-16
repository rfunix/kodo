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
