use forgen::driver::ForgenCompiler;
use forgen::project::HyperGridRegistry;
use std::fs;

#[test]
fn test_hypergrid_registry_and_cas_install() {
    let registry = HyperGridRegistry::new();

    // 1. Lookup
    let redis_pkg = registry.lookup("redis");
    assert!(
        redis_pkg.is_some(),
        "redis package must exist in HyperGrid registry"
    );
    let redis = redis_pkg.unwrap();
    assert_eq!(redis.name, "redis");
    assert!(redis.digest.starts_with("sha256:"));
    assert!(redis.capabilities.contains(&"net.connect".to_string()));

    // 2. Search
    let results = registry.search("client");
    assert!(
        !results.is_empty(),
        "Should find packages matching 'client'"
    );

    // 3. Install into temporary project
    let temp_proj = std::env::temp_dir().join("datara_hypergrid_test_proj");
    let _ = fs::remove_dir_all(&temp_proj);
    let _ = fs::create_dir_all(&temp_proj);

    let install_res = registry.install(redis, &temp_proj);
    assert!(
        install_res.is_ok(),
        "Package installation into CAS/project must succeed"
    );

    // Verify linked files in project
    let installed_file = temp_proj.join("packages").join("redis").join("redis.dtr");
    assert!(
        installed_file.exists(),
        "Linked package source file must exist"
    );

    // Verify datara.toml was updated
    let manifest_file = temp_proj.join("datara.toml");
    assert!(
        manifest_file.exists(),
        "datara.toml must be generated/updated"
    );
    let content = fs::read_to_string(&manifest_file).unwrap();
    assert!(content.contains("redis = \"1.4.0\""));

    let _ = fs::remove_dir_all(&temp_proj);
}

#[test]
fn test_jit_predictive_auto_install() {
    let temp_proj = std::env::temp_dir().join("datara_jit_autoinstall_proj");
    let _ = fs::remove_dir_all(&temp_proj);
    let _ = fs::create_dir_all(&temp_proj);

    let main_dtr = temp_proj.join("main.dtr");
    let code = r#"
use redis

fn main() {
    let r = Redis.connect("127.0.0.1", 6379)
    print("[OK] Redis client compiled seamlessly via JIT HyperGrid")
}
"#;
    fs::write(&main_dtr, code).unwrap();

    // Set FORGEN_AUTO_INSTALL=1
    unsafe {
        std::env::set_var("FORGEN_AUTO_INSTALL", "1");
    }

    let compiler = ForgenCompiler::new("test");
    let res = compiler.run_file(&main_dtr, &[]);
    assert!(
        res.is_ok(),
        "JIT Auto-install build should succeed: {:?}",
        res.err()
    );
    let (stdout, stderr, exit_code, _) = res.unwrap();
    assert_eq!(exit_code, 0, "Execution failed: stderr={}", stderr);
    assert!(stdout.contains("[OK] Redis client compiled seamlessly via JIT HyperGrid"));

    // Verify datara.toml was created and contains redis
    let manifest_file = temp_proj.join("datara.toml");
    assert!(
        manifest_file.exists(),
        "datara.toml must be updated by auto-installer"
    );

    let _ = fs::remove_dir_all(&temp_proj);
}
