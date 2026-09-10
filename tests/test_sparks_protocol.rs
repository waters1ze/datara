use forgen::project::pm::{
    HyperGridRegistry, SparksDeterminismReceipt, SparksPackageManifest, create_ed25519_keypair,
    create_tar, sha256_hexdigest, sign_ed25519_message,
};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

fn temp_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "forgen_sparks_test_{}_{}",
        name,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Spawns a lightweight local HTTP mock server routing paths to byte payloads.
fn spawn_sparks_mock_server(
    routes: HashMap<String, (String, Vec<u8>)>,
) -> (String, Arc<AtomicBool>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop_flag);
    let routes_arc = Arc::new(routes);

    thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        while !stop_clone.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(500)));
                    let mut buf = [0u8; 2048];
                    let n = stream.read(&mut buf).unwrap_or(0);
                    let req_str = String::from_utf8_lossy(&buf[..n]);
                    let first_line = req_str.lines().next().unwrap_or("");
                    let mut parts = first_line.split_whitespace();
                    let _method = parts.next();
                    let path = parts.next().unwrap_or("/");

                    if let Some((content_type, payload)) = routes_arc.get(path) {
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: {}\r\nConnection: close\r\n\r\n",
                            payload.len(),
                            content_type
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(payload);
                    } else {
                        let header = "HTTP/1.1 404 NOT FOUND\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                        let _ = stream.write_all(header.as_bytes());
                    }
                    let _ = stream.flush();
                    let _ = stream.shutdown(std::net::Shutdown::Write);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    });

    (format!("http://127.0.0.1:{}", port), stop_flag)
}

fn make_test_tarball(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut map = HashMap::new();
    for (name, content) in files {
        map.insert(name.to_string(), content.to_vec());
    }
    create_tar(&map).expect("Tar creation must succeed")
}

#[test]
fn test_sparks_sparse_index_e2e_verified_installation() {
    let temp_dir = temp_test_dir("e2e_verified");
    let proj_dir = temp_dir.join("consumer_app");
    fs::create_dir_all(&proj_dir).unwrap();

    // 1. Generate author ed25519 keypair
    let (pk_hex, sk_hex) = create_ed25519_keypair();

    // 2. Build tarball containing source code and capabilities.json sidecar
    let lib_code = b"pub fn hash_fast(input: String) -> String => input\n";
    let caps_json = br#"{"capabilities": ["Capability<FileRead>"]}"#;
    let tarball_bytes =
        make_test_tarball(&[("hash.dtr", lib_code), ("capabilities.json", caps_json)]);

    let sha256_hash = sha256_hexdigest(&tarball_bytes);
    let signature_hex = sign_ed25519_message(&tarball_bytes, &sk_hex).unwrap();

    let manifest = SparksPackageManifest {
        schema: 1,
        name: "sparks/crypto_core".to_string(),
        version: "1.0.0".to_string(),
        description: "Crypto primitives".to_string(),
        author: "Datara Core <core@datara.dev>".to_string(),
        license: "MIT".to_string(),
        tarball_url: "/packages/crypto_core/package.tar".to_string(),
        sha256: format!("sha256:{}", sha256_hash),
        public_key: Some(pk_hex),
        signature: Some(signature_hex),
        capabilities: vec!["Capability<FileRead>".to_string()],
        determinism_receipt: Some(SparksDeterminismReceipt {
            checksum: "d41d8cd98f00b204e9800998ecf8427e".to_string(),
            compiler: "forgen 1.0.0".to_string(),
            target: "x86_64-pc-windows-msvc".to_string(),
        }),
        dependencies: HashMap::new(),
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).unwrap();

    let mut routes = HashMap::new();
    routes.insert(
        "/packages/crypto_core/1.0.0.json".to_string(),
        ("application/json".to_string(), manifest_bytes),
    );
    routes.insert(
        "/packages/crypto_core/package.tar".to_string(),
        ("application/x-tar".to_string(), tarball_bytes),
    );

    let (server_url, stop_flag) = spawn_sparks_mock_server(routes);

    let mut registry = HyperGridRegistry::new();
    registry.store_path = temp_dir.join("store");
    fs::create_dir_all(&registry.store_path).unwrap();

    let install_res = registry.fetch_and_install_sparks(
        "sparks/crypto_core",
        Some("1.0.0"),
        &server_url,
        &proj_dir,
    );
    assert!(
        install_res.is_ok(),
        "Sparks package installation should succeed: {:?}",
        install_res.err()
    );

    let installed_dir = proj_dir.join("packages").join("sparks").join("crypto_core");
    assert!(
        installed_dir.exists(),
        "Package directory must exist at {}",
        installed_dir.display()
    );
    assert!(installed_dir.join("hash.dtr").exists());

    stop_flag.store(true, Ordering::Relaxed);
}

#[test]
fn test_sparks_sha256_tampering_rejected() {
    let temp_dir = temp_test_dir("tamper_sha");
    let proj_dir = temp_dir.join("app");
    fs::create_dir_all(&proj_dir).unwrap();

    let tarball_bytes = make_test_tarball(&[("code.dtr", b"fn x() -> Int => 42\n")]);

    let manifest = SparksPackageManifest {
        schema: 1,
        name: "sparks/tampered".to_string(),
        version: "1.0.0".to_string(),
        description: "Tampered test".to_string(),
        author: "Hacker <bad@evil.com>".to_string(),
        license: "MIT".to_string(),
        tarball_url: "/package.tar".to_string(),
        sha256: "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            .to_string(),
        public_key: None,
        signature: None,
        capabilities: vec![],
        determinism_receipt: None,
        dependencies: HashMap::new(),
    };

    let registry = HyperGridRegistry::new();
    let res = registry.install_sparks_manifest(&manifest, &tarball_bytes, &proj_dir);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(
        err.contains("[E-SPARKS-003]") || err.contains("SHA-256 artifact checksum mismatch"),
        "Expected SHA256 mismatch error, got: {}",
        err
    );
}

#[test]
fn test_sparks_ed25519_signature_tampering_rejected() {
    let temp_dir = temp_test_dir("tamper_sig");
    let proj_dir = temp_dir.join("app");
    fs::create_dir_all(&proj_dir).unwrap();

    let (pk_hex, _sk_hex) = create_ed25519_keypair();
    let tarball_bytes = make_test_tarball(&[("code.dtr", b"fn x() -> Int => 1\n")]);
    let sha = sha256_hexdigest(&tarball_bytes);

    // Completely bogus 64-byte signature
    let bogus_sig = "ff".repeat(64);

    let manifest = SparksPackageManifest {
        schema: 1,
        name: "sparks/bad_sig".to_string(),
        version: "1.0.0".to_string(),
        description: "Bad sig test".to_string(),
        author: "Dev <dev@datara.dev>".to_string(),
        license: "MIT".to_string(),
        tarball_url: "/package.tar".to_string(),
        sha256: format!("sha256:{}", sha),
        public_key: Some(pk_hex),
        signature: Some(bogus_sig),
        capabilities: vec![],
        determinism_receipt: None,
        dependencies: HashMap::new(),
    };

    let registry = HyperGridRegistry::new();
    let res = registry.install_sparks_manifest(&manifest, &tarball_bytes, &proj_dir);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(
        err.contains("[E-SPARKS-004]") || err.contains("signature verification failed"),
        "Expected signature error, got: {}",
        err
    );
}

#[test]
fn test_sparks_capability_mismatch_rejected() {
    let temp_dir = temp_test_dir("cap_mismatch");
    let proj_dir = temp_dir.join("app");
    fs::create_dir_all(&proj_dir).unwrap();

    // Archive contains Capability<NetworkConnect>
    let sidecar = br#"{"capabilities": ["Capability<NetworkConnect>"]}"#;
    let tarball_bytes = make_test_tarball(&[
        ("main.dtr", b"fn main() {}\n"),
        ("capabilities.json", sidecar),
    ]);
    let sha = sha256_hexdigest(&tarball_bytes);

    // Manifest declares Capability<FileRead> (mismatch!)
    let manifest = SparksPackageManifest {
        schema: 1,
        name: "sparks/sneaky_caps".to_string(),
        version: "1.0.0".to_string(),
        description: "Sneaky caps test".to_string(),
        author: "Dev <dev@datara.dev>".to_string(),
        license: "MIT".to_string(),
        tarball_url: "/package.tar".to_string(),
        sha256: format!("sha256:{}", sha),
        public_key: None,
        signature: None,
        capabilities: vec!["Capability<FileRead>".to_string()],
        determinism_receipt: None,
        dependencies: HashMap::new(),
    };

    let registry = HyperGridRegistry::new();
    let res = registry.install_sparks_manifest(&manifest, &tarball_bytes, &proj_dir);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(
        err.contains("[E-SPARKS-002]") || err.contains("Capability declaration mismatch"),
        "Expected capability mismatch error, got: {}",
        err
    );
}

#[test]
fn test_sparks_schema_999_rejected() {
    let temp_dir = temp_test_dir("schema_reject");
    let proj_dir = temp_dir.join("app");
    fs::create_dir_all(&proj_dir).unwrap();

    let tarball_bytes = make_test_tarball(&[("main.dtr", b"fn main() {}\n")]);
    let sha = sha256_hexdigest(&tarball_bytes);

    let manifest = SparksPackageManifest {
        schema: 999,
        name: "sparks/future_pkg".to_string(),
        version: "2.0.0".to_string(),
        description: "Future schema".to_string(),
        author: "Time Traveler <future@datara.dev>".to_string(),
        license: "MIT".to_string(),
        tarball_url: "/package.tar".to_string(),
        sha256: format!("sha256:{}", sha),
        public_key: None,
        signature: None,
        capabilities: vec![],
        determinism_receipt: None,
        dependencies: HashMap::new(),
    };

    let registry = HyperGridRegistry::new();
    let res = registry.install_sparks_manifest(&manifest, &tarball_bytes, &proj_dir);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(
        err.contains("[E-SPARKS-001]") || err.contains("Unsupported Sparks schema version 999"),
        "Expected unsupported schema error, got: {}",
        err
    );
}
