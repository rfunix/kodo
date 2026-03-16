//! Database intent strategy.
//!
//! Generates typed database query functions with contracts for the `database`
//! intent. Produces a connection function, per-table query functions, and
//! named query stubs.

use kodo_ast::{
    Block, Expr, Function, IntentDecl, NodeId, Ownership, Param, Span, Stmt, TypeExpr, Visibility,
};

use crate::helpers::{get_string_config, get_string_list_config};
use crate::{ResolvedIntent, ResolverStrategy, Result};

/// Generates typed database query functions with contracts.
///
/// Config keys:
/// - `driver` (string): The database driver name (e.g., `"sqlite"`, `"postgres"`).
/// - `tables` (list): Table names for which accessor functions are generated.
/// - `queries` (list): Named query function stubs to generate.
///
/// Each table gets a `query_<table>` function with a contract requiring a non-empty
/// table name. Each named query gets a function stub with a contract.
pub(crate) struct DatabaseStrategy;

impl ResolverStrategy for DatabaseStrategy {
    fn handles(&self) -> &[&str] {
        &["database"]
    }

    fn valid_keys(&self) -> &[&str] {
        &["driver", "tables", "queries"]
    }

    fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
        let span = intent.span;
        let driver = get_string_config(intent, "driver").unwrap_or("sqlite");
        let tables = get_string_list_config(intent, "tables");
        let queries = get_string_list_config(intent, "queries");

        let mut generated = Vec::new();
        let mut descriptions = Vec::new();

        // Generate a connect function
        let connect_func = generate_db_connect(driver, span);
        descriptions.push(format!("  - `db_connect() -> String` (driver: {driver})"));
        generated.push(connect_func);

        // Generate query_<table> functions for each table
        for table in &tables {
            let func = generate_db_table_query(table, span);
            descriptions.push(format!("  - `query_{table}(id: Int) -> String`"));
            generated.push(func);
        }

        // Generate named query stubs
        for query in &queries {
            let func = generate_db_named_query(query, span);
            descriptions.push(format!("  - `{query}(id: Int) -> String`"));
            generated.push(func);
        }

        Ok(ResolvedIntent {
            generated_functions: generated,
            generated_types: tables.iter().map(|t| format!("{t}Row")).collect(),
            description: format!(
                "Generated database layer (driver: {driver}):\n{}",
                descriptions.join("\n")
            ),
        })
    }
}

/// Generates a database connection function stub.
pub(crate) fn generate_db_connect(driver: &str, span: Span) -> Function {
    let body_expr = Expr::StringLit(format!("connected:{driver}"), span);

    Function {
        id: NodeId(0),
        span,
        name: "db_connect".to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![],
        return_type: TypeExpr::Named("String".to_string()),
        requires: vec![],
        ensures: vec![],
        body: Block {
            span,
            stmts: vec![Stmt::Return {
                span,
                value: Some(body_expr),
            }],
        },
    }
}

/// Generates a table query function with a contract requiring a valid ID.
pub(crate) fn generate_db_table_query(table: &str, span: Span) -> Function {
    let func_name = format!("query_{table}");
    let body_expr = Expr::Call {
        callee: Box::new(Expr::Ident("println".to_string(), span)),
        args: vec![Expr::StringLit(format!("querying table: {table}"), span)],
        span,
    };

    // requires { id > 0 }
    let requires = vec![Expr::BinaryOp {
        left: Box::new(Expr::Ident("id".to_string(), span)),
        op: kodo_ast::BinOp::Gt,
        right: Box::new(Expr::IntLit(0, span)),
        span,
    }];

    Function {
        id: NodeId(0),
        span,
        name: func_name,
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![Param {
            name: "id".to_string(),
            ty: TypeExpr::Named("Int".to_string()),
            span,
            ownership: Ownership::Owned,
        }],
        return_type: TypeExpr::Named("String".to_string()),
        requires,
        ensures: vec![],
        body: Block {
            span,
            stmts: vec![Stmt::Expr(body_expr)],
        },
    }
}

/// Generates a named query function stub with a contract.
pub(crate) fn generate_db_named_query(name: &str, span: Span) -> Function {
    let body_expr = Expr::Call {
        callee: Box::new(Expr::Ident("println".to_string(), span)),
        args: vec![Expr::StringLit(format!("executing query: {name}"), span)],
        span,
    };

    // requires { id > 0 }
    let requires = vec![Expr::BinaryOp {
        left: Box::new(Expr::Ident("id".to_string(), span)),
        op: kodo_ast::BinOp::Gt,
        right: Box::new(Expr::IntLit(0, span)),
        span,
    }];

    Function {
        id: NodeId(0),
        span,
        name: name.to_string(),
        visibility: Visibility::Private,
        is_async: false,
        generic_params: vec![],
        annotations: vec![],
        params: vec![Param {
            name: "id".to_string(),
            ty: TypeExpr::Named("Int".to_string()),
            span,
            ownership: Ownership::Owned,
        }],
        return_type: TypeExpr::Named("String".to_string()),
        requires,
        ensures: vec![],
        body: Block {
            span,
            stmts: vec![Stmt::Expr(body_expr)],
        },
    }
}
