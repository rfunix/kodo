//! UI tests that verify error messages match expected error code annotations.
//!
//! Each `.ko` file in `tests/fixtures/invalid/` can contain `// ERROR Exxxx` comments.
//! This test runner compiles each file and verifies that the expected error code appears
//! in the structured JSON output.

use std::path::Path;
use std::process::Command;

fn get_kodoc_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_kodoc"))
}

fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root")
        .to_path_buf()
}

/// Extracts expected error codes from `// ERROR Exxxx` comments in source.
fn extract_expected_errors(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| {
            if let Some(comment_start) = line.find("// ERROR ") {
                let code = line[comment_start + 9..].trim().to_string();
                Some(code)
            } else {
                None
            }
        })
        .collect()
}

/// Runs kodoc check with --json-errors on a fixture file and returns stdout.
fn run_check_json(fixture_relative: &str) -> (bool, String) {
    let root = workspace_root();
    let fixture = root.join("tests/fixtures").join(fixture_relative);

    let output = Command::new(get_kodoc_path())
        .args(["check", fixture.to_str().unwrap(), "--json-errors"])
        .output()
        .expect("failed to run kodoc");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    (output.status.success(), stdout)
}

#[test]
fn ui_test_syntax_error() {
    let root = workspace_root();
    let source =
        std::fs::read_to_string(root.join("tests/fixtures/invalid/syntax_error.ko")).unwrap();
    let expected = extract_expected_errors(&source);
    assert!(!expected.is_empty(), "no ERROR annotations found");

    let (success, stdout) = run_check_json("invalid/syntax_error.ko");
    assert!(!success, "expected kodoc to fail for syntax_error.ko");
    for code in &expected {
        assert!(
            stdout.contains(code),
            "Expected error code {code} in output but got: {stdout}"
        );
    }
}

#[test]
fn ui_test_type_error_return() {
    let root = workspace_root();
    let source =
        std::fs::read_to_string(root.join("tests/fixtures/invalid/type_error_return.ko")).unwrap();
    let expected = extract_expected_errors(&source);
    assert!(!expected.is_empty(), "no ERROR annotations found");

    let (success, stdout) = run_check_json("invalid/type_error_return.ko");
    assert!(!success, "expected kodoc to fail for type_error_return.ko");
    for code in &expected {
        assert!(
            stdout.contains(code),
            "Expected error code {code} in output but got: {stdout}"
        );
    }
}

#[test]
fn ui_test_undefined_var() {
    let root = workspace_root();
    let source =
        std::fs::read_to_string(root.join("tests/fixtures/invalid/undefined_var.ko")).unwrap();
    let expected = extract_expected_errors(&source);
    assert!(!expected.is_empty(), "no ERROR annotations found");

    let (success, stdout) = run_check_json("invalid/undefined_var.ko");
    assert!(!success, "expected kodoc to fail for undefined_var.ko");
    for code in &expected {
        assert!(
            stdout.contains(code),
            "Expected error code {code} in output but got: {stdout}"
        );
    }
}

#[test]
fn ui_test_missing_meta() {
    let root = workspace_root();
    let source =
        std::fs::read_to_string(root.join("tests/fixtures/invalid/missing_meta.ko")).unwrap();
    let expected = extract_expected_errors(&source);
    assert!(!expected.is_empty(), "no ERROR annotations found");

    let (success, stdout) = run_check_json("invalid/missing_meta.ko");
    assert!(!success, "expected kodoc to fail for missing_meta.ko");
    for code in &expected {
        assert!(
            stdout.contains(code),
            "Expected error code {code} in output but got: {stdout}"
        );
    }
}

#[test]
fn ui_test_empty_purpose() {
    let root = workspace_root();
    let source =
        std::fs::read_to_string(root.join("tests/fixtures/invalid/empty_purpose.ko")).unwrap();
    let expected = extract_expected_errors(&source);
    assert!(!expected.is_empty(), "no ERROR annotations found");

    let (success, stdout) = run_check_json("invalid/empty_purpose.ko");
    assert!(!success, "expected kodoc to fail for empty_purpose.ko");
    for code in &expected {
        assert!(
            stdout.contains(code),
            "Expected error code {code} in output but got: {stdout}"
        );
    }
}

#[test]
fn ui_test_explain_known_code() {
    let output = Command::new(get_kodoc_path())
        .args(["explain", "E0200"])
        .output()
        .expect("failed to run kodoc");

    assert!(output.status.success(), "kodoc explain E0200 should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Type Mismatch"),
        "expected 'Type Mismatch' in explain output"
    );
    assert!(
        stdout.contains("```kodo"),
        "expected code examples in explain output"
    );
}

#[test]
fn ui_test_explain_unknown_code() {
    let output = Command::new(get_kodoc_path())
        .args(["explain", "E9999"])
        .output()
        .expect("failed to run kodoc");

    assert!(
        !output.status.success(),
        "kodoc explain E9999 should exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown error code"),
        "expected 'unknown error code' in stderr"
    );
}

#[test]
fn ui_test_json_output_includes_suggestion() {
    let (success, stdout) = run_check_json("invalid/type_error_return.ko");
    assert!(!success);

    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {e}\nstdout: {stdout}"));
    let first_error = &json["errors"][0];
    assert!(
        first_error["suggestion"].is_string(),
        "expected suggestion in JSON output, got: {first_error}"
    );
}
