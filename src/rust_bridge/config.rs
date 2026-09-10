use serde::{Deserialize, Serialize};
use std::path::Path;

fn default_ret() -> String {
    "Int".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RustFunctionDecl {
    String(String),
    Structured {
        name: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default = "default_ret")]
        ret: String,
        call: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RustCrateConfig {
    pub path: Option<String>,
    pub version: Option<String>,
    pub git: Option<String>,
    pub branch: Option<String>,
    #[serde(default)]
    pub functions: Vec<RustFunctionDecl>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRustFunction {
    pub name: String,
    pub params: Vec<(String, String)>, // (param_name, datara_type)
    pub ret_type: String,              // "Int", "Float", "Bool", "Str", "Unit"
    pub rust_call: String,             // e.g. "fixture_rust_crate::add(a, b)"
}

impl RustCrateConfig {
    /// Resolve all functions for this crate, either from explicit config or by scanning src/lib.rs.
    pub fn resolve_functions(
        &self,
        crate_name: &str,
        base_dir: &Path,
    ) -> Result<Vec<ResolvedRustFunction>, String> {
        let mut resolved = Vec::new();

        if !self.functions.is_empty() {
            for f in &self.functions {
                match f {
                    RustFunctionDecl::String(sig) => {
                        let parsed = parse_function_signature(sig, crate_name)?;
                        resolved.push(parsed);
                    }
                    RustFunctionDecl::Structured {
                        name,
                        args,
                        ret,
                        call,
                    } => {
                        let params: Vec<(String, String)> = args
                            .iter()
                            .enumerate()
                            .map(|(i, a)| (format!("a{}", i), a.clone()))
                            .collect();
                        let rust_call = if let Some(c) = call {
                            c.clone()
                        } else {
                            let arg_list = params
                                .iter()
                                .map(|(p, _)| p.as_str())
                                .collect::<Vec<_>>()
                                .join(", ");
                            format!("{}::{}({})", crate_name, name, arg_list)
                        };
                        resolved.push(ResolvedRustFunction {
                            name: name.clone(),
                            params,
                            ret_type: ret.clone(),
                            rust_call,
                        });
                    }
                }
            }
            return Ok(resolved);
        }

        // Auto-discover from crate source if path is provided
        if let Some(ref rel_path) = self.path {
            let crate_dir = if Path::new(rel_path).is_absolute() {
                Path::new(rel_path).to_path_buf()
            } else {
                base_dir.join(rel_path)
            };

            let lib_rs = crate_dir.join("src").join("lib.rs");
            if lib_rs.exists() {
                if let Ok(src) = std::fs::read_to_string(&lib_rs) {
                    let discovered = scan_rust_source_for_functions(&src, crate_name);
                    if !discovered.is_empty() {
                        return Ok(discovered);
                    }
                }
            }
        }

        Ok(resolved)
    }
}

/// Parse signatures like:
/// `fn add(a: Int, b: Int) -> Int`
/// `rust_add(a: Int, b: Int) -> Int = fixture_rust_crate::add(a, b)`
/// or bare `add`
pub fn parse_function_signature(
    sig: &str,
    crate_name: &str,
) -> Result<ResolvedRustFunction, String> {
    let sig = sig.trim();
    let (sig_part, custom_call) = if let Some((left, right)) = sig.split_once('=') {
        (left.trim(), Some(right.trim().to_string()))
    } else {
        (sig, None)
    };

    let trimmed_sig = sig_part.strip_prefix("fn ").unwrap_or(sig_part).trim();

    if let Some((name_part, rest)) = trimmed_sig.split_once('(') {
        let name = name_part.trim().to_string();
        let (args_str, ret_str) = if let Some((a, r)) = rest.split_once(')') {
            let ret = if let Some((_, r_ty)) = r.split_once("->") {
                r_ty.trim().to_string()
            } else {
                "Unit".to_string()
            };
            (a.trim(), ret)
        } else {
            return Err(format!(
                "Unclosed parenthesis in function signature: '{}'",
                sig
            ));
        };

        let mut params = Vec::new();
        if !args_str.is_empty() {
            for (idx, arg) in args_str.split(',').enumerate() {
                let arg = arg.trim();
                if arg.is_empty() {
                    continue;
                }
                if let Some((pname, ptype)) = arg.split_once(':') {
                    params.push((pname.trim().to_string(), ptype.trim().to_string()));
                } else {
                    params.push((format!("a{}", idx), arg.to_string()));
                }
            }
        }

        let rust_call = if let Some(c) = custom_call {
            c
        } else {
            let arg_names = params
                .iter()
                .map(|(p, _)| p.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}::{}({})", crate_name, name, arg_names)
        };

        Ok(ResolvedRustFunction {
            name,
            params,
            ret_type: if ret_str.is_empty() {
                "Unit".into()
            } else {
                ret_str
            },
            rust_call,
        })
    } else {
        // Bare name, e.g. "add" -> default signature
        let name = trimmed_sig.to_string();
        let rust_call = custom_call.unwrap_or_else(|| format!("{}::{}(a0, a1)", crate_name, name));
        Ok(ResolvedRustFunction {
            name,
            params: vec![("a0".into(), "Int".into()), ("a1".into(), "Int".into())],
            ret_type: "Int".into(),
            rust_call,
        })
    }
}

/// Simple regex-free scanner for public functions in Rust `src/lib.rs`.
pub fn scan_rust_source_for_functions(src: &str, crate_name: &str) -> Vec<ResolvedRustFunction> {
    let mut functions = Vec::new();

    for line in src.lines() {
        let line = line.trim();
        if !line.starts_with("pub fn ") {
            continue;
        }

        let after_fn = &line["pub fn ".len()..];
        if let Some((name, rest)) = after_fn.split_once('(') {
            let name = name.trim().to_string();
            if let Some((args_part, after_args)) = rest.split_once(')') {
                let mut params = Vec::new();
                for (idx, arg) in args_part.split(',').enumerate() {
                    let arg = arg.trim();
                    if arg.is_empty() || arg == "&self" || arg == "&mut self" || arg == "self" {
                        continue;
                    }
                    if let Some((pname, ptype)) = arg.split_once(':') {
                        let datara_ty = map_rust_type_to_datara(ptype.trim());
                        params.push((pname.trim().to_string(), datara_ty));
                    } else {
                        params.push((format!("a{}", idx), "Int".to_string()));
                    }
                }

                let ret_type = if let Some((_, ret_part)) = after_args.split_once("->") {
                    let ret_clean = ret_part.split('{').next().unwrap_or("").trim();
                    map_rust_type_to_datara(ret_clean)
                } else {
                    "Unit".to_string()
                };

                let arg_list = params
                    .iter()
                    .map(|(p, _)| p.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                let rust_call = format!("{}::{}({})", crate_name, name, arg_list);

                functions.push(ResolvedRustFunction {
                    name,
                    params,
                    ret_type,
                    rust_call,
                });
            }
        }
    }

    functions
}

pub fn map_rust_type_to_datara(rty: &str) -> String {
    match rty.trim() {
        "i64" | "u64" | "isize" | "usize" | "i128" | "u128" => "Int".to_string(),
        "i32" | "u32" => "Int".to_string(),
        "i16" | "u16" | "i8" | "u8" => "Int".to_string(),
        "f64" => "Float".to_string(),
        "f32" => "Float".to_string(),
        "bool" => "Bool".to_string(),
        "&str" | "String" => "Str".to_string(),
        "()" => "Unit".to_string(),
        _ => "Int".to_string(),
    }
}

pub fn map_datara_type_to_rust_c(dty: &str) -> &'static str {
    match dty {
        "Int" | "i64" => "i64",
        "Float" | "f64" => "f64",
        "Bool" | "bool" => "bool",
        "Str" | "String" => "*const std::os::raw::c_char",
        "Unit" | "void" => "()",
        _ => "i64",
    }
}
