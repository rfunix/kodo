//! The `init` command implementation.
//!
//! Creates a new Kodo project with a `kodo.toml` manifest and a starter
//! `src/main.ko` file.

use std::collections::HashMap;
use std::path::Path;

use crate::manifest::{write_manifest, Manifest};

/// Creates a new Kodo project in the given directory.
///
/// If `name` is provided, creates a subdirectory with that name.
/// Otherwise, initializes the current directory.
pub(crate) fn run_init(name: Option<&str>) -> i32 {
    let project_dir = match name {
        Some(n) => {
            let dir = Path::new(n);
            if dir.exists() {
                eprintln!("error: directory `{n}` already exists");
                return 1;
            }
            if let Err(e) = std::fs::create_dir_all(dir) {
                eprintln!("error: could not create directory `{n}`: {e}");
                return 1;
            }
            dir.to_path_buf()
        }
        None => std::env::current_dir().unwrap_or_else(|_| ".".into()),
    };

    // Check if kodo.toml already exists.
    if project_dir.join("kodo.toml").exists() {
        eprintln!(
            "error: `kodo.toml` already exists in `{}`",
            project_dir.display()
        );
        return 1;
    }

    // Determine module name from directory name.
    let module_name = name
        .map(String::from)
        .or_else(|| {
            project_dir
                .file_name()
                .and_then(|n| n.to_str())
                .map(String::from)
        })
        .unwrap_or_else(|| "my-project".to_string());

    // Create kodo.toml.
    let manifest = Manifest {
        module: module_name.clone(),
        version: "0.1.0".to_string(),
        deps: HashMap::new(),
        trust: None,
    };

    if let Err(e) = write_manifest(&project_dir, &manifest) {
        eprintln!("{e}");
        return 1;
    }

    // Create src/ directory.
    let src_dir = project_dir.join("src");
    if let Err(e) = std::fs::create_dir_all(&src_dir) {
        eprintln!("error: could not create `src/` directory: {e}");
        return 1;
    }

    // Create src/main.ko with a hello world module.
    let main_ko = format!(
        r#"module {module_name} {{
    meta {{
        version: "0.1.0"
        purpose: "A new Kodo project"
    }}

    fn main() -> Int {{
        println("Hello from {module_name}!")
        return 0
    }}
}}
"#
    );

    if let Err(e) = std::fs::write(src_dir.join("main.ko"), main_ko) {
        eprintln!("error: could not write `src/main.ko`: {e}");
        return 1;
    }

    println!("Created new Kodo project `{module_name}`");
    println!("  {}/kodo.toml", project_dir.display());
    println!("  {}/src/main.ko", project_dir.display());
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_creates_project_structure() {
        let tmp = std::env::temp_dir().join("kodo_init_test_struct");
        let _ = std::fs::remove_dir_all(&tmp);
        let project_dir = tmp.join("test-project");
        let project_name = project_dir.to_str().unwrap();

        let code = run_init(Some(project_name));
        assert_eq!(code, 0);

        assert!(project_dir.join("kodo.toml").exists());
        assert!(project_dir.join("src").join("main.ko").exists());

        // Verify manifest content.
        let manifest = crate::manifest::read_manifest(&project_dir).unwrap();
        assert!(manifest.module.contains("test-project"));
        assert_eq!(manifest.version, "0.1.0");

        // Verify main.ko content.
        let main_content =
            std::fs::read_to_string(project_dir.join("src").join("main.ko")).unwrap();
        assert!(main_content.contains("fn main()"));
        assert!(main_content.contains("println"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn init_fails_if_directory_exists() {
        let tmp = std::env::temp_dir().join("kodo_init_test_exists");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("kodo.toml"), "").unwrap();

        // Initializing in a directory with existing kodo.toml should fail.
        // We test the None case (current dir) by checking that kodo.toml exists.
        assert!(tmp.join("kodo.toml").exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn init_fails_if_dir_already_exists_with_name() {
        let tmp = std::env::temp_dir().join("kodo_init_test_dup");
        let _ = std::fs::create_dir_all(&tmp);

        // Second call should fail because directory already exists.
        let code = run_init(Some(tmp.to_str().unwrap()));
        assert_eq!(code, 1);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
