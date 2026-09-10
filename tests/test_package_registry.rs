use forgen::driver::ForgenCompiler;
use forgen::project::pm::{DataraLock, HyperGridPackage, HyperGridRegistry, create_tar};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

fn temp_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("forgen_pm_test_{}_{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Helper that spawns a lightweight local HTTP server serving a single payload.
fn spawn_mock_http_server(payload: Vec<u8>) -> (String, Arc<AtomicBool>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop_flag);

    thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        while !stop_clone.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(500)));
                    let mut buf = [0u8; 1024];
                    let _ = stream.read(&mut buf);
                    let response_header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/x-tar\r\nConnection: close\r\n\r\n",
                        payload.len()
                    );
                    let _ = stream.write_all(response_header.as_bytes());
                    let _ = stream.write_all(&payload);
                    let _ = stream.flush();
                    let _ = stream.shutdown(std::net::Shutdown::Write);
                    let mut drain = [0u8; 512];
                    let _ = stream.read(&mut drain);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });

    (format!("http://127.0.0.1:{}", port), stop_flag)
}

#[test]
fn test_registry_http_fetch_and_sha256_verification() {
    let temp_dir = temp_test_dir("http_fetch");
    let proj_dir = temp_dir.join("my_app");
    fs::create_dir_all(&proj_dir).unwrap();

    // 1. Build a valid tar package in memory
    let mut files = HashMap::new();
    let lib_code = r#"
pub fn calculate_magic(val: Int) -> Int => val * 3 + 7
"#;
    files.insert("magic.dtr".to_string(), lib_code.as_bytes().to_vec());

    let pkg_files = HashMap::from([("magic.dtr".to_string(), lib_code.to_string())]);
    let content_digest = HyperGridRegistry::compute_digest(&pkg_files);
    let meta = HyperGridPackage {
        name: "magic_math".to_string(),
        version: "1.0.0".to_string(),
        description: "Magic math routines".to_string(),
        author: "Datara Test <test@datara.org>".to_string(),
        license: "MIT".to_string(),
        digest: content_digest.clone(),
        capabilities: vec![],
        dependencies: vec![],
        entry: "magic.dtr".to_string(),
        files: pkg_files,
    };
    let meta_json = serde_json::to_string_pretty(&meta).unwrap();
    files.insert("package.json".to_string(), meta_json.as_bytes().to_vec());

    let tar_bytes = create_tar(&files).expect("Tar creation should succeed");
    let expected_sha256 = format!(
        "sha256:{}",
        forgen::project::pm::sha256_hexdigest(&tar_bytes)
    );

    // 2. Serve tar over local HTTP mock server
    let (server_url, stop_flag) = spawn_mock_http_server(tar_bytes);
    let pkg_url = format!("{}/packages/magic_math-1.0.0.tar", server_url);

    // 3. Registry download and install
    let mut registry = HyperGridRegistry::new();
    registry.store_path = temp_dir.join("store");
    fs::create_dir_all(&registry.store_path).unwrap();

    let install_res = registry.fetch_and_install_tarball(
        "magic_math",
        "1.0.0",
        &pkg_url,
        &expected_sha256,
        &proj_dir,
    );
    assert!(
        install_res.is_ok(),
        "Installation from HTTP registry must succeed: {:?}",
        install_res.err()
    );

    // 4. Verify files on disk and datara.lock
    assert!(proj_dir.join("packages/magic_math/magic.dtr").exists());
    let lock = DataraLock::load(&proj_dir).expect("datara.lock must be created");
    let locked_pkg = lock
        .packages
        .get("magic_math")
        .expect("magic_math must be recorded in datara.lock");
    assert_eq!(locked_pkg.version, "1.0.0");
    assert_eq!(locked_pkg.digest, content_digest);
    assert_eq!(locked_pkg.source, pkg_url);

    // 5. Test compiling Datara code using the installed package
    let main_dtr = proj_dir.join("main.dtr");
    let main_src = r#"
use magic_math.calculate_magic

fn main() {
    let res = calculate_magic(10)
    out res
}
"#;
    fs::write(&main_dtr, main_src).unwrap();
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_file(&main_dtr, None);
    assert!(
        res.success,
        "Compilation with fetched package failed: {:?}",
        res.error
    );

    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "37", "10 * 3 + 7 = 37");

    stop_flag.store(true, Ordering::Relaxed);
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_registry_tampered_tarball_rejection() {
    let temp_dir = temp_test_dir("tampered_tarball");
    let proj_dir = temp_dir.join("my_app");
    fs::create_dir_all(&proj_dir).unwrap();

    // 1. Build a valid tar package
    let mut files = HashMap::new();
    files.insert(
        "lib.dtr".to_string(),
        b"pub fn original() -> Int => 1".to_vec(),
    );
    let tar_bytes = create_tar(&files).expect("Tar creation should succeed");

    // True SHA-256 of the untampered package
    let expected_sha256 = format!(
        "sha256:{}",
        forgen::project::pm::sha256_hexdigest(&tar_bytes)
    );

    // Tamper with tar bytes: modify byte 100
    let mut tampered_bytes = tar_bytes;
    tampered_bytes[10] ^= 0xFF;

    // 2. Serve tampered tar
    let (server_url, stop_flag) = spawn_mock_http_server(tampered_bytes);
    let pkg_url = format!("{}/packages/tampered_pkg-1.0.0.tar", server_url);

    // 3. Registry download attempt
    let mut registry = HyperGridRegistry::new();
    registry.store_path = temp_dir.join("store");
    fs::create_dir_all(&registry.store_path).unwrap();

    let install_res = registry.fetch_and_install_tarball(
        "tampered_pkg",
        "1.0.0",
        &pkg_url,
        &expected_sha256,
        &proj_dir,
    );

    assert!(
        install_res.is_err(),
        "Installation of tampered package must fail"
    );
    let err_msg = install_res.err().unwrap();
    assert!(
        err_msg.contains("SHA-256 checksum mismatch"),
        "Error message must explicitly note SHA-256 checksum mismatch: {}",
        err_msg
    );

    // Verify nothing was installed to packages/
    assert!(!proj_dir.join("packages/tampered_pkg").exists());
    // Verify no lockfile entry was written
    assert!(DataraLock::load(&proj_dir).is_none());

    stop_flag.store(true, Ordering::Relaxed);
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_pinned_datara_lock_generation_and_restore() {
    let temp_dir = temp_test_dir("pinned_lock");
    let proj_dir = temp_dir.join("proj");
    fs::create_dir_all(&proj_dir).unwrap();

    let mut registry = HyperGridRegistry::new();
    registry.store_path = temp_dir.join("store");
    fs::create_dir_all(&registry.store_path).unwrap();

    // 1. Install uuid into project
    let uuid_pkg = registry
        .lookup("uuid")
        .expect("uuid pkg must exist")
        .clone();
    let install_res = registry.install(&uuid_pkg, &proj_dir);
    assert!(install_res.is_ok());

    // 2. Verify datara.lock exists and is pinned
    let lock = DataraLock::load(&proj_dir).expect("datara.lock must exist");
    assert!(lock.packages.contains_key("uuid"));
    let locked_uuid = lock.packages.get("uuid").unwrap();
    assert_eq!(locked_uuid.version, uuid_pkg.version);
    assert_eq!(locked_uuid.digest, uuid_pkg.digest);

    // 3. Delete packages/ directory completely
    let packages_dir = proj_dir.join("packages");
    fs::remove_dir_all(&packages_dir).unwrap();
    assert!(!packages_dir.exists());

    // 4. Restore from lockfile
    let restored = lock
        .restore(&registry, &proj_dir)
        .expect("Restore must succeed");
    assert_eq!(restored, 1, "Exactly 1 package must be restored");
    assert!(
        packages_dir.join("uuid").exists(),
        "packages/uuid must be restored"
    );
    assert!(packages_dir.join("uuid/uuid.dtr").exists());

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_offline_mode_cache_hit() {
    let temp_dir = temp_test_dir("offline_hit");
    let proj_dir = temp_dir.join("proj");
    fs::create_dir_all(&proj_dir).unwrap();

    let mut registry = HyperGridRegistry::new();
    registry.store_path = temp_dir.join("store");
    fs::create_dir_all(&registry.store_path).unwrap();

    // 1. Populate CAS cache beforehand
    let mut files = HashMap::new();
    files.insert(
        "cached.dtr".to_string(),
        "pub fn cached() -> Int => 100".to_string(),
    );
    let pkg = HyperGridPackage {
        name: "cached_lib".to_string(),
        version: "2.1.0".to_string(),
        description: "A cached package".to_string(),
        author: "Datara Team".to_string(),
        license: "MIT".to_string(),
        digest: "sha256:1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
            .to_string(),
        capabilities: vec![],
        dependencies: vec![],
        entry: "cached.dtr".to_string(),
        files,
    };
    registry.register(pkg.clone());

    // Pre-install into CAS
    let cas_dir = registry.store_path.join(&pkg.name).join(&pkg.version);
    fs::create_dir_all(&cas_dir).unwrap();
    let meta_json = serde_json::to_string_pretty(&pkg).unwrap();
    fs::write(cas_dir.join("package.json"), meta_json).unwrap();
    fs::write(cas_dir.join("cached.dtr"), "pub fn cached() -> Int => 100").unwrap();

    // 2. Enable offline mode and use an unreachable URL
    registry.set_offline(true);
    assert!(registry.is_offline());

    let unreachable_url = "http://127.0.0.1:59999/does_not_exist.tar";
    let install_res = registry.fetch_and_install_tarball(
        "cached_lib",
        "2.1.0",
        unreachable_url,
        &pkg.digest,
        &proj_dir,
    );

    assert!(
        install_res.is_ok(),
        "Offline install with cache hit must succeed without network: {:?}",
        install_res.err()
    );
    assert!(proj_dir.join("packages/cached_lib/cached.dtr").exists());

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_offline_mode_cache_miss_failure() {
    let temp_dir = temp_test_dir("offline_miss");
    let proj_dir = temp_dir.join("proj");
    fs::create_dir_all(&proj_dir).unwrap();

    let mut registry = HyperGridRegistry::new();
    registry.store_path = temp_dir.join("store");
    fs::create_dir_all(&registry.store_path).unwrap();

    // Enable offline mode
    registry.set_offline(true);
    assert!(registry.is_offline());

    // Attempt to install a package NOT in CAS cache
    let install_res = registry.fetch_and_install_tarball(
        "uncached_lib",
        "0.5.0",
        "http://unreachable-registry.datara.org/uncached.tar",
        "sha256:0000000000000000000000000000000000000000000000000000000000000000",
        &proj_dir,
    );

    assert!(
        install_res.is_err(),
        "Offline install with cache miss must fail"
    );
    let err_msg = install_res.err().unwrap();
    assert!(
        err_msg.contains("Offline mode active") && err_msg.contains("not found in local CAS cache"),
        "Error message must clearly state offline mode and cache miss: {}",
        err_msg
    );

    let _ = fs::remove_dir_all(&temp_dir);
}
