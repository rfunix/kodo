//! # kotest — UI test harness for the Kōdo compiler
//!
//! Inspired by Rust's `compiletest`, this tool reads `.ko` files with embedded
//! test directives and verifies that the compiler produces the expected output.
//!
//! ## Usage
//!
//! ```bash
//! # Run all UI tests
//! cargo run -p kotest -- tests/ui/
//!
//! # Auto-update baselines
//! cargo run -p kotest -- tests/ui/ --bless
//!
//! # Run a specific test
//! cargo run -p kotest -- tests/ui/basics/hello.ko
//!
//! # Filter by pattern
//! cargo run -p kotest -- tests/ui/ --filter contracts
//! ```

mod compare;
mod directives;

use clap::Parser;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Kōdo UI test harness — compiletest-inspired.
#[derive(Parser)]
#[command(name = "kotest", about = "UI test harness for the Kōdo compiler")]
struct Cli {
    /// Path to a test file or directory of tests.
    #[arg()]
    path: PathBuf,

    /// Auto-update baseline files with actual output.
    #[arg(long, default_value_t = false)]
    bless: bool,

    /// Filter tests by substring match on filename.
    #[arg(long)]
    filter: Option<String>,

    /// Path to the kodoc binary (auto-detected if not specified).
    #[arg(long)]
    kodoc: Option<PathBuf>,

    /// Show verbose output for passing tests.
    #[arg(long, short, default_value_t = false)]
    verbose: bool,

    /// Default contract mode for tests that don't specify compile-flags.
    /// Passed as `--contracts=<mode>` to kodoc.
    #[arg(long)]
    contracts: Option<String>,
}

/// Counters for test results.
static PASSED: AtomicUsize = AtomicUsize::new(0);
static FAILED: AtomicUsize = AtomicUsize::new(0);
static SKIPPED: AtomicUsize = AtomicUsize::new(0);
static BLESSED: AtomicUsize = AtomicUsize::new(0);

fn main() {
    let cli = Cli::parse();
    let kodoc = find_kodoc(&cli.kodoc);

    let test_files = collect_test_files(&cli.path, cli.filter.as_deref());

    if test_files.is_empty() {
        eprintln!("kotest: no test files found in {}", cli.path.display());
        std::process::exit(1);
    }

    eprintln!("\nrunning {} UI tests", test_files.len());
    eprintln!();

    for test_file in &test_files {
        run_single_test(
            test_file,
            &kodoc,
            cli.bless,
            cli.verbose,
            cli.contracts.as_deref(),
        );
    }

    let passed = PASSED.load(Ordering::Relaxed);
    let failed = FAILED.load(Ordering::Relaxed);
    let skipped = SKIPPED.load(Ordering::Relaxed);
    let blessed = BLESSED.load(Ordering::Relaxed);

    eprintln!();
    if cli.bless && blessed > 0 {
        eprintln!("blessed {blessed} baselines");
    }
    eprintln!(
        "test result: {}. {passed} passed; {failed} failed; {skipped} skipped",
        if failed > 0 { "FAILED" } else { "ok" }
    );

    if failed > 0 {
        std::process::exit(1);
    }
}

/// Finds the kodoc binary, either from CLI arg or by searching common locations.
fn find_kodoc(explicit: &Option<PathBuf>) -> PathBuf {
    if let Some(path) = explicit {
        return path.clone();
    }

    // Try cargo build output in target/debug
    let workspace_root = find_workspace_root();
    let debug_path = workspace_root.join("target/debug/kodoc");
    if debug_path.exists() {
        return debug_path;
    }

    let release_path = workspace_root.join("target/release/kodoc");
    if release_path.exists() {
        return release_path;
    }

    // Try PATH
    if let Ok(output) = Command::new("which").arg("kodoc").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            return PathBuf::from(path);
        }
    }

    eprintln!("kotest: could not find kodoc binary. Build first with `cargo build -p kodoc` or use --kodoc");
    std::process::exit(1);
}

/// Finds the workspace root by looking for Cargo.toml with `\[workspace\]`.
fn find_workspace_root() -> PathBuf {
    let mut dir = std::env::current_dir().expect("could not get current directory");
    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                if content.contains("[workspace]") {
                    return dir;
                }
            }
        }
        if !dir.pop() {
            // Fallback to current directory
            return std::env::current_dir().expect("could not get current directory");
        }
    }
}

/// Collects all `.ko` test files from a path (file or directory).
fn collect_test_files(path: &Path, filter: Option<&str>) -> Vec<PathBuf> {
    let mut files = Vec::new();

    if path.is_file() {
        if path.extension().and_then(|e| e.to_str()) == Some("ko") {
            files.push(path.to_path_buf());
        }
    } else if path.is_dir() {
        collect_test_files_recursive(path, &mut files);
        files.sort();
    }

    if let Some(filter) = filter {
        files.retain(|f| {
            f.to_string_lossy()
                .to_lowercase()
                .contains(&filter.to_lowercase())
        });
    }

    files
}

/// Recursively collects `.ko` files from a directory.
fn collect_test_files_recursive(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_test_files_recursive(&path, files);
        } else if path.extension().and_then(|e| e.to_str()) == Some("ko") {
            files.push(path);
        }
    }
}

/// Runs a single UI test and reports the result.
fn run_single_test(
    test_path: &Path,
    kodoc: &Path,
    bless: bool,
    verbose: bool,
    default_contracts: Option<&str>,
) {
    let source = match std::fs::read_to_string(test_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("test {} ... SKIP (read error: {e})", test_path.display());
            SKIPPED.fetch_add(1, Ordering::Relaxed);
            return;
        }
    };

    let mut dirs = match directives::parse_directives(&source) {
        Some(d) => d,
        None => {
            // No directives — infer mode from path
            let mode = directives::infer_mode_from_path(test_path);
            directives::TestDirectives {
                mode,
                ..Default::default()
            }
        }
    };

    // Inject default --contracts mode from CLI if the test doesn't specify one
    if let Some(mode) = default_contracts {
        let has_contracts_flag = dirs
            .compile_flags
            .iter()
            .any(|f| f.starts_with("--contracts"));
        if !has_contracts_flag {
            dirs.compile_flags.push(format!("--contracts={mode}"));
        }
    }

    let test_name = test_path
        .strip_prefix(find_workspace_root())
        .unwrap_or(test_path)
        .display()
        .to_string();

    let result = execute_test(test_path, kodoc, &dirs, bless);

    match result {
        TestResult::Pass => {
            if verbose {
                eprintln!("test {test_name} ... ok");
            } else {
                eprint!(".");
            }
            PASSED.fetch_add(1, Ordering::Relaxed);
        }
        TestResult::Blessed => {
            eprintln!("test {test_name} ... blessed");
            PASSED.fetch_add(1, Ordering::Relaxed);
            BLESSED.fetch_add(1, Ordering::Relaxed);
        }
        TestResult::Fail(reason) => {
            eprintln!("test {test_name} ... FAILED");
            eprintln!("  {reason}");
            FAILED.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// The result of running a single test.
enum TestResult {
    Pass,
    Blessed,
    Fail(String),
}

/// Executes a test according to its directives and returns the result.
fn execute_test(
    test_path: &Path,
    kodoc: &Path,
    dirs: &directives::TestDirectives,
    bless: bool,
) -> TestResult {
    match dirs.mode {
        directives::TestMode::CheckPass => execute_check_pass(test_path, kodoc, dirs, bless),
        directives::TestMode::CompileFail => execute_compile_fail(test_path, kodoc, dirs, bless),
        directives::TestMode::RunPass => execute_run_pass(test_path, kodoc, dirs, bless),
        directives::TestMode::RunFail => execute_run_fail(test_path, kodoc, dirs, bless),
    }
}

/// Executes a `check-pass` test: compilation must succeed.
fn execute_check_pass(
    test_path: &Path,
    kodoc: &Path,
    dirs: &directives::TestDirectives,
    _bless: bool,
) -> TestResult {
    let mut args = vec!["check".to_string(), test_path.to_string_lossy().to_string()];
    args.extend(dirs.compile_flags.clone());

    let output = Command::new(kodoc).args(&args).output();

    match output {
        Ok(out) => {
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let stdout = String::from_utf8_lossy(&out.stdout);
                TestResult::Fail(format!(
                    "expected check-pass but compilation failed:\nstderr: {stderr}\nstdout: {stdout}"
                ))
            } else {
                TestResult::Pass
            }
        }
        Err(e) => TestResult::Fail(format!("failed to run kodoc: {e}")),
    }
}

/// Executes a `compile-fail` test: compilation must fail with expected errors.
fn execute_compile_fail(
    test_path: &Path,
    kodoc: &Path,
    dirs: &directives::TestDirectives,
    bless: bool,
) -> TestResult {
    let mut args = vec![
        "check".to_string(),
        test_path.to_string_lossy().to_string(),
        "--json-errors".to_string(),
    ];
    args.extend(dirs.compile_flags.clone());

    let output = match Command::new(kodoc).args(&args).output() {
        Ok(out) => out,
        Err(e) => return TestResult::Fail(format!("failed to run kodoc: {e}")),
    };

    if output.status.success() {
        return TestResult::Fail("expected compile-fail but compilation succeeded".to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Check expected error codes
    for code in &dirs.error_codes {
        if !stdout.contains(code) && !stderr.contains(code) {
            return TestResult::Fail(format!("expected error code {code} not found in output"));
        }
    }

    // Check inline annotations (bidirectional)
    if !dirs.annotations.is_empty() {
        let annotation_errors = compare::verify_annotations(&dirs.annotations, &stdout);
        if !annotation_errors.is_empty() {
            return TestResult::Fail(annotation_errors.join("\n  "));
        }
    }

    // Compare stderr baseline
    let stderr_result = compare::compare_output(test_path, &dirs.stderr_ext, &stderr, bless);
    match stderr_result {
        compare::CompareResult::Match => {}
        compare::CompareResult::NoBaseline(path) => {
            if bless {
                return TestResult::Blessed;
            }
            return TestResult::Fail(format!(
                "no stderr baseline: {}\n  Run with --bless to create it",
                path.display()
            ));
        }
        compare::CompareResult::Mismatch { diff, .. } => {
            if bless {
                return TestResult::Blessed;
            }
            return TestResult::Fail(format!("stderr mismatch:\n{diff}"));
        }
    }

    if bless {
        TestResult::Blessed
    } else {
        TestResult::Pass
    }
}

/// Executes a `run-pass` test: must compile AND run successfully.
fn execute_run_pass(
    test_path: &Path,
    kodoc: &Path,
    dirs: &directives::TestDirectives,
    bless: bool,
) -> TestResult {
    let output_dir = std::env::temp_dir().join("kotest");
    let _ = std::fs::create_dir_all(&output_dir);
    let binary_name = test_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let binary_path = output_dir.join(&binary_name);

    // Compile
    let mut args = vec![
        "build".to_string(),
        test_path.to_string_lossy().to_string(),
        "-o".to_string(),
        binary_path.to_string_lossy().to_string(),
    ];
    args.extend(dirs.compile_flags.clone());

    let compile = match Command::new(kodoc).args(&args).output() {
        Ok(out) => out,
        Err(e) => return TestResult::Fail(format!("failed to run kodoc: {e}")),
    };

    if !compile.status.success() {
        let stderr = String::from_utf8_lossy(&compile.stderr);
        let stdout = String::from_utf8_lossy(&compile.stdout);
        return TestResult::Fail(format!(
            "expected run-pass but compilation failed:\nstderr: {stderr}\nstdout: {stdout}"
        ));
    }

    // Run
    let run = match Command::new(&binary_path).output() {
        Ok(out) => out,
        Err(e) => return TestResult::Fail(format!("failed to execute binary: {e}")),
    };

    if !run.status.success() {
        let stderr = String::from_utf8_lossy(&run.stderr);
        return TestResult::Fail(format!(
            "binary exited with non-zero status:\nstderr: {stderr}"
        ));
    }

    let stdout = String::from_utf8_lossy(&run.stdout).to_string();

    // Compare stdout baseline
    let stdout_result = compare::compare_output(test_path, &dirs.stdout_ext, &stdout, bless);
    match stdout_result {
        compare::CompareResult::Match => {}
        compare::CompareResult::NoBaseline(path) => {
            if !stdout.is_empty() {
                if bless {
                    return TestResult::Blessed;
                }
                return TestResult::Fail(format!(
                    "no stdout baseline: {}\n  Run with --bless to create it",
                    path.display()
                ));
            }
        }
        compare::CompareResult::Mismatch { diff, .. } => {
            if bless {
                return TestResult::Blessed;
            }
            return TestResult::Fail(format!("stdout mismatch:\n{diff}"));
        }
    }

    // Check certificate baseline if requested
    if dirs.check_cert {
        let cert_path = format!("{}.cert.json", test_path.to_string_lossy());
        let cert_path = PathBuf::from(cert_path.replace(".ko.cert.json", ".cert.json"));
        if cert_path.exists() || bless {
            let actual_cert_path = test_path.with_extension("ko.cert.json");
            if actual_cert_path.exists() {
                let cert_content = std::fs::read_to_string(&actual_cert_path).unwrap_or_default();
                let cert_result =
                    compare::compare_output(test_path, "cert.json", &cert_content, bless);
                match cert_result {
                    compare::CompareResult::Match => {}
                    compare::CompareResult::NoBaseline(path) => {
                        if bless {
                            return TestResult::Blessed;
                        }
                        return TestResult::Fail(format!("no cert baseline: {}", path.display()));
                    }
                    compare::CompareResult::Mismatch { diff, .. } => {
                        if bless {
                            return TestResult::Blessed;
                        }
                        return TestResult::Fail(format!("cert mismatch:\n{diff}"));
                    }
                }
            }
        }
    }

    // Clean up binary
    let _ = std::fs::remove_file(&binary_path);

    if bless {
        TestResult::Blessed
    } else {
        TestResult::Pass
    }
}

/// Executes a `run-fail` test: must compile but fail at runtime.
fn execute_run_fail(
    test_path: &Path,
    kodoc: &Path,
    dirs: &directives::TestDirectives,
    bless: bool,
) -> TestResult {
    let output_dir = std::env::temp_dir().join("kotest");
    let _ = std::fs::create_dir_all(&output_dir);
    let binary_name = test_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let binary_path = output_dir.join(&binary_name);

    // Compile
    let mut args = vec![
        "build".to_string(),
        test_path.to_string_lossy().to_string(),
        "-o".to_string(),
        binary_path.to_string_lossy().to_string(),
    ];
    args.extend(dirs.compile_flags.clone());

    let compile = match Command::new(kodoc).args(&args).output() {
        Ok(out) => out,
        Err(e) => return TestResult::Fail(format!("failed to run kodoc: {e}")),
    };

    if !compile.status.success() {
        let stderr = String::from_utf8_lossy(&compile.stderr);
        return TestResult::Fail(format!(
            "expected run-fail but compilation failed:\nstderr: {stderr}"
        ));
    }

    // Run — should fail
    let run = match Command::new(&binary_path).output() {
        Ok(out) => out,
        Err(e) => return TestResult::Fail(format!("failed to execute binary: {e}")),
    };

    if run.status.success() {
        return TestResult::Fail(
            "expected runtime failure but binary exited successfully".to_string(),
        );
    }

    let stderr = String::from_utf8_lossy(&run.stderr).to_string();

    // Compare stderr baseline
    let stderr_result = compare::compare_output(test_path, &dirs.stderr_ext, &stderr, bless);
    match stderr_result {
        compare::CompareResult::Match => {}
        compare::CompareResult::NoBaseline(path) => {
            if bless {
                return TestResult::Blessed;
            }
            return TestResult::Fail(format!("no stderr baseline: {}", path.display()));
        }
        compare::CompareResult::Mismatch { diff, .. } => {
            if bless {
                return TestResult::Blessed;
            }
            return TestResult::Fail(format!("stderr mismatch:\n{diff}"));
        }
    }

    // Clean up binary
    let _ = std::fs::remove_file(&binary_path);

    if bless {
        TestResult::Blessed
    } else {
        TestResult::Pass
    }
}
