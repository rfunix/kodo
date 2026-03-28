//! # ko — The Kōdo Build Tool
//!
//! A build tool for Kōdo projects that reads `kodo.toml` configuration
//! and orchestrates compilation, testing, and dependency management by
//! delegating to the `kodoc` compiler.
//!
//! ## Usage
//!
//! ```text
//! ko init my_project     # scaffold new project
//! ko build               # compile current project
//! ko run                 # build and execute
//! ko test                # run tests
//! ko check               # type-check without codegen
//! ko add <source>        # add a dependency
//! ko remove <name>       # remove a dependency
//! ko update              # re-resolve all dependencies
//! ```

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use clap::{Parser, Subcommand};
use serde::Deserialize;

// ─── CLI ────────────────────────────────────────────────────────────────────

/// The Kōdo build tool.
#[derive(Parser)]
#[command(
    name = "ko",
    version,
    about = "Build tool for the Kōdo language",
    long_about = "ko orchestrates Kōdo projects via `kodo.toml`. \
                  Delegates to the `kodoc` compiler for all compilation steps."
)]
struct Cli {
    /// Subcommand to execute.
    #[command(subcommand)]
    cmd: Cmd,
}

/// Available build tool commands.
#[derive(Subcommand)]
enum Cmd {
    /// Compile the current project to a native binary.
    Build {
        /// Emit MIR instead of native code.
        #[arg(long)]
        emit_mir: bool,
        /// Output path for the compiled binary.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Contract verification mode: static, runtime, recoverable, off.
        #[arg(long, default_value = "runtime")]
        contracts: String,
        /// Use LLVM backend (release-optimised).
        #[arg(long)]
        release: bool,
    },
    /// Build and run the current project.
    Run {
        /// Contract verification mode.
        #[arg(long, default_value = "runtime")]
        contracts: String,
    },
    /// Run tests for the current project.
    Test {
        /// Filter tests by name substring.
        #[arg(long)]
        filter: Option<String>,
    },
    /// Type-check the project without generating code.
    Check {
        /// Output errors as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Initialize a new Kōdo project.
    Init {
        /// Project name (creates a subdirectory with this name).
        name: String,
    },
    /// Add a dependency to the current project.
    Add {
        /// Dependency source: a git URL or a local path.
        source: String,
        /// Git tag to use (for git dependencies).
        #[arg(long)]
        tag: Option<String>,
    },
    /// Remove a named dependency from the current project.
    Remove {
        /// Name of the dependency to remove.
        name: String,
    },
    /// Re-resolve and update all (or one) dependencies.
    Update {
        /// Name of a specific dependency to update (updates all if omitted).
        name: Option<String>,
    },
}

// ─── Manifest ───────────────────────────────────────────────────────────────

/// Minimal view of `kodo.toml` — only what `ko` needs to locate the entry point.
#[derive(Deserialize)]
struct Manifest {
    /// Module name, used to infer binary output name and fallback entry file.
    module: String,
}

/// Reads `kodo.toml` from `dir`, returning `None` if absent or unparseable.
fn read_manifest(dir: &Path) -> Option<Manifest> {
    let content = std::fs::read_to_string(dir.join("kodo.toml")).ok()?;
    toml::from_str(&content).ok()
}

// ─── Entry point resolution ──────────────────────────────────────────────────

/// Finds the project entry point to pass to `kodoc`.
///
/// Resolution order:
/// 1. `src/main.ko`
/// 2. `src/<module>.ko` (from `kodo.toml`)
/// 3. Any `.ko` file in `src/` that contains `fn main()`
/// 4. Any `.ko` file in the current directory that contains `fn main()`
fn find_entry_point(dir: &Path, manifest: Option<&Manifest>) -> Result<PathBuf, String> {
    // 1. Canonical location.
    let src_main = dir.join("src").join("main.ko");
    if src_main.exists() {
        return Ok(src_main);
    }

    // 2. src/<module>.ko
    if let Some(m) = manifest {
        let p = dir.join("src").join(format!("{}.ko", m.module));
        if p.exists() {
            return Ok(p);
        }
    }

    // 3. Scan src/ for fn main().
    if let Some(found) = scan_for_main(&dir.join("src")) {
        return Ok(found);
    }

    // 4. Scan current dir.
    if let Some(found) = scan_for_main(dir) {
        return Ok(found);
    }

    Err(
        "could not find entry point — expected `src/main.ko` or a `.ko` file with `fn main()`"
            .to_string(),
    )
}

/// Scans `dir` for the first `.ko` file that contains `fn main()`.
fn scan_for_main(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("ko") && file_has_main(&path) {
            return Some(path);
        }
    }
    None
}

/// Returns `true` if `path` contains the string `fn main()`.
fn file_has_main(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|s| s.contains("fn main()"))
        .unwrap_or(false)
}

// ─── kodoc resolution ────────────────────────────────────────────────────────

/// Resolves the path to the `kodoc` binary.
///
/// Prefers a sibling `kodoc` in the same directory as the running `ko`
/// binary so that the two binaries from the same build are always used
/// together. Falls back to `kodoc` on `PATH`.
fn find_kodoc() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join("kodoc");
            if sibling.exists() {
                return sibling;
            }
        }
    }
    PathBuf::from("kodoc")
}

// ─── Dispatch helper ─────────────────────────────────────────────────────────

/// Spawns `kodoc` with `args`, inherits stdio, and returns the process exit code.
fn run_kodoc(args: &[&str]) -> ExitCode {
    let kodoc = find_kodoc();
    match Command::new(&kodoc).args(args).status() {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: could not launch `{}`: {e}", kodoc.display());
            ExitCode::FAILURE
        }
    }
}

// ─── main ────────────────────────────────────────────────────────────────────

fn main() -> ExitCode {
    let cli = Cli::parse();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    match cli.cmd {
        Cmd::Init { name } => run_kodoc(&["init", &name]),

        Cmd::Build {
            emit_mir,
            output,
            contracts,
            release,
        } => {
            let manifest = read_manifest(&cwd);
            let entry = match find_entry_point(&cwd, manifest.as_ref()) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let entry_str = entry.to_string_lossy().into_owned();
            let output_str = output
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();

            let mut args: Vec<&str> = vec!["build", &entry_str, "--contracts", &contracts];
            if !output_str.is_empty() {
                args.extend(["--output", &output_str]);
            }
            if emit_mir {
                args.push("--emit-mir");
            }
            if release {
                args.push("--release");
            }
            run_kodoc(&args)
        }

        Cmd::Run { contracts } => {
            let manifest = read_manifest(&cwd);
            let entry = match find_entry_point(&cwd, manifest.as_ref()) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let bin_name = manifest
                .as_ref()
                .map(|m| m.module.clone())
                .unwrap_or_else(|| {
                    entry
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("a.out")
                        .to_string()
                });
            let bin_path = cwd.join(&bin_name);
            let entry_str = entry.to_string_lossy().into_owned();
            let bin_str = bin_path.to_string_lossy().into_owned();

            let build = run_kodoc(&[
                "build",
                &entry_str,
                "--output",
                &bin_str,
                "--contracts",
                &contracts,
            ]);
            if build != ExitCode::SUCCESS {
                return build;
            }

            match Command::new(&bin_path).status() {
                Ok(s) if s.success() => ExitCode::SUCCESS,
                Ok(_) => ExitCode::FAILURE,
                Err(e) => {
                    eprintln!("error: could not run `{}`: {e}", bin_path.display());
                    ExitCode::FAILURE
                }
            }
        }

        Cmd::Test { filter } => {
            let manifest = read_manifest(&cwd);
            let entry = match find_entry_point(&cwd, manifest.as_ref()) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let entry_str = entry.to_string_lossy().into_owned();
            let filter_str = filter.unwrap_or_default();
            let mut args: Vec<&str> = vec!["test", &entry_str];
            if !filter_str.is_empty() {
                args.extend(["--filter", &filter_str]);
            }
            run_kodoc(&args)
        }

        Cmd::Check { json } => {
            let manifest = read_manifest(&cwd);
            let entry = match find_entry_point(&cwd, manifest.as_ref()) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let entry_str = entry.to_string_lossy().into_owned();
            let mut args: Vec<&str> = vec!["check", &entry_str];
            if json {
                args.push("--json-errors");
            }
            run_kodoc(&args)
        }

        Cmd::Add { source, tag } => {
            let tag_str = tag.unwrap_or_default();
            let mut args: Vec<&str> = vec!["add", &source];
            if !tag_str.is_empty() {
                args.extend(["--tag", &tag_str]);
            }
            run_kodoc(&args)
        }

        Cmd::Remove { name } => run_kodoc(&["remove", &name]),

        Cmd::Update { name } => {
            let name_str = name.unwrap_or_default();
            if name_str.is_empty() {
                run_kodoc(&["update"])
            } else {
                run_kodoc(&["update", &name_str])
            }
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        dir
    }

    #[test]
    fn find_entry_prefers_src_main_ko() {
        let dir = make_temp_dir("ko_test_src_main");
        fs::write(dir.join("src").join("main.ko"), "fn main() {}").unwrap();
        let entry = find_entry_point(&dir, None).unwrap();
        assert_eq!(entry, dir.join("src").join("main.ko"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_entry_falls_back_to_module_name() {
        let dir = make_temp_dir("ko_test_module_name");
        fs::write(
            dir.join("src").join("myapp.ko"),
            "module myapp { fn main() {} }",
        )
        .unwrap();
        let manifest = Manifest {
            module: "myapp".to_string(),
        };
        let entry = find_entry_point(&dir, Some(&manifest)).unwrap();
        assert_eq!(entry, dir.join("src").join("myapp.ko"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_entry_scans_src_for_fn_main() {
        let dir = make_temp_dir("ko_test_scan_main");
        fs::write(
            dir.join("src").join("app.ko"),
            "module app { fn main() -> Int { return 0 } }",
        )
        .unwrap();
        let entry = find_entry_point(&dir, None).unwrap();
        assert_eq!(entry, dir.join("src").join("app.ko"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_entry_errors_when_no_ko_files() {
        let dir = make_temp_dir("ko_test_no_ko");
        let result = find_entry_point(&dir, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("entry point"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_has_main_true() {
        let dir = make_temp_dir("ko_test_has_main");
        let path = dir.join("test.ko");
        fs::write(&path, "fn main() {}").unwrap();
        assert!(file_has_main(&path));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_has_main_false_for_lib() {
        let dir = make_temp_dir("ko_test_no_main");
        let path = dir.join("lib.ko");
        fs::write(&path, "fn helper() {}").unwrap();
        assert!(!file_has_main(&path));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_manifest_parses_valid_toml() {
        let dir = make_temp_dir("ko_test_manifest");
        fs::write(
            dir.join("kodo.toml"),
            "module = \"myapp\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let m = read_manifest(&dir).unwrap();
        assert_eq!(m.module, "myapp");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_manifest_none_when_absent() {
        let dir = make_temp_dir("ko_test_no_manifest");
        assert!(read_manifest(&dir).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_kodoc_returns_non_empty_path() {
        let path = find_kodoc();
        assert!(!path.as_os_str().is_empty());
    }
}
