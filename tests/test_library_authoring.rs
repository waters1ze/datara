use forgen::driver::ForgenCompiler;
use forgen::project::init::ProjectInitializer;
use std::fs;
use std::path::Path;

#[test]
fn test_community_library_creation_and_import() {
    let test_dir = Path::new("scratch/test_lib_workspace");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).unwrap();

    // 1. Initialize a community library inside scratch/test_lib_workspace/custom_math
    let lib_name = "custom_math";
    let init_res = ProjectInitializer::init_lib(Some(lib_name), test_dir);
    assert!(init_res.is_ok(), "Library initialization must succeed");

    let lib_dir = test_dir.join(lib_name);
    assert!(
        lib_dir.join("datara.toml").exists(),
        "datara.toml must exist"
    );
    assert!(
        lib_dir.join("src/lib.dtr").exists(),
        "src/lib.dtr must exist"
    );
    assert!(lib_dir.join("README.md").exists(), "README.md must exist");

    // 2. Create an app inside the workspace that uses the library
    let app_code = r#"
use custom_math

fn main() {
    let helper = create_helper(7)
    let answer = helper.multiply(6)
    out answer
}
"#;
    let app_file = test_dir.join("app.dtr");
    fs::write(&app_file, app_code).unwrap();

    // 3. Compile and execute the application
    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_file(&app_file, None);
    assert!(
        res.success,
        "Compilation of app importing community library failed: {:?}",
        res.error
    );

    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();

    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "42", "7 * 6 must equal 42");

    // Clean up
    let _ = fs::remove_dir_all(test_dir);
}

#[test]
fn test_lib_container_subdirectories_resolution() {
    let test_dir = Path::new("scratch/test_container_workspace");
    if test_dir.exists() {
        let _ = fs::remove_dir_all(test_dir);
    }
    fs::create_dir_all(test_dir).unwrap();

    // Create lib/tensor_lib/src/lib.dtr
    let tensor_dir = test_dir.join("lib").join("tensor_lib").join("src");
    fs::create_dir_all(&tensor_dir).unwrap();
    let lib_code = r#"
class Tensor {
    rows: Int
    cols: Int
}

behavior Tensor {
    total_elements() -> Int => this.rows * this.cols
}
"#;
    fs::write(tensor_dir.join("lib.dtr"), lib_code).unwrap();

    // Create app.dtr
    let app_code = r#"
use tensor_lib

fn main() {
    let t = Tensor { rows: 8, cols: 8 }
    out t.total_elements()
}
"#;
    let app_file = test_dir.join("app.dtr");
    fs::write(&app_file, app_code).unwrap();

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_file(&app_file, None);
    assert!(
        res.success,
        "Compilation with lib/ container resolution failed: {:?}",
        res.error
    );

    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();

    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "64", "8 * 8 must equal 64");

    // Clean up
    let _ = fs::remove_dir_all(test_dir);
}
