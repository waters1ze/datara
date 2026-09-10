use crate::ast::{Decl, ExternFnDecl, Param, TypeNode};
use crate::diagnostics::SourceSpan;
use crate::rust_bridge::config::{
    ResolvedRustFunction, RustCrateConfig, map_datara_type_to_rust_c,
};
use std::path::Path;

/// Generate Cargo.toml for the staticlib wrapper crate.
pub fn generate_wrapper_cargo_toml(
    crate_name: &str,
    config: &RustCrateConfig,
    base_dir: &Path,
) -> Result<String, String> {
    let sanitized_name = crate_name.replace('-', "_");
    let mut toml = String::new();
    toml.push_str("[package]\n");
    toml.push_str(&format!("name = \"{}_datara_wrapper\"\n", sanitized_name));
    toml.push_str("version = \"0.1.0\"\n");
    toml.push_str("edition = \"2021\"\n\n");

    toml.push_str("[lib]\n");
    toml.push_str("crate-type = [\"staticlib\", \"rlib\"]\n\n");

    toml.push_str("[dependencies]\n");
    if let Some(ref rel_path) = config.path {
        let abs_path = if Path::new(rel_path).is_absolute() {
            Path::new(rel_path).to_path_buf()
        } else {
            base_dir.join(rel_path)
        };
        // Use forward slashes for TOML compatibility on Windows
        let path_str = abs_path.to_string_lossy().replace('\\', "/");
        toml.push_str(&format!(
            "{} = {{ path = \"{}\" }}\n",
            sanitized_name, path_str
        ));
    } else if let Some(ref git) = config.git {
        toml.push_str(&format!("{} = {{ git = \"{}\"", sanitized_name, git));
        if let Some(ref branch) = config.branch {
            toml.push_str(&format!(", branch = \"{}\"", branch));
        }
        toml.push_str(" }\n");
    } else if let Some(ref ver) = config.version {
        toml.push_str(&format!("{} = \"{}\"\n", sanitized_name, ver));
    } else {
        return Err(format!(
            "Rust crate '{}' must specify either 'path', 'version', or 'git' in datara.toml",
            crate_name
        ));
    }

    Ok(toml)
}

/// Generate src/lib.rs for the staticlib wrapper crate with #[no_mangle] extern "C" trampolines.
pub fn generate_wrapper_lib_rs(crate_name: &str, functions: &[ResolvedRustFunction]) -> String {
    let sanitized_name = crate_name.replace('-', "_");
    let mut code = String::new();
    code.push_str("//! Auto-generated Datara Rust Bridge Wrapper\n");
    code.push_str("#![allow(unused_imports, non_snake_case, dead_code)]\n\n");

    for f in functions {
        code.push_str("#[no_mangle]\n");
        code.push_str(&format!("pub extern \"C\" fn {}(", f.name));

        for (i, (param_name, param_ty)) in f.params.iter().enumerate() {
            if i > 0 {
                code.push_str(", ");
            }
            let c_ty = map_datara_type_to_rust_c(param_ty);
            code.push_str(&format!("{}: {}", param_name, c_ty));
        }

        let ret_c = map_datara_type_to_rust_c(&f.ret_type);
        if ret_c != "()" {
            code.push_str(&format!(") -> {} {{\n", ret_c));
        } else {
            code.push_str(") {\n");
        }

        // Call expression
        let call_expr = if f.rust_call.contains("::") {
            f.rust_call.clone()
        } else {
            let arg_list = f
                .params
                .iter()
                .map(|(p, _)| p.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}::{}({})", sanitized_name, f.name, arg_list)
        };

        code.push_str(&format!("    {}\n", call_expr));
        code.push_str("}\n\n");
    }

    code
}

/// Generate Datara AST `Decl::ExternFn` definitions for the given functions.
pub fn generate_datara_extern_decls(functions: &[ResolvedRustFunction]) -> Vec<Decl> {
    let mut decls = Vec::new();

    for f in functions {
        let params: Vec<Param> = f
            .params
            .iter()
            .map(|(pname, pty)| Param {
                name: pname.clone(),
                type_node: Some(TypeNode::new(pty, SourceSpan::default())),
                ownership_mode: String::new(),
                span: SourceSpan::default(),
            })
            .collect();

        let return_type = if f.ret_type != "Unit" && !f.ret_type.is_empty() {
            Some(TypeNode::new(&f.ret_type, SourceSpan::default()))
        } else {
            None
        };

        decls.push(Decl::ExternFn(ExternFnDecl {
            abi: "C".to_string(),
            name: f.name.clone(),
            params,
            return_type,
            span: SourceSpan::default(),
        }));
    }

    decls
}
