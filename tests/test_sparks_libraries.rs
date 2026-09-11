//! Test suite verifying that seed Sparks packages (mathx, strx, jsonx) compile
//! and execute with 100% correct results.

use forgen::driver::ForgenCompiler;
use std::fs;
use std::process::Command;

#[test]
fn test_sparks_libraries_e2e_execution() {
    let mathx_src =
        fs::read_to_string("packages/sparks/mathx/src/lib.dtr").expect("mathx source must exist");
    let strx_src =
        fs::read_to_string("packages/sparks/strx/src/lib.dtr").expect("strx source must exist");
    let jsonx_src =
        fs::read_to_string("packages/sparks/jsonx/src/lib.dtr").expect("jsonx source must exist");

    let combined_src = format!(
        r#"
{}
{}
{}

fn main() {{
    // 1. mathx verification
    let c = clamp(15, 0, 10)
    let p = pow(2, 8)
    let g = gcd(48, 18)
    let l = lcm(12, 18)
    let f = factorial(5)
    out "MATHX_OK:" + c + "," + p + "," + g + "," + l + "," + f

    // 2. strx verification
    let rep = repeat("abc", 3)
    let padded = pad_left("42", 5, "0")
    let sw = starts_with("datara_lang", "datara")
    let ew = ends_with("main.dtr", ".dtr")
    out "STRX_OK:" + rep + "," + padded + "," + sw + "," + ew

    // 3. jsonx verification
    let valid_json = "{{\"key\": [1, 2, 3]}}"
    let tokens = count_tokens(valid_json)
    let is_valid = validate_brackets(valid_json)
    out "JSONX_OK:" + tokens + "," + is_valid
}}
"#,
        mathx_src, strx_src, jsonx_src
    );

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(&combined_src, "test_sparks_e2e.dtr", None);
    assert!(res.success, "Sparks compilation failed: {:?}", res.error);

    let exe = res.exe_path.expect("exe_path missing");
    let out = Command::new(&exe)
        .output()
        .expect("Failed to execute binary");

    let _ = fs::remove_file(&exe);
    let _ = fs::remove_file(exe.with_extension("obj"));

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("MATHX_OK:10,256,6,36,120"),
        "Mathx output mismatch: {}",
        stdout
    );
    assert!(
        stdout.contains("STRX_OK:abcabcabc,00042,true,true"),
        "Strx output mismatch: {}",
        stdout
    );
    assert!(
        stdout.contains("JSONX_OK:7,true"),
        "Jsonx output mismatch: {}",
        stdout
    );
}
