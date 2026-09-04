use forgen::driver::ForgenCompiler;
use std::fs;

#[test]
fn test_in_memory_jit_zero_disk_artifacts() {
    let temp_dir = std::env::temp_dir().join(format!("datara_jit_test_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);
    let source_path = temp_dir.join("main.dtr");

    let code = r#"
fn compute_sum(n: Int) -> Int {
    mut total = 0
    mut i = 1
    while i <= n {
        total = total + i
        i = i + 1
    }
    return total
}

fn main() {
    let s = compute_sum(100)
    println(fmt"SUM: {s}")
}
"#;
    fs::write(&source_path, code).unwrap();

    let compiler = ForgenCompiler::new("release");

    // 1. Run via in-memory JIT
    let result = compiler.run_source(code, "test_jit", &[], true);
    assert!(
        result.is_ok(),
        "JIT execution should succeed: {:?}",
        result.err()
    );

    let (stdout, stderr, exit_code, _duration) = result.unwrap();
    assert_eq!(exit_code, 0);
    assert_eq!(stderr, "");
    assert!(
        stdout.contains("SUM: 5050"),
        "Expected 'SUM: 5050', got: '{}'",
        stdout
    );

    // 2. Verify that NO .obj or .exe files were generated on disk!
    let exe_candidate = source_path.with_extension("exe");
    let obj_candidate = source_path.with_extension("obj");
    assert!(
        !exe_candidate.exists(),
        "JIT must NOT generate .exe on disk!"
    );
    assert!(
        !obj_candidate.exists(),
        "JIT must NOT generate .obj on disk!"
    );

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_cranelift_aot_build_still_produces_exe() {
    let temp_dir = std::env::temp_dir().join(format!("datara_aot_test_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);
    let source_path = temp_dir.join("aot_main.dtr");
    let target_exe = temp_dir.join("aot_main.exe");

    let code = r#"
fn main() {
    println("AOT STANDALONE SUCCESS")
}
"#;
    fs::write(&source_path, code).unwrap();

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_file(&source_path, Some(&target_exe));
    assert!(res.success, "AOT build should succeed: {:?}", res.error);
    assert!(
        target_exe.exists(),
        "AOT build must generate executable file on disk"
    );

    let (stdout, _stderr, code, _) = compiler
        .codegen
        .run_executable(&target_exe, &[])
        .expect("AOT executable must run");
    assert_eq!(code, 0);
    assert!(stdout.contains("AOT STANDALONE SUCCESS"));

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_jit_uuid_v4_generation() {
    let compiler = ForgenCompiler::new("release");
    let code = r#"
fn main() {
    let u1 = uuid_v4()
    let u2 = uuid_v4()
    println(fmt"UUID1:{u1}")
    println(fmt"UUID2:{u2}")
}
"#;
    let result = compiler.run_source(code, "uuid_test", &[], true);
    assert!(result.is_ok(), "UUID run failed: {:?}", result.err());
    let (stdout, _, code, _) = result.unwrap();
    assert_eq!(code, 0);

    let lines: Vec<&str> = stdout.lines().collect();
    let u1_line = lines
        .iter()
        .find(|l| l.starts_with("UUID1:"))
        .expect("missing UUID1");
    let u2_line = lines
        .iter()
        .find(|l| l.starts_with("UUID2:"))
        .expect("missing UUID2");
    let u1 = &u1_line["UUID1:".len()..];
    let u2 = &u2_line["UUID2:".len()..];

    // RFC 4122 format: 8-4-4-4-12 = 36 chars
    assert_eq!(u1.len(), 36, "UUID1 length must be 36");
    assert_eq!(u2.len(), 36, "UUID2 length must be 36");
    assert_ne!(u1, u2, "Two generated UUIDs must be distinct");
    assert_eq!(&u1[14..15], "4", "UUID version nibble must be 4");
    let variant_char = u1.chars().nth(19).unwrap();
    assert!(
        variant_char == '8' || variant_char == '9' || variant_char == 'a' || variant_char == 'b',
        "UUID variant nibble must be 8, 9, a, or b, got {}",
        variant_char
    );
}
