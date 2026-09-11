use forgen::driver::ForgenCompiler;
use std::fs;
use std::path::Path;

#[test]
fn test_phase4_showcase_cli_app() {
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_project(Path::new("examples/showcase/cli_app"), None);
    assert!(res.success, "cli_app compilation failed: {:?}", res.error);
    let exe = res.exe_path.expect("cli_app binary path");
    let (stdout, stderr, code, _) = compiler
        .codegen
        .run_executable(&exe, &[])
        .expect("run cli_app");
    assert_eq!(code, 0, "cli_app failed with stderr: {}", stderr);
    assert!(
        stdout.contains("Analysis Results:"),
        "missing analysis: {}",
        stdout
    );
    assert!(
        stdout.contains("Tokens:  23"),
        "tokens mismatch: {}",
        stdout
    );
    assert!(
        stdout.contains("Strings: 8"),
        "strings count mismatch: {}",
        stdout
    );
    assert!(
        stdout.contains("STATUS_OK"),
        "missing status OK: {}",
        stdout
    );
}

#[test]
fn test_phase4_sparks_seed_packages() {
    let pkgs = ["mathx", "strx", "jsonx"];
    for name in pkgs {
        let base = Path::new("packages/sparks").join(name);
        let toml_path = base.join("datara.toml");
        let caps_path = base.join("capabilities.json");
        let lib_path = base.join("src/lib.dtr");

        assert!(toml_path.exists(), "missing datara.toml for {}", name);
        assert!(caps_path.exists(), "missing capabilities.json for {}", name);
        assert!(lib_path.exists(), "missing src/lib.dtr for {}", name);

        let toml_content = fs::read_to_string(&toml_path).expect("read toml");
        assert!(toml_content.contains("manifest_schema = 1"));
        assert!(toml_content.contains(&format!("name = \"{}\"", name)));

        let caps_content = fs::read_to_string(&caps_path).expect("read caps");
        let parsed: serde_json::Value = serde_json::from_str(&caps_content).expect("parse json");
        assert_eq!(parsed["manifest_schema"], 1);
        assert_eq!(parsed["package"], name);
    }
}

#[test]
fn test_phase4_tutorial_all_steps() {
    let steps = [
        "step01_hello",
        "step02_types_and_vars",
        "step03_control_flow",
        "step04_functions",
        "step05_records",
        "step06_modules",
        "step07_outcomes",
        "step08_capabilities",
        "step09_interop",
        "step10_production_cli",
    ];

    let compiler = ForgenCompiler::new("release");
    for step in steps {
        let step_dir = Path::new("examples/tutorial").join(step);
        let res = compiler.compile_project(&step_dir, None);
        assert!(
            res.success,
            "Tutorial {} compilation failed: {:?}",
            step, res.error
        );
        let exe = res.exe_path.expect("tutorial binary path");
        let (_, stderr, code, _) = compiler
            .codegen
            .run_executable(&exe, &[])
            .unwrap_or_else(|e| panic!("run failed on {}: {}", step, e));
        assert_eq!(code, 0, "Tutorial {} exited with error: {}", step, stderr);
    }
}
