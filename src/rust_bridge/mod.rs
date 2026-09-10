pub mod builder;
pub mod cli;
pub mod codegen;
pub mod config;

pub use builder::{build_rust_crate_wrapper, find_cargo};
pub use cli::run_rust_bridge_cli;
pub use codegen::{
    generate_datara_extern_decls, generate_wrapper_cargo_toml, generate_wrapper_lib_rs,
};
pub use config::{
    ResolvedRustFunction, RustCrateConfig, RustFunctionDecl, parse_function_signature,
    scan_rust_source_for_functions,
};

use crate::ast::Decl;
use crate::diagnostics::{DiagnosticEngine, ErrorCode};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Result of processing Rust dependencies: injected AST declarations and libraries to link.
#[derive(Debug, Default)]
pub struct RustBridgeArtifacts {
    pub extern_decls: Vec<Decl>,
    pub link_libraries: Vec<String>,
}

/// Locate datara.toml in `start_dir` or its parent directories.
pub fn find_manifest(start_dir: &Path) -> Option<(PathBuf, crate::project::DataraManifest)> {
    let mut current = if start_dir.is_file() {
        start_dir.parent().unwrap_or(start_dir).to_path_buf()
    } else {
        start_dir.to_path_buf()
    };

    loop {
        let manifest_path = current.join("datara.toml");
        if manifest_path.exists() {
            if let Ok(manifest) = crate::project::DataraManifest::from_file(&manifest_path) {
                return Some((current, manifest));
            }
        }
        if !current.pop() {
            break;
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        let manifest_path = cwd.join("datara.toml");
        if manifest_path.exists() {
            if let Ok(manifest) = crate::project::DataraManifest::from_file(&manifest_path) {
                return Some((cwd, manifest));
            }
        }
    }

    None
}

/// Process all Rust dependencies from manifest or imports, building their staticlibs
/// and generating corresponding Datara `Decl::ExternFn` declarations.
pub fn process_rust_dependencies(
    rust_deps: &HashMap<String, RustCrateConfig>,
    base_dir: &Path,
    lto: bool,
    diag: &mut DiagnosticEngine,
) -> RustBridgeArtifacts {
    let mut artifacts = RustBridgeArtifacts::default();

    for (crate_name, config) in rust_deps {
        let functions = match config.resolve_functions(crate_name, base_dir) {
            Ok(fns) => fns,
            Err(e) => {
                diag.error(
                    ErrorCode::ResolveUnreachableModule,
                    format!("Rust bridge error for crate '{}': {}", crate_name, e),
                    None,
                );
                continue;
            }
        };

        let staticlib_path =
            match build_rust_crate_wrapper(crate_name, config, &functions, base_dir, lto) {
                Ok(p) => p,
                Err(e) => {
                    diag.error(ErrorCode::CodegenBackendFailed, e, None);
                    continue;
                }
            };

        // Generate Datara AST declarations
        let decls = generate_datara_extern_decls(&functions);
        artifacts.extern_decls.extend(decls);

        // Add staticlib to link libraries
        artifacts
            .link_libraries
            .push(staticlib_path.to_string_lossy().to_string());
    }

    artifacts
}

/// Expand Rust dependencies for a program: scans manifest or `use rust.<crate>` imports,
/// compiles the staticlib wrapper, injects extern declarations, and links the static library.
pub fn expand_rust_dependencies(
    program: &mut crate::ast::Program,
    base_dir: Option<&Path>,
    diag: &mut DiagnosticEngine,
) {
    let start_dir = base_dir.unwrap_or_else(|| Path::new(".")).to_path_buf();

    let mut rust_deps = HashMap::new();
    let mut project_root = start_dir.clone();

    // 1. Check datara.toml
    if let Some((root, manifest)) = find_manifest(&start_dir) {
        rust_deps.extend(manifest.rust_dependencies());
        project_root = root;
    }

    // 2. Check program `use rust.<crate>` declarations
    for decl in &program.declarations {
        if let Decl::Use(u) = decl {
            if u.path.first().map(|s| s.as_str()) == Some("rust") && u.path.len() > 1 {
                let crate_name = u.path[1].clone();
                if !rust_deps.contains_key(&crate_name) {
                    let candidate_paths = [
                        project_root
                            .join("tests")
                            .join("fixtures")
                            .join(&crate_name),
                        project_root.join("crates").join(&crate_name),
                        project_root.join(&crate_name),
                        start_dir.join(&crate_name),
                        start_dir.join("tests").join("fixtures").join(&crate_name),
                    ];
                    let mut found_path = None;
                    for p in &candidate_paths {
                        if p.join("Cargo.toml").exists() {
                            found_path = Some(p.clone());
                            break;
                        }
                    }

                    if let Some(path) = found_path {
                        rust_deps.insert(
                            crate_name,
                            RustCrateConfig {
                                path: Some(path.to_string_lossy().to_string()),
                                ..Default::default()
                            },
                        );
                    }
                }
            }
        }
    }

    if rust_deps.is_empty() {
        return;
    }

    let lto = false;
    let artifacts = process_rust_dependencies(&rust_deps, &project_root, lto, diag);

    // Inject extern declarations into program AST
    for decl in artifacts.extern_decls {
        program.declarations.insert(0, decl);
    }

    // Add staticlibs to link libraries
    for lib in artifacts.link_libraries {
        if !program.link_libraries.contains(&lib) {
            program.link_libraries.push(lib);
        }
    }
}
