/// Build script for kodoc — locates `libkodo_runtime.a` and copies it to
/// OUT_DIR so it can be embedded in the binary via `include_bytes!`.
use std::env;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let embedded_path = out_dir.join("embedded_runtime.a");
    let target = env::var("TARGET").unwrap_or_default();

    let profile = if out_dir.to_string_lossy().contains("release") {
        "release"
    } else {
        "debug"
    };

    // Walk up from OUT_DIR to find the `target/` directory root.
    // OUT_DIR is typically: target/<triple>/<profile>/build/<crate>-<hash>/out
    // or:                   target/<profile>/build/<crate>-<hash>/out
    let target_dir = out_dir.ancestors().find_map(|p| {
        if p.ends_with("target") {
            Some(p.to_path_buf())
        } else if p.join("target").is_dir() {
            Some(p.join("target"))
        } else {
            None
        }
    });

    let mut runtime_path = None;

    if let Some(target_root) = &target_dir {
        // Build candidates: with and without target triple.
        let mut candidates = vec![
            // Cross-compile: target/<triple>/<profile>/
            target_root.join(&target).join(profile).join("libkodo_runtime.a"),
        ];
        // Native compile: target/<profile>/
        candidates.push(target_root.join(profile).join("libkodo_runtime.a"));
        candidates.push(target_root.join("debug").join("libkodo_runtime.a"));
        candidates.push(target_root.join("release").join("libkodo_runtime.a"));

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

    if let Some(path) = &runtime_path {
        std::fs::copy(path, &embedded_path).expect("failed to copy libkodo_runtime.a to OUT_DIR");
        println!("cargo:rerun-if-changed={}", path.display());
    } else {
        // Write an empty file — runtime will be looked up externally at runtime.
        std::fs::write(&embedded_path, b"").expect("failed to write empty embedded_runtime.a");
    }

    println!("cargo:rerun-if-env-changed=KODO_RUNTIME_LIB");
}
