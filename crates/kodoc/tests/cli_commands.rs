//! E2E tests for kodoc CLI subcommands: fmt, annotate, audit, and fix.
//!
//! Each test invokes the `kodoc` binary via `std::process::Command` and
//! verifies exit codes, stdout contents, and JSON validity as appropriate.

use std::process::Command;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns the path to the `kodoc` binary built by cargo.
fn get_kodoc_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_kodoc"))
}

/// Writes a `.ko` source file to a unique temp directory and returns its path.
fn write_temp_ko(source: &str, name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("kodo_cli_tests").join(name);
    std::fs::create_dir_all(&dir).expect("could not create temp dir");
    let path = dir.join(format!("{name}.ko"));
    std::fs::write(&path, source).expect("could not write temp .ko file");
    path
}

/// A minimal valid Kōdo module used across multiple tests.
fn valid_source() -> &'static str {
    r#"module hello {
    meta { purpose: "CLI test" }
    fn main() -> Int {
        return 0
    }
}"#
}

/// A Kōdo module with annotation metadata useful for audit/annotate tests.
fn annotated_source() -> &'static str {
    r#"module audit_test {
    meta { purpose: "Audit test", version: "1.0.0" }

    @confidence(0.9)
    @authored_by(agent: "test")
    fn safe_fn(x: Int) -> Int
        requires { x > 0 }
        ensures { result > 0 }
    {
        return x
    }

    fn unreviewed_fn(y: Int) -> Int {
        return y
    }
}"#
}

/// A Kōdo module with a low-confidence annotation for policy violation tests.
fn low_confidence_source() -> &'static str {
    r#"module low_conf {
    meta { purpose: "Low confidence test" }

    @confidence(0.5)
    fn risky_fn(x: Int) -> Int {
        return x
    }
}"#
}

/// A Kōdo module with a deliberate syntax error.
fn invalid_source() -> &'static str {
    r#"module broken {
    fn oops( -> Int {
        return 0
    }
}"#
}

// ---------------------------------------------------------------------------
// fmt tests
// ---------------------------------------------------------------------------

#[test]
fn test_fmt_valid_file() {
    let path = write_temp_ko(valid_source(), "fmt_valid");
    let output = Command::new(get_kodoc_path())
        .arg("fmt")
        .arg(&path)
        .output()
        .expect("failed to run kodoc fmt");

    assert!(
        output.status.success(),
        "kodoc fmt exited with non-zero status\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // kodoc fmt formats in-place; verify the file still contains valid content.
    let formatted = std::fs::read_to_string(&path).expect("could not read formatted file");
    assert!(
        formatted.contains("module"),
        "expected 'module' in formatted file contents, got:\n{formatted}",
    );
}

#[test]
fn test_fmt_formats_consistently() {
    // kodoc fmt modifies the file in-place. Idempotency means running fmt twice
    // on the same file produces the same file contents both times.
    let path = write_temp_ko(valid_source(), "fmt_idempotent");
    let kodoc = get_kodoc_path();

    let first = Command::new(&kodoc)
        .arg("fmt")
        .arg(&path)
        .output()
        .expect("failed to run kodoc fmt (first pass)");

    assert!(
        first.status.success(),
        "kodoc fmt (first pass) failed\nstderr: {}",
        String::from_utf8_lossy(&first.stderr),
    );

    let after_first = std::fs::read_to_string(&path).expect("could not read file after first fmt");

    let second = Command::new(&kodoc)
        .arg("fmt")
        .arg(&path)
        .output()
        .expect("failed to run kodoc fmt (second pass)");

    assert!(
        second.status.success(),
        "kodoc fmt (second pass) failed\nstderr: {}",
        String::from_utf8_lossy(&second.stderr),
    );

    let after_second =
        std::fs::read_to_string(&path).expect("could not read file after second fmt");

    assert_eq!(
        after_first, after_second,
        "kodoc fmt is not idempotent: file contents differ after first and second run",
    );
}

#[test]
fn test_fmt_invalid_file_exits_nonzero() {
    let path = write_temp_ko(invalid_source(), "fmt_invalid");
    let output = Command::new(get_kodoc_path())
        .arg("fmt")
        .arg(&path)
        .output()
        .expect("failed to run kodoc fmt on invalid file");

    assert!(
        !output.status.success(),
        "expected kodoc fmt to exit non-zero on a file with syntax errors",
    );
}

// ---------------------------------------------------------------------------
// annotate tests
// ---------------------------------------------------------------------------

#[test]
fn test_annotate_suggests_contracts() {
    let path = write_temp_ko(annotated_source(), "annotate_basic");
    let output = Command::new(get_kodoc_path())
        .arg("annotate")
        .arg(&path)
        .output()
        .expect("failed to run kodoc annotate");

    assert!(
        output.status.success(),
        "kodoc annotate exited with non-zero status\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn test_annotate_json_output() {
    let path = write_temp_ko(annotated_source(), "annotate_json");
    let output = Command::new(get_kodoc_path())
        .arg("annotate")
        .arg("--json")
        .arg(&path)
        .output()
        .expect("failed to run kodoc annotate --json");

    assert!(
        output.status.success(),
        "kodoc annotate --json exited with non-zero status\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str::<serde_json::Value>(&stdout)
        .expect("kodoc annotate --json did not produce valid JSON");
}

// ---------------------------------------------------------------------------
// audit tests
// ---------------------------------------------------------------------------

#[test]
fn test_audit_basic_output() {
    let path = write_temp_ko(annotated_source(), "audit_basic");
    let output = Command::new(get_kodoc_path())
        .arg("audit")
        .arg(&path)
        .output()
        .expect("failed to run kodoc audit");

    assert!(
        output.status.success(),
        "kodoc audit exited with non-zero status\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout).to_lowercase(),
        String::from_utf8_lossy(&output.stderr).to_lowercase(),
    );
    assert!(
        combined.contains("confidence") || combined.contains("audit"),
        "expected 'confidence' or 'audit' in kodoc audit output, got:\n{combined}",
    );
}

#[test]
fn test_audit_json_output() {
    let path = write_temp_ko(annotated_source(), "audit_json");
    let output = Command::new(get_kodoc_path())
        .arg("audit")
        .arg("--json")
        .arg(&path)
        .output()
        .expect("failed to run kodoc audit --json");

    assert!(
        output.status.success(),
        "kodoc audit --json exited with non-zero status\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str::<serde_json::Value>(&stdout)
        .expect("kodoc audit --json did not produce valid JSON");
}

#[test]
fn test_audit_policy_exits_nonzero_on_violation() {
    let path = write_temp_ko(low_confidence_source(), "audit_policy_violation");
    let output = Command::new(get_kodoc_path())
        .arg("audit")
        .arg("--policy")
        .arg("min_confidence=0.99,contracts=all_verified")
        .arg(&path)
        .output()
        .expect("failed to run kodoc audit --policy");

    assert!(
        !output.status.success(),
        "expected kodoc audit to exit non-zero when policy min_confidence=0.99 is violated by a @confidence(0.5) function\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

// ---------------------------------------------------------------------------
// fix tests
// ---------------------------------------------------------------------------

#[test]
fn test_fix_on_valid_file() {
    let path = write_temp_ko(valid_source(), "fix_valid");
    let output = Command::new(get_kodoc_path())
        .arg("fix")
        .arg(&path)
        .output()
        .expect("failed to run kodoc fix");

    assert!(
        output.status.success(),
        "kodoc fix exited with non-zero status on a valid file\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn test_fix_dry_run_on_valid_file() {
    // kodoc fix does not have a --json flag; use --dry-run to inspect patches
    // without modifying the file.
    let path = write_temp_ko(valid_source(), "fix_dry_run_valid");
    let output = Command::new(get_kodoc_path())
        .arg("fix")
        .arg("--dry-run")
        .arg(&path)
        .output()
        .expect("failed to run kodoc fix --dry-run");

    assert!(
        output.status.success(),
        "kodoc fix --dry-run exited with non-zero status on a valid file\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
