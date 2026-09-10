use forgen::driver::ForgenCompiler;
use std::fs;

#[test]
fn test_crypto_sha256_and_base64() {
    let code = r#"
fn main() {
    let hash = sha256("hello world")
    assert(hash == "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9")

    let enc = base64_encode("Datara 2026")
    assert(enc == "RGF0YXJhIDIwMjY=")

    let dec = base64_decode(enc)
    assert(dec == "Datara 2026")

    print("[OK] Crypto primitives verified")
}
"#;
    let temp_dir = std::env::temp_dir().join("datara_crypto_test");
    let _ = fs::create_dir_all(&temp_dir);
    let test_file = temp_dir.join("crypto_test.dtr");
    fs::write(&test_file, code).unwrap();

    let compiler = ForgenCompiler::new("test");
    let res = compiler.run_file(&test_file, &[]);
    assert!(res.is_ok(), "Compilation failed: {:?}", res.err());
    let (stdout, stderr, exit_code, _time) = res.unwrap();
    assert_eq!(exit_code, 0, "Non-zero exit code: stderr={}", stderr);
    assert!(stdout.contains("[OK] Crypto primitives verified"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_socket_creation_and_close() {
    let code = r#"
fn main() {
    let sock = socket_create(1)
    assert(sock >= 0)
    socket_close(sock)
    print("[OK] Socket lifecycle verified")
}
"#;
    let temp_dir = std::env::temp_dir().join("datara_sock_test");
    let _ = fs::create_dir_all(&temp_dir);
    let test_file = temp_dir.join("sock_test.dtr");
    fs::write(&test_file, code).unwrap();

    let compiler = ForgenCompiler::new("test");
    let res = compiler.run_file(&test_file, &[]);
    assert!(
        res.is_ok(),
        "Socket test compilation failed: {:?}",
        res.err()
    );
    let (stdout, stderr, exit_code, _time) = res.unwrap();
    assert_eq!(exit_code, 0, "Non-zero exit code: stderr={}", stderr);
    assert!(stdout.contains("[OK] Socket lifecycle verified"));

    let _ = fs::remove_dir_all(&temp_dir);
}
