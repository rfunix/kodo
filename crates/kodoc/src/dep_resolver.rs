//! Dependency resolution -- clones git repos, resolves paths, manages cache.
//!
//! The resolver takes a parsed manifest and produces a list of resolved
//! dependencies, each with an absolute path to where its `.ko` source files
//! can be found. Git dependencies are cloned into `~/.kodo/cache/`.

use std::path::{Path, PathBuf};

use crate::lockfile::{LockFile, LockedPackage};
use crate::manifest::{Dependency, Manifest};

/// A fully resolved dependency with its source directory.
#[derive(Debug)]
pub(crate) struct ResolvedDep {
    /// The dependency name.
    #[allow(dead_code)]
    pub name: String,
    /// The directory containing the dependency's `.ko` source files.
    pub source_dir: PathBuf,
    /// The resolved commit SHA (for git deps only).
    #[allow(dead_code)]
    pub commit: Option<String>,
}

/// Returns the root cache directory (`~/.kodo/cache/`).
fn cache_dir() -> Result<PathBuf, String> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "error: could not determine home directory".to_string())?;
    Ok(PathBuf::from(home).join(".kodo").join("cache"))
}

/// Computes a short hash of a URL for use as a cache directory name.
///
/// Uses a simple djb2 hash to avoid pulling in a full hashing crate
/// just for directory naming.
fn url_hash(url: &str) -> String {
    let mut hash: u64 = 5381;
    for byte in url.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(u64::from(byte));
    }
    format!("{hash:016x}")
}

/// Clones a git repository to the cache and checks out the specified tag.
///
/// Returns the path to the cloned directory and the resolved commit SHA.
fn clone_git_dep(git_url: &str, tag: &str) -> Result<(PathBuf, String), String> {
    let cache = cache_dir()?;
    let repo_hash = url_hash(git_url);
    let dep_dir = cache.join(&repo_hash).join(tag);

    // If already cached, just read the commit SHA.
    if dep_dir.exists() {
        let sha = read_head_sha(&dep_dir)?;
        return Ok((dep_dir, sha));
    }

    // Create parent directories.
    std::fs::create_dir_all(&dep_dir).map_err(|e| {
        format!(
            "error: could not create cache directory `{}`: {e}",
            dep_dir.display()
        )
    })?;

    // Clone the repository.
    let clone_status = std::process::Command::new("git")
        .args(["clone", "--depth", "1", "--branch", tag, git_url])
        .arg(&dep_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .status()
        .map_err(|e| format!("error: could not run `git clone`: {e}"))?;

    if !clone_status.success() {
        // Clean up the failed clone directory.
        let _ = std::fs::remove_dir_all(&dep_dir);
        return Err(format!(
            "error: `git clone` failed for `{git_url}` at tag `{tag}`"
        ));
    }

    let sha = read_head_sha(&dep_dir)?;
    Ok((dep_dir, sha))
}

/// Reads the HEAD commit SHA from a git repository directory.
fn read_head_sha(repo_dir: &Path) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_dir)
        .output()
        .map_err(|e| format!("error: could not run `git rev-parse HEAD`: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "error: could not read commit SHA in `{}`",
            repo_dir.display()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Resolves all dependencies from a manifest.
///
/// Git deps are cloned to `~/.kodo/cache/`. Path deps are resolved relative
/// to the manifest directory. Returns a list of resolved dependencies and
/// a lock file with the resolved SHAs.
pub(crate) fn resolve_deps(
    manifest: &Manifest,
    manifest_dir: &Path,
) -> Result<(Vec<ResolvedDep>, LockFile), String> {
    let mut resolved = Vec::new();
    let mut locked_packages = Vec::new();

    for (name, dep) in &manifest.deps {
        match dep {
            Dependency::Git { git, tag } => {
                let (source_dir, commit) = clone_git_dep(git, tag)?;
                // Look for a `src/` subdirectory; fall back to the repo root.
                let src_dir = if source_dir.join("src").is_dir() {
                    source_dir.join("src")
                } else {
                    source_dir.clone()
                };
                resolved.push(ResolvedDep {
                    name: name.clone(),
                    source_dir: src_dir,
                    commit: Some(commit.clone()),
                });
                locked_packages.push(LockedPackage {
                    name: name.clone(),
                    git: Some(git.clone()),
                    tag: Some(tag.clone()),
                    commit: Some(commit),
                    path: None,
                });
            }
            Dependency::Path { path } => {
                let abs_path = if Path::new(path).is_absolute() {
                    PathBuf::from(path)
                } else {
                    manifest_dir.join(path)
                };
                if !abs_path.exists() {
                    return Err(format!(
                        "error: path dependency `{name}` not found at `{}`",
                        abs_path.display()
                    ));
                }
                // Look for a `src/` subdirectory; fall back to the path itself.
                let src_dir = if abs_path.join("src").is_dir() {
                    abs_path.join("src")
                } else {
                    abs_path.clone()
                };
                resolved.push(ResolvedDep {
                    name: name.clone(),
                    source_dir: src_dir,
                    commit: None,
                });
                locked_packages.push(LockedPackage {
                    name: name.clone(),
                    git: None,
                    tag: None,
                    commit: None,
                    path: Some(path.clone()),
                });
            }
        }
    }

    // Check for conflicts: same dep name with different sources.
    // Since deps is a HashMap, names are already unique. This is a no-op
    // but kept for clarity / future transitive dep support.

    let lockfile = LockFile {
        package: locked_packages,
    };
    Ok((resolved, lockfile))
}

/// Resolves dependencies using an existing lock file when available.
///
/// If a lock file entry matches a manifest dependency, uses the locked
/// commit SHA to find the cached clone. Falls back to fresh resolution
/// if the cache is missing.
pub(crate) fn resolve_deps_from_lock(
    manifest: &Manifest,
    manifest_dir: &Path,
    lockfile: &LockFile,
) -> Result<(Vec<ResolvedDep>, LockFile), String> {
    let mut resolved = Vec::new();
    let mut locked_packages = Vec::new();

    for (name, dep) in &manifest.deps {
        match dep {
            Dependency::Git { git, tag } => {
                // Check if we have a locked entry with a commit.
                let locked = lockfile.package.iter().find(|p| {
                    p.name == *name
                        && p.git.as_deref() == Some(git.as_str())
                        && p.tag.as_deref() == Some(tag.as_str())
                });

                if let Some(locked_pkg) = locked {
                    if let Some(ref commit) = locked_pkg.commit {
                        // Try to use the cached clone.
                        let cache = cache_dir()?;
                        let repo_hash = url_hash(git);
                        let dep_dir = cache.join(&repo_hash).join(tag);
                        if dep_dir.exists() {
                            let src_dir = if dep_dir.join("src").is_dir() {
                                dep_dir.join("src")
                            } else {
                                dep_dir.clone()
                            };
                            resolved.push(ResolvedDep {
                                name: name.clone(),
                                source_dir: src_dir,
                                commit: Some(commit.clone()),
                            });
                            locked_packages.push(locked_pkg.clone());
                            continue;
                        }
                    }
                }

                // Fall back to fresh clone.
                let (source_dir, commit) = clone_git_dep(git, tag)?;
                let src_dir = if source_dir.join("src").is_dir() {
                    source_dir.join("src")
                } else {
                    source_dir.clone()
                };
                resolved.push(ResolvedDep {
                    name: name.clone(),
                    source_dir: src_dir,
                    commit: Some(commit.clone()),
                });
                locked_packages.push(LockedPackage {
                    name: name.clone(),
                    git: Some(git.clone()),
                    tag: Some(tag.clone()),
                    commit: Some(commit),
                    path: None,
                });
            }
            Dependency::Path { path } => {
                let abs_path = if Path::new(path).is_absolute() {
                    PathBuf::from(path)
                } else {
                    manifest_dir.join(path)
                };
                if !abs_path.exists() {
                    return Err(format!(
                        "error: path dependency `{name}` not found at `{}`",
                        abs_path.display()
                    ));
                }
                let src_dir = if abs_path.join("src").is_dir() {
                    abs_path.join("src")
                } else {
                    abs_path.clone()
                };
                resolved.push(ResolvedDep {
                    name: name.clone(),
                    source_dir: src_dir,
                    commit: None,
                });
                locked_packages.push(LockedPackage {
                    name: name.clone(),
                    git: None,
                    tag: None,
                    commit: None,
                    path: Some(path.clone()),
                });
            }
        }
    }

    let lockfile = LockFile {
        package: locked_packages,
    };
    Ok((resolved, lockfile))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn url_hash_is_deterministic() {
        let h1 = url_hash("https://github.com/user/repo");
        let h2 = url_hash("https://github.com/user/repo");
        assert_eq!(h1, h2);
    }

    #[test]
    fn url_hash_differs_for_different_urls() {
        let h1 = url_hash("https://github.com/user/repo-a");
        let h2 = url_hash("https://github.com/user/repo-b");
        assert_ne!(h1, h2);
    }

    #[test]
    fn resolve_path_dep() {
        let tmp = std::env::temp_dir().join("kodo_resolve_path_test");
        let dep_dir = tmp.join("my-dep");
        let _ = std::fs::create_dir_all(&dep_dir);
        std::fs::write(
            dep_dir.join("lib.ko"),
            "module mydep {\n    fn helper() -> Int { return 1 }\n}\n",
        )
        .unwrap();

        let manifest = Manifest {
            module: "test".to_string(),
            version: "0.1.0".to_string(),
            deps: HashMap::from([(
                "mydep".to_string(),
                Dependency::Path {
                    path: "my-dep".to_string(),
                },
            )]),
            trust: None,
        };

        let (resolved, lockfile) = resolve_deps(&manifest, &tmp).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "mydep");
        assert!(resolved[0].source_dir.exists());
        assert!(resolved[0].commit.is_none());
        assert_eq!(lockfile.package.len(), 1);
        assert_eq!(lockfile.package[0].path.as_deref(), Some("my-dep"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn resolve_path_dep_missing() {
        let manifest = Manifest {
            module: "test".to_string(),
            version: "0.1.0".to_string(),
            deps: HashMap::from([(
                "missing".to_string(),
                Dependency::Path {
                    path: "/absolutely/nonexistent/path".to_string(),
                },
            )]),
            trust: None,
        };

        let result = resolve_deps(&manifest, Path::new("/tmp"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn resolve_path_dep_with_src_subdir() {
        let tmp = std::env::temp_dir().join("kodo_resolve_src_test");
        let dep_dir = tmp.join("my-dep").join("src");
        let _ = std::fs::create_dir_all(&dep_dir);
        std::fs::write(
            dep_dir.join("lib.ko"),
            "module mydep {\n    fn helper() -> Int { return 1 }\n}\n",
        )
        .unwrap();

        let manifest = Manifest {
            module: "test".to_string(),
            version: "0.1.0".to_string(),
            deps: HashMap::from([(
                "mydep".to_string(),
                Dependency::Path {
                    path: "my-dep".to_string(),
                },
            )]),
            trust: None,
        };

        let (resolved, _) = resolve_deps(&manifest, &tmp).unwrap();
        assert!(resolved[0].source_dir.ends_with("src"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn cache_dir_is_under_home() {
        let dir = cache_dir().unwrap();
        assert!(dir.to_str().unwrap_or("").contains(".kodo"));
        assert!(dir.to_str().unwrap_or("").contains("cache"));
    }
}
