use forgen::driver::ForgenCompiler;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn get_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("fixture_rust_crate")
}

#[test]
fn test_rust_bridge_manifest_staticlib_compilation_and_call() {
    let temp_dir = std::env::temp_dir().join(format!("datara_rust_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(temp_dir.join("src")).expect("Must create temp src dir");

    let fixture_path = get_fixture_path();
    let fixture_path_str = fixture_path.to_string_lossy().replace('\\', "/");

    let toml_content = format!(
        r#"[package]
name = "test_rust_app"
version = "0.1.0"
entry = "src/main.dtr"

[dependencies.rust.fixture_rust_crate]
path = "{}"
functions = [
    "fn add(a: Int, b: Int) -> Int",
    "fn mult(a: Int, b: Int) -> Int",
    "fn fibonacci(n: Int) -> Int",
    "fn square(x: Float) -> Float"
]
"#,
        fixture_path_str
    );

    fs::write(temp_dir.join("datara.toml"), &toml_content).expect("Must write datara.toml");

    let source = r#"
fn main() {
    mut s = 0
    mut p = 0
    mut fib = 0
    mut sq = 0.0
    unsafe(justification: "Calling safe Rust crate functions via staticlib FFI bridge") {
        s = add(15, 27)
        p = mult(6, 7)
        fib = fibonacci(10)
        sq = square(4.0)
    }

    out "SUM: " + s
    out "PROD: " + p
    out "FIB: " + fib
    out "SQ: " + sq
}
"#;

    let main_dtr = temp_dir.join("src").join("main.dtr");
    fs::write(&main_dtr, source).expect("Must write main.dtr");

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_project(&temp_dir, None);
    assert!(
        res.success,
        "Rust bridge project compilation failed: {:?}\nDiagnostics:\n{}",
        res.error, res.diagnostics
    );

    let exe = res.exe_path.expect("Must produce native .exe");
    assert!(exe.exists(), "Exe must exist: {}", exe.display());

    let (stdout, stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("Must run native executable");
    assert_eq!(code, 0, "Execution failed with code {}: {}", code, stderr);

    assert!(
        stdout.contains("SUM: 42"),
        "Expected SUM: 42, got: {}",
        stdout
    );
    assert!(
        stdout.contains("PROD: 42"),
        "Expected PROD: 42, got: {}",
        stdout
    );
    assert!(
        stdout.contains("FIB: 55"),
        "Expected FIB: 55, got: {}",
        stdout
    );
    assert!(
        stdout.contains("SQ: 16"),
        "Expected SQ: 16, got: {}",
        stdout
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_rust_bridge_auto_discovery_without_explicit_functions() {
    let temp_dir =
        std::env::temp_dir().join(format!("datara_rust_autodiscover_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(temp_dir.join("src")).expect("Must create temp src dir");

    let fixture_path = get_fixture_path();
    let fixture_path_str = fixture_path.to_string_lossy().replace('\\', "/");

    let toml_content = format!(
        r#"[package]
name = "test_autodiscover_app"
version = "0.1.0"
entry = "src/main.dtr"

[dependencies.rust.fixture_rust_crate]
path = "{}"
"#,
        fixture_path_str
    );

    fs::write(temp_dir.join("datara.toml"), &toml_content).expect("Must write datara.toml");

    let source = r#"
fn main() {
    mut sum = 0
    mut prod = 0
    unsafe(justification: "Calling auto-discovered Rust functions") {
        sum = add(100, 250)
        prod = mult(10, 20)
    }
    out "AUTO_SUM: " + sum
    out "AUTO_PROD: " + prod
}
"#;

    let main_dtr = temp_dir.join("src").join("main.dtr");
    fs::write(&main_dtr, source).expect("Must write main.dtr");

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_project(&temp_dir, None);
    assert!(
        res.success,
        "Auto-discovery compilation failed: {:?}\nDiagnostics:\n{}",
        res.error, res.diagnostics
    );

    let exe = res.exe_path.expect("Must produce native .exe");
    let (stdout, stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("Must run native executable");
    assert_eq!(code, 0, "Execution failed with code {}: {}", code, stderr);

    assert!(
        stdout.contains("AUTO_SUM: 350"),
        "Expected AUTO_SUM: 350, got: {}",
        stdout
    );
    assert!(
        stdout.contains("AUTO_PROD: 200"),
        "Expected AUTO_PROD: 200, got: {}",
        stdout
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_rust_bridge_honest_diagnostics_on_missing_crate() {
    let temp_dir = std::env::temp_dir().join(format!("datara_rust_diag_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(temp_dir.join("src")).expect("Must create temp src dir");

    let toml_content = r#"[package]
name = "test_bad_app"
version = "0.1.0"
entry = "src/main.dtr"

[dependencies.rust.non_existent_crate_xyz]
path = "path/that/does/not/exist/anywhere"
"#;

    fs::write(temp_dir.join("datara.toml"), toml_content).expect("Must write datara.toml");

    let source = r#"
fn main() {
    out 1
}
"#;
    let main_dtr = temp_dir.join("src").join("main.dtr");
    fs::write(&main_dtr, source).expect("Must write main.dtr");

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_project(&temp_dir, None);
    assert!(
        !res.success,
        "Compilation with invalid crate path should fail"
    );
    assert!(
        res.diagnostics
            .contains("Failed to compile Rust bridge wrapper")
            || res.diagnostics.contains("error")
            || res.diagnostics.contains("non_existent_crate_xyz"),
        "Diagnostics must mention the failed crate: {}",
        res.diagnostics
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_rust_bridge_dce_zero_cost() {
    let source = r#"
fn compute(a: Int, b: Int) -> Int => a * 2 + b

fn main() {
    let res = compute(21, 58)
    out res
}
"#;
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source_native(source, "test_rust_dce_run.dtr", None);
    assert!(
        res.success,
        "Pure program compilation failed: {:?}",
        res.error
    );

    let exe = res.exe_path.expect("Must produce native .exe file");
    assert!(exe.exists(), "Executable must exist: {}", exe.display());

    let (stdout, stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe, &[])
        .expect("Must run native executable");
    assert_eq!(code, 0, "Execution failed with code {}: {}", code, stderr);
    assert_eq!(stdout.trim(), "100");

    let bytes = fs::read(&exe).expect("Must read exe binary bytes");
    let contents = String::from_utf8_lossy(&bytes);

    let forbidden_patterns = [
        "fixture_rust_crate",
        "datara_wrapper",
        "rust_add",
        "rust_mult",
        "rust_fibonacci",
    ];

    for pat in &forbidden_patterns {
        assert!(
            !contents.contains(pat),
            "DCE Violation: Polyglot Rust symbol '{}' found in pure Datara binary",
            pat
        );
    }

    if let Ok(spec) = forgen::codegen::linker::ensure_linker() {
        let dumpbin = spec.program.with_file_name("dumpbin.exe");
        if dumpbin.exists() {
            if let Ok(output) = Command::new(&dumpbin)
                .args(["/SYMBOLS", &exe.to_string_lossy()])
                .output()
            {
                let text = String::from_utf8_lossy(&output.stdout);
                for pat in &forbidden_patterns {
                    assert!(
                        !text.contains(pat),
                        "dumpbin /SYMBOLS found forbidden Rust reference: {}",
                        pat
                    );
                }
            }
        }
    }

    let _ = fs::remove_file(&exe);
    let _ = fs::remove_file(exe.with_extension("obj"));
    let _ = fs::remove_file(exe.with_extension("pdb"));
}

#[test]
fn test_rust_bridge_lto_architecture_and_docs() {
    // Verifies that Cranelift documents cross-language LTO skipping,
    // and that find_cargo correctly locates the local cargo binary.
    let cargo_opt = forgen::rust_bridge::find_cargo();
    assert!(
        cargo_opt.is_some(),
        "Must locate Cargo binary on the system"
    );
    let cargo = cargo_opt.unwrap();
    assert!(
        cargo.exists(),
        "Cargo binary must exist at: {}",
        cargo.display()
    );

    // Verify config parsing
    let parsed = forgen::rust_bridge::parse_function_signature(
        "fn add(a: Int, b: Int) -> Int",
        "fixture_rust_crate",
    )
    .unwrap();
    assert_eq!(parsed.name, "add");
    assert_eq!(parsed.ret_type, "Int");
    assert_eq!(parsed.params.len(), 2);
}
