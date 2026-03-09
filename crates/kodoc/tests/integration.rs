//! Integration tests for the kodoc compiler pipeline.
//!
//! These tests exercise the full compilation pipeline from source text
//! through parsing, type checking, contract verification, and MIR lowering.

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

/// Runs the full pipeline (parse → type check → contracts → MIR) on a source string.
/// Returns Ok(()) on success, Err(String) with the error message on failure.
fn run_full_pipeline(source: &str) -> Result<(), String> {
    let module = kodo_parser::parse(source).map_err(|e| format!("parse error: {e}"))?;

    let mut checker = kodo_types::TypeChecker::new();
    // Load stdlib prelude (Option, Result).
    for (_name, prelude_source) in kodo_std::prelude_sources() {
        if let Ok(prelude_mod) = kodo_parser::parse(prelude_source) {
            let _ = checker.check_module(&prelude_mod);
        }
    }
    checker
        .check_module(&module)
        .map_err(|e| format!("type error: {e}"))?;

    for func in &module.functions {
        let contracts = kodo_contracts::extract_contracts(func);
        kodo_contracts::verify_contracts(&contracts, kodo_contracts::ContractMode::Runtime)
            .map_err(|e| format!("contract error: {e}"))?;
    }

    kodo_mir::lowering::lower_module(&module).map_err(|e| format!("MIR error: {e}"))?;

    Ok(())
}

// ========== Valid fixtures: full pipeline must succeed ==========

#[test]
fn pipeline_valid_hello() {
    let source = read_fixture("valid/hello.ko");
    run_full_pipeline(&source).unwrap();
}

#[test]
fn pipeline_valid_minimal() {
    let source = read_fixture("valid/minimal.ko");
    run_full_pipeline(&source).unwrap();
}

#[test]
fn pipeline_valid_expressions() {
    let source = read_fixture("valid/expressions.ko");
    run_full_pipeline(&source).unwrap();
}

#[test]
fn pipeline_valid_contracts() {
    let source = read_fixture("valid/contracts.ko");
    run_full_pipeline(&source).unwrap();
}

// ========== Invalid fixtures: must fail at the expected stage ==========

#[test]
fn pipeline_type_error_return_mismatch() {
    let source = read_fixture("invalid/type_error_return.ko");
    let err = run_full_pipeline(&source).unwrap_err();
    assert!(
        err.starts_with("type error:"),
        "expected type error, got: {err}"
    );
    assert!(
        err.contains("mismatch"),
        "expected mismatch in error: {err}"
    );
}

#[test]
fn pipeline_syntax_error() {
    let source = read_fixture("invalid/syntax_error.ko");
    let err = run_full_pipeline(&source).unwrap_err();
    assert!(
        err.starts_with("parse error:"),
        "expected parse error, got: {err}"
    );
}

#[test]
fn pipeline_undefined_variable() {
    let source = read_fixture("invalid/undefined_var.ko");
    let err = run_full_pipeline(&source).unwrap_err();
    assert!(
        err.starts_with("type error:"),
        "expected type error, got: {err}"
    );
    assert!(
        err.contains("undefined") || err.contains("Undefined"),
        "expected undefined variable error: {err}"
    );
}

// ========== Contract fixtures ==========

#[test]
fn pipeline_valid_contracts_fixture() {
    let source = read_fixture("contracts/valid_contracts.ko");
    run_full_pipeline(&source).unwrap();
}

#[test]
fn pipeline_invalid_precondition() {
    let source = read_fixture("contracts/invalid_precondition.ko");
    // The string literal in the requires clause should cause a contract validation failure.
    // Note: contract verification collects failures but does not return Err for them —
    // it reports them in VerificationResult.failures. So the pipeline may succeed
    // but we should check the contract verification result directly.
    let module = kodo_parser::parse(&source).unwrap();

    let mut checker = kodo_types::TypeChecker::new();
    checker.check_module(&module).unwrap();

    for func in &module.functions {
        let contracts = kodo_contracts::extract_contracts(func);
        let result =
            kodo_contracts::verify_contracts(&contracts, kodo_contracts::ContractMode::Runtime)
                .unwrap();
        if !contracts.is_empty() {
            assert!(
                !result.failures.is_empty(),
                "expected contract validation failures for function `{}`",
                func.name
            );
        }
    }
}

// ========== Parse-only tests (preserved from original) ==========

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

// ========== All examples pass check ==========

#[test]
fn all_examples_pass_check() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let examples_dir = workspace_root.join("examples");

    let mut checked = 0;
    for entry in std::fs::read_dir(&examples_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("ko") {
            continue;
        }
        let filename = path.file_name().unwrap().to_str().unwrap();

        // Files with "error" in the name are expected to fail
        if filename.contains("error") {
            let source = std::fs::read_to_string(&path).unwrap();
            let result = run_full_pipeline(&source);
            assert!(
                result.is_err(),
                "expected {filename} to fail pipeline, but it passed"
            );
        } else {
            let source = std::fs::read_to_string(&path).unwrap();
            run_full_pipeline(&source).unwrap_or_else(|e| {
                panic!("example {filename} failed pipeline: {e}");
            });
        }
        checked += 1;
    }

    assert!(
        checked >= 4,
        "expected at least 4 example files, found {checked}"
    );
}

// ========== CLI exit code tests ==========

#[test]
fn cli_check_valid_exits_zero() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let fixture = workspace_root.join("tests/fixtures/valid/hello.ko");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_kodoc"))
        .args(["check", fixture.to_str().unwrap()])
        .output()
        .expect("failed to run kodoc");

    assert!(
        output.status.success(),
        "kodoc check should exit 0 for valid file, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cli_check_invalid_exits_nonzero() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let fixture = workspace_root.join("tests/fixtures/invalid/type_error_return.ko");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_kodoc"))
        .args(["check", fixture.to_str().unwrap()])
        .output()
        .expect("failed to run kodoc");

    assert!(
        !output.status.success(),
        "kodoc check should exit non-zero for type error file"
    );
}

#[test]
fn cli_lex_valid_exits_zero() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let fixture = workspace_root.join("tests/fixtures/valid/hello.ko");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_kodoc"))
        .args(["lex", fixture.to_str().unwrap()])
        .output()
        .expect("failed to run kodoc");

    assert!(
        output.status.success(),
        "kodoc lex should exit 0 for valid file"
    );
}

#[test]
fn cli_parse_valid_exits_zero() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let fixture = workspace_root.join("tests/fixtures/valid/hello.ko");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_kodoc"))
        .args(["parse", fixture.to_str().unwrap()])
        .output()
        .expect("failed to run kodoc");

    assert!(
        output.status.success(),
        "kodoc parse should exit 0 for valid file"
    );
}

// ========== JSON error output tests ==========

#[test]
fn json_errors_parse_error() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let fixture = workspace_root.join("tests/fixtures/invalid/syntax_error.ko");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_kodoc"))
        .args(["check", fixture.to_str().unwrap(), "--json-errors"])
        .output()
        .expect("failed to run kodoc");

    assert!(
        !output.status.success(),
        "kodoc check --json-errors should exit non-zero for syntax error"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {e}\nstdout: {stdout}"));

    assert_eq!(json["status"], "failed");
    assert!(json["errors"].as_array().unwrap().len() > 0);
    let first_error = &json["errors"][0];
    assert!(first_error["code"].as_str().unwrap().starts_with("E01"));
    assert!(first_error["message"].as_str().is_some());
    assert_eq!(first_error["severity"], "error");
}

#[test]
fn json_errors_type_error() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let fixture = workspace_root.join("tests/fixtures/invalid/type_error_return.ko");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_kodoc"))
        .args(["check", fixture.to_str().unwrap(), "--json-errors"])
        .output()
        .expect("failed to run kodoc");

    assert!(
        !output.status.success(),
        "kodoc check --json-errors should exit non-zero for type error"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {e}\nstdout: {stdout}"));

    assert_eq!(json["status"], "failed");
    let first_error = &json["errors"][0];
    assert!(first_error["code"].as_str().unwrap().starts_with("E02"));
    assert!(first_error["span"].is_object());
    assert!(first_error["span"]["line"].as_u64().unwrap() > 0);
}

#[test]
fn json_errors_success() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let fixture = workspace_root.join("tests/fixtures/valid/hello.ko");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_kodoc"))
        .args(["check", fixture.to_str().unwrap(), "--json-errors"])
        .output()
        .expect("failed to run kodoc");

    assert!(
        output.status.success(),
        "kodoc check --json-errors should exit 0 for valid file, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {e}\nstdout: {stdout}"));

    assert_eq!(json["status"], "ok");
    assert_eq!(json["errors"].as_array().unwrap().len(), 0);
    assert_eq!(json["warnings"].as_array().unwrap().len(), 0);
}

// ========== Module meta validation tests ==========

#[test]
fn module_without_meta_is_error() {
    let source = read_fixture("invalid/missing_meta.ko");
    let module = kodo_parser::parse(&source).unwrap();

    let mut checker = kodo_types::TypeChecker::new();
    let err = checker.check_module(&module).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("meta"), "expected meta error, got: {msg}");
}

#[test]
fn module_with_empty_purpose_is_error() {
    let source = read_fixture("invalid/empty_purpose.ko");
    let module = kodo_parser::parse(&source).unwrap();

    let mut checker = kodo_types::TypeChecker::new();
    let err = checker.check_module(&module).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("purpose"),
        "expected purpose error, got: {msg}"
    );
}

#[test]
fn module_with_missing_purpose_is_error() {
    let source = read_fixture("invalid/missing_purpose.ko");
    let module = kodo_parser::parse(&source).unwrap();

    let mut checker = kodo_types::TypeChecker::new();
    let err = checker.check_module(&module).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("purpose"),
        "expected purpose error, got: {msg}"
    );
}

// ========== Meta in JSON output tests ==========

#[test]
fn meta_included_in_json_output() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root");
    let fixture = workspace_root.join("tests/fixtures/valid/hello.ko");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_kodoc"))
        .args(["check", fixture.to_str().unwrap(), "--json-errors"])
        .output()
        .expect("failed to run kodoc");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not valid JSON: {e}\nstdout: {stdout}"));

    assert_eq!(json["status"], "ok");
    assert!(
        json["meta"].is_object(),
        "expected meta object in JSON output"
    );
    assert_eq!(json["meta"]["module"], "hello");
    assert!(json["meta"]["purpose"].as_str().is_some());
}
