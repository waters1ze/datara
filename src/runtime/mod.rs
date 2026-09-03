//! The C runtime that generated Datara programs link against.
//!
//! `build.rs` compiles `src/runtime/datara_runtime.c` into `OUT_DIR` and passes
//! its location to this crate through `DATARA_RUNTIME_LIB`. Nothing in the
//! compiler may reference the runtime by an absolute path: the checkout
//! location is not knowable at compile time, and a stale checked-in object file
//! silently diverges from its source (that is how a float-printing bug once
//! survived into released builds).

use std::path::PathBuf;

pub mod parallel;

pub use parallel::{ExecutionStrategy, ParallelRuntime, TaskHandle};

/// Path to the native runtime archive (`datara_runtime.lib` on MSVC,
/// `libdatara_runtime.a` elsewhere).
///
/// Uses `env!` rather than `option_env!` on purpose: if `build.rs` did not run,
/// the compiler would otherwise link nothing and every generated program would
/// fail much later with an unrelated missing-symbol error.
pub fn runtime_lib_path() -> PathBuf {
    let lib_name = runtime_lib_name();

    // 1. Fresh compile-time baked OUT_DIR path from build.rs (top priority)
    let baked = PathBuf::from(env!("DATARA_RUNTIME_LIB"));
    if baked.exists() {
        return baked;
    }

    // 2. Check relative to current executable (e.g. dist installation or portable zip)
    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        let next_to_exe = exe_dir.join(lib_name);
        if next_to_exe.exists() {
            return next_to_exe;
        }
        let in_runtime_dir = exe_dir.join("runtime").join(lib_name);
        if in_runtime_dir.exists() {
            return in_runtime_dir;
        }
        let in_parent_runtime = exe_dir.parent().map(|p| p.join("runtime").join(lib_name));
        if let Some(p) = in_parent_runtime
            && p.exists()
        {
            return p;
        }
    }

    // 3. Check DATARA_HOME environment variable
    if let Ok(home) = std::env::var("DATARA_HOME") {
        let in_home_runtime = std::path::Path::new(&home).join("runtime").join(lib_name);
        if in_home_runtime.exists() {
            return in_home_runtime;
        }
        let in_home_root = std::path::Path::new(&home).join(lib_name);
        if in_home_root.exists() {
            return in_home_root;
        }
    }

    // 4. Fall back to baked path
    baked
}

/// Human-readable name of the runtime archive, for diagnostics.
pub fn runtime_lib_name() -> &'static str {
    if cfg!(target_env = "msvc") {
        "datara_runtime.lib"
    } else {
        "libdatara_runtime.a"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_lib_path_is_absolute_and_exists() {
        let path = runtime_lib_path();
        assert!(path.is_absolute(), "expected absolute path, got {:?}", path);
        assert!(
            path.exists(),
            "runtime archive missing at {:?}; run `cargo build` so build.rs regenerates it",
            path
        );
    }
}
