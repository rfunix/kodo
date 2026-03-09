//! Snapshot tests for the Kōdo type checker error messages.
//!
//! Uses `insta` to capture error message output for invalid fixtures.
//! Any change to error messages will be flagged as a snapshot diff.

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
fn snapshot_type_error_return_mismatch() {
    let source = read_fixture("invalid/type_error_return.ko");
    let module = kodo_parser::parse(&source).unwrap();
    let mut checker = kodo_types::TypeChecker::new();
    let err = checker.check_module(&module).unwrap_err();
    insta::assert_snapshot!(err.to_string());
}

#[test]
fn snapshot_type_error_undefined_var() {
    let source = read_fixture("invalid/undefined_var.ko");
    let module = kodo_parser::parse(&source).unwrap();
    let mut checker = kodo_types::TypeChecker::new();
    let err = checker.check_module(&module).unwrap_err();
    insta::assert_snapshot!(err.to_string());
}
