use forgen::driver::ForgenCompiler;
use forgen::project::{
    DataraManifest, ProjectDiscovery, ProjectInitializer, ProjectKind, ProjectRunner,
};
use std::fs;

#[test]
fn test_level_1_single_file_discovery_and_execution() {
    let temp_dir = std::env::temp_dir().join("datara_test_lvl1");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let single_file = temp_dir.join("hello.dtr");
    fs::write(
        &single_file,
        r#"
fn main() {
    out "Hello from Single File Level 1!"
}
"#,
    )
    .unwrap();

    // 1. Auto-discovery
    let layout = ProjectDiscovery::discover(Some(&single_file)).unwrap();
    assert!(matches!(layout.kind, ProjectKind::SingleFile(_)));
    assert_eq!(layout.name, "hello");
    assert_eq!(layout.source_files.len(), 1);
    assert!(layout.manifest.is_none());

    // 2. Compilation and Run
    let compiler = ForgenCompiler::new("release");
    let res = compiler.run_project(&layout, &[]);
    assert!(res.is_ok(), "Run project should succeed: {:?}", res.err());

    let (stdout, stderr, code, _) = res.unwrap();
    assert_eq!(code, 0);
    assert!(stdout.contains("Hello from Single File Level 1!"));
    assert!(stderr.is_empty());

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_level_2_directory_project_discovery_and_modules() {
    let temp_dir = std::env::temp_dir().join("datara_test_lvl2_dir");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    // Create main.dtr and helper.dtr in directory without datara.toml
    let main_file = temp_dir.join("main.dtr");
    fs::write(
        &main_file,
        r#"
fn calculate_bonus(salary: Int) -> Int {
    return salary + 500
}

fn main() {
    let bonus = calculate_bonus(1000)
    out "Bonus result: "
    out bonus
}
"#,
    )
    .unwrap();

    // 1. Discover by directory
    let layout = ProjectDiscovery::discover(Some(&temp_dir)).unwrap();
    assert!(matches!(layout.kind, ProjectKind::Directory(_)));
    assert_eq!(layout.source_files.len(), 1);
    assert!(layout.entry_point.ends_with("main.dtr"));
    assert!(layout.manifest.is_none());

    // 2. Run directory project
    let compiler = ForgenCompiler::new("release");
    let res = compiler.run_project(&layout, &[]);
    assert!(res.is_ok(), "Directory project run failed: {:?}", res.err());

    let (stdout, _, code, _) = res.unwrap();
    assert_eq!(code, 0);
    assert!(stdout.contains("1500"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_level_3_manifest_project_init_and_test_runner() {
    let temp_parent = std::env::temp_dir().join("datara_test_lvl3_parent");
    let _ = fs::remove_dir_all(&temp_parent);
    fs::create_dir_all(&temp_parent).unwrap();

    // 1. forgen init my_service
    let init_res = ProjectInitializer::init(Some("my_service"), &temp_parent);
    assert!(
        init_res.is_ok(),
        "ProjectInitializer::init failed: {:?}",
        init_res.err()
    );

    let project_dir = temp_parent.join("my_service");
    assert!(project_dir.join("datara.toml").exists());
    assert!(project_dir.join("src/main.dtr").exists());
    assert!(project_dir.join("tests/test_main.dtr").exists());
    assert!(project_dir.join("examples/demo.dtr").exists());
    assert!(project_dir.join(".gitignore").exists());

    // 2. Parse manifest
    let manifest = DataraManifest::from_file(&project_dir.join("datara.toml")).unwrap();
    assert_eq!(manifest.package.name, "my_service");
    assert_eq!(manifest.package.version, "1.0.0");
    assert_eq!(manifest.package.entry, Some("src/main.dtr".to_string()));

    // 3. Discover project
    let layout = ProjectDiscovery::discover(Some(&project_dir)).unwrap();
    assert!(matches!(layout.kind, ProjectKind::ManifestProject(_)));
    assert_eq!(layout.name, "my_service");
    assert_eq!(layout.test_files.len(), 1);
    assert_eq!(layout.example_files.len(), 1);

    // 4. Run main project
    let compiler = ForgenCompiler::new("release");
    let run_res = compiler.run_project(&layout, &[]);
    assert!(
        run_res.is_ok(),
        "Run Level 3 project failed: {:?}",
        run_res.err()
    );
    let (stdout, _, code, _) = run_res.unwrap();
    assert_eq!(code, 0);
    assert!(stdout.contains("Hello from Datara!"));

    // 5. Run test suite through ProjectRunner
    let test_report = ProjectRunner::run_tests(&layout, &compiler);
    assert_eq!(test_report.total, 1);
    assert_eq!(test_report.passed, 1);
    assert_eq!(test_report.failed, 0);
    assert!(test_report.results[0].passed);
    assert!(
        test_report.results[0]
            .output
            .contains("PASS: test_addition")
    );

    let _ = fs::remove_dir_all(&temp_parent);
}

#[test]
fn test_whole_program_domain_specialization_on_project() {
    let temp_dir = std::env::temp_dir().join("datara_test_domain_proj");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let main_dtr = temp_dir.join("main.dtr");
    fs::write(
        &main_dtr,
        r#"
class Point {
    x: Int
    y: Int
}

fn sum_point(p: Point) -> Int {
    return p.x + p.y
}

fn unused_function() -> Int {
    return 9999
}

fn main() {
    mut pt = Point { x: 100, y: 200 }
    let s = sum_point(pt)
    out s
}
"#,
    )
    .unwrap();

    let layout = ProjectDiscovery::discover(Some(&temp_dir)).unwrap();
    let compiler = ForgenCompiler::new("domain");

    let comp_res = compiler.compile_files(&layout.source_files, None);
    assert!(
        comp_res.success,
        "Domain compilation failed: {:?}",
        comp_res.error
    );

    let rep = comp_res.optimization_report.unwrap();
    assert!(rep.symbols_analyzed >= 3);
    assert!(
        rep.removed_symbols >= 1,
        "Dead unused_function should be stripped"
    );

    let exe = comp_res.exe_path.unwrap();
    let (stdout, _, code, _) = compiler.codegen.run_executable(&exe, &[]).unwrap();
    assert_eq!(code, 0);
    assert!(stdout.contains("300"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_datara_test_runner_function_isolation_and_filtering() {
    let temp_parent = std::env::temp_dir().join("datara_test_fn_runner_parent");
    let temp_dir = temp_parent.join("my_test_app");
    let _ = fs::remove_dir_all(&temp_parent);
    fs::create_dir_all(temp_dir.join("tests")).unwrap();

    let suite_dtr = temp_dir.join("tests").join("suite.dtr");
    fs::write(
        &suite_dtr,
        r#"
test fn test_addition_ok() {
    let a = 10
    let b = 20
    assert(a + b == 30)
}

@test fn test_trapping_fails() {
    assert(1 == 2)
}

test fn test_multiplication_ok() {
    let x = 5 * 6
    assert(x == 30)
}
"#,
    )
    .unwrap();

    let layout = ProjectDiscovery::discover(Some(&temp_dir)).unwrap();
    let compiler = ForgenCompiler::new("release");

    // 1. Run all tests: 2 should pass, 1 should fail, runner should not abort
    let rep_all = ProjectRunner::run_tests(&layout, &compiler);
    assert_eq!(rep_all.total, 3, "Expected 3 test functions discovered");
    assert_eq!(rep_all.passed, 2, "Expected 2 tests to pass");
    assert_eq!(rep_all.failed, 1, "Expected 1 test to fail");

    // 2. Filter by "addition": exactly 1 test should run and pass
    let rep_filter_add = ProjectRunner::run_tests_filtered(&layout, &compiler, Some("addition"));
    assert_eq!(rep_filter_add.total, 1);
    assert_eq!(rep_filter_add.passed, 1);
    assert_eq!(rep_filter_add.failed, 0);
    assert_eq!(rep_filter_add.results[0].name, "test_addition_ok");

    // 3. Filter by "trapping": exactly 1 test should run and fail
    let rep_filter_trap = ProjectRunner::run_tests_filtered(&layout, &compiler, Some("trapping"));
    assert_eq!(rep_filter_trap.total, 1);
    assert_eq!(rep_filter_trap.passed, 0);
    assert_eq!(rep_filter_trap.failed, 1);
    assert_eq!(rep_filter_trap.results[0].name, "test_trapping_fails");

    let _ = fs::remove_dir_all(&temp_parent);
}
