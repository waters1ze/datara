//! Integration test: Validates that DPM / HyperGridRegistry in the Datara compiler
//! can fetch, verify (ed25519 + clean hex SHA-256), and install packages directly
//! from the local Sparks package registry (D:\DATARA\sparks) over HTTP.

use forgen::project::pm::HyperGridRegistry;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

fn sparks_repo_root() -> PathBuf {
    if let Ok(env_path) = std::env::var("SPARKS_REPO_PATH") {
        let p = PathBuf::from(env_path);
        if p.exists() {
            return p;
        }
    }
    let default_path = PathBuf::from(r"D:\DATARA\sparks");
    if default_path.exists() {
        return default_path;
    }
    let alt = PathBuf::from("../sparks");
    if alt.exists() {
        return alt;
    }
    default_path
}

fn temp_consumer_dir(test_id: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "forgen_sparks_remote_{}_{}",
        test_id,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Lightweight static HTTP file server serving the Sparks directory tree over HTTP.
fn spawn_sparks_file_server(root_path: PathBuf) -> (String, Arc<AtomicBool>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind ephemeral test port");
    let port = listener.local_addr().unwrap().port();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop_flag);

    thread::spawn(move || {
        let _ = listener.set_nonblocking(true);
        while !stop_clone.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(500)));
                    let mut buf = [0u8; 4096];
                    let n = stream.read(&mut buf).unwrap_or(0);
                    let req_str = String::from_utf8_lossy(&buf[..n]);
                    let first_line = req_str.lines().next().unwrap_or("");
                    let mut parts = first_line.split_whitespace();
                    let _method = parts.next();
                    let raw_path = parts.next().unwrap_or("/");

                    // Map request URL path to local sparks repository file
                    let relative_path = raw_path.trim_start_matches('/');
                    let file_path = root_path.join(relative_path);

                    if file_path.exists() && file_path.is_file() {
                        if let Ok(content) = fs::read(&file_path) {
                            let content_type = if raw_path.ends_with(".json") {
                                "application/json"
                            } else if raw_path.ends_with(".tar") {
                                "application/x-tar"
                            } else {
                                "application/octet-stream"
                            };
                            let header = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: {}\r\nConnection: close\r\n\r\n",
                                content.len(),
                                content_type
                            );
                            let _ = stream.write_all(header.as_bytes());
                            let _ = stream.write_all(&content);
                        } else {
                            let header = "HTTP/1.1 500 INTERNAL ERROR\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                            let _ = stream.write_all(header.as_bytes());
                        }
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

#[test]
fn test_sparks_remote_e2e_all_seed_packages() {
    let repo_root = sparks_repo_root();
    if !repo_root.exists() {
        eprintln!(
            "Sparks repository not found at {}; skipping local registry test in isolated test environment",
            repo_root.display()
        );
        return;
    }

    let (server_url, stop_flag) = spawn_sparks_file_server(repo_root);

    let seed_packages = [
        ("sparks/crypto_core", "1.0.0", "crypto_core"),
        ("sparks/math_simd", "1.0.0", "math_simd"),
        ("sparks/http_router", "1.0.0", "http_router"),
        ("sparks/lockstep_engine", "1.0.0", "lockstep_engine"),
        ("sparks/toy_kv", "1.0.0", "toy_kv"),
    ];

    for (pkg_name, version, raw_id) in seed_packages {
        let consumer_dir = temp_consumer_dir(raw_id);
        let mut registry = HyperGridRegistry::new();
        registry.store_path = consumer_dir.join(".forgen_store");
        fs::create_dir_all(&registry.store_path).unwrap();

        let install_result =
            registry.fetch_and_install_sparks(pkg_name, Some(version), &server_url, &consumer_dir);

        assert!(
            install_result.is_ok(),
            "Failed to install {} v{} from remote server {}: {:?}",
            pkg_name,
            version,
            server_url,
            install_result.err()
        );

        let installed_pkg_dir = consumer_dir.join("packages").join("sparks").join(raw_id);
        assert!(
            installed_pkg_dir.exists(),
            "Installed package dir must exist at {}",
            installed_pkg_dir.display()
        );
        assert!(
            installed_pkg_dir.join("main.dtr").exists(),
            "main.dtr must be present in installed package {}",
            pkg_name
        );
        assert!(
            installed_pkg_dir.join("capabilities.json").exists(),
            "capabilities.json sidecar must be present in installed package {}",
            pkg_name
        );

        let _ = fs::remove_dir_all(&consumer_dir);
    }

    stop_flag.store(true, Ordering::Relaxed);
}
