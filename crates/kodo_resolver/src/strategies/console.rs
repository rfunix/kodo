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
