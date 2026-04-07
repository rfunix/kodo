//! Dependency management commands: `add`, `remove`, and `update`.
//!
//! These commands modify `kodo.toml` and `kodo.lock` to manage project
//! dependencies. Git dependencies are cloned and their commit SHAs are
//! recorded in the lock file for reproducibility.

use std::path::Path;

use crate::dep_resolver;
use crate::lockfile;
use crate::manifest::{self, Dependency};

/// Adds a git dependency to the current project.
///
/// Parses the current `kodo.toml`, adds the dependency, resolves it
/// (clones the repo, records the SHA), and updates both `kodo.toml`
/// and `kodo.lock`.
pub(crate) fn run_add(url: &str, tag: &str, name: Option<&str>) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: could not determine current directory: {e}");
            return 1;
        }
    };

    let mut man = match manifest::read_manifest(&cwd) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{e}");
            eprintln!("hint: run `kodoc init` first to create a project");
            return 1;
        }
    };

    // Derive dep name from URL if not provided: last path segment minus .git.
    let dep_name = name.map(String::from).unwrap_or_else(|| {
        url.trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("dep")
            .trim_end_matches(".git")
            .to_string()
    });

    if man.deps.contains_key(&dep_name) {
        eprintln!("error: dependency `{dep_name}` already exists in kodo.toml");
        eprintln!("hint: use `kodoc remove {dep_name}` first, then add again");
        return 1;
    }

    // Add the dependency.
    man.deps.insert(
        dep_name.clone(),
        Dependency::Git {
            git: url.to_string(),
            tag: tag.to_string(),
        },
    );

    // Resolve all dependencies to update the lock file.
    let (_, lockfile) = match dep_resolver::resolve_deps(&man, &cwd) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };

    // Write updated manifest and lock file.
    if let Err(e) = manifest::write_manifest(&cwd, &man) {
        eprintln!("{e}");
        return 1;
    }
    if let Err(e) = lockfile::write_lockfile(&cwd, &lockfile) {
        eprintln!("{e}");
        return 1;
    }

    println!("Added dependency `{dep_name}` ({url} @ {tag})");
    0
}

/// Adds a path dependency to the current project.
///
/// Resolves the path, verifies it exists, and updates `kodo.toml` and `kodo.lock`.
pub(crate) fn run_add_path(dep_path: &str, name: Option<&str>) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: could not determine current directory: {e}");
            return 1;
        }
    };

    let mut man = match manifest::read_manifest(&cwd) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{e}");
            eprintln!("hint: run `kodoc init` first to create a project");
            return 1;
        }
    };

    // Derive dep name from path if not provided.
    let dep_name = name.map(String::from).unwrap_or_else(|| {
        Path::new(dep_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("dep")
            .to_string()
    });

    if man.deps.contains_key(&dep_name) {
        eprintln!("error: dependency `{dep_name}` already exists in kodo.toml");
        return 1;
    }

    man.deps.insert(
        dep_name.clone(),
        Dependency::Path {
            path: dep_path.to_string(),
        },
    );

    // Resolve to verify paths and update lock file.
    let (_, lockfile) = match dep_resolver::resolve_deps(&man, &cwd) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };

    if let Err(e) = manifest::write_manifest(&cwd, &man) {
        eprintln!("{e}");
        return 1;
    }
    if let Err(e) = lockfile::write_lockfile(&cwd, &lockfile) {
        eprintln!("{e}");
        return 1;
    }

    println!("Added dependency `{dep_name}` (path: {dep_path})");
    0
}

/// Removes a dependency from the current project.
///
/// Removes the entry from both `kodo.toml` and `kodo.lock`.
pub(crate) fn run_remove(name: &str) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: could not determine current directory: {e}");
            return 1;
        }
    };

    let mut man = match manifest::read_manifest(&cwd) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };

    if man.deps.remove(name).is_none() {
        eprintln!("error: dependency `{name}` not found in kodo.toml");
        return 1;
    }

    // Update lock file: remove the package entry.
    let mut lockfile = match lockfile::read_lockfile(&cwd) {
        Ok(lf) => lf,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    lockfile.package.retain(|p| p.name != name);

    if let Err(e) = manifest::write_manifest(&cwd, &man) {
        eprintln!("{e}");
        return 1;
    }
    if let Err(e) = lockfile::write_lockfile(&cwd, &lockfile) {
        eprintln!("{e}");
        return 1;
    }

    println!("Removed dependency `{name}`");
    0
}

/// Re-resolves dependencies and updates `kodo.lock`.
///
/// If `name` is provided, only that dependency is re-resolved.
/// Otherwise, all dependencies are re-resolved.
pub(crate) fn run_update(name: Option<&str>) -> i32 {
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: could not determine current directory: {e}");
            return 1;
        }
    };

    let man = match manifest::read_manifest(&cwd) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };

    if let Some(dep_name) = name {
        if !man.deps.contains_key(dep_name) {
            eprintln!("error: dependency `{dep_name}` not found in kodo.toml");
            return 1;
        }
    }

    // For update, we always do a fresh resolve (ignoring existing lock file).
    let (_, lockfile) = match dep_resolver::resolve_deps(&man, &cwd) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };

    if let Err(e) = lockfile::write_lockfile(&cwd, &lockfile) {
        eprintln!("{e}");
        return 1;
    }

    let count = lockfile.package.len();
    println!("Updated {count} dependencies in kodo.lock");
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn add_path_dep_and_remove() {
        let tmp = std::env::temp_dir().join("kodo_deps_test_add_remove");
        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::create_dir_all(&tmp);

        // Create a manifest.
        let man = manifest::Manifest {
            module: "test-project".to_string(),
            version: "0.1.0".to_string(),
            deps: HashMap::new(),
            trust: None,
        };
        manifest::write_manifest(&tmp, &man).unwrap();

        // Create a fake dependency directory.
        let dep_dir = tmp.join("my-lib");
        let _ = std::fs::create_dir_all(&dep_dir);
        std::fs::write(
            dep_dir.join("lib.ko"),
            "module mylib { fn foo() -> Int { return 1 } }",
        )
        .unwrap();

        // Read, add dep, write.
        let mut man = manifest::read_manifest(&tmp).unwrap();
        man.deps.insert(
            "mylib".to_string(),
            Dependency::Path {
                path: "my-lib".to_string(),
            },
        );
        manifest::write_manifest(&tmp, &man).unwrap();

        // Resolve.
        let (resolved, lockfile) = dep_resolver::resolve_deps(&man, &tmp).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "mylib");
        lockfile::write_lockfile(&tmp, &lockfile).unwrap();

        // Verify lock file.
        let lf = lockfile::read_lockfile(&tmp).unwrap();
        assert_eq!(lf.package.len(), 1);

        // Remove.
        let mut man = manifest::read_manifest(&tmp).unwrap();
        man.deps.remove("mylib");
        manifest::write_manifest(&tmp, &man).unwrap();

        let man = manifest::read_manifest(&tmp).unwrap();
        assert!(man.deps.is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn dep_name_from_url() {
        let url = "https://github.com/user/kodo-json.git";
        let name = url
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("dep")
            .trim_end_matches(".git");
        assert_eq!(name, "kodo-json");
    }

    #[test]
    fn dep_name_from_url_no_git_suffix() {
        let url = "https://github.com/user/my-lib";
        let name = url
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("dep")
            .trim_end_matches(".git");
        assert_eq!(name, "my-lib");
    }
}
