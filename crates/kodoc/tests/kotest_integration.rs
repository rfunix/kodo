//! Integration tests that run UI test files through the kotest harness.
//!
//! These tests validate that the `tests/ui/` directory structure works correctly
//! with the kodoc compiler, ensuring all check-pass tests compile and all
//! compile-fail tests produce the expected errors.

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

/// Reads a UI test file and extracts its mode from the `//@ ` directive.
fn extract_mode(source: &str) -> Option<&str> {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed == "//@ check-pass" {
            return Some("check-pass");
        }
        if trimmed == "//@ compile-fail" {
            return Some("compile-fail");
        }
        if trimmed == "//@ run-pass" {
            return Some("run-pass");
        }
        if trimmed == "//@ run-fail" {
            return Some("run-fail");
        }
    }
    None
}

/// Extracts expected error codes from `//@ error-code: Exxxx` directives.
fn extract_error_codes(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix("//@ error-code:")
                .map(|code| code.trim().to_string())
        })
        .collect()
}

/// Extracts extra compile flags from `//@ compile-flags: ...` directives.
fn extract_compile_flags(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix("//@ compile-flags:")
                .map(|flags| flags.trim().to_string())
        })
        .flat_map(|flags| {
            flags
                .split_whitespace()
                .map(String::from)
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Collects all `.ko` files recursively from a directory.
fn collect_ko_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    if !dir.exists() {
        return files;
    }
    for entry in walkdir(dir) {
        if entry.extension().and_then(|e| e.to_str()) == Some("ko") {
            files.push(entry);
        }
    }
    files.sort();
    files
}

/// Simple recursive directory walk.
fn walkdir(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut result = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                result.extend(walkdir(&path));
            } else {
                result.push(path);
            }
        }
    }
    result
}

/// Runs all check-pass UI tests.
#[test]
fn ui_check_pass_tests() {
    let root = workspace_root();
    let ui_dir = root.join("tests/ui");
    let kodoc = get_kodoc_path();

    let files = collect_ko_files(&ui_dir);
    let mut tested = 0;

    for file in &files {
        let source = std::fs::read_to_string(file).unwrap();
        let mode = match extract_mode(&source) {
            Some(m) => m,
            None => continue,
        };
        if mode != "check-pass" {
            continue;
        }

        let extra_flags = extract_compile_flags(&source);
        let mut args = vec!["check".to_string(), file.to_string_lossy().to_string()];
        args.extend(extra_flags);

        let output = Command::new(&kodoc)
            .args(&args)
            .output()
            .expect("failed to run kodoc");

        let rel_path = file.strip_prefix(&root).unwrap_or(file);
        assert!(
            output.status.success(),
            "UI test {} (check-pass) failed:\nstdout: {}\nstderr: {}",
            rel_path.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        tested += 1;
    }

    assert!(
        tested >= 5,
        "expected at least 5 check-pass UI tests, found {tested}"
    );
}

/// Runs all compile-fail UI tests.
#[test]
fn ui_compile_fail_tests() {
    let root = workspace_root();
    let ui_dir = root.join("tests/ui");
    let kodoc = get_kodoc_path();

    let files = collect_ko_files(&ui_dir);
    let mut tested = 0;

    for file in &files {
        let source = std::fs::read_to_string(file).unwrap();
        let mode = match extract_mode(&source) {
            Some(m) => m,
            None => continue,
        };
        if mode != "compile-fail" {
            continue;
        }

        let error_codes = extract_error_codes(&source);
        let extra_flags = extract_compile_flags(&source);

        let mut args = vec![
            "check".to_string(),
            file.to_string_lossy().to_string(),
            "--json-errors".to_string(),
        ];
        // Add extra flags but avoid duplicate --json-errors
        for flag in &extra_flags {
            if flag != "--json-errors" {
                args.push(flag.clone());
            }
        }

        let output = Command::new(&kodoc)
            .args(&args)
            .output()
            .expect("failed to run kodoc");

        let rel_path = file.strip_prefix(&root).unwrap_or(file);
        assert!(
            !output.status.success(),
            "UI test {} (compile-fail) should have failed but succeeded",
            rel_path.display(),
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        for code in &error_codes {
            assert!(
                stdout.contains(code) || stderr.contains(code),
                "UI test {}: expected error code {code} not found\nstdout: {stdout}\nstderr: {stderr}",
                rel_path.display(),
            );
        }

        tested += 1;
    }

    assert!(
        tested >= 3,
        "expected at least 3 compile-fail UI tests, found {tested}"
    );
}
