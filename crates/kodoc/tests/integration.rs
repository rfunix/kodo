//! Integration tests for the kodoc compiler pipeline.
//!
//! These tests exercise the full compilation pipeline from source text
//! through parsing and (eventually) code generation.

use std::path::Path;

/// Helper to read a fixture file from the tests directory.
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
fn parse_valid_hello() {
    let source = read_fixture("valid/hello.ko");
    let result = kodo_parser::parse(&source);
    assert!(result.is_ok(), "failed to parse hello.ko: {result:?}");
    let module = result.unwrap();
    assert_eq!(module.name, "hello");
    assert!(module.meta.is_some());
    assert!(!module.functions.is_empty());
}

#[test]
fn parse_invalid_missing_meta_still_parses() {
    // missing_meta.ko is a module without a meta block — this should parse
    // fine since meta is optional. The error would come from a later
    // semantic analysis pass that enforces mandatory meta blocks.
    let source = read_fixture("invalid/missing_meta.ko");
    let result = kodo_parser::parse(&source);
    assert!(
        result.is_ok(),
        "failed to parse missing_meta.ko: {result:?}"
    );
    let module = result.unwrap();
    assert!(module.meta.is_none());
}

#[test]
fn lex_valid_hello() {
    let source = read_fixture("valid/hello.ko");
    let tokens = kodo_lexer::tokenize(&source);
    assert!(tokens.is_ok(), "failed to tokenize hello.ko: {tokens:?}");
    assert!(!tokens.unwrap().is_empty());
}
