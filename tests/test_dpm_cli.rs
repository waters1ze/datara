use forgen::driver::ForgenCompiler;
use forgen::project::{DataraLock, HyperGridRegistry, VerificationStatus};
use std::fs;

#[test]
fn test_dpm_lifecycle_init_add_verify_remove() {
    let temp_proj = std::env::temp_dir().join("datara_dpm_test_lifecycle");
    let _ = fs::remove_dir_all(&temp_proj);

    // 1. Init
    let res_init = HyperGridRegistry::init_project(&temp_proj, "test_microservice", false);
    assert!(res_init.is_ok(), "dpm init should succeed");
    assert!(temp_proj.join("datara.toml").exists());
    assert!(temp_proj.join("src").join("main.dtr").exists());
    assert!(temp_proj.join(".gitignore").exists());

    let registry = HyperGridRegistry::new();

    // 2. Add 'uuid' and 'redis'
    let uuid_pkg = registry.lookup("uuid").expect("uuid must exist");
    let install_res = registry.install(uuid_pkg, &temp_proj);
    assert!(install_res.is_ok(), "dpm add uuid should succeed");

    let redis_pkg = registry.lookup("redis").expect("redis must exist");
    let _ = registry.install(redis_pkg, &temp_proj);

    // 3. Check packages directory and datara.toml
    assert!(
        temp_proj
            .join("packages")
            .join("uuid")
            .join("uuid.dtr")
            .exists()
    );
    assert!(
        temp_proj
            .join("packages")
            .join("redis")
            .join("redis.dtr")
            .exists()
    );

    let toml_content = fs::read_to_string(temp_proj.join("datara.toml")).unwrap();
    assert!(toml_content.contains("uuid = \"1.1.0\""));
    assert!(toml_content.contains("redis = \"1.4.0\""));

    // 4. Verify datara.lock exists and has entries
    let lock = DataraLock::load(&temp_proj).expect("datara.lock must exist after install");
    assert_eq!(lock.version, 1);
    assert!(lock.packages.contains_key("uuid"));
    assert!(lock.packages.contains_key("redis"));

    // 5. List packages
    let installed = registry.list_installed(&temp_proj);
    assert_eq!(installed.len(), 2);
    assert_eq!(installed[0].name, "redis");
    assert_eq!(installed[1].name, "uuid");

    // 6. Verify integrity
    let verifications = registry.verify(&temp_proj).expect("verify must succeed");
    assert_eq!(verifications.len(), 2);
    for v in verifications {
        assert_eq!(v.status, VerificationStatus::Valid);
    }

    // 7. Remove 'redis'
    let remove_res = registry.remove("redis", &temp_proj);
    assert!(remove_res.is_ok());
    assert!(!temp_proj.join("packages").join("redis").exists());
    assert!(temp_proj.join("packages").join("uuid").exists());

    let toml_after = fs::read_to_string(temp_proj.join("datara.toml")).unwrap();
    assert!(!toml_after.contains("redis ="));
    assert!(toml_after.contains("uuid ="));

    let lock_after = DataraLock::load(&temp_proj).unwrap();
    assert!(!lock_after.packages.contains_key("redis"));
    assert!(lock_after.packages.contains_key("uuid"));

    let _ = fs::remove_dir_all(&temp_proj);
}

#[test]
fn test_dpm_compile_with_installed_package() {
    let temp_proj = std::env::temp_dir().join("datara_dpm_compile_test");
    let _ = fs::remove_dir_all(&temp_proj);

    let _ = HyperGridRegistry::init_project(&temp_proj, "calc_app", false);
    let registry = HyperGridRegistry::new();
    let uuid_pkg = registry.lookup("uuid").expect("uuid package must exist");
    registry
        .install(uuid_pkg, &temp_proj)
        .expect("install uuid must succeed");

    let main_dtr = temp_proj.join("src").join("main.dtr");
    let code = r#"
use uuid

fn main() {
    let id = Uuid.v4()
    if str_len(id) > 0 {
        println("[OK] Uuid generated: " + id)
    }
}
"#;
    fs::write(&main_dtr, code).unwrap();

    let compiler = ForgenCompiler::new("test");
    let res = compiler.run_file(&main_dtr, &[]);
    assert!(
        res.is_ok(),
        "Compilation with installed package should succeed: {:?}",
        res.err()
    );
    let (stdout, stderr, exit_code, _) = res.unwrap();
    assert_eq!(exit_code, 0, "Failed with stderr: {}", stderr);
    assert!(stdout.contains("[OK] Uuid generated:"));

    let _ = fs::remove_dir_all(&temp_proj);
}
