//! Lock file (`kodo.lock`) for reproducible builds.
//!
//! The lock file records the exact commit SHA for each git dependency,
//! ensuring that builds are deterministic even if tags are moved.
//! Path dependencies are also recorded for completeness.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Represents the contents of a `kodo.lock` file.
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct LockFile {
    /// The list of locked packages.
    #[serde(default)]
    pub package: Vec<LockedPackage>,
}

/// A single locked package entry.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct LockedPackage {
    /// The dependency name (matches the key in `kodo.toml` deps).
    pub name: String,
    /// The git repository URL (for git deps).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<String>,
    /// The git tag (for git deps).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// The resolved commit SHA (for git deps).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// The local filesystem path (for path deps).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Reads and parses a `kodo.lock` file from the given directory.
///
/// Returns an empty lock file if the file does not exist.
pub(crate) fn read_lockfile(dir: &Path) -> Result<LockFile, String> {
    let lock_path = dir.join("kodo.lock");
    if !lock_path.exists() {
        return Ok(LockFile::default());
    }
    let content = std::fs::read_to_string(&lock_path)
        .map_err(|e| format!("error: could not read `{}`: {e}", lock_path.display()))?;
    toml::from_str(&content)
        .map_err(|e| format!("error: could not parse `{}`: {e}", lock_path.display()))
}

/// Writes a `LockFile` to `kodo.lock` in the given directory.
pub(crate) fn write_lockfile(dir: &Path, lockfile: &LockFile) -> Result<(), String> {
    let lock_path = dir.join("kodo.lock");
    let content = toml::to_string_pretty(lockfile)
        .map_err(|e| format!("error: could not serialize lock file: {e}"))?;
    std::fs::write(&lock_path, content)
        .map_err(|e| format!("error: could not write `{}`: {e}", lock_path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_lockfile_with_git_package() {
        let toml_str = r#"
[[package]]
name = "json"
git = "https://github.com/user/kodo-json"
tag = "v1.0.0"
commit = "a1b2c3d4e5f6"
"#;
        let lockfile: LockFile = toml::from_str(toml_str).unwrap();
        assert_eq!(lockfile.package.len(), 1);
        assert_eq!(lockfile.package[0].name, "json");
        assert_eq!(lockfile.package[0].commit.as_deref(), Some("a1b2c3d4e5f6"));
    }

    #[test]
    fn parse_lockfile_with_path_package() {
        let toml_str = r#"
[[package]]
name = "utils"
path = "../shared-utils"
"#;
        let lockfile: LockFile = toml::from_str(toml_str).unwrap();
        assert_eq!(lockfile.package.len(), 1);
        assert_eq!(lockfile.package[0].name, "utils");
        assert_eq!(lockfile.package[0].path.as_deref(), Some("../shared-utils"));
        assert!(lockfile.package[0].git.is_none());
    }

    #[test]
    fn read_lockfile_missing_returns_empty() {
        let result = read_lockfile(std::path::Path::new("/nonexistent/path"));
        let lockfile = result.unwrap();
        assert!(lockfile.package.is_empty());
    }

    #[test]
    fn roundtrip_lockfile() {
        let lockfile = LockFile {
            package: vec![
                LockedPackage {
                    name: "json".to_string(),
                    git: Some("https://github.com/user/kodo-json".to_string()),
                    tag: Some("v1.0.0".to_string()),
                    commit: Some("abc123".to_string()),
                    path: None,
                },
                LockedPackage {
                    name: "utils".to_string(),
                    git: None,
                    tag: None,
                    commit: None,
                    path: Some("../utils".to_string()),
                },
            ],
        };
        let serialized = toml::to_string_pretty(&lockfile).unwrap();
        let deserialized: LockFile = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.package.len(), 2);
        assert_eq!(deserialized.package[0].name, "json");
        assert_eq!(deserialized.package[1].name, "utils");
    }

    #[test]
    fn write_and_read_lockfile() {
        let tmp = std::env::temp_dir().join("kodo_lockfile_test");
        let _ = std::fs::create_dir_all(&tmp);
        let lockfile = LockFile {
            package: vec![LockedPackage {
                name: "test-dep".to_string(),
                git: None,
                tag: None,
                commit: None,
                path: Some("./local-dep".to_string()),
            }],
        };
        write_lockfile(&tmp, &lockfile).unwrap();
        let read_back = read_lockfile(&tmp).unwrap();
        assert_eq!(read_back.package.len(), 1);
        assert_eq!(read_back.package[0].name, "test-dep");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
