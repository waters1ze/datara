//! Linker discovery for generated programs.
//!
//! The backend previously carried seven hardcoded absolute paths: three
//! `link.exe` candidates, two `/LIBPATH` entries and an MSVC version
//! (`14.50.35717`) and SDK version (`10.0.26100.0`) baked into the binary. The
//! compiler therefore worked on exactly one machine, and stopped working the
//! moment either toolchain was updated.
//!
//! This module finds the toolchain at runtime instead:
//!
//! * Windows/MSVC — `vswhere.exe` locates the Visual Studio install, then the
//!   newest `VC\Tools\MSVC\<version>` and `Windows Kits\10\Lib\<version>` are
//!   chosen by version. If discovery fails (for example inside a developer
//!   prompt where `LIB` is already configured) it falls back to `link.exe` on
//!   `PATH`.
//! * Unix — `cc` / `clang` / `gcc`, honouring the `CC` environment variable.

use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkerFlavor {
    Msvc,
    Unix,
}

#[derive(Debug, Clone)]
pub struct LinkerSpec {
    pub program: PathBuf,
    pub flavor: LinkerFlavor,
    /// Additional library search directories to pass explicitly.
    pub lib_paths: Vec<PathBuf>,
    /// Platform libraries every Datara program needs.
    pub system_libs: Vec<String>,
}

/// Compare directory names as dotted-numeric versions, newest first.
/// Falls back to lexicographic order for names that are not numeric.
fn version_key(name: &str) -> Vec<u32> {
    name.split('.')
        .map(|part| part.trim().parse::<u32>().unwrap_or(0))
        .collect()
}

/// Pick the subdirectory of `dir` with the highest dotted-numeric name.
fn newest_subdir(dir: &Path) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();

    candidates.sort_by(|a, b| {
        let ka = a.file_name().and_then(OsStr::to_str).map(version_key);
        let kb = b.file_name().and_then(OsStr::to_str).map(version_key);
        kb.cmp(&ka)
    });

    candidates.into_iter().next()
}

fn program_files_x86() -> Option<PathBuf> {
    env::var_os("ProgramFiles(x86)")
        .or_else(|| env::var_os("ProgramFiles"))
        .map(PathBuf::from)
        .filter(|p| p.exists())
}

/// Ask `vswhere.exe` for the newest Visual Studio installation root.
fn vs_install_root() -> Option<PathBuf> {
    let pf = program_files_x86()?;
    let vswhere = pf.join("Microsoft Visual Studio/Installer/vswhere.exe");
    if !vswhere.exists() {
        return None;
    }

    let out = Command::new(&vswhere)
        .args([
            "-latest",
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-property",
            "installationPath",
        ])
        .output()
        .ok()?;

    if !out.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.exists())
}

/// Newest `VC\Tools\MSVC\<version>` directory.
fn msvc_tools_dir() -> Option<PathBuf> {
    if let Ok(dir) = env::var("VCToolsInstallDir") {
        let p = PathBuf::from(dir.trim());
        if p.exists() {
            return Some(p);
        }
    }

    let root = vs_install_root()?;
    newest_subdir(&root.join("VC/Tools/MSVC"))
}

/// Newest Windows 10/11 SDK `Lib\<version>` directory.
fn windows_sdk_lib_dir() -> Option<PathBuf> {
    let base = program_files_x86()?.join("Windows Kits/10/Lib");
    newest_subdir(&base)
}

/// `link.exe` for the host architecture.
fn find_msvc_linker(msvc: &Path) -> Option<PathBuf> {
    let host = if cfg!(target_arch = "x86_64") {
        "Hostx64"
    } else {
        "Hostx86"
    };
    let target = if cfg!(target_arch = "x86_64") {
        "x64"
    } else {
        "x86"
    };

    let direct = msvc.join("bin").join(host).join(target).join("link.exe");
    if direct.exists() {
        return Some(direct);
    }

    // Older layouts put the binaries one level higher.
    let flat = msvc.join("bin").join("link.exe");
    if flat.exists() {
        return Some(flat);
    }

    None
}

fn discover_msvc() -> LinkerSpec {
    let mut lib_paths: Vec<PathBuf> = Vec::new();
    let mut program: Option<PathBuf> = None;

    if let Some(msvc) = msvc_tools_dir() {
        program = find_msvc_linker(&msvc);

        let arch = if cfg!(target_arch = "x86_64") {
            "x64"
        } else {
            "x86"
        };
        let msvc_lib = msvc.join("lib").join(arch);
        if msvc_lib.exists() {
            lib_paths.push(msvc_lib);
        }
    }

    if let Some(sdk) = windows_sdk_lib_dir() {
        let arch = if cfg!(target_arch = "x86_64") {
            "x64"
        } else {
            "x86"
        };
        for sub in ["ucrt", "um"] {
            let p = sdk.join(sub).join(arch);
            if p.exists() {
                lib_paths.push(p);
            }
        }
    }

    // Inside a developer prompt `link.exe` is on PATH and LIB is already set,
    // so an empty lib_paths list is fine there.
    let program = program
        .or_else(|| which("link.exe"))
        .unwrap_or_else(|| PathBuf::from(if cfg!(windows) { "link.exe" } else { "link" }));

    LinkerSpec {
        program,
        flavor: LinkerFlavor::Msvc,
        lib_paths,
        system_libs: vec![
            "legacy_stdio_definitions.lib".into(),
            "msvcrt.lib".into(),
            "ucrt.lib".into(),
            "vcruntime.lib".into(),
            "kernel32.lib".into(),
            "user32.lib".into(),
            "ws2_32.lib".into(),
        ],
    }
}

fn discover_unix() -> LinkerSpec {
    let program = env::var("CC")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .or_else(|| which("cc"))
        .or_else(|| which("clang"))
        .or_else(|| which("gcc"))
        .unwrap_or_else(|| PathBuf::from("cc"));

    let mut system_libs = vec!["m".to_string()];
    if cfg!(target_os = "linux") {
        system_libs.push("pthread".to_string());
        system_libs.push("dl".to_string());
    }

    LinkerSpec {
        program,
        flavor: LinkerFlavor::Unix,
        lib_paths: Vec::new(),
        system_libs,
    }
}

fn which(program: &str) -> Option<PathBuf> {
    let separator = if cfg!(windows) { ';' } else { ':' };
    env::var_os("PATH")
        .and_then(|paths| {
            env::split_paths(&paths)
                .map(|dir| dir.join(program))
                .find(|p| p.exists())
        })
        .or_else(|| {
            // Keep the separator referenced so the branch above reads clearly.
            let _ = separator;
            None
        })
}

/// Discover the linker for the current host.
pub fn discover() -> LinkerSpec {
    if cfg!(target_env = "msvc") {
        discover_msvc()
    } else {
        discover_unix()
    }
}

/// Build the argument list that links `object` and the runtime into `output`.
pub fn link_args(
    spec: &LinkerSpec,
    object: &Path,
    runtime_lib: &Path,
    output: &Path,
    exports: &[String],
) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();

    let is_shared = output
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            e.eq_ignore_ascii_case("dll")
                || e.eq_ignore_ascii_case("so")
                || e.eq_ignore_ascii_case("dylib")
        })
        .unwrap_or(false);

    match spec.flavor {
        LinkerFlavor::Msvc => {
            args.push("/NOLOGO".into());
            if is_shared {
                args.push("/DLL".into());
                args.push("/NOENTRY".into());
                for exp in exports {
                    args.push(format!("/EXPORT:{}", exp));
                }
            } else {
                args.push("/SUBSYSTEM:CONSOLE".into());
            }
            args.push(format!("/OUT:{}", output.display()));
            args.push(object.display().to_string());
            args.push(runtime_lib.display().to_string());
            args.push("/LIBPATH:.".into());
            if let Some(parent) = output.parent() {
                args.push(format!("/LIBPATH:{}", parent.display()));
            }
            for p in &spec.lib_paths {
                args.push(format!("/LIBPATH:{}", p.display()));
            }
            for lib in &spec.system_libs {
                args.push(lib.clone());
            }
            if let Ok(extra) = std::env::var("DATARA_LINK_LIBS") {
                for lib in extra.split(';') {
                    let trimmed = lib.trim();
                    if !trimmed.is_empty() {
                        args.push(trimmed.to_string());
                    }
                }
            }
        }
        LinkerFlavor::Unix => {
            if is_shared {
                args.push("-shared".into());
            } else if cfg!(target_os = "linux") {
                args.push("-no-pie".into());
            }
            args.push("-o".into());
            args.push(output.display().to_string());
            args.push(object.display().to_string());
            args.push(runtime_lib.display().to_string());
            for p in &spec.lib_paths {
                args.push(format!("-L{}", p.display()));
            }
            for lib in &spec.system_libs {
                args.push(format!("-l{}", lib));
            }
        }
    }

    args
}

/// Human-readable description of the toolchain in use, for diagnostics.
pub fn describe(spec: &LinkerSpec) -> String {
    let mut out = format!("linker: {}", spec.program.display());
    if !spec.lib_paths.is_empty() {
        let paths: Vec<String> = spec
            .lib_paths
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        out.push_str(&format!("\nlibrary paths: {}", paths.join(", ")));
    }
    out
}
