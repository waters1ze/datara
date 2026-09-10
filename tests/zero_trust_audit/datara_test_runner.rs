use std::fs;
use std::process::Command;

fn datara_bin() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("CARGO_BIN_EXE_datara") {
        let pb = std::path::PathBuf::from(p);
        if pb.exists() {
            return pb;
        }
    }
    let ext = if cfg!(windows) { ".exe" } else { "" };
    let candidates = [
        format!("target/release/datara{}", ext),
        format!("target/debug/datara{}", ext),
        format!("target/x86_64-unknown-linux-gnu/release/datara{}", ext),
        format!("target/x86_64-unknown-linux-gnu/debug/datara{}", ext),
    ];
    for c in candidates {
        let p = std::path::PathBuf::from(c);
        if p.exists() {
            return p;
        }
    }
    std::path::PathBuf::from(format!("target/release/datara{}", ext))
}

#[test]
fn audit_datara_test_passing_and_failing_and_filter_and_panic_handling() {
    let temp_dir = std::env::temp_dir().join(format!("zta_datara_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let tests_dir = temp_dir.join("tests");
    fs::create_dir_all(&tests_dir).unwrap();

    // 1. Project manifest
    fs::write(
        temp_dir.join("datara.toml"),
        "[package]\nname = 'test_suite_demo'\nversion = '0.1.0'\n",
    )
    .unwrap();

    // 2. Test file with 3 tests: passing, failing, panicking
    let test_src = r#"
fn dynamic_zero() -> Int {
    return 0
}

@test
fn test_alpha_pass() {
    let x = 10 + 20
    out x
}

@test
fn test_beta_fail() {
    out "FAIL: explicit test assertion failure"
}

@test
fn test_gamma_panic() {
    let x = 9223372036854775807
    let y = 1
    let crash = x + y
    out crash
}
"#;
    fs::write(tests_dir.join("suite_test.dtr"), test_src).unwrap();

    let bin = datara_bin();
    assert!(
        bin.exists(),
        "datara binary must exist at {}",
        bin.display()
    );

    // Case A: Filter only test_alpha_pass -> PASSING, exit code 0
    let out_pass = Command::new(&bin)
        .arg("test")
        .arg(temp_dir.to_str().unwrap())
        .arg("alpha")
        .output()
        .expect("Run datara test alpha");

    let stdout_pass = String::from_utf8_lossy(&out_pass.stdout);
    assert_eq!(
        out_pass.status.code().unwrap_or(-1),
        0,
        "Passing test must exit with code 0! Stdout:\n{}",
        stdout_pass
    );
    assert!(
        stdout_pass.contains("test test_alpha_pass ... ok"),
        "Must report test_alpha_pass ... ok"
    );
    assert!(
        stdout_pass.contains("1 passed; 0 failed"),
        "Summary must report 1 passed; 0 failed"
    );

    // Case B: Filter only test_beta_fail -> FAILING, exit code 1
    let out_fail = Command::new(&bin)
        .arg("test")
        .arg(temp_dir.to_str().unwrap())
        .arg("beta")
        .output()
        .expect("Run datara test beta");

    let stdout_fail = String::from_utf8_lossy(&out_fail.stdout);
    assert_ne!(
        out_fail.status.code().unwrap_or(0),
        0,
        "Failing test must exit with non-zero code!"
    );
    assert!(
        stdout_fail.contains("test test_beta_fail ... FAILED"),
        "Must report test_beta_fail ... FAILED"
    );
    assert!(
        stdout_fail.contains("0 passed; 1 failed"),
        "Summary must report 0 passed; 1 failed"
    );

    // Case C: Filter only test_gamma_panic -> PANIC IN TEST = FAIL (NOT CRASH OF RUNNER), exit code 1
    let out_panic = Command::new(&bin)
        .arg("test")
        .arg(temp_dir.to_str().unwrap())
        .arg("gamma")
        .output()
        .expect("Run datara test gamma");

    let stdout_panic = String::from_utf8_lossy(&out_panic.stdout);
    assert_ne!(
        out_panic.status.code().unwrap_or(0),
        0,
        "Panicking test must cause test run to report failure"
    );
    assert!(
        stdout_panic.contains("test test_gamma_panic ... FAILED"),
        "Panicking test must be captured as FAILED without crashing the runner: {}",
        stdout_panic
    );

    let _ = fs::remove_dir_all(&temp_dir);
}
