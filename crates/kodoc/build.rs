/// Build script for kodoc — locates `libkodo_runtime.a` and embeds its path
/// so it can be included in the binary via `include_bytes!`.
use std::env;
use std::path::PathBuf;

fn main() {
    // The runtime crate is built as a staticlib in the same workspace.
    // Cargo places it alongside our binary in the target directory.
    let out_dir = env::var("OUT_DIR").unwrap();
    let profile = if out_dir.contains("release") {
        "release"
    } else {
        "debug"
    };

    // Walk up from OUT_DIR to find the target directory root.
    // OUT_DIR is typically: target/<profile>/build/<crate>-<hash>/out
    let out_path = PathBuf::from(&out_dir);
    let target_dir = out_path
        .ancestors()
        .find(|p| p.ends_with("target") || p.join(profile).exists())
        .map(|p| {
            if p.ends_with("target") {
                p.to_path_buf()
            } else {
                p.join("target")
            }
        });

    let mut runtime_path = None;

    if let Some(target) = &target_dir {
        // Try the current profile first, then the other.
        let candidates = [
            target.join(profile).join("libkodo_runtime.a"),
            target.join("debug").join("libkodo_runtime.a"),
            target.join("release").join("libkodo_runtime.a"),
        ];
        for candidate in &candidates {
            if candidate.exists() {
                runtime_path = Some(candidate.clone());
                break;
            }
        }
    }

    // Also try KODO_RUNTIME_LIB env var.
    if runtime_path.is_none() {
        if let Ok(path) = env::var("KODO_RUNTIME_LIB") {
            let p = PathBuf::from(&path);
            if p.exists() {
                runtime_path = Some(p);
            }
        }
    }

    if let Some(path) = runtime_path {
        println!("cargo:rustc-env=KODO_RUNTIME_LIB_PATH={}", path.display());
        println!("cargo:rerun-if-changed={}", path.display());
    } else {
        // If not found, set a placeholder — the embedded bytes will be empty
        // and the runtime will fall back to external lookup.
        println!("cargo:rustc-env=KODO_RUNTIME_LIB_PATH=");
    }

    println!("cargo:rerun-if-env-changed=KODO_RUNTIME_LIB");
}
