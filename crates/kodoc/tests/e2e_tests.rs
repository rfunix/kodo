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
