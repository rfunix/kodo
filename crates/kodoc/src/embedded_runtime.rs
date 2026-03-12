//! Embedded runtime library for self-contained `kodoc` binaries.
//!
//! At compile time, `libkodo_runtime.a` is embedded into the binary via
//! `include_bytes!`. At runtime, it is extracted to a cache directory on
//! first use, so the linker can consume it.

use std::path::PathBuf;

/// The embedded runtime bytes, copied to OUT_DIR by build.rs.
/// Empty when the runtime was not found at build time.
const EMBEDDED_RUNTIME: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/embedded_runtime.a"));

/// Returns the path to `libkodo_runtime.a`, extracting from the embedded
/// copy if necessary.
///
/// Search order:
/// 1. `KODO_RUNTIME_LIB` environment variable
/// 2. Next to the current executable
/// 3. Common cargo target directories (`target/debug/`, `target/release/`)
/// 4. Embedded copy (extracted to `~/.kodo/lib/`)
pub fn find_runtime_lib() -> Result<PathBuf, String> {
    // 1. Check KODO_RUNTIME_LIB env var.
    if let Ok(path) = std::env::var("KODO_RUNTIME_LIB") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
    }

    // 2. Check relative to the current executable.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("libkodo_runtime.a");
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }

    // 3. Check common cargo target directories.
    let candidates = [
        "target/debug/libkodo_runtime.a",
        "target/release/libkodo_runtime.a",
    ];
    for candidate in &candidates {
        let p = PathBuf::from(candidate);
        if p.exists() {
            return Ok(p);
        }
    }

    // 4. Extract embedded runtime to ~/.kodo/lib/.
    extract_embedded_runtime()
}

/// Extracts the embedded runtime to `~/.kodo/lib/libkodo_runtime.a`.
fn extract_embedded_runtime() -> Result<PathBuf, String> {
    if EMBEDDED_RUNTIME.is_empty() {
        return Err(
            "could not find libkodo_runtime.a — build the workspace first with `cargo build`"
                .to_string(),
        );
    }

    let lib_dir = dirs_path()
        .ok_or_else(|| "could not determine home directory for runtime extraction".to_string())?;

    std::fs::create_dir_all(&lib_dir)
        .map_err(|e| format!("could not create {}: {e}", lib_dir.display()))?;

    let runtime_path = lib_dir.join("libkodo_runtime.a");

    // Only write if missing or size differs (avoids unnecessary I/O).
    let needs_write = match std::fs::metadata(&runtime_path) {
        Ok(meta) => meta.len() != EMBEDDED_RUNTIME.len() as u64,
        Err(_) => true,
    };

    if needs_write {
        std::fs::write(&runtime_path, EMBEDDED_RUNTIME)
            .map_err(|e| format!("could not write {}: {e}", runtime_path.display()))?;
    }

    Ok(runtime_path)
}

/// Returns the path `~/.kodo/lib/`.
fn dirs_path() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join(".kodo").join("lib"))
    }
    #[cfg(not(unix))]
    {
        std::env::var("USERPROFILE")
            .ok()
            .map(|h| PathBuf::from(h).join(".kodo").join("lib"))
    }
}
