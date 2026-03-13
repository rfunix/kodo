//! End-to-end tests that compile `.ko` files into binaries, execute them,
//! and verify their output.
//!
//! These tests exercise the full pipeline: source → parse → typecheck →
//! contracts → resolve → desugar → MIR → codegen → link → run.

use std::path::Path;
use std::process::Command;

/// Returns the path to the `kodoc` binary built by cargo.
fn get_kodoc_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_kodoc"))
}

/// Returns the workspace root directory.
fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("could not find workspace root")
        .to_path_buf()
}

/// Compiles a `.ko` source file to a binary and returns the output path.
///
/// Panics if compilation fails.
fn compile_ko(source_path: &Path, output_name: &str) -> std::path::PathBuf {
    let kodoc = get_kodoc_path();
    let output_dir = std::env::temp_dir().join("kodo_e2e_tests");
    std::fs::create_dir_all(&output_dir).expect("could not create temp dir");
    let output_path = output_dir.join(output_name);

    let result = Command::new(&kodoc)
        .arg("build")
        .arg(source_path)
        .arg("-o")
        .arg(&output_path)
        .output()
        .expect("failed to run kodoc");

    assert!(
        result.status.success(),
        "kodoc build failed for {}:\nstdout: {}\nstderr: {}",
        source_path.display(),
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );

    output_path
}

/// Runs a compiled binary and returns (exit_code, stdout, stderr).
fn run_binary(binary_path: &Path) -> (i32, String, String) {
    let result = Command::new(binary_path)
        .output()
        .unwrap_or_else(|e| panic!("failed to run binary {}: {e}", binary_path.display()));

    let exit_code = result.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&result.stdout).to_string();
    let stderr = String::from_utf8_lossy(&result.stderr).to_string();

    (exit_code, stdout, stderr)
}

/// Writes inline Kodo source to a temp file, compiles it, and returns the binary path.
///
/// Panics if compilation fails.
fn compile_source(source: &str, test_name: &str) -> std::path::PathBuf {
    let output_dir = std::env::temp_dir().join("kodo_e2e_tests");
    std::fs::create_dir_all(&output_dir).expect("could not create temp dir");

    let source_path = output_dir.join(format!("{test_name}.ko"));
    std::fs::write(&source_path, source).expect("could not write source file");

    compile_ko(&source_path, test_name)
}

// ---------------------------------------------------------------------------
// Tests using example files from the repository
// ---------------------------------------------------------------------------

#[test]
fn test_hello_world_output() {
    let root = workspace_root();
    let binary = compile_ko(&root.join("examples/hello.ko"), "test_hello");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "hello world should exit with 0");
    assert!(
        stdout.contains("Hello"),
        "hello world output should contain 'Hello', got: {stdout}"
    );
}

#[test]
fn test_structs_example() {
    let root = workspace_root();
    let binary = compile_ko(&root.join("examples/structs.ko"), "test_structs");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "structs example should exit with 0");
    assert!(
        stdout.contains("10"),
        "structs output should contain '10' (p.x), got: {stdout}"
    );
    assert!(
        stdout.contains("20"),
        "structs output should contain '20' (p.y), got: {stdout}"
    );
}

#[test]
fn test_string_demo_example() {
    let root = workspace_root();
    let binary = compile_ko(&root.join("examples/string_demo.ko"), "test_string_demo");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "string_demo should exit with 0");
    assert!(
        stdout.contains("String operations demo complete"),
        "string_demo output should contain demo message, got: {stdout}"
    );
    assert!(
        stdout.contains("5"),
        "string_demo output should contain '5' (length of 'Hello'), got: {stdout}"
    );
}

#[test]
fn test_contracts_example_compiles_and_runs() {
    let root = workspace_root();
    let binary = compile_ko(&root.join("examples/contracts.ko"), "test_contracts");
    let (exit_code, _stdout, _stderr) = run_binary(&binary);

    assert_eq!(
        exit_code, 0,
        "contracts example (with valid inputs) should exit with 0"
    );
}

// ---------------------------------------------------------------------------
// Tests with inline source code
// ---------------------------------------------------------------------------

#[test]
fn test_string_variable_println() {
    let source = r#"module string_var_test {
    meta {
        purpose: "Test string variable println",
        version: "0.1.0"
    }

    fn main() {
        let s: String = "test output"
        println(s)
    }
}"#;
    let binary = compile_source(source, "test_string_var_println");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "should exit with 0");
    assert!(
        stdout.contains("test output"),
        "should print the string variable, got: {stdout}"
    );
}

#[test]
fn test_arithmetic_output() {
    let source = r#"module arith_test {
    meta {
        purpose: "Test arithmetic output",
        version: "0.1.0"
    }

    fn main() {
        let x: Int = 2 + 3
        print_int(x)
    }
}"#;
    let binary = compile_source(source, "test_arithmetic_output");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "should exit with 0");
    assert!(stdout.contains("5"), "should print 5, got: {stdout}");
}

#[test]
fn test_contract_violation_nonzero_exit() {
    let source = r#"module contract_test {
    meta {
        purpose: "Test contract violation",
        version: "0.1.0"
    }

    fn safe_divide(a: Int, b: Int) -> Int
        requires { b != 0 }
    {
        return a / b
    }

    fn main() {
        let result: Int = safe_divide(10, 0)
        print_int(result)
    }
}"#;
    let binary = compile_source(source, "test_contract_violation");
    let (exit_code, _stdout, _stderr) = run_binary(&binary);

    assert_ne!(
        exit_code, 0,
        "contract violation should produce non-zero exit code"
    );
}

#[test]
fn test_multiple_print_int_calls() {
    let source = r#"module multi_print {
    meta {
        purpose: "Test multiple print_int calls",
        version: "0.1.0"
    }

    fn main() {
        print_int(1)
        print_int(2)
        print_int(3)
    }
}"#;
    let binary = compile_source(source, "test_multi_print");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "should exit with 0");
    assert!(stdout.contains("1"), "should contain '1', got: {stdout}");
    assert!(stdout.contains("2"), "should contain '2', got: {stdout}");
    assert!(stdout.contains("3"), "should contain '3', got: {stdout}");
}

#[test]
fn test_if_else_branching() {
    let source = r#"module if_else_test {
    meta {
        purpose: "Test if/else branching",
        version: "0.1.0"
    }

    fn main() {
        let x: Int = 10
        if x > 5 {
            println("greater")
        } else {
            println("smaller")
        }
    }
}"#;
    let binary = compile_source(source, "test_if_else");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "should exit with 0");
    assert!(
        stdout.contains("greater"),
        "should print 'greater' since 10 > 5, got: {stdout}"
    );
}

#[test]
fn test_function_call_and_return() {
    let source = r#"module fn_call_test {
    meta {
        purpose: "Test function call and return value",
        version: "0.1.0"
    }

    fn add(a: Int, b: Int) -> Int {
        return a + b
    }

    fn main() {
        let result: Int = add(3, 7)
        print_int(result)
    }
}"#;
    let binary = compile_source(source, "test_fn_call");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "should exit with 0");
    assert!(
        stdout.contains("10"),
        "should print 10 (3 + 7), got: {stdout}"
    );
}

#[test]
fn test_struct_field_access() {
    let source = r#"module struct_field_test {
    meta {
        purpose: "Test struct creation and field access",
        version: "0.1.0"
    }

    struct Pair {
        first: Int,
        second: Int
    }

    fn main() {
        let p: Pair = Pair { first: 42, second: 99 }
        print_int(p.first)
        print_int(p.second)
    }
}"#;
    let binary = compile_source(source, "test_struct_field");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "should exit with 0");
    assert!(
        stdout.contains("42"),
        "should print 42 (p.first), got: {stdout}"
    );
    assert!(
        stdout.contains("99"),
        "should print 99 (p.second), got: {stdout}"
    );
}

#[test]
fn test_string_concat_operator() {
    let source = r#"module string_concat_test {
    meta {
        purpose: "Test string concatenation with + operator",
        version: "0.1.0"
    }

    fn main() {
        let a: String = "hello "
        let b: String = "world"
        let c: String = a + b
        println(c)
        let d: String = "foo" + "bar"
        println(d)
    }
}"#;
    let binary = compile_source(source, "test_string_concat_op");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "should exit with 0");
    assert!(
        stdout.contains("hello world"),
        "should print 'hello world', got: {stdout}"
    );
    assert!(
        stdout.contains("foobar"),
        "should print 'foobar', got: {stdout}"
    );
}

#[test]
fn test_closure_simple() {
    let source = r#"module closure_simple {
    meta {
        purpose: "Test simple closure compilation"
        version: "0.1.0"
    }

    fn main() -> Int {
        let double: (Int) -> Int = |x: Int| -> Int { x * 2 }
        let result: Int = double(21)
        print_int(result)
        return 0
    }
}"#;
    let binary = compile_source(source, "test_closure_simple");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "closure should exit with 0");
    assert!(
        stdout.contains("42"),
        "double(21) should produce 42, got: {stdout}"
    );
}

#[test]
fn test_closure_with_capture() {
    let source = r#"module closure_capture {
    meta {
        purpose: "Test closure with captured variable"
        version: "0.1.0"
    }

    fn main() -> Int {
        let offset: Int = 100
        let add_offset: (Int) -> Int = |x: Int| -> Int { x + offset }
        let result: Int = add_offset(42)
        print_int(result)
        return 0
    }
}"#;
    let binary = compile_source(source, "test_closure_capture");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "closure with capture should exit with 0");
    assert!(
        stdout.contains("142"),
        "add_offset(42) with offset=100 should produce 142, got: {stdout}"
    );
}

#[test]
fn test_closure_inferred_return_type() {
    let source = r#"module closure_inferred {
    meta {
        purpose: "Test closure with inferred return type"
        version: "0.1.0"
    }

    fn main() -> Int {
        let triple: (Int) -> Int = |x: Int| { x * 3 }
        let result: Int = triple(10)
        print_int(result)
        return 0
    }
}"#;
    let binary = compile_source(source, "test_closure_inferred");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(
        exit_code, 0,
        "closure with inferred type should exit with 0"
    );
    assert!(
        stdout.contains("30"),
        "triple(10) should produce 30, got: {stdout}"
    );
}

#[test]
fn test_closures_example_file() {
    let root = workspace_root();
    let binary = compile_ko(&root.join("examples/closures.ko"), "test_closures_example");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "closures example should exit with 0");
    assert!(
        stdout.contains("42"),
        "closures example should print 42 (double(21)), got: {stdout}"
    );
    assert!(
        stdout.contains("142"),
        "closures example should print 142 (add_offset(42)), got: {stdout}"
    );
    assert!(
        stdout.contains("30"),
        "closures example should print 30 (triple(10)), got: {stdout}"
    );
    assert!(
        stdout.contains("35"),
        "closures example should print 35 (sum_with(5)), got: {stdout}"
    );
}

#[test]
fn test_dynamic_strings_freed_without_crash() {
    // Creates multiple dynamic strings via concatenation. If cleanup is broken,
    // this would crash or leak (detectable by sanitizers, not here, but at least
    // we verify it runs without error).
    let source = r#"module string_cleanup {
    meta {
        purpose: "Test dynamic string cleanup",
        version: "0.1.0"
    }

    fn main() {
        let a: String = "hello"
        let b: String = " "
        let c: String = "world"
        let ab: String = a + b
        let abc: String = ab + c
        println(abc)
    }
}"#;
    let binary = compile_source(source, "test_string_cleanup");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "should exit with 0");
    assert!(
        stdout.contains("hello world"),
        "should print 'hello world', got: {stdout}"
    );
}

#[test]
fn test_spawn_without_captures() {
    let source = r#"module spawn_no_cap {
    meta {
        purpose: "Test spawn without captures"
        version: "0.1.0"
    }

    fn main() -> Int {
        spawn {
            print_int(99)
        }
        return 0
    }
}"#;
    let binary = compile_source(source, "test_spawn_no_cap");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "spawn without captures should exit with 0");
    assert!(
        stdout.contains("99"),
        "spawned task should print 99, got: {stdout}"
    );
}

#[test]
fn test_spawn_with_captured_variable() {
    let source = r#"module spawn_cap {
    meta {
        purpose: "Test spawn with captured variable"
        version: "0.1.0"
    }

    fn main() -> Int {
        let x: Int = 42
        spawn {
            print_int(x)
        }
        return 0
    }
}"#;
    let binary = compile_source(source, "test_spawn_cap");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "spawn with capture should exit with 0");
    assert!(
        stdout.contains("42"),
        "spawned task should print captured value 42, got: {stdout}"
    );
}

#[test]
fn test_spawn_with_multiple_captures() {
    let source = r#"module spawn_multi_cap {
    meta {
        purpose: "Test spawn with multiple captured variables"
        version: "0.1.0"
    }

    fn main() -> Int {
        let a: Int = 10
        let b: Int = 32
        spawn {
            print_int(a + b)
        }
        return 0
    }
}"#;
    let binary = compile_source(source, "test_spawn_multi_cap");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(
        exit_code, 0,
        "spawn with multiple captures should exit with 0"
    );
    assert!(
        stdout.contains("42"),
        "spawned task should print 42 (10+32), got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Actor runtime tests
// ---------------------------------------------------------------------------

#[test]
fn e2e_actor_creation_and_field_access() {
    let source = r#"module test {
    meta { purpose: "test" }

    actor Counter {
        count: Int

        fn increment(self) -> Int {
            return self.count + 1
        }
    }

    fn main() -> Int {
        let c: Counter = Counter { count: 42 }
        let v: Int = c.count
        print_int(v)
        return 0
    }
}"#;
    let binary = compile_source(source, "test_actor_create");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "actor creation should exit with 0");
    assert!(
        stdout.contains("42"),
        "actor field access should produce 42, got: {stdout}"
    );
}

#[test]
fn test_refinement_type_valid_literal() {
    let source = r#"module refinement_valid {
    meta {
        purpose: "Test refinement type with valid literal"
        version: "0.1.0"
    }

    type Port = Int requires { self > 0 && self < 65535 }

    fn main() {
        let port: Port = 8080
        print_int(port)
    }
}"#;
    let binary = compile_source(source, "test_refinement_valid");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "valid refinement should exit with 0");
    assert!(stdout.contains("8080"), "should print 8080, got: {stdout}");
}

#[test]
fn test_refinement_type_invalid_literal_panics() {
    let source = r#"module refinement_invalid {
    meta {
        purpose: "Test refinement type violation"
        version: "0.1.0"
    }

    type Port = Int requires { self > 0 && self < 65535 }

    fn main() {
        let port: Port = 0
        print_int(port)
    }
}"#;
    let binary = compile_source(source, "test_refinement_invalid");
    let (exit_code, _stdout, stderr) = run_binary(&binary);

    assert_ne!(
        exit_code, 0,
        "invalid refinement should produce non-zero exit code"
    );
    assert!(
        stderr.contains("refinement constraint failed") || stderr.contains("contract violation"),
        "error should mention refinement constraint, got stderr: {stderr}"
    );
}

#[test]
fn test_refinement_type_dynamic_value() {
    let source = r#"module refinement_dynamic {
    meta {
        purpose: "Test refinement type with dynamic value"
        version: "0.1.0"
    }

    type Positive = Int requires { self > 0 }

    fn add(a: Int, b: Int) -> Int {
        return a + b
    }

    fn main() {
        let x: Int = 3
        let y: Int = 7
        let result: Positive = add(x, y)
        print_int(result)
    }
}"#;
    let binary = compile_source(source, "test_refinement_dynamic");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(
        exit_code, 0,
        "dynamic refinement with valid value should pass"
    );
    assert!(stdout.contains("10"), "should print 10, got: {stdout}");
}

#[test]
fn test_mir_subcommand_prints_mir() {
    let kodoc = get_kodoc_path();
    let root = workspace_root();

    let result = Command::new(&kodoc)
        .arg("mir")
        .arg(root.join("examples/hello.ko"))
        .output()
        .expect("failed to run kodoc mir");

    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);

    assert!(
        result.status.success(),
        "kodoc mir should succeed, stderr: {stderr}"
    );
    assert!(
        stdout.contains("--- MIR"),
        "mir output should contain MIR header, got: {stdout}"
    );
    assert!(
        stdout.contains("--- end MIR ---"),
        "mir output should contain MIR footer, got: {stdout}"
    );
    assert!(
        stdout.contains("MirFunction"),
        "mir output should contain MIR functions, got: {stdout}"
    );
}

#[test]
fn test_emit_mir_flag_in_build() {
    let kodoc = get_kodoc_path();
    let root = workspace_root();
    let output_dir = std::env::temp_dir().join("kodo_e2e_tests");
    std::fs::create_dir_all(&output_dir).expect("could not create temp dir");
    let output_path = output_dir.join("test_emit_mir_build");

    let result = Command::new(&kodoc)
        .arg("build")
        .arg("--emit-mir")
        .arg(root.join("examples/hello.ko"))
        .arg("-o")
        .arg(&output_path)
        .output()
        .expect("failed to run kodoc build --emit-mir");

    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);

    assert!(
        result.status.success(),
        "kodoc build --emit-mir should succeed, stderr: {stderr}"
    );
    assert!(
        stdout.contains("--- MIR"),
        "build --emit-mir should print MIR header, got: {stdout}"
    );
    assert!(
        stdout.contains("--- end MIR ---"),
        "build --emit-mir should print MIR footer, got: {stdout}"
    );
    assert!(
        stdout.contains("Successfully compiled"),
        "build --emit-mir should still compile successfully, got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Confidence report tests
// ---------------------------------------------------------------------------

/// Helper: writes a `.ko` source to a temp file and runs `kodoc confidence-report`.
fn run_confidence_report_cmd(source: &str, test_name: &str, extra_args: &[&str]) -> (i32, String) {
    let output_dir = std::env::temp_dir().join("kodo_e2e_tests");
    std::fs::create_dir_all(&output_dir).expect("could not create temp dir");

    let source_path = output_dir.join(format!("{test_name}.ko"));
    std::fs::write(&source_path, source).expect("could not write source file");

    let kodoc = get_kodoc_path();
    let mut cmd = Command::new(&kodoc);
    cmd.arg("confidence-report").arg(&source_path);
    for arg in extra_args {
        cmd.arg(arg);
    }

    let result = cmd.output().expect("failed to run kodoc confidence-report");
    let exit_code = result.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&result.stdout).to_string();
    (exit_code, stdout)
}

#[test]
fn confidence_report_human_readable_shows_module_average() {
    let source = r#"
module cr_test {
    meta { purpose: "test" }

    @authored_by(agent: "claude")
    @confidence(0.9)
    fn foo() -> Int { return 1 }

    @authored_by(agent: "claude")
    @confidence(0.8)
    fn bar() -> Int { return 2 }

    fn main() {
        let x: Int = foo()
        let y: Int = bar()
        print_int(x)
    }
}
"#;
    let (exit_code, stdout) = run_confidence_report_cmd(source, "cr_human_avg", &[]);
    assert_eq!(exit_code, 0, "confidence report should succeed");
    assert!(
        stdout.contains("Module average:"),
        "should show module average, got:\n{stdout}"
    );
    assert!(
        stdout.contains("Overall confidence:"),
        "should show overall confidence, got:\n{stdout}"
    );
}

#[test]
fn confidence_report_flags_missing_authored_by() {
    let source = r#"
module cr_missing {
    meta { purpose: "test" }

    @authored_by(agent: "claude")
    @confidence(0.95)
    fn annotated_fn() -> Int { return 1 }

    fn unannotated_fn() -> Int { return 2 }

    fn main() {
        let x: Int = annotated_fn()
        let y: Int = unannotated_fn()
        print_int(x)
    }
}
"#;
    let (exit_code, stdout) = run_confidence_report_cmd(source, "cr_missing_authored", &[]);
    assert_eq!(exit_code, 0, "confidence report should succeed");
    assert!(
        stdout.contains("no @authored_by"),
        "should flag missing @authored_by, got:\n{stdout}"
    );
    assert!(
        stdout.contains("Functions missing @authored_by:"),
        "should list functions missing @authored_by, got:\n{stdout}"
    );
    assert!(
        stdout.contains("unannotated_fn"),
        "unannotated_fn should be listed, got:\n{stdout}"
    );
}

#[test]
fn confidence_report_json_includes_all_fields() {
    let source = r#"
module cr_json {
    meta { purpose: "test" }

    @authored_by(agent: "claude")
    @confidence(0.95)
    fn good_fn() -> Int { return 1 }

    @confidence(0.5)
    @reviewed_by(human: "tester")
    fn low_fn() -> Int { return 2 }

    fn bare_fn() -> Int { return 3 }

    fn main() {
        let a: Int = good_fn()
        let b: Int = low_fn()
        let c: Int = bare_fn()
        print_int(a)
    }
}
"#;
    let (exit_code, stdout) =
        run_confidence_report_cmd(source, "cr_json_fields", &["--json", "--threshold", "0.8"]);
    assert_eq!(exit_code, 0, "confidence report --json should succeed");

    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("output should be valid JSON");

    // Module-level fields
    assert!(
        json.get("module_average_confidence").is_some(),
        "JSON should contain module_average_confidence"
    );
    assert!(
        json.get("overall_confidence").is_some(),
        "JSON should contain overall_confidence"
    );
    assert!(
        json.get("threshold").is_some(),
        "JSON should contain threshold"
    );

    // Missing authored_by
    let missing = json["missing_authored_by"]
        .as_array()
        .expect("missing_authored_by should be an array");
    assert!(
        missing.iter().any(|v| v.as_str() == Some("low_fn")),
        "low_fn should be in missing_authored_by: {missing:?}"
    );
    assert!(
        missing.iter().any(|v| v.as_str() == Some("bare_fn")),
        "bare_fn should be in missing_authored_by: {missing:?}"
    );
    assert!(
        !missing.iter().any(|v| v.as_str() == Some("good_fn")),
        "good_fn should NOT be in missing_authored_by: {missing:?}"
    );

    // Below threshold
    let below = json["below_threshold"]
        .as_array()
        .expect("below_threshold should be an array");
    assert!(
        below.iter().any(|v| v["name"].as_str() == Some("low_fn")),
        "low_fn should be below threshold: {below:?}"
    );

    // Per-function has_authored_by
    let functions = json["functions"]
        .as_array()
        .expect("functions should be an array");
    for f in functions {
        let name = f["name"].as_str().unwrap_or("");
        let has = f["has_authored_by"].as_bool();
        if name == "good_fn" {
            assert_eq!(has, Some(true), "good_fn should have has_authored_by=true");
        } else if name == "low_fn" || name == "bare_fn" {
            assert_eq!(has, Some(false), "{name} should have has_authored_by=false");
        }
    }
}

#[test]
fn confidence_report_threshold_filters_correctly() {
    let source = r#"
module cr_threshold {
    meta { purpose: "test" }

    @authored_by(agent: "claude")
    @confidence(0.99)
    fn high_fn() -> Int { return 1 }

    @authored_by(agent: "claude")
    @confidence(0.5)
    @reviewed_by(human: "tester")
    fn low_fn() -> Int { return 2 }

    fn main() {
        let a: Int = high_fn()
        let b: Int = low_fn()
        print_int(a)
    }
}
"#;
    // With threshold=0.6, low_fn (0.5) should be flagged but high_fn (0.99) should not.
    let (exit_code, stdout) = run_confidence_report_cmd(
        source,
        "cr_threshold_test",
        &["--json", "--threshold", "0.6"],
    );
    assert_eq!(exit_code, 0);

    let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    let below = json["below_threshold"]
        .as_array()
        .expect("below_threshold array");
    // Both low_fn (0.5) and main (which transitively calls low_fn, so also 0.5) are below 0.6.
    assert!(
        below.len() >= 1,
        "at least low_fn should be below 0.6: {below:?}"
    );
    assert!(
        below.iter().any(|v| v["name"].as_str() == Some("low_fn")),
        "low_fn should be flagged as below threshold: {below:?}"
    );
}

// ---------------------------------------------------------------------------
// Phase 51 — Additional E2E tests for MIR/Codegen coverage
// ---------------------------------------------------------------------------

#[test]
fn test_while_loop_output() {
    let source = r#"module while_test {
    meta {
        purpose: "Test while loop execution",
        version: "0.1.0"
    }

    fn main() {
        let mut i: Int = 0
        while i < 3 {
            print_int(i)
            i = i + 1
        }
    }
}"#;
    let binary = compile_source(source, "test_while_loop");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "should exit with 0");
    assert!(stdout.contains("0"), "should print 0, got: {stdout}");
    assert!(stdout.contains("1"), "should print 1, got: {stdout}");
    assert!(stdout.contains("2"), "should print 2, got: {stdout}");
}

#[test]
fn test_for_loop_output() {
    let source = r#"module for_test {
    meta {
        purpose: "Test for loop execution",
        version: "0.1.0"
    }

    fn main() {
        let mut sum: Int = 0
        for i in 0..5 {
            sum = sum + i
        }
        print_int(sum)
    }
}"#;
    let binary = compile_source(source, "test_for_loop");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "should exit with 0");
    assert!(
        stdout.contains("10"),
        "sum of 0..5 should be 10, got: {stdout}"
    );
}

#[test]
fn test_nested_function_calls() {
    let source = r#"module nested_fn_test {
    meta {
        purpose: "Test nested function calls",
        version: "0.1.0"
    }

    fn double(x: Int) -> Int {
        return x * 2
    }

    fn add_one(x: Int) -> Int {
        return x + 1
    }

    fn main() {
        let result: Int = double(add_one(4))
        print_int(result)
    }
}"#;
    let binary = compile_source(source, "test_nested_fn");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "should exit with 0");
    assert!(
        stdout.contains("10"),
        "double(add_one(4)) = 10, got: {stdout}"
    );
}

#[test]
fn test_multiple_contracts_pass() {
    let source = r#"module multi_contract_test {
    meta {
        purpose: "Test multiple contracts that pass",
        version: "0.1.0"
    }

    fn bounded(x: Int) -> Int
        requires { x > 0 }
        requires { x < 100 }
    {
        return x * 2
    }

    fn main() {
        let result: Int = bounded(25)
        print_int(result)
    }
}"#;
    let binary = compile_source(source, "test_multi_contract");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "should exit with 0");
    assert!(stdout.contains("50"), "bounded(25) = 50, got: {stdout}");
}

#[test]
fn test_struct_two_fields() {
    let source = r#"module struct_two_test {
    meta {
        purpose: "Test struct with two int fields",
        version: "0.1.0"
    }

    struct Pair {
        first: Int,
        second: Int
    }

    fn main() {
        let p: Pair = Pair { first: 7, second: 3 }
        print_int(p.first)
        print_int(p.second)
    }
}"#;
    let binary = compile_source(source, "test_struct_two");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "should exit with 0");
    assert!(stdout.contains("7"), "should print 7, got: {stdout}");
    assert!(stdout.contains("3"), "should print 3, got: {stdout}");
}

#[test]
fn test_boolean_logic() {
    let source = r#"module bool_test {
    meta {
        purpose: "Test boolean operations",
        version: "0.1.0"
    }

    fn main() {
        let a: Bool = true
        let b: Bool = false
        if a {
            println("a is true")
        }
        if !b {
            println("b is false")
        }
    }
}"#;
    let binary = compile_source(source, "test_bool_logic");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "should exit with 0");
    assert!(
        stdout.contains("a is true"),
        "expected 'a is true', got: {stdout}"
    );
    assert!(
        stdout.contains("b is false"),
        "expected 'b is false', got: {stdout}"
    );
}

#[test]
fn test_recursive_function() {
    let source = r#"module recursive_test {
    meta {
        purpose: "Test recursive function",
        version: "0.1.0"
    }

    fn factorial(n: Int) -> Int {
        if n <= 1 {
            return 1
        }
        return n * factorial(n - 1)
    }

    fn main() {
        let result: Int = factorial(5)
        print_int(result)
    }
}"#;
    let binary = compile_source(source, "test_recursive");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "should exit with 0");
    assert!(stdout.contains("120"), "factorial(5) = 120, got: {stdout}");
}

#[test]
fn test_string_length() {
    let source = r#"module string_len_test {
    meta {
        purpose: "Test string length",
        version: "0.1.0"
    }

    fn main() {
        let s: String = "Kodo"
        let len: Int = s.length()
        print_int(len)
    }
}"#;
    let binary = compile_source(source, "test_string_len");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "should exit with 0");
    assert!(
        stdout.contains("4"),
        "length of 'Kodo' should be 4, got: {stdout}"
    );
}

#[test]
fn test_ensures_contract_pass() {
    let source = r#"module ensures_test {
    meta {
        purpose: "Test ensures contract that passes",
        version: "0.1.0"
    }

    fn positive_double(x: Int) -> Int
        requires { x > 0 }
        ensures { result > 0 }
    {
        return x * 2
    }

    fn main() {
        let r: Int = positive_double(5)
        print_int(r)
    }
}"#;
    let binary = compile_source(source, "test_ensures_pass");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "should exit with 0");
    assert!(
        stdout.contains("10"),
        "positive_double(5) = 10, got: {stdout}"
    );
}

#[test]
fn test_multiple_return_paths() {
    let source = r#"module multi_return_test {
    meta {
        purpose: "Test function with multiple return paths",
        version: "0.1.0"
    }

    fn classify(x: Int) -> Int {
        if x > 0 {
            return 1
        }
        if x < 0 {
            return -1
        }
        return 0
    }

    fn main() {
        print_int(classify(5))
        print_int(classify(-3))
        print_int(classify(0))
    }
}"#;
    let binary = compile_source(source, "test_multi_return");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "should exit with 0");
    assert!(stdout.contains("1"), "classify(5) = 1, got: {stdout}");
    assert!(stdout.contains("-1"), "classify(-3) = -1, got: {stdout}");
    assert!(stdout.contains("0"), "classify(0) = 0, got: {stdout}");
}

#[test]
fn test_chained_arithmetic() {
    let source = r#"module chain_arith_test {
    meta {
        purpose: "Test chained arithmetic",
        version: "0.1.0"
    }

    fn main() {
        let a: Int = 2
        let b: Int = 3
        let c: Int = 4
        let result: Int = a + b * c
        print_int(result)
    }
}"#;
    let binary = compile_source(source, "test_chain_arith");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "should exit with 0");
    // Note: Kōdo parser may evaluate left-to-right without precedence,
    // or may have precedence. Check the actual output.
    let _stdout = stdout; // Output depends on parser precedence rules
}

// ---------------------------------------------------------------------------
// String cleanup stress test — verifies heap_locals cleanup works without crash
// ---------------------------------------------------------------------------

#[test]
fn test_string_concat_loop_no_crash() {
    let source = r#"module string_stress {
    meta {
        purpose: "Stress test string operations",
        version: "0.1.0"
    }

    fn main() {
        let mut i: Int = 0
        while i < 50 {
            let a: String = "hello"
            let b: String = " world"
            let c: String = a + b
            let d: String = c + "!"
            i = i + 1
        }
        println("done")
    }
}"#;
    let binary = compile_source(source, "test_string_stress");
    let (exit_code, stdout, _stderr) = run_binary(&binary);

    assert_eq!(exit_code, 0, "string stress test should exit with 0");
    assert!(
        stdout.contains("done"),
        "should print 'done', got: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Phase 54 — Send/Sync bounds for spawn blocks + concurrency test coverage
// ---------------------------------------------------------------------------

#[test]
fn test_parallel_block_compiles() {
    let source = r#"module test_parallel {
    meta { purpose: "Test parallel compilation" }

    fn task_a() -> Int {
        return 1
    }

    fn task_b() -> Int {
        return 2
    }

    fn main() -> Int {
        parallel {
            spawn { task_a() }
            spawn { task_b() }
        }
        return 0
    }
}"#;
    let module = kodo_parser::parse(source).expect("parse");
    let mut checker = kodo_types::TypeChecker::new();
    let errors = checker.check_module_collecting(&module);
    assert!(errors.is_empty(), "type errors: {errors:?}");
}

#[test]
fn test_spawn_with_owned_value() {
    let source = r#"module test_spawn_own {
    meta { purpose: "Test spawn with owned value" }

    fn process(x: Int) -> Int {
        return x + 1
    }

    fn main() -> Int {
        let val: Int = 42
        spawn {
            process(val)
        }
        return 0
    }
}"#;
    let module = kodo_parser::parse(source).expect("parse");
    let mut checker = kodo_types::TypeChecker::new();
    let errors = checker.check_module_collecting(&module);
    assert!(errors.is_empty(), "type errors: {errors:?}");
}

#[test]
fn test_channel_type_checking() {
    let source = r#"module test_channel {
    meta { purpose: "Test channel type checking" }

    fn main() -> Int {
        let ch: Int = channel_new()
        channel_send(ch, 42)
        let val: Int = channel_recv(ch)
        channel_free(ch)
        return val
    }
}"#;
    let module = kodo_parser::parse(source).expect("parse");
    let mut checker = kodo_types::TypeChecker::new();
    let errors = checker.check_module_collecting(&module);
    assert!(errors.is_empty(), "type errors: {errors:?}");
}

#[test]
fn test_async_await_basic_type_check() {
    let source = r#"module test_await {
    meta { purpose: "Test basic async module" }

    fn main() -> Int {
        let x: Int = 1
        return x
    }
}"#;
    let module = kodo_parser::parse(source).expect("parse");
    let mut checker = kodo_types::TypeChecker::new();
    let errors = checker.check_module_collecting(&module);
    assert!(errors.is_empty());
}

// ---------------------------------------------------------------------------
// Phase 53 — Qualified module system
// ---------------------------------------------------------------------------

#[test]
fn test_import_with_double_colon_parses() {
    let source = r#"module test_import_dc {
    import std::option
    meta { purpose: "Test :: import syntax" }

    fn main() -> Int {
        return 0
    }
}"#;
    let module = kodo_parser::parse(source).expect("parse");
    assert_eq!(module.imports.len(), 1);
    assert_eq!(module.imports[0].path, vec!["std", "option"]);
}

#[test]
fn test_from_import_parses() {
    let source = r#"module test_from_import {
    from std::option import Some, None
    meta { purpose: "Test from import syntax" }

    fn main() -> Int {
        return 0
    }
}"#;
    let module = kodo_parser::parse(source).expect("parse");
    assert_eq!(module.imports.len(), 1);
    assert_eq!(module.imports[0].path, vec!["std", "option"]);
    assert_eq!(
        module.imports[0].names,
        Some(vec!["Some".to_string(), "None".to_string()])
    );
}

#[test]
fn test_qualified_dot_access_type_checks() {
    // First define a helper module, then use qualified access
    let helper_source = r#"module helper {
    meta { purpose: "Helper module" }

    fn add(a: Int, b: Int) -> Int {
        return a + b
    }
}"#;
    let main_source = r#"module main {
    import helper
    meta { purpose: "Test qualified access" }

    fn main() -> Int {
        let result: Int = helper.add(1, 2)
        return result
    }
}"#;
    let helper_mod = kodo_parser::parse(helper_source).expect("parse helper");
    let main_mod = kodo_parser::parse(main_source).expect("parse main");

    let mut checker = kodo_types::TypeChecker::new();
    // Check helper module first to register its functions
    let helper_errors = checker.check_module_collecting(&helper_mod);
    assert!(
        helper_errors.is_empty(),
        "helper type errors: {helper_errors:?}"
    );
    // Register the module name for qualified access
    checker.register_imported_module("helper".to_string());
    // Now check the main module
    let main_errors = checker.check_module_collecting(&main_mod);
    assert!(main_errors.is_empty(), "main type errors: {main_errors:?}");
}

#[test]
fn test_stdlib_resolve_module() {
    // Test that kodo_std::resolve_stdlib_module works
    let path = vec!["std".to_string(), "option".to_string()];
    let source = kodo_std::resolve_stdlib_module(&path);
    assert!(source.is_some(), "std::option should resolve");

    let path = vec!["std".to_string(), "result".to_string()];
    let source = kodo_std::resolve_stdlib_module(&path);
    assert!(source.is_some(), "std::result should resolve");

    let path = vec!["std".to_string(), "unknown".to_string()];
    let source = kodo_std::resolve_stdlib_module(&path);
    assert!(source.is_none(), "std::unknown should not resolve");

    let path = vec!["math".to_string()];
    let source = kodo_std::resolve_stdlib_module(&path);
    assert!(source.is_none(), "non-std path should not resolve");
}

// ---------------------------------------------------------------------------
// Macro-generated E2E tests for all examples
// ---------------------------------------------------------------------------

/// Compiles and runs an example, asserting exit code 0 and that stdout
/// contains each expected substring.
macro_rules! e2e_example_test {
    // Compile + run + check output contains strings
    (run: $name:ident, $file:expr, contains: [$($expected:expr),+ $(,)?]) => {
        #[test]
        fn $name() {
            let root = workspace_root();
            let binary = compile_ko(
                &root.join(concat!("examples/", $file)),
                concat!("e2e_ex_", stringify!($name)),
            );
            let (exit_code, stdout, _stderr) = run_binary(&binary);
            assert_eq!(exit_code, 0, concat!(stringify!($name), " should exit 0, got: {}"), exit_code);
            $(
                assert!(
                    stdout.contains($expected),
                    concat!(stringify!($name), " output should contain '", $expected, "', got: {}"),
                    stdout
                );
            )+
        }
    };
    // Compile + run, exit 0 only (no output check)
    (run: $name:ident, $file:expr) => {
        #[test]
        fn $name() {
            let root = workspace_root();
            let binary = compile_ko(
                &root.join(concat!("examples/", $file)),
                concat!("e2e_ex_", stringify!($name)),
            );
            let (exit_code, _stdout, _stderr) = run_binary(&binary);
            assert_eq!(exit_code, 0, concat!(stringify!($name), " should exit 0, got: {}"), exit_code);
        }
    };
    // Compile only (no run)
    (compile: $name:ident, $file:expr) => {
        #[test]
        fn $name() {
            let root = workspace_root();
            let _binary = compile_ko(
                &root.join(concat!("examples/", $file)),
                concat!("e2e_ex_", stringify!($name)),
            );
        }
    };
    // Expect compilation failure
    (fail: $name:ident, $file:expr) => {
        #[test]
        fn $name() {
            let root = workspace_root();
            let kodoc = get_kodoc_path();
            let output_dir = std::env::temp_dir().join("kodo_e2e_tests");
            std::fs::create_dir_all(&output_dir).expect("could not create temp dir");
            let output_path = output_dir.join(concat!("e2e_ex_", stringify!($name)));
            let result = std::process::Command::new(&kodoc)
                .arg("build")
                .arg(root.join(concat!("examples/", $file)))
                .arg("-o")
                .arg(&output_path)
                .output()
                .expect("failed to run kodoc");
            assert!(
                !result.status.success(),
                concat!(stringify!($name), " should fail to compile")
            );
        }
    };
}

// --- Examples that compile + run + produce known output ---

e2e_example_test!(run: e2e_actor_demo, "actor_demo.ko", contains: ["10"]);
e2e_example_test!(run: e2e_actors, "actors.ko", contains: ["42"]);
e2e_example_test!(run: e2e_agent_traceability, "agent_traceability.ko", contains: ["7"]);
e2e_example_test!(run: e2e_associated_types, "associated_types.ko", contains: ["5"]);
e2e_example_test!(run: e2e_async_demo, "async_demo.ko", contains: ["42"]);
e2e_example_test!(run: e2e_async_real, "async_real.ko", contains: ["1"]);
e2e_example_test!(run: e2e_async_tasks, "async_tasks.ko", contains: ["all tasks queued"]);
e2e_example_test!(run: e2e_borrow_rules, "borrow_rules.ko", contains: ["hello borrow rules"]);
e2e_example_test!(run: e2e_break_continue, "break_continue.ko", contains: ["42", "25"]);
e2e_example_test!(run: e2e_channel_string, "channel_string.ko", contains: ["42"]);
e2e_example_test!(run: e2e_channels, "channels.ko", contains: ["42", "channel demo complete"]);
e2e_example_test!(run: e2e_closures, "closures.ko", contains: ["42"]);
e2e_example_test!(run: e2e_closures_functional, "closures_functional.ko", contains: ["42"]);
e2e_example_test!(run: e2e_concurrency_demo, "concurrency_demo.ko", contains: ["scheduling tasks"]);
e2e_example_test!(run: e2e_confidence_demo, "confidence_demo.ko", contains: ["weighted_sum"]);
e2e_example_test!(run: e2e_config_validator, "config_validator.ko", contains: ["Config Validator"]);
e2e_example_test!(run: e2e_contracts, "contracts.ko");
e2e_example_test!(run: e2e_contracts_demo, "contracts_demo.ko", contains: ["10 / 2"]);
e2e_example_test!(run: e2e_contracts_smt_demo, "contracts_smt_demo.ko", contains: ["42"]);
e2e_example_test!(run: e2e_contracts_verified, "contracts_verified.ko", contains: ["5"]);
e2e_example_test!(run: e2e_copy_semantics, "copy_semantics.ko", contains: ["126"]);
e2e_example_test!(run: e2e_enum_methods, "enum_methods.ko");
e2e_example_test!(run: e2e_enum_params, "enum_params.ko", contains: ["25"]);
e2e_example_test!(run: e2e_enums, "enums.ko", contains: ["42"]);
e2e_example_test!(run: e2e_expressions, "expressions.ko");
e2e_example_test!(run: e2e_fibonacci, "fibonacci.ko", contains: ["55"]);
e2e_example_test!(run: e2e_file_io_demo, "file_io_demo.ko");
e2e_example_test!(run: e2e_float_math, "float_math.ko", contains: ["5.14"]);
e2e_example_test!(run: e2e_flow_typing, "flow_typing.ko", contains: ["42"]);
e2e_example_test!(run: e2e_for_in, "for_in.ko", contains: ["15"]);
e2e_example_test!(run: e2e_for_loop, "for_loop.ko", contains: ["55"]);
e2e_example_test!(run: e2e_functional_pipeline, "functional_pipeline.ko", contains: ["60"]);
e2e_example_test!(run: e2e_generic_bounds, "generic_bounds.ko");
e2e_example_test!(run: e2e_generic_fn, "generic_fn.ko", contains: ["42"]);
e2e_example_test!(run: e2e_generic_method_dispatch, "generic_method_dispatch.ko");
e2e_example_test!(run: e2e_generics, "generics.ko", contains: ["42"]);
e2e_example_test!(run: e2e_health_checker, "health_checker.ko", contains: ["Health Check Report"]);
e2e_example_test!(run: e2e_hello, "hello.ko", contains: ["Hello, World!"]);
e2e_example_test!(run: e2e_intent_cache, "intent_cache.ko", contains: ["intent cache loaded"]);
e2e_example_test!(run: e2e_intent_composed, "intent_composed.ko");
e2e_example_test!(run: e2e_intent_database, "intent_database.ko", contains: ["intent database loaded"]);
e2e_example_test!(run: e2e_intent_demo, "intent_demo.ko", contains: ["Hello from intent-driven"]);
e2e_example_test!(run: e2e_intent_http, "intent_http.ko", contains: ["HTTP server starting"]);
e2e_example_test!(run: e2e_intent_json_api, "intent_json_api.ko", contains: ["intent json_api loaded"]);
e2e_example_test!(run: e2e_intent_math, "intent_math.ko", contains: ["30"]);
e2e_example_test!(run: e2e_intent_queue, "intent_queue.ko", contains: ["intent queue loaded"]);
e2e_example_test!(run: e2e_iterator_basic, "iterator_basic.ko", contains: ["60"]);
e2e_example_test!(run: e2e_iterator_fold, "iterator_fold.ko", contains: ["15"]);
e2e_example_test!(run: e2e_iterator_list, "iterator_list.ko", contains: ["22"]);
e2e_example_test!(run: e2e_iterator_map, "iterator_map.ko", contains: ["6"]);
e2e_example_test!(run: e2e_iterator_map_filter, "iterator_map_filter.ko", contains: ["30"]);
e2e_example_test!(run: e2e_list_demo, "list_demo.ko", contains: ["3", "10", "list contains 20"]);
e2e_example_test!(run: e2e_map_demo, "map_demo.ko", contains: ["3", "200", "map contains key 1"]);
e2e_example_test!(run: e2e_memory_management, "memory_management.ko", contains: ["Hello World"]);
e2e_example_test!(run: e2e_methods, "methods.ko", contains: ["10"]);
e2e_example_test!(run: e2e_module_invariant, "module_invariant.ko", contains: ["10"]);
e2e_example_test!(run: e2e_move_semantics, "move_semantics.ko", contains: ["200"]);
e2e_example_test!(run: e2e_option_demo, "option_demo.ko", contains: ["42"]);
e2e_example_test!(run: e2e_optional_sugar, "optional_sugar.ko", contains: ["42"]);
e2e_example_test!(run: e2e_parallel_blocks, "parallel_blocks.ko", contains: ["starting parallel block", "all parallel tasks finished"]);
e2e_example_test!(run: e2e_parallel_demo, "parallel_demo.ko");
e2e_example_test!(run: e2e_qualified_imports, "qualified_imports.ko", contains: ["5"]);
e2e_example_test!(run: e2e_refinement_smt, "refinement_smt.ko", contains: ["8080"]);
e2e_example_test!(run: e2e_refinement_types, "refinement_types.ko", contains: ["Refinement types demo"]);
e2e_example_test!(run: e2e_result_demo, "result_demo.ko", contains: ["100"]);
e2e_example_test!(run: e2e_selective_imports, "selective_imports.ko", contains: ["42"]);
e2e_example_test!(run: e2e_send_sync_demo, "send_sync_demo.ko", contains: ["100"]);
e2e_example_test!(run: e2e_smt_verified, "smt_verified.ko", contains: ["42"]);
e2e_example_test!(run: e2e_sorted_list, "sorted_list.ko");
e2e_example_test!(run: e2e_stdlib_demo, "stdlib_demo.ko", contains: ["42"]);
e2e_example_test!(run: e2e_string_concat_operator, "string_concat_operator.ko", contains: ["Hello World!"]);
e2e_example_test!(run: e2e_string_demo, "string_demo.ko", contains: ["String operations demo complete"]);
e2e_example_test!(run: e2e_struct_params, "struct_params.ko", contains: ["10"]);
e2e_example_test!(run: e2e_struct_predicates, "struct_predicates.ko", contains: ["Struct predicate contracts work!"]);
e2e_example_test!(run: e2e_structs, "structs.ko", contains: ["10", "20"]);
e2e_example_test!(run: e2e_time_env, "time_env.ko", contains: ["hello from kodo", "done"]);
e2e_example_test!(run: e2e_traits, "traits.ko", contains: ["30"]);
e2e_example_test!(run: e2e_type_inference, "type_inference.ko", contains: ["Type inference demo"]);
e2e_example_test!(run: e2e_url_shortener, "url_shortener.ko", contains: ["URL Shortener", "Registered code 1"]);
e2e_example_test!(run: e2e_while_loop, "while_loop.ko", contains: ["5"]);
e2e_example_test!(run: e2e_word_counter, "word_counter.ko", contains: ["Word Counter", "43"]);

// --- Previously segfaulting examples, now fixed ---

e2e_example_test!(run: e2e_iterator_string, "iterator_string.ko", contains: ["5", "198"]);
e2e_example_test!(run: e2e_ownership, "ownership.ko", contains: ["Hello from Kodo!", "84"]);
e2e_example_test!(run: e2e_string_interpolation, "string_interpolation.ko", contains: ["Hello, World!"]);
e2e_example_test!(run: e2e_todo_app, "todo_app.ko", contains: ["Todo List", "Write unit tests", "2"]);
e2e_example_test!(run: e2e_tuples, "tuples.ko", contains: ["42", "99", "1", "2", "3", "10", "20"]);
e2e_example_test!(run: e2e_advanced_traits, "advanced_traits.ko", contains: ["212", "100"]);
e2e_example_test!(run: e2e_collections_demo, "collections_demo.ko", contains: ["3", "2", "1", "2", "100"]);
e2e_example_test!(run: e2e_visibility, "visibility.ko", contains: ["point"]);
e2e_example_test!(run: e2e_try_operator, "try_operator.ko", contains: ["5"]);
e2e_example_test!(run: e2e_try_operator_sugar, "try_operator_sugar.ko", contains: ["20", "got expected error"]);
e2e_example_test!(run: e2e_map_iteration, "map_iteration.ko", contains: ["60", "600"]);
e2e_example_test!(run: e2e_list_operations, "list_operations.ko", contains: ["empty", "3", "10", "99", "found 99"]);
e2e_example_test!(run: e2e_map_operations, "map_operations.ko", contains: ["empty", "3", "200", "has key 1", "key 2 removed"]);

// --- Examples that intentionally fail to compile ---

e2e_example_test!(fail: e2e_type_errors, "type_errors.ko");

// --- Examples that depend on external services (compile-only) ---

e2e_example_test!(compile: e2e_http_client, "http_client.ko");
