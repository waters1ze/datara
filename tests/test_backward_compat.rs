use forgen::driver::ForgenCompiler;
use forgen::project::manifest::{DataraManifest, validate_edition, validate_semver};
use forgen::project::pm::{DataraLock, sha256_hexdigest};
use forgen::runtime::{
    COMPILER_DATARA_RT_ABI_VERSION, datara_rt_abi_version, verify_runtime_abi,
    verify_runtime_abi_version,
};
use std::fs;
use std::path::PathBuf;

#[test]
fn test_backward_compat_stdlib_golden_fixtures() {
    let temp_dir =
        std::env::temp_dir().join(format!("datara_compat_stdlib_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("Must create temp dir");

    let source = r#"
use stdlib.text.string.StringUtils
use stdlib.result.result.Outcome
use stdlib.result.option.Maybe

fn main() {
    let s_util = StringUtils { prefix: "GOLDEN" }
    let q = s_util.quote("datara_0_1_0")
    out "QUOTED: " + q

    let opt = Maybe<Int> { is_some: true, value: 100 }
    let v = opt.unwrap_or(0)
    out "OPT_VAL: " + v

    let res = Outcome<String> { is_success: true, value: "STABLE", error_msg: "" }
    let s = res.unwrap()
    out "RES_VAL: " + s
}
"#;

    let main_dtr = temp_dir.join("main.dtr");
    fs::write(&main_dtr, source).expect("Must write main.dtr");

    let compiler = ForgenCompiler::new("release");
    let files = vec![
        main_dtr,
        PathBuf::from("stdlib/text/string.dtr"),
        PathBuf::from("stdlib/result/result.dtr"),
        PathBuf::from("stdlib/result/option.dtr"),
    ];

    let res = compiler.compile_files(&files, None);
    assert!(
        res.success,
        "Backward compatibility golden test failed:\n{}\n{:?}",
        res.diagnostics, res.error
    );

    let exe = res.exe_path.expect("Must produce native .exe");
    let (stdout, stderr, code, _) = compiler
        .codegen
        .run_executable(&exe, &[])
        .expect("Must run executable");
    assert_eq!(code, 0, "Execution failed with stderr: {}", stderr);

    assert!(stdout.contains("QUOTED: 'datara_0_1_0'"));
    assert!(stdout.contains("OPT_VAL: 100"));
    assert!(stdout.contains("RES_VAL: STABLE"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_backward_compat_datara_toml_semver_and_edition() {
    // 1. Semver validation
    assert!(validate_semver("0.1.0").is_ok());
    assert!(validate_semver("1.0.0").is_ok());
    assert!(validate_semver("2.1.3-alpha.1").is_ok());
    assert!(validate_semver("3.0.0+build.2026").is_ok());
    assert!(validate_semver("1.0.0-rc.2+sha.123abc").is_ok());

    assert!(validate_semver("").is_err());
    assert!(validate_semver("invalid").is_err());
    assert!(validate_semver("1.0").is_err());
    assert!(validate_semver("v1.0.0").is_err());
    assert!(validate_semver("1.0.0.0").is_err());
    assert!(validate_semver("01.0.0").is_err()); // Leading zero forbidden in semver numeric IDs

    // 2. Edition validation
    assert!(validate_edition("2024").is_ok());
    assert!(validate_edition("2025").is_ok());
    assert!(validate_edition("2026").is_ok());

    assert!(validate_edition("1999").is_err());
    assert!(validate_edition("2030").is_err());
    assert!(validate_edition("rust").is_err());

    // 3. Manifest parsing with edition & semver
    let valid_toml = r#"
[package]
name = "compat_pkg"
version = "0.1.0"
edition = "2026"
entry = "src/main.dtr"
"#;
    let manifest = DataraManifest::parse(valid_toml).expect("Valid manifest should parse");
    assert_eq!(manifest.package.version, "0.1.0");
    assert_eq!(manifest.package.edition.as_deref(), Some("2026"));

    // Manifest with default edition
    let no_edition_toml = r#"
[package]
name = "compat_pkg_default"
version = "1.2.3"
"#;
    let manifest_default =
        DataraManifest::parse(no_edition_toml).expect("Default edition manifest should parse");
    assert_eq!(manifest_default.package.edition.as_deref(), Some("2026"));

    // Manifest with invalid semver
    let bad_semver_toml = r#"
[package]
name = "bad_pkg"
version = "v1.0"
"#;
    let err = DataraManifest::parse(bad_semver_toml).unwrap_err();
    assert!(err.contains("not valid semver"), "Error was: {}", err);

    // Manifest with invalid edition
    let bad_edition_toml = r#"
[package]
name = "bad_pkg"
version = "1.0.0"
edition = "2099"
"#;
    let err_edition = DataraManifest::parse(bad_edition_toml).unwrap_err();
    assert!(
        err_edition.contains("Unsupported edition '2099'"),
        "Error was: {}",
        err_edition
    );
}

#[test]
fn test_backward_compat_runtime_abi_version_mismatch() {
    // 1. Linked runtime version matches compiler
    let rt_ver = unsafe { datara_rt_abi_version() };
    assert_eq!(
        rt_ver, COMPILER_DATARA_RT_ABI_VERSION,
        "Runtime ABI version must match compiler version"
    );
    assert!(verify_runtime_abi().is_ok());

    // 2. Exact error format on mismatch
    let err = verify_runtime_abi_version(2, 1).unwrap_err();
    assert!(
        err.contains("runtime v2 vs compiler v1"),
        "Diagnostic must contain 'runtime v2 vs compiler v1', got: {}",
        err
    );

    let err_rev = verify_runtime_abi_version(1, 2).unwrap_err();
    assert!(
        err_rev.contains("runtime v1 vs compiler v2"),
        "Diagnostic must contain 'runtime v1 vs compiler v2', got: {}",
        err_rev
    );

    // Matching version succeeds
    assert!(verify_runtime_abi_version(1, 1).is_ok());
    assert!(verify_runtime_abi_version(2, 2).is_ok());
}

#[test]
fn test_backward_compat_datara_lock_sha256_integrity() {
    let temp_dir = std::env::temp_dir().join(format!("datara_lock_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("Must create temp dir");

    let mut lock = DataraLock::default();
    let sample_data = b"package contents for dependency test";
    let hash = sha256_hexdigest(sample_data);
    let digest = format!("sha256:{}", hash);

    lock.insert_or_update(
        "dep_alpha",
        "0.1.0",
        &digest,
        "https://registry.datara.org/pkg/dep_alpha-0.1.0.tar.gz",
        vec![],
    );

    lock.save(&temp_dir).expect("Must save datara.lock");

    let loaded = DataraLock::load(&temp_dir).expect("Must load datara.lock");
    assert_eq!(loaded.version, 1);
    let pkg = loaded
        .packages
        .get("dep_alpha")
        .expect("dep_alpha must exist in lock");
    assert_eq!(pkg.version, "0.1.0");
    assert_eq!(pkg.digest, digest);
    assert!(pkg.digest.starts_with("sha256:"));

    let _ = fs::remove_dir_all(&temp_dir);
}
