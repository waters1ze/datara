use forgen::driver::ForgenCompiler;
use forgen::project::pm::{DataraLock, HyperGridRegistry};
use std::fs;

#[test]
fn test_universal_ffi_syntax_and_aliasing() {
    let source = r#"
use python.scipy as scipy
use rust.serde as serde
use c.kernel32 as win32
use npm.path as path

fn main() -> Int {
    let a = 10
    let b = 20
    return a + b
}
"#;
    let compiler = ForgenCompiler::new("check");
    let res = compiler.check_source(source, "universal_ffi_test.dtr");
    assert!(
        res.success,
        "Universal FFI statements should resolve and check cleanly: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_dpm_merkle_tree_and_lockfile_integrity() {
    let registry = HyperGridRegistry::new();
    let pkg = registry.lookup("std").or_else(|| registry.lookup("math"));
    if let Some(p) = pkg {
        assert!(!p.digest.is_empty(), "Package digest must not be empty");
        assert_eq!(
            p.digest.len(),
            64,
            "SHA-256 Merkle digest must be 64 hex characters"
        );
    }

    let temp_dir = std::env::temp_dir().join(format!(
        "datara_dpm_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = fs::create_dir_all(&temp_dir);

    let mut lock = DataraLock::load(&temp_dir).unwrap_or_default();
    lock.insert_or_update(
        "matrix_math",
        "1.2.0",
        "a1b2c3d4e5f678901234567890abcdef1234567890abcdef1234567890abcdef",
        "hypergrid",
        vec!["std".to_string()],
    );

    let save_res = lock.save(&temp_dir);
    assert!(save_res.is_ok(), "Lockfile saving must succeed");

    let loaded_lock = DataraLock::load(&temp_dir).expect("Must load saved lockfile");
    let entry = loaded_lock
        .packages
        .get("matrix_math")
        .expect("Must contain matrix_math");
    assert_eq!(entry.version, "1.2.0");
    assert_eq!(
        entry.digest,
        "a1b2c3d4e5f678901234567890abcdef1234567890abcdef1234567890abcdef"
    );

    let _ = fs::remove_file(temp_dir.join("datara.lock"));
    let _ = fs::remove_dir(&temp_dir);
}
