//! Worker intent strategy.
//!
//! Generates a worker loop that calls a task function repeatedly for the
//! `worker` intent. Supports configurable max iterations and an optional
//! error handler.

use kodo_ast::{BinOp, Block, Expr, IntentDecl, Span, Stmt, TypeExpr};

use crate::helpers::{
    get_fn_ref_config, get_int_config, get_string_config, make_call, make_function, make_let,
    make_println,
};
use crate::{ResolvedIntent, ResolverStrategy, Result};

/// Generates a worker loop that calls a task function repeatedly.
///
/// Config keys:
/// - `task` (fn ref): Function to call each iteration.
/// - `max_iterations` (integer, optional): Maximum iterations. Default: `10`.
/// - `on_error` (fn ref, optional): Function to call when task returns non-zero.
pub(crate) struct WorkerStrategy;

impl ResolverStrategy for WorkerStrategy {
    fn handles(&self) -> &[&str] {
        &["worker"]
    }

    fn valid_keys(&self) -> &[&str] {
        &["task", "max_iterations", "on_error"]
    }

    fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
        let span = intent.span;
        let task = get_fn_ref_config(intent, "task")
            .or_else(|| get_string_config(intent, "task"))
            .unwrap_or("do_work");
        let max_iterations = get_int_config(intent, "max_iterations").unwrap_or(10);
        let on_error =
            get_fn_ref_config(intent, "on_error").or_else(|| get_string_config(intent, "on_error"));

        let main_func = generate_worker_main(task, max_iterations, on_error, span);

        let error_desc = if let Some(handler) = on_error {
            format!(", on_error=`{handler}()`")
        } else {
            String::new()
        };

        let description = format!(
            "Generated worker: task=`{task}()`, max_iterations={max_iterations}{error_desc}"
        );

        Ok(ResolvedIntent {
            generated_functions: vec![main_func],
            generated_types: vec![],
            description,
        })
    }
}

/// Generates `kodo_main()` with a worker loop.
fn generate_worker_main(
    task: &str,
    max_iterations: i64,
    on_error: Option<&str>,
    span: Span,
) -> kodo_ast::Function {
    let mut stmts = Vec::new();

    stmts.push(make_println(
        &format!("Worker starting ({max_iterations} iterations)"),
        span,
    ));

    // for i in 0..max_iterations { task_call }
    let mut loop_body = Vec::new();

    if let Some(error_handler) = on_error {
        // let status: Int = task()
        loop_body.push(make_let(
            "status",
            TypeExpr::Named("Int".to_string()),
            make_call(task, vec![], span),
            span,
        ));
        // if status != 0 { on_error() }
        let condition = Expr::BinaryOp {
            op: BinOp::Ne,
            left: Box::new(Expr::Ident("status".to_string(), span)),
            right: Box::new(Expr::IntLit(0, span)),
            span,
        };
        loop_body.push(Stmt::Expr(Expr::If {
            condition: Box::new(condition),
            then_branch: Block {
                span,
                stmts: vec![Stmt::Expr(make_call(error_handler, vec![], span))],
            },
            else_branch: None,
            span,
        }));
    } else {
        // Simple: just call task()
        loop_body.push(Stmt::Expr(make_call(task, vec![], span)));
    }

    stmts.push(Stmt::For {
        span,
        name: "i".to_string(),
        start: Expr::IntLit(0, span),
        end: Expr::IntLit(max_iterations, span),
        inclusive: false,
        body: Block {
            span,
            stmts: loop_body,
        },
    });

    stmts.push(make_println("Worker completed", span));
    stmts.push(Stmt::Return {
        span,
        value: Some(Expr::IntLit(0, span)),
    });

    make_function("kodo_main", TypeExpr::Named("Int".to_string()), stmts, span)
}
