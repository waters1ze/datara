use forgen::driver::ForgenCompiler;
use forgen::project::pm::http::fetch_url;
use forgen::project::pm::tar::{create_tar, extract_tar, sanitize_tar_path};
use std::collections::HashMap;
use std::fs;

#[test]
fn audit_modules_private_symbol_access_rejection() {
    let temp_dir = std::env::temp_dir().join("audit_modules_privacy_sandbox");
    let _ = fs::create_dir_all(&temp_dir);

    let mod_path = temp_dir.join("helper.dtr");
    let mod_source = r#"
export fn public_helper() -> Int {
    return 100
}

fn secret_internal() -> Int {
    return 666
}
"#;
    fs::write(&mod_path, mod_source).expect("Write helper module");

    // 1. Importing private item MUST fail
    let main_bad_path = temp_dir.join("main_bad.dtr");
    let main_bad_source = r#"
use helper.secret_internal

fn main() {
    let x = secret_internal()
    out x
}
"#;
    fs::write(&main_bad_path, main_bad_source).expect("Write bad main");

    let compiler = ForgenCompiler::new("debug");
    let bad_res = compiler.compile_file(&main_bad_path, None);
    assert!(
        !bad_res.success,
        "Compiler MUST reject importing a non-exported private item"
    );
    let diag = bad_res.diagnostics;
    assert!(
        diag.contains("E0804") || diag.to_lowercase().contains("private"),
        "Diagnostic must state that private item cannot be accessed: {}",
        diag
    );

    // 2. Importing public item MUST succeed
    let main_good_path = temp_dir.join("main_good.dtr");
    let main_good_source = r#"
use helper.public_helper

fn main() {
    let x = public_helper()
    out x
}
"#;
    fs::write(&main_good_path, main_good_source).expect("Write good main");
    let good_res = compiler.compile_file(&main_good_path, None);
    assert!(
        good_res.success,
        "Importing public item must succeed: {:?}",
        good_res.error
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn audit_traits_dispatch_inspection_1_vs_3_implementers() {
    let source = r#"
trait Worker {
    fn work(&self) -> Int;
}

class Alpha { v: Int }
class Beta { v: Int }
class Gamma { v: Int }
class Delta { v: Int }

impl Worker for Alpha { fn work(&self) -> Int { self.v * 1 } }
impl Worker for Beta { fn work(&self) -> Int { self.v * 2 } }
impl Worker for Gamma { fn work(&self) -> Int { self.v * 3 } }
impl Worker for Delta { fn work(&self) -> Int { self.v * 4 } }

fn main() {
    let a = Alpha { v: 10 }
    let b = Beta { v: 20 }
    let g = Gamma { v: 30 }
    let d = Delta { v: 40 }

    out a.work()
    out b.work()
    out g.work()
    out d.work()
}
"#;
    let compiler = ForgenCompiler::new("release").with_llvm(true);
    let res = compiler.compile_source(source, "audit_trait_dispatch.dtr", None);
    assert!(res.success, "Compilation must succeed: {:?}", res.error);

    let clif = res.clif_source.expect("CLIF source generated");
    let llvm = res.llvm_source.expect("LLVM IR generated");

    // Inspect CLIF: check if indirect calls (call_indirect) exist or if it's static dispatch
    let clif_has_indirect = clif.contains("call_indirect");
    let llvm_has_indirect = llvm.contains("call i64 %") || llvm.contains("call i32 %");

    println!(
        "FORENSIC FACT [Trait Dispatch]: clif_has_indirect={}, llvm_has_indirect={}",
        clif_has_indirect, llvm_has_indirect
    );

    // Verify execution correctness
    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines, vec!["10", "40", "90", "160"]);

    // Forensic deduction: Datara compiler resolves trait method calls at compile-time directly
    // to their concrete class implementations (Alpha_work, Beta_work, etc.), meaning calls are
    // static direct calls. No vtable indirection or call_indirect is emitted in CLIF.
}

#[test]
fn audit_dpm_zip_slip_rejection_and_normal_package_installation() {
    // 1. Direct path sanitizer check
    let evil_path = "../../../evil.exe";
    let sanitize_res = sanitize_tar_path(evil_path);
    assert!(
        sanitize_res.is_err(),
        "sanitize_tar_path MUST reject traversal path '../'"
    );
    let err_msg = sanitize_res.err().unwrap();
    assert!(
        err_msg.contains("Path traversal ('..') detected"),
        "Error message must specify path traversal rejection: {}",
        err_msg
    );

    // 2. Tar archive creation with evil file
    let mut evil_files = HashMap::new();
    evil_files.insert("../evil.txt".to_string(), b"hacked".to_vec());
    let evil_tar = create_tar(&evil_files);
    assert!(
        evil_tar.is_err(),
        "create_tar MUST fail when encountering path traversal entry"
    );

    // 3. Normal package creation and extraction
    let mut legit_files = HashMap::new();
    legit_files.insert(
        "pkg/lib.dtr".to_string(),
        b"export fn foo() => 42\n".to_vec(),
    );
    legit_files.insert(
        "pkg/package.toml".to_string(),
        b"[package]\nname = 'test'\n".to_vec(),
    );

    let tar_bytes = create_tar(&legit_files).expect("Legitimate tar creation must succeed");

    let dest_dir = std::env::temp_dir().join("audit_dpm_extract_dest");
    let _ = fs::remove_dir_all(&dest_dir);
    fs::create_dir_all(&dest_dir).expect("Create dest dir");

    let extracted = extract_tar(&tar_bytes).expect("Extract legitimate tar");
    assert_eq!(extracted.len(), 2);
    assert!(extracted.contains_key("pkg/lib.dtr"));
    assert!(extracted.contains_key("pkg/package.toml"));

    // 4. Verification of file:// URL protocol in dpm http fetcher
    let archive_path = dest_dir.join("package.tar");
    fs::write(&archive_path, &tar_bytes).expect("Write tar file");

    let file_url = format!(
        "file://{}",
        archive_path.to_string_lossy().replace('\\', "/")
    );
    let fetched = fetch_url(&file_url).expect("fetch_url on file:// protocol must succeed");
    assert_eq!(
        fetched, tar_bytes,
        "Fetched bytes must match written tar bytes exactly"
    );

    let _ = fs::remove_dir_all(&dest_dir);
}
