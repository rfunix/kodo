//! Kodo package manifest (`kodo.toml`) parser and writer.
//!
//! The manifest describes a Kodo project: its module name, version, and
//! dependencies. Dependencies can be either git repositories (with a tag)
//! or local filesystem paths.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Represents the contents of a `kodo.toml` manifest file.
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Manifest {
    /// The module name for this project.
    pub module: String,
    /// The project version (semver-ish, e.g. "0.1.0").
    pub version: String,
    /// Dependencies, keyed by name.
    #[serde(default)]
    pub deps: HashMap<String, Dependency>,
}

/// A single dependency specification.
///
/// Either a git repository with a tag, or a local filesystem path.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub(crate) enum Dependency {
    /// A git-hosted dependency.
    Git {
        /// The git repository URL.
        git: String,
        /// The git tag to checkout.
        tag: String,
    },
    /// A local path dependency.
    Path {
        /// The filesystem path, relative to the manifest directory.
        path: String,
    },
}

/// Reads and parses a `kodo.toml` manifest from the given directory.
///
/// Returns an error string if the file does not exist or cannot be parsed.
pub(crate) fn read_manifest(dir: &Path) -> Result<Manifest, String> {
    let manifest_path = dir.join("kodo.toml");
    let content = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("error: could not read `{}`: {e}", manifest_path.display()))?;
    toml::from_str(&content)
        .map_err(|e| format!("error: could not parse `{}`: {e}", manifest_path.display()))
}

/// Writes a `Manifest` to `kodo.toml` in the given directory.
///
/// Overwrites the file if it already exists.
pub(crate) fn write_manifest(dir: &Path, manifest: &Manifest) -> Result<(), String> {
    let manifest_path = dir.join("kodo.toml");
    let content = toml::to_string_pretty(manifest)
        .map_err(|e| format!("error: could not serialize manifest: {e}"))?;
    std::fs::write(&manifest_path, content)
        .map_err(|e| format!("error: could not write `{}`: {e}", manifest_path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_manifest_with_git_dep() {
        let toml_str = r#"
module = "my-app"
version = "0.1.0"

[deps]
json = { git = "https://github.com/user/kodo-json", tag = "v1.0.0" }
"#;
        let manifest: Manifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.module, "my-app");
        assert_eq!(manifest.version, "0.1.0");
        assert_eq!(manifest.deps.len(), 1);
        assert!(matches!(
            manifest.deps.get("json"),
            Some(Dependency::Git { git, tag }) if git == "https://github.com/user/kodo-json" && tag == "v1.0.0"
        ));
    }

    #[test]
    fn parse_manifest_with_path_dep() {
        let toml_str = r#"
module = "my-app"
version = "0.1.0"

[deps]
utils = { path = "../shared-utils" }
"#;
        let manifest: Manifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.deps.len(), 1);
        assert!(matches!(
            manifest.deps.get("utils"),
            Some(Dependency::Path { path }) if path == "../shared-utils"
        ));
    }

    #[test]
    fn parse_manifest_no_deps() {
        let toml_str = r#"
module = "simple"
version = "0.1.0"
"#;
        let manifest: Manifest = toml::from_str(toml_str).unwrap();
        assert!(manifest.deps.is_empty());
    }

    #[test]
    fn roundtrip_manifest() {
        let manifest = Manifest {
            module: "roundtrip".to_string(),
            version: "1.0.0".to_string(),
            deps: HashMap::from([(
                "utils".to_string(),
                Dependency::Path {
                    path: "../utils".to_string(),
                },
            )]),
        };
        let serialized = toml::to_string_pretty(&manifest).unwrap();
        let deserialized: Manifest = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.module, "roundtrip");
        assert_eq!(deserialized.deps.len(), 1);
    }

    #[test]
    fn read_manifest_missing_file() {
        let result = read_manifest(std::path::Path::new("/nonexistent/path"));
        assert!(result.is_err());
    }

    #[test]
    fn write_and_read_manifest() {
        let tmp = std::env::temp_dir().join("kodo_manifest_test");
        let _ = std::fs::create_dir_all(&tmp);
        let manifest = Manifest {
            module: "test-project".to_string(),
            version: "0.1.0".to_string(),
            deps: HashMap::new(),
        };
        write_manifest(&tmp, &manifest).unwrap();
        let read_back = read_manifest(&tmp).unwrap();
        assert_eq!(read_back.module, "test-project");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
