//! Code actions (quick fixes) provider for the Kōdo LSP server.
//!
//! Provides code actions such as "Add missing contract" for functions
//! without `requires`/`ensures` clauses, "Add type annotation" for
//! untyped `let` bindings, and machine-applicable fix patches from
//! the type checker.

use std::collections::HashMap;

#[allow(clippy::wildcard_imports)]
use tower_lsp::lsp_types::*;

use crate::utils::{format_type_expr, infer_type_hint, offset_to_line_col};

/// Returns code actions available for the given range.
///
/// Currently provides two code actions:
/// - "Add missing contract" for functions without `requires`/`ensures` clauses
/// - "Add type annotation" for `let` bindings without explicit type annotations
pub(crate) fn code_actions_for_source(
    source: &str,
    uri: &Url,
    range: &Range,
) -> CodeActionResponse {
    let Ok(module) = kodo_parser::parse(source) else {
        return Vec::new();
    };

    let mut actions: CodeActionResponse = Vec::new();

    // Code action: "Add missing contract" for functions without contracts
    for func in &module.functions {
        let (func_line, _) = offset_to_line_col(source, func.span.start);
        let (func_end_line, _) = offset_to_line_col(source, func.span.end);

        // Check if the cursor range overlaps a function without contracts
        if range.start.line <= func_end_line
            && range.end.line >= func_line
            && func.requires.is_empty()
            && func.ensures.is_empty()
        {
            // Build the contract text to insert before the function body
            let params_str: Vec<String> = func
                .params
                .iter()
                .map(|p| format!("{}: {}", p.name, format_type_expr(&p.ty)))
                .collect();
            let ret_str = format_type_expr(&func.return_type);

            let contract_text = format!(
                "\n        requires {{ /* precondition for {}({}) */ true }}\
                     \n        ensures {{ /* postcondition -> {} */ true }}",
                func.name,
                params_str.join(", "),
                ret_str,
            );

            // Find the position right before the opening brace of the body
            let body_start = func.body.span.start;
            let (insert_line, insert_col) = offset_to_line_col(source, body_start);

            let mut changes = HashMap::new();
            changes.insert(
                uri.clone(),
                vec![TextEdit {
                    range: Range::new(
                        Position::new(insert_line, insert_col),
                        Position::new(insert_line, insert_col),
                    ),
                    new_text: contract_text,
                }],
            );

            actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: format!("Add missing contract for `{}`", func.name),
                kind: Some(CodeActionKind::QUICKFIX),
                edit: Some(WorkspaceEdit {
                    changes: Some(changes),
                    ..Default::default()
                }),
                ..Default::default()
            }));
        }
    }

    // Code actions from FixPatch: type-check errors with machine-applicable patches.
    add_fix_patch_actions(source, &module, uri, range, &mut actions);

    // Code action: "Add type annotation" for let bindings without explicit type
    for func in &module.functions {
        for stmt in &func.body.stmts {
            if let kodo_ast::Stmt::Let {
                span,
                name,
                ty: None,
                value,
                ..
            } = stmt
            {
                let (let_line, _) = offset_to_line_col(source, span.start);
                let (let_end_line, _) = offset_to_line_col(source, span.end);

                if range.start.line <= let_end_line && range.end.line >= let_line {
                    // Infer a type hint from the value expression
                    let inferred = infer_type_hint(value);

                    // Find position right after the variable name to insert `: Type`
                    // Look for the name in the source around the let statement
                    #[allow(clippy::cast_possible_truncation)]
                    let source_len_u32 = source.len() as u32;
                    let let_source =
                        &source[span.start as usize..span.end.min(source_len_u32) as usize];
                    if let Some(name_pos) = let_source.find(name.as_str()) {
                        #[allow(clippy::cast_possible_truncation)]
                        let insert_offset = span.start + name_pos as u32 + name.len() as u32;
                        let (il, ic) = offset_to_line_col(source, insert_offset);

                        let mut changes = HashMap::new();
                        changes.insert(
                            uri.clone(),
                            vec![TextEdit {
                                range: Range::new(Position::new(il, ic), Position::new(il, ic)),
                                new_text: format!(": {inferred}"),
                            }],
                        );

                        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                            title: format!("Add type annotation for `{name}`"),
                            kind: Some(CodeActionKind::QUICKFIX),
                            edit: Some(WorkspaceEdit {
                                changes: Some(changes),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }));
                    }
                }
            }
        }
    }

    actions
}

/// Adds code actions from type error fix patches.
///
/// Runs the type checker on the module and converts any [`kodo_ast::FixPatch`] results
/// into LSP `CodeAction` entries with `TextEdit` replacements.
fn add_fix_patch_actions(
    source: &str,
    module: &kodo_ast::Module,
    uri: &Url,
    range: &Range,
    actions: &mut CodeActionResponse,
) {
    let mut checker = kodo_types::TypeChecker::new();
    let type_errors = checker.check_module_collecting(module);
    for err in &type_errors {
        if let Some(patch) = kodo_ast::Diagnostic::fix_patch(err) {
            let (start_line, start_col) =
                offset_to_line_col(source, u32::try_from(patch.start_offset).unwrap_or(0));
            let (end_line, end_col) =
                offset_to_line_col(source, u32::try_from(patch.end_offset).unwrap_or(0));

            let patch_range = Range::new(
                Position::new(start_line, start_col),
                Position::new(end_line, end_col),
            );

            // Only include if the action is relevant to the requested range.
            if range.start.line <= end_line && range.end.line >= start_line {
                let mut changes = HashMap::new();
                changes.insert(
                    uri.clone(),
                    vec![TextEdit {
                        range: patch_range,
                        new_text: patch.replacement.clone(),
                    }],
                );

                actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: patch.description.clone(),
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![Diagnostic {
                        range: patch_range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: Some(NumberOrString::String(
                            kodo_ast::Diagnostic::code(err).to_string(),
                        )),
                        source: Some("kodo".to_string()),
                        message: err.to_string(),
                        ..Default::default()
                    }]),
                    edit: Some(WorkspaceEdit {
                        changes: Some(changes),
                        ..Default::default()
                    }),
                    ..Default::default()
                }));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{Position, Range, Url};

    #[test]
    fn code_action_for_type_error_via_fix_patch() {
        // Source without meta block — triggers MissingMeta type error with FixPatch
        let source = r#"module test {
    fn add(a: Int, b: Int) -> Int {
        return a + b
    }
}"#;
        let uri = Url::parse("file:///test.ko").unwrap();
        let full_range = Range::new(Position::new(0, 0), Position::new(4, 1));
        let actions = code_actions_for_source(source, &uri, &full_range);
        let fix_patch_actions: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                CodeActionOrCommand::CodeAction(ca) => {
                    if ca.diagnostics.is_some() {
                        Some(ca)
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();
        assert!(
            !fix_patch_actions.is_empty(),
            "should generate code actions from fix patches for missing meta"
        );
        for action in &fix_patch_actions {
            assert!(
                action.edit.is_some(),
                "fix patch action should have an edit"
            );
            assert_eq!(action.kind, Some(CodeActionKind::QUICKFIX));
        }
    }

    #[test]
    fn code_action_for_missing_contract() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn add(a: Int, b: Int) -> Int {
        return a + b
    }
}"#;
        let uri = Url::parse("file:///test.ko").unwrap();
        let range = Range::new(Position::new(6, 0), Position::new(8, 0));
        let actions = code_actions_for_source(source, &uri, &range);

        let contract_actions: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                CodeActionOrCommand::CodeAction(ca) => {
                    if ca.title.contains("Add missing contract") {
                        Some(ca)
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();

        assert!(
            !contract_actions.is_empty(),
            "should suggest adding contract for function without contracts"
        );
        assert!(contract_actions[0].title.contains("add"));
        assert!(contract_actions[0].edit.is_some());
    }

    #[test]
    fn no_contract_action_when_contracts_exist() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn divide(a: Int, b: Int) -> Int
        requires { b > 0 }
    {
        return a / b
    }
}"#;
        let uri = Url::parse("file:///test.ko").unwrap();
        let range = Range::new(Position::new(6, 0), Position::new(10, 0));
        let actions = code_actions_for_source(source, &uri, &range);

        let contract_actions: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                CodeActionOrCommand::CodeAction(ca) => {
                    if ca.title.contains("Add missing contract") {
                        Some(ca)
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();

        assert!(
            contract_actions.is_empty(),
            "should NOT suggest contract action when contracts already exist"
        );
    }

    #[test]
    fn code_action_add_type_annotation_for_untyped_let() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn main() {
        let x = 42
    }
}"#;
        let uri = Url::parse("file:///test.ko").unwrap();
        let range = Range::new(Position::new(7, 0), Position::new(7, 20));
        let actions = code_actions_for_source(source, &uri, &range);

        let type_actions: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                CodeActionOrCommand::CodeAction(ca) => {
                    if ca.title.contains("Add type annotation") {
                        Some(ca)
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();

        assert!(
            !type_actions.is_empty(),
            "should suggest adding type annotation for untyped let"
        );
        assert!(type_actions[0].title.contains("x"));
    }

    #[test]
    fn no_type_annotation_action_when_typed() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }

    fn main() {
        let x: Int = 42
    }
}"#;
        let uri = Url::parse("file:///test.ko").unwrap();
        let range = Range::new(Position::new(7, 0), Position::new(7, 20));
        let actions = code_actions_for_source(source, &uri, &range);

        let type_actions: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                CodeActionOrCommand::CodeAction(ca) => {
                    if ca.title.contains("Add type annotation") {
                        Some(ca)
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();

        assert!(
            type_actions.is_empty(),
            "should NOT suggest type annotation when type already present"
        );
    }

    #[test]
    fn code_actions_empty_for_range_outside_functions() {
        let source = r#"module test {
    meta {
        purpose: "test",
        version: "1.0.0"
    }
}"#;
        let uri = Url::parse("file:///test.ko").unwrap();
        let range = Range::new(Position::new(0, 0), Position::new(0, 10));
        let actions = code_actions_for_source(source, &uri, &range);
        // No functions means no contract or type annotation actions
        // (there may be fix_patch actions for meta-level errors though)
        let contract_actions: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                CodeActionOrCommand::CodeAction(ca) => {
                    if ca.title.contains("Add missing contract")
                        || ca.title.contains("Add type annotation")
                    {
                        Some(ca)
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();
        assert!(
            contract_actions.is_empty(),
            "no contract/type annotation actions outside functions"
        );
    }
}
