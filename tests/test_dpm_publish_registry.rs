use forgen::project::{DataraLock, HyperGridRegistry, VerificationStatus};
use std::fs;

#[test]
fn test_dpm_publish_merkle_digest_capabilities_and_index() {
    let base_temp = std::env::temp_dir().join("datara_publish_test_suite");
    let proj_dir = base_temp.join("auth_service");
    let consumer_dir = base_temp.join("client_app");
    let _ = fs::remove_dir_all(&base_temp);

    // 1. Initialize project
    let _ = HyperGridRegistry::init_project(&proj_dir, "auth_service", false);

    // 2. Add realistic source code using net and fs
    let src_file = proj_dir.join("src").join("main.dtr");
    let code = r#"
use stdlib.net.socket
use stdlib.io.fs

class AuthService {
    port: Int
}

behavior AuthService {
    fn start(port: Int) -> Bool {
        let stream = TcpStream.connect("127.0.0.1", port)
        let _ = file_write("audit.log", "auth connected\n")
        return true
    }
}

fn main() {
    let s = AuthService { port: 8080 }
}
"#;
    fs::write(&src_file, code).unwrap();

    // 3. Publish to registry
    let mut reg = HyperGridRegistry::new();
    let pkg = reg.publish(&proj_dir).expect("Publish must succeed");

    // 4. Validate Merkle digest seal
    assert!(
        pkg.digest.starts_with("merkle:sha256:"),
        "Digest must be a formal Merkle root seal: {}",
        pkg.digest
    );

    // 5. Validate automated capability audit
    assert!(
        pkg.capabilities.contains(&"net.network".to_string()),
        "Expected net.network capability, got: {:?}",
        pkg.capabilities
    );
    assert!(
        pkg.capabilities.contains(&"fs.filesystem".to_string()),
        "Expected fs.filesystem capability, got: {:?}",
        pkg.capabilities
    );

    // 6. Validate .dtr-pkg bundle creation
    let dist_bundle = proj_dir.join("dist").join("auth_service-0.1.0.dtr-pkg");
    assert!(
        dist_bundle.exists(),
        "Package bundle must exist in dist/ directory"
    );

    let cas_bundle = reg
        .store_path
        .join("bundles")
        .join("auth_service-0.1.0.dtr-pkg");
    assert!(
        cas_bundle.exists(),
        "Package bundle must exist in CAS bundles/ directory"
    );

    // 7. Validate Git-backed index metadata
    let index_file = reg.store_path.join("index.json");
    assert!(index_file.exists(), "CAS index.json must exist");
    let index_content = fs::read_to_string(&index_file).unwrap();
    assert!(index_content.contains("auth_service"));
    assert!(index_content.contains(&pkg.digest));

    // 8. Test installation and Merkle verification in another project
    let _ = HyperGridRegistry::init_project(&consumer_dir, "client_app", false);
    let install_res = reg.install(&pkg, &consumer_dir);
    assert!(
        install_res.is_ok(),
        "Installation of published package must succeed"
    );

    let lock = DataraLock::load(&consumer_dir).expect("Lock file must exist");
    let locked_entry = lock
        .packages
        .get("auth_service")
        .expect("auth_service in lock");
    assert_eq!(locked_entry.digest, pkg.digest);

    let verifications = reg
        .verify(&consumer_dir)
        .expect("Verification must succeed");
    let auth_v = verifications
        .iter()
        .find(|v| v.name == "auth_service")
        .expect("entry found");
    assert_eq!(auth_v.status, VerificationStatus::Valid);

    let _ = fs::remove_dir_all(&base_temp);
}
