use forgen::driver::ForgenCompiler;
use std::fs;
use std::path::PathBuf;

fn create_temp_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("forgen_mod_test_{}_{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn test_multimodule_public_import_and_execution() {
    let temp_dir = create_temp_test_dir("pub_import");
    let math_file = temp_dir.join("math.dtr");
    let main_file = temp_dir.join("main.dtr");

    let math_src = r#"
pub struct Vector {
    x: Int,
    y: Int,
}

pub fn add(a: Int, b: Int) -> Int => a + b
"#;

    let main_src = r#"
use math.Vector
use math.add

fn main() {
    let v = Vector { x: 10, y: 20 }
    let sum = add(v.x, v.y)
    out sum
}
"#;

    fs::write(&math_file, math_src).unwrap();
    fs::write(&main_file, main_src).unwrap();

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_file(&main_file, None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "30");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_directory_module_loading() {
    let temp_dir = create_temp_test_dir("dir_mod");
    let geom_dir = temp_dir.join("geom");
    fs::create_dir_all(&geom_dir).unwrap();
    let mod_file = geom_dir.join("mod.dtr");
    let main_file = temp_dir.join("main.dtr");

    let mod_src = r#"
pub struct Point {
    x: Int,
    y: Int,
}

pub fn dist_sq(p: Point) -> Int => p.x * p.x + p.y * p.y
"#;

    let main_src = r#"
use geom

fn main() {
    let pt = Point { x: 3, y: 4 }
    let d = dist_sq(pt)
    out d
}
"#;

    fs::write(&mod_file, mod_src).unwrap();
    fs::write(&main_file, main_src).unwrap();

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_file(&main_file, None);
    assert!(
        res.success,
        "Directory module compilation failed: {:?}",
        res.error
    );

    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "25");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_private_item_import_rejection() {
    let temp_dir = create_temp_test_dir("private_import");
    let secret_file = temp_dir.join("secret_mod.dtr");
    let main_file = temp_dir.join("main.dtr");

    let secret_src = r#"
fn hidden_calculation() -> Int => 42
pub fn public_calculation() -> Int => hidden_calculation() * 2
"#;

    let main_src = r#"
use secret_mod.hidden_calculation

fn main() {
    out hidden_calculation()
}
"#;

    fs::write(&secret_file, secret_src).unwrap();
    fs::write(&main_file, main_src).unwrap();

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_file(&main_file, None);
    assert!(!res.success, "Should fail when importing private item");
    let err_str = res.error.unwrap_or_default();
    assert!(
        err_str.contains("E0042") || err_str.contains("private item"),
        "Error should reference private item access: {}",
        err_str
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_private_item_access_rejection() {
    let temp_dir = create_temp_test_dir("private_access");
    let secret_file = temp_dir.join("secret_mod.dtr");
    let main_file = temp_dir.join("main.dtr");

    let secret_src = r#"
fn hidden_fn() -> Int => 99
pub fn ok_fn() -> Int => 100
"#;

    let main_src = r#"
use secret_mod

fn main() {
    out hidden_fn()
}
"#;

    fs::write(&secret_file, secret_src).unwrap();
    fs::write(&main_file, main_src).unwrap();

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_file(&main_file, None);
    assert!(
        !res.success,
        "Should fail when accessing private item across modules"
    );
    let err_str = res.error.unwrap_or_default();
    assert!(
        err_str.contains("E0042") || err_str.contains("private item"),
        "Error should reference private item access: {}",
        err_str
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_private_struct_access_rejection() {
    let temp_dir = create_temp_test_dir("private_struct");
    let types_file = temp_dir.join("types_mod.dtr");
    let main_file = temp_dir.join("main.dtr");

    let types_src = r#"
struct SecretState {
    data: Int,
}

pub struct PublicState {
    id: Int,
}
"#;

    let main_src = r#"
use types_mod

fn main() {
    let s = SecretState { data: 55 }
    out s.data
}
"#;

    fs::write(&types_file, types_src).unwrap();
    fs::write(&main_file, main_src).unwrap();

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_file(&main_file, None);
    assert!(
        !res.success,
        "Should fail when instantiating private struct across modules"
    );
    let err_str = res.error.unwrap_or_default();
    assert!(
        err_str.contains("E0042")
            || err_str.contains("private class")
            || err_str.contains("private item"),
        "Error should reference private class access: {}",
        err_str
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_circular_import_cycle_rejection() {
    let temp_dir = create_temp_test_dir("cycle");
    let a_file = temp_dir.join("cycle_a.dtr");
    let b_file = temp_dir.join("cycle_b.dtr");
    let main_file = temp_dir.join("main.dtr");

    let a_src = r#"
use cycle_b
pub fn a() -> Int => 1
"#;

    let b_src = r#"
use cycle_a
pub fn b() -> Int => 2
"#;

    let main_src = r#"
use cycle_a

fn main() {
    out a()
}
"#;

    fs::write(&a_file, a_src).unwrap();
    fs::write(&b_file, b_src).unwrap();
    fs::write(&main_file, main_src).unwrap();

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_file(&main_file, None);
    assert!(!res.success, "Circular import must fail");
    let err_str = res.error.unwrap_or_default();
    assert!(
        err_str.contains("Circular module import") || err_str.contains("E-RESOLVE-004"),
        "Error should report circular import cycle: {}",
        err_str
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_pub_trait_across_modules() {
    let temp_dir = create_temp_test_dir("pub_trait");
    let trait_file = temp_dir.join("traits_mod.dtr");
    let main_file = temp_dir.join("main.dtr");

    let trait_src = r#"
pub trait Describable {
    fn describe(&self) -> Str;
}
"#;

    let main_src = r#"
use traits_mod.Describable

pub struct Widget {
    name: Str,
}

impl Describable for Widget {
    fn describe(&self) -> Str {
        return "Widget: " + self.name;
    }
}

fn main() {
    let w = Widget { name: "Cog" }
    out w.describe()
}
"#;

    fs::write(&trait_file, trait_src).unwrap();
    fs::write(&main_file, main_src).unwrap();

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_file(&main_file, None);
    assert!(
        res.success,
        "Pub trait across modules failed: {:?}",
        res.error
    );

    let (stdout, _, code, _) = compiler
        .codegen
        .run_executable(&res.exe_path.unwrap(), &[])
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "Widget: Cog");

    let _ = fs::remove_dir_all(&temp_dir);
}
