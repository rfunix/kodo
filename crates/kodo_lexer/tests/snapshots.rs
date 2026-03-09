//! Snapshot tests for the Kōdo lexer.
//!
//! Uses `insta` to capture token stream output. Any change to the lexer's
//! output will be flagged as a snapshot diff.

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

/// Formats tokens into a stable string for snapshot comparison.
fn format_tokens(source: &str) -> String {
    let tokens = kodo_lexer::tokenize(source).unwrap();
    tokens
        .iter()
        .map(|t| format!("{:?} @ {}..{}", t.kind, t.span.start, t.span.end))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn snapshot_hello() {
    let source = read_fixture("valid/hello.ko");
    insta::assert_snapshot!(format_tokens(&source));
}

#[test]
fn snapshot_minimal() {
    let source = read_fixture("valid/minimal.ko");
    insta::assert_snapshot!(format_tokens(&source));
}

#[test]
fn snapshot_expressions() {
    let source = read_fixture("valid/expressions.ko");
    insta::assert_snapshot!(format_tokens(&source));
}

#[test]
fn snapshot_contracts() {
    let source = read_fixture("valid/contracts.ko");
    insta::assert_snapshot!(format_tokens(&source));
}
