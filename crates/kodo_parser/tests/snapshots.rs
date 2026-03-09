//! Snapshot tests for the Kōdo parser.
//!
//! Uses `insta` to capture AST debug output. Any change to the AST
//! structure will be flagged as a snapshot diff.

use std::path::Path;

/// Helper to read a fixture file from the workspace tests directory.
fn read_fixture(relative_path: &str) -> String {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let fixture_path = workspace_root.join("tests/fixtures").join(relative_path);
    std::fs::read_to_string(&fixture_path)
        .unwrap_or_else(|e| panic!("could not read fixture {}: {e}", fixture_path.display()))
}

#[test]
fn snapshot_ast_hello() {
    let source = read_fixture("valid/hello.ko");
    let module = kodo_parser::parse(&source).unwrap();
    insta::assert_debug_snapshot!(module);
}

#[test]
fn snapshot_ast_minimal() {
    let source = read_fixture("valid/minimal.ko");
    let module = kodo_parser::parse(&source).unwrap();
    insta::assert_debug_snapshot!(module);
}

#[test]
fn snapshot_ast_expressions() {
    let source = read_fixture("valid/expressions.ko");
    let module = kodo_parser::parse(&source).unwrap();
    insta::assert_debug_snapshot!(module);
}

#[test]
fn snapshot_ast_contracts() {
    let source = read_fixture("valid/contracts.ko");
    let module = kodo_parser::parse(&source).unwrap();
    insta::assert_debug_snapshot!(module);
}
