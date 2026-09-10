//! CLI command implementation for `dpm rust-bridge <crate> --api manifest.toml`.
//!
//! Generates a Rust shim crate (`cdylib` / `staticlib` with `extern "C" fn` trampolines
//! wrapped in `std::panic::catch_unwind`), compiles it via `cargo`, and emits
//! a Datara `.dtr` module with `extern fn` bindings and zero-copy buffer views.

use crate::rust_bridge::builder::find_cargo;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BridgeParam {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BridgeFunction {
    pub name: String,
    #[serde(default)]
    pub signature: Option<String>,
    #[serde(default)]
    pub params: Vec<BridgeParam>,
    #[serde(default = "default_return_type")]
    pub return_type: String,
    #[serde(default)]
    pub code: Option<String>,
}

fn default_return_type() -> String {
    "Unit".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BridgeCrateSpec {
    pub name: Option<String>,
    pub version: Option<String>,
    pub path: Option<String>,
    pub git: Option<String>,
    pub features: Option<Vec<String>>,
    #[serde(default)]
    pub dependencies: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BridgeManifest {
    #[serde(rename = "crate")]
    pub crate_spec: Option<BridgeCrateSpec>,
    pub package: Option<BridgeCrateSpec>,
    #[serde(default)]
    pub functions: Vec<BridgeFunction>,
}

impl BridgeManifest {
    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read manifest file '{}': {}", path.display(), e))?;
        toml::from_str(&content).map_err(|e| {
            format!(
                "Failed to parse manifest TOML in '{}': {}",
                path.display(),
                e
            )
        })
    }
}

/// Run `dpm rust-bridge <crate> [--api manifest.toml] [--out-dir <dir>]`
pub fn run_rust_bridge_cli(args: &[String]) -> Result<(), String> {
    let mut crate_name_opt = None;
    let mut api_path_opt = None;
    let mut out_dir_opt = None;

    let start_idx = args
        .iter()
        .position(|a| a == "rust-bridge" || a == "rust_bridge")
        .map(|pos| pos + 1)
        .unwrap_or(1);

    let mut i = start_idx;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--api" {
            if i + 1 < args.len() {
                api_path_opt = Some(args[i + 1].clone());
                i += 2;
                continue;
            }
        } else if arg.starts_with("--api=") {
            api_path_opt = Some(arg.trim_start_matches("--api=").to_string());
            i += 1;
            continue;
        } else if arg == "--out-dir" || arg == "-o" {
            if i + 1 < args.len() {
                out_dir_opt = Some(args[i + 1].clone());
                i += 2;
                continue;
            }
        } else if arg.starts_with("--out-dir=") {
            out_dir_opt = Some(arg.trim_start_matches("--out-dir=").to_string());
            i += 1;
            continue;
        } else if !arg.starts_with('-') && crate_name_opt.is_none() {
            crate_name_opt = Some(arg.clone());
        }
        i += 1;
    }

    let crate_name = match crate_name_opt {
        Some(name) => name,
        None => {
            return Err(
                "Usage: dpm rust-bridge <crate_name> --api manifest.toml [--out-dir <dir>]"
                    .to_string(),
            );
        }
    };

    let api_path = match api_path_opt {
        Some(p) => PathBuf::from(p),
        None => {
            // Check default candidates
            let candidate1 = PathBuf::from(format!("{}_api.toml", crate_name));
            let candidate2 = PathBuf::from("manifest.toml");
            let candidate3 = PathBuf::from("api.toml");
            if candidate1.exists() {
                candidate1
            } else if candidate2.exists() {
                candidate2
            } else if candidate3.exists() {
                candidate3
            } else {
                return Err(format!(
                    "No API manifest provided. Please specify `--api manifest.toml` or create '{}_api.toml'.",
                    crate_name
                ));
            }
        }
    };

    if !api_path.exists() {
        return Err(format!(
            "API manifest not found at '{}'",
            api_path.display()
        ));
    }

    let manifest = BridgeManifest::load_from_file(&api_path)?;

    let sanitized_crate = crate_name.replace('-', "_");
    let out_dir = out_dir_opt
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("bridges/{}_bridge", sanitized_crate)));

    println!(":: [DPM] Generating Rust Bridge for '{}'...", crate_name);
    println!("   API Manifest: {}", api_path.display());
    println!("   Target Dir:   {}", out_dir.display());

    let (shim_dir, cdylib_path, dtr_path) =
        build_bridge_crate(&crate_name, &manifest, &out_dir, &api_path)?;

    println!(
        "[DONE] Generated Rust shim crate at: {}",
        shim_dir.display()
    );
    println!(
        "[DONE] Compiled cdylib/staticlib:    {}",
        cdylib_path.display()
    );
    println!(
        "[DONE] Generated Datara binding:     {}",
        dtr_path.display()
    );

    Ok(())
}

/// Builds the bridge crate and outputs (shim_dir, cdylib_path, dtr_path).
pub fn build_bridge_crate(
    crate_name: &str,
    manifest: &BridgeManifest,
    out_dir: &Path,
    manifest_path: &Path,
) -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let sanitized_crate = crate_name.replace('-', "_");
    let shim_name = format!("{}_bridge", sanitized_crate);
    let shim_dir = out_dir.to_path_buf();
    let src_dir = shim_dir.join("src");
    fs::create_dir_all(&src_dir)
        .map_err(|e| format!("Failed to create directory '{}': {}", src_dir.display(), e))?;

    // 1. Generate Cargo.toml
    let mut cargo_toml = String::new();
    cargo_toml.push_str("[package]\n");
    cargo_toml.push_str(&format!("name = \"{}\"\n", shim_name));
    cargo_toml.push_str("version = \"0.1.0\"\n");
    cargo_toml.push_str("edition = \"2021\"\n\n");

    cargo_toml.push_str("[lib]\n");
    cargo_toml.push_str("crate-type = [\"cdylib\", \"staticlib\"]\n\n");

    cargo_toml.push_str("[dependencies]\n");

    let crate_spec = manifest.crate_spec.as_ref().or(manifest.package.as_ref());

    if let Some(spec) = crate_spec {
        if let Some(ref path) = spec.path {
            let manifest_parent = manifest_path.parent().unwrap_or(Path::new("."));
            let abs_path = if Path::new(path).is_absolute() {
                PathBuf::from(path)
            } else {
                manifest_parent.join(path)
            };
            let p_str = abs_path.to_string_lossy().replace('\\', "/");
            cargo_toml.push_str(&format!("{} = {{ path = \"{}\"", crate_name, p_str));
            if let Some(ref feats) = spec.features {
                let f_list = feats
                    .iter()
                    .map(|f| format!("\"{}\"", f))
                    .collect::<Vec<_>>()
                    .join(", ");
                cargo_toml.push_str(&format!(", features = [{}]", f_list));
            }
            cargo_toml.push_str(" }\n");
        } else if let Some(ref git) = spec.git {
            cargo_toml.push_str(&format!("{} = {{ git = \"{}\"", crate_name, git));
            if let Some(ref feats) = spec.features {
                let f_list = feats
                    .iter()
                    .map(|f| format!("\"{}\"", f))
                    .collect::<Vec<_>>()
                    .join(", ");
                cargo_toml.push_str(&format!(", features = [{}]", f_list));
            }
            cargo_toml.push_str(" }\n");
        } else if let Some(ref ver) = spec.version {
            cargo_toml.push_str(&format!("{} = \"{}\"\n", crate_name, ver));
        } else {
            cargo_toml.push_str(&format!("{} = \"*\"\n", crate_name));
        }

        for (dep_k, dep_v) in &spec.dependencies {
            cargo_toml.push_str(&format!("{} = \"{}\"\n", dep_k, dep_v));
        }
    } else {
        // Default dependency declaration
        cargo_toml.push_str(&format!("{} = \"*\"\n", crate_name));
    }

    fs::write(shim_dir.join("Cargo.toml"), &cargo_toml).map_err(|e| {
        format!(
            "Failed to write Cargo.toml in '{}': {}",
            shim_dir.display(),
            e
        )
    })?;

    // 2. Generate src/lib.rs
    let lib_rs_code = generate_shim_lib_rs(crate_name, &manifest.functions)?;
    fs::write(src_dir.join("lib.rs"), &lib_rs_code).map_err(|e| {
        format!(
            "Failed to write src/lib.rs in '{}': {}",
            shim_dir.display(),
            e
        )
    })?;

    // 3. Compile cdylib / staticlib via Cargo
    let cargo = find_cargo().ok_or_else(|| {
        "Cargo executable not found. Ensure Rust/Cargo is installed\n\
         (checked $USERPROFILE/.cargo/bin/cargo.exe, $CARGO, and PATH)."
            .to_string()
    })?;

    let mut cmd = Command::new(&cargo);
    cmd.arg("build").arg("--release").current_dir(&shim_dir);

    // Strip ASan/sanitizer flags from child cargo invocation.
    // Proc-macro crates (e.g. zerocopy_derive, paste) must be compiled for host
    // and fail with E0463 when built with sanitizer flags.
    cmd.env_remove("RUSTFLAGS");
    cmd.env_remove("RUSTDOCFLAGS");
    cmd.env_remove("CARGO_ENCODED_RUSTFLAGS");
    cmd.env_remove("CARGO_BUILD_RUSTFLAGS");

    // Forward PATH with Cargo bin dir
    if let Some(cargo_bin) = cargo.parent() {
        let mut paths = vec![cargo_bin.to_path_buf()];
        if let Ok(p) = std::env::var("PATH") {
            paths.extend(std::env::split_paths(&p));
        }
        if let Ok(new_path) = std::env::join_paths(paths) {
            cmd.env("PATH", new_path);
        }
    }

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to execute cargo at '{}': {}", cargo.display(), e))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Cargo build failed for Rust bridge '{}':\n--- stdout ---\n{}\n--- stderr ---\n{}",
            shim_name,
            stdout.trim(),
            stderr.trim()
        ));
    }

    // Locate compiled cdylib / dll / lib
    let target_release = shim_dir.join("target").join("release");
    let (cdylib_name, import_lib_name) = if cfg!(windows) {
        (
            format!("{}.dll", shim_name),
            format!("{}.dll.lib", shim_name),
        )
    } else if cfg!(target_os = "macos") {
        (
            format!("lib{}.dylib", shim_name),
            format!("lib{}.a", shim_name),
        )
    } else {
        (
            format!("lib{}.so", shim_name),
            format!("lib{}.a", shim_name),
        )
    };

    let built_cdylib = target_release.join(&cdylib_name);
    let final_cdylib = shim_dir.join(&cdylib_name);

    if built_cdylib.exists() {
        let _ = fs::copy(&built_cdylib, &final_cdylib);
    }

    let built_lib = target_release.join(&import_lib_name);
    if built_lib.exists() {
        let _ = fs::copy(&built_lib, shim_dir.join(&import_lib_name));
    }
    // Also copy staticlib if available
    let static_name = if cfg!(windows) {
        format!("{}.lib", shim_name)
    } else {
        format!("lib{}.a", shim_name)
    };
    let built_static = target_release.join(&static_name);
    if built_static.exists() {
        let _ = fs::copy(&built_static, shim_dir.join(&static_name));
    }

    // 4. Generate C header (.h)
    let h_code = generate_c_header(&shim_name, &manifest.functions);
    let h_path = shim_dir.join(format!("{}.h", shim_name));
    fs::write(&h_path, &h_code)
        .map_err(|e| format!("Failed to write C header '{}': {}", h_path.display(), e))?;

    // 5. Generate Datara binding module (.dtr)
    let dtr_code = generate_datara_binding_dtr(&shim_name, &manifest.functions);
    let dtr_path = shim_dir.join(format!("{}.dtr", shim_name));
    fs::write(&dtr_path, &dtr_code).map_err(|e| {
        format!(
            "Failed to write Datara binding '{}': {}",
            dtr_path.display(),
            e
        )
    })?;

    Ok((shim_dir, final_cdylib, dtr_path))
}

fn generate_c_header(shim_name: &str, functions: &[BridgeFunction]) -> String {
    let mut h = String::new();
    let guard = format!("{}_H", shim_name.to_uppercase());
    h.push_str(&format!("#ifndef {}\n#define {}\n\n", guard, guard));
    h.push_str("#include <stdbool.h>\n#include <stdint.h>\n\n");
    h.push_str("#ifdef __cplusplus\nextern \"C\" {\n#endif\n\n");

    for f in functions {
        let ret_c = match f.return_type.as_str() {
            "Int" => "int64_t",
            "Float" => "double",
            "Bool" => "bool",
            "String" | "Str" => "const char*",
            "Pointer" | "RawPtr" => "const uint8_t*",
            _ => "void",
        };
        let mut params_c = Vec::new();
        for p in &f.params {
            let p_ty = match p.param_type.as_str() {
                "Int" => "int64_t",
                "Float" => "double",
                "Bool" => "bool",
                "String" | "Str" => "const char*",
                "Pointer" | "RawPtr" => "const uint8_t*",
                _ => "int64_t",
            };
            params_c.push(format!("{} {}", p_ty, p.name));
        }
        if params_c.is_empty() {
            params_c.push("void".to_string());
        }
        h.push_str(&format!("{} {}({});\n", ret_c, f.name, params_c.join(", ")));
    }

    h.push_str("\n#ifdef __cplusplus\n}\n#endif\n");
    h.push_str(&format!("\n#endif // {}\n", guard));
    h
}

fn generate_shim_lib_rs(crate_name: &str, functions: &[BridgeFunction]) -> Result<String, String> {
    let sanitized_crate = crate_name.replace('-', "_");
    let mut code = String::new();
    code.push_str("//! Auto-generated Datara Rust Bridge Shim\n");
    code.push_str("#![allow(unused_imports, non_snake_case, dead_code, unused_variables)]\n\n");
    code.push_str("use std::cell::RefCell;\n");
    code.push_str("use std::ffi::{CStr, CString};\n");
    code.push_str("use std::os::raw::c_char;\n\n");

    // Thread-local ring buffer for returning zero-leak CStrings across C-ABI
    code.push_str("thread_local! {\n");
    code.push_str(
        "    static STRING_RING: RefCell<Vec<CString>> = const { RefCell::new(Vec::new()) };\n",
    );
    code.push_str("}\n\n");

    code.push_str("fn __bridge_ret_str(s: String) -> *const c_char {\n");
    code.push_str("    let cs = CString::new(s).unwrap_or_default();\n");
    code.push_str("    let ptr = cs.as_ptr();\n");
    code.push_str("    STRING_RING.with(|ring| {\n");
    code.push_str("        let mut b = ring.borrow_mut();\n");
    code.push_str("        if b.len() >= 32 { b.remove(0); }\n");
    code.push_str("        b.push(cs);\n");
    code.push_str("    });\n");
    code.push_str("    ptr\n");
    code.push_str("}\n\n");

    for f in functions {
        let ret_c = match f.return_type.as_str() {
            "Int" => "i64",
            "Float" => "f64",
            "Bool" => "bool",
            "String" | "Str" => "*const c_char",
            "Pointer" | "RawPtr" => "*const u8",
            _ => "()",
        };

        code.push_str("#[unsafe(no_mangle)]\n");
        if ret_c == "()" {
            code.push_str(&format!("pub extern \"C\" fn {}(", f.name));
        } else {
            code.push_str(&format!("pub extern \"C\" fn {}(", f.name));
        }

        let mut param_decls = Vec::new();
        for p in &f.params {
            let p_ty = match p.param_type.as_str() {
                "Int" => "i64",
                "Float" => "f64",
                "Bool" => "bool",
                "String" | "Str" => "*const c_char",
                "Pointer" | "RawPtr" => "*const u8",
                _ => "i64",
            };
            param_decls.push(format!("{}: {}", p.name, p_ty));
        }
        code.push_str(&param_decls.join(", "));
        if ret_c == "()" {
            code.push_str(") {\n");
        } else {
            code.push_str(&format!(") -> {} {{\n", ret_c));
        }

        code.push_str(
            "    let __panic_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {\n",
        );

        // Convert parameters
        for p in &f.params {
            if p.param_type == "String" || p.param_type == "Str" {
                code.push_str(&format!(
                    "        let {p} = if {p}.is_null() {{ \"\" }} else {{ unsafe {{ CStr::from_ptr({p}).to_str().unwrap_or(\"\") }} }};\n",
                    p = p.name
                ));
            }
        }

        // Execute function code or direct call
        if let Some(ref custom_code) = f.code {
            for line in custom_code.lines() {
                code.push_str(&format!("        {}\n", line));
            }
        } else {
            let call_args = f
                .params
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            code.push_str(&format!(
                "        {}::{}({})\n",
                sanitized_crate, f.name, call_args
            ));
        }

        code.push_str("    }));\n\n");

        // Return handling
        code.push_str("    match __panic_res {\n");
        if f.return_type == "String" || f.return_type == "Str" {
            code.push_str("        Ok(res) => __bridge_ret_str(res.to_string()),\n");
            code.push_str("        Err(_) => __bridge_ret_str(String::new()),\n");
        } else if f.return_type == "Int" {
            code.push_str("        Ok(res) => res as i64,\n");
            code.push_str("        Err(_) => 0,\n");
        } else if f.return_type == "Float" {
            code.push_str("        Ok(res) => res as f64,\n");
            code.push_str("        Err(_) => 0.0,\n");
        } else if f.return_type == "Bool" {
            code.push_str("        Ok(res) => res,\n");
            code.push_str("        Err(_) => false,\n");
        } else if f.return_type == "Pointer" || f.return_type == "RawPtr" {
            code.push_str("        Ok(res) => res as *const u8,\n");
            code.push_str("        Err(_) => std::ptr::null(),\n");
        } else {
            code.push_str("        Ok(_) => (),\n");
            code.push_str("        Err(_) => (),\n");
        }
        code.push_str("    }\n");
        code.push_str("}\n\n");
    }

    Ok(code)
}

fn generate_datara_binding_dtr(shim_name: &str, functions: &[BridgeFunction]) -> String {
    let mut code = String::new();
    code.push_str(&format!(
        "// Auto-generated Datara Rust Bridge Binding: {}\n// Generated by: dpm rust-bridge\n\n",
        shim_name
    ));

    for f in functions {
        let ret_str = if f.return_type != "Unit" && !f.return_type.is_empty() {
            format!(" -> {}", f.return_type)
        } else {
            String::new()
        };

        let mut params_str = Vec::new();
        for p in &f.params {
            params_str.push(format!("{}: {}", p.name, p.param_type));
        }

        code.push_str(&format!(
            "extern fn {}({}){}\n",
            f.name,
            params_str.join(", "),
            ret_str
        ));
    }

    // Zero-copy buffer view constructor for Datara
    code.push_str("\n// Zero-copy buffer view constructor\n");
    code.push_str("fn make_buffer_view(ptr: Pointer, len: Int) -> Pointer {\n");
    code.push_str("    return ptr\n");
    code.push_str("}\n");

    code
}
