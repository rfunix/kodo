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

    fn list_entry(key: &str, items: &[&str]) -> IntentConfigEntry {
        let vals = items
            .iter()
            .map(|s| IntentConfigValue::StringLit(s.to_string(), dummy_span()))
            .collect();
        IntentConfigEntry {
            key: key.to_string(),
            value: IntentConfigValue::List(vals, dummy_span()),
            span: dummy_span(),
        }
    }

    #[test]
    fn db_connect_has_driver_in_body() {
        let func = generate_db_connect("postgres", dummy_span());
        assert_eq!(func.name, "db_connect");
        // body returns a string literal containing the driver name
        if let kodo_ast::Stmt::Return {
            value: Some(kodo_ast::Expr::StringLit(s, _)),
            ..
        } = &func.body.stmts[0]
        {
            assert!(s.contains("postgres"));
        } else {
            panic!("Expected return with string literal");
        }
    }

    #[test]
    fn db_table_query_has_correct_name_and_contract() {
        let func = generate_db_table_query("users", dummy_span());
        assert_eq!(func.name, "query_users");
        assert_eq!(func.params.len(), 1);
        assert_eq!(func.params[0].name, "id");
        assert_eq!(func.requires.len(), 1, "Should require id > 0");
    }

    #[test]
    fn db_named_query_has_requires_clause() {
        let func = generate_db_named_query("find_active_users", dummy_span());
        assert_eq!(func.name, "find_active_users");
        assert_eq!(func.requires.len(), 1);
    }

    #[test]
    fn database_strategy_with_tables_and_queries() {
        let intent = make_intent(
            "database",
            vec![
                str_entry("driver", "sqlite"),
                list_entry("tables", &["users", "posts"]),
                list_entry("queries", &["find_admin"]),
            ],
        );
        let result = DatabaseStrategy.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap();
        // 1 connect + 2 table queries + 1 named query = 4
        assert_eq!(resolved.generated_functions.len(), 4);
        assert!(resolved.description.contains("sqlite"));
        // generated_types should include Row types for each table
        assert_eq!(resolved.generated_types.len(), 2);
        assert!(resolved.generated_types.contains(&"usersRow".to_string()));
    }

    #[test]
    fn database_strategy_defaults_to_sqlite() {
        let intent = make_intent("database", vec![]);
        let result = DatabaseStrategy.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap();
        // just the connect function
        assert_eq!(resolved.generated_functions.len(), 1);
        assert!(resolved.description.contains("sqlite"));
    }

    #[test]
    fn database_strategy_handles_database_intent() {
        assert!(DatabaseStrategy.handles().contains(&"database"));
    }

    #[test]
    fn database_strategy_valid_keys() {
        let keys = DatabaseStrategy.valid_keys();
        assert!(keys.contains(&"driver"));
        assert!(keys.contains(&"tables"));
        assert!(keys.contains(&"queries"));
    }
}
