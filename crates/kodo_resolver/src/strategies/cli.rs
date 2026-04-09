//! CLI tool intent strategy.
//!
//! Generates a CLI tool with command dispatch using `args()` and `exit()`.
//! Produces a `cli_help()` function and a `kodo_main()` function with
//! argument parsing and command dispatch via if-else chains.

use kodo_ast::{BinOp, Expr, IntentDecl, Span, Stmt, TypeExpr};

use crate::helpers::{
    config_value_as_str, get_nested_list_config, get_string_config, make_call, make_function,
    make_if_chain, make_let, make_println,
};
use crate::{ResolvedIntent, ResolverStrategy, Result};

/// Generates a CLI tool with command dispatch using `args()` and `exit()`.
///
/// Config keys:
/// - `name` (string, optional): Tool name for help text. Default: `"tool"`.
/// - `version` (string, optional): Version string. Default: `"0.1.0"`.
/// - `commands` (list of lists): Each sub-list is `[command_name, handler_fn, description]`.
pub(crate) struct CliStrategy;

impl ResolverStrategy for CliStrategy {
    fn handles(&self) -> &[&str] {
        &["cli"]
    }

    fn valid_keys(&self) -> &[&str] {
        &["name", "version", "commands"]
    }

    fn resolve(&self, intent: &IntentDecl) -> Result<ResolvedIntent> {
        let span = intent.span;
        let name = get_string_config(intent, "name").unwrap_or("tool");
        let version = get_string_config(intent, "version").unwrap_or("0.1.0");
        let commands = get_nested_list_config(intent, "commands");

        // Parse commands: each is [name_str, handler_fn_ref, description_str]
        let mut cmd_entries: Vec<(String, String, String)> = Vec::new();
        for cmd in &commands {
            if cmd.len() >= 2 {
                let cmd_name = config_value_as_str(&cmd[0])
                    .unwrap_or("unknown")
                    .to_string();
                let handler = config_value_as_str(&cmd[1])
                    .unwrap_or("unknown")
                    .to_string();
                let desc = if cmd.len() >= 3 {
                    config_value_as_str(&cmd[2]).unwrap_or("").to_string()
                } else {
                    String::new()
                };
                cmd_entries.push((cmd_name, handler, desc));
            }
        }

        let mut generated = Vec::new();

        // Generate cli_help() function
        let help_func = generate_cli_help(name, version, &cmd_entries, span);
        generated.push(help_func);

        // Generate kodo_main() with dispatch
        let main_func = generate_cli_main(&cmd_entries, span);
        generated.push(main_func);

        let cmd_desc: Vec<String> = cmd_entries
            .iter()
            .map(|(n, h, d)| format!("  - `{n}` → `{h}()` — {d}"))
            .collect();

        let description = format!(
            "Generated CLI tool `{name}` v{version} with {} commands:\n{}",
            cmd_entries.len(),
            cmd_desc.join("\n")
        );

        Ok(ResolvedIntent {
            generated_functions: generated,
            generated_types: vec![],
            description,
        })
    }
}

/// Generates the `cli_help()` function that prints usage info.
fn generate_cli_help(
    name: &str,
    version: &str,
    commands: &[(String, String, String)],
    span: Span,
) -> kodo_ast::Function {
    let mut stmts = Vec::new();
    stmts.push(make_println(&format!("{name} v{version}"), span));
    stmts.push(make_println("", span));
    stmts.push(make_println("Commands:", span));
    for (cmd_name, _, desc) in commands {
        stmts.push(make_println(&format!("  {cmd_name}  {desc}"), span));
    }
    stmts.push(make_println("  help  Show this help message", span));

    make_function("cli_help", TypeExpr::Unit, stmts, span)
}

/// Generates the `kodo_main()` function with arg parsing and command dispatch.
fn generate_cli_main(commands: &[(String, String, String)], span: Span) -> kodo_ast::Function {
    let mut stmts = Vec::new();

    // let cmd: String = "help"
    stmts.push(make_let(
        "cmd",
        TypeExpr::Named("String".to_string()),
        Expr::StringLit("help".to_string(), span),
        span,
    ));

    // Build if-else chain for each command
    let mut branches: Vec<(Expr, Vec<Stmt>)> = Vec::new();
    for (cmd_name, handler, _) in commands {
        let condition = Expr::BinaryOp {
            op: BinOp::Eq,
            left: Box::new(Expr::Ident("cmd".to_string(), span)),
            right: Box::new(Expr::StringLit(cmd_name.clone(), span)),
            span,
        };
        let body = vec![Stmt::Expr(make_call(handler, vec![], span))];
        branches.push((condition, body));
    }

    // Add "help" command
    let help_condition = Expr::BinaryOp {
        op: BinOp::Eq,
        left: Box::new(Expr::Ident("cmd".to_string(), span)),
        right: Box::new(Expr::StringLit("help".to_string(), span)),
        span,
    };
    branches.push((
        help_condition,
        vec![Stmt::Expr(make_call("cli_help", vec![], span))],
    ));

    // Else: unknown command
    let else_body = vec![
        make_println("Unknown command", span),
        Stmt::Expr(make_call("cli_help", vec![], span)),
        Stmt::Expr(make_call("exit", vec![Expr::IntLit(1, span)], span)),
    ];

    stmts.push(Stmt::Expr(make_if_chain(branches, &else_body, span)));
    stmts.push(Stmt::Return {
        span,
        value: Some(Expr::IntLit(0, span)),
    });

    make_function("kodo_main", TypeExpr::Named("Int".to_string()), stmts, span)
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

    fn nested_list_entry(key: &str, rows: &[(&str, &str, &str)]) -> IntentConfigEntry {
        let vals = rows
            .iter()
            .map(|(cmd, handler, desc)| {
                IntentConfigValue::List(
                    vec![
                        IntentConfigValue::StringLit(cmd.to_string(), dummy_span()),
                        IntentConfigValue::StringLit(handler.to_string(), dummy_span()),
                        IntentConfigValue::StringLit(desc.to_string(), dummy_span()),
                    ],
                    dummy_span(),
                )
            })
            .collect();
        IntentConfigEntry {
            key: key.to_string(),
            value: IntentConfigValue::List(vals, dummy_span()),
            span: dummy_span(),
        }
    }

    #[test]
    fn cli_strategy_resolves_with_defaults() {
        let intent = make_intent("cli", vec![]);
        let result = CliStrategy.resolve(&intent);
        assert!(result.is_ok());
        let resolved = result.unwrap();
        // Always generates cli_help + kodo_main
        assert_eq!(resolved.generated_functions.len(), 2);
        assert!(resolved.description.contains("tool"));
        assert!(resolved.description.contains("0.1.0"));
    }

    #[test]
    fn cli_strategy_custom_name_and_version() {
        let intent = make_intent(
            "cli",
            vec![str_entry("name", "mycli"), str_entry("version", "2.0.0")],
        );
        let resolved = CliStrategy.resolve(&intent).unwrap();
        assert!(resolved.description.contains("mycli"));
        assert!(resolved.description.contains("2.0.0"));
    }

    #[test]
    fn cli_strategy_with_commands() {
        let intent = make_intent(
            "cli",
            vec![nested_list_entry(
                "commands",
                &[
                    ("build", "run_build", "Build the project"),
                    ("test", "run_tests", "Run all tests"),
                ],
            )],
        );
        let resolved = CliStrategy.resolve(&intent).unwrap();
        assert_eq!(resolved.generated_functions.len(), 2);
        assert!(resolved.description.contains("build"));
        assert!(resolved.description.contains("run_build"));
        assert!(resolved.description.contains("test"));
    }

    #[test]
    fn cli_strategy_generates_cli_help_and_main() {
        let intent = make_intent("cli", vec![]);
        let resolved = CliStrategy.resolve(&intent).unwrap();
        let names: Vec<&str> = resolved
            .generated_functions
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        assert!(names.contains(&"cli_help"));
        assert!(names.contains(&"kodo_main"));
    }

    #[test]
    fn cli_strategy_handles_cli_intent() {
        assert!(CliStrategy.handles().contains(&"cli"));
    }

    #[test]
    fn cli_strategy_valid_keys() {
        let keys = CliStrategy.valid_keys();
        assert!(keys.contains(&"name"));
        assert!(keys.contains(&"version"));
        assert!(keys.contains(&"commands"));
    }
}
