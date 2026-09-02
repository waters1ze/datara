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
use std::sync::{Mutex, OnceLock};

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

/// Locate `clang` / `clang.exe` on PATH or standard system toolchain locations.
pub fn find_clang() -> Option<PathBuf> {
    which(if cfg!(windows) { "clang.exe" } else { "clang" })
        .or_else(|| {
            if cfg!(windows) {
                let p1 = PathBuf::from(r"C:\Program Files\LLVM\bin\clang.exe");
                if p1.exists() {
                    return Some(p1);
                }
                let p2 = PathBuf::from(r"C:\Program Files (x86)\LLVM\bin\clang.exe");
                if p2.exists() {
                    return Some(p2);
                }
                if let Some(root) = vs_install_root() {
                    let p3 = root.join("VC/Tools/Llvm/x64/bin/clang.exe");
                    if p3.exists() {
                        return Some(p3);
                    }
                    let p4 = root.join("VC/Tools/Llvm/bin/clang.exe");
                    if p4.exists() {
                        return Some(p4);
                    }
                }
            }
            None
        })
}

pub fn linker_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Locate `llc` / `llc.exe` on PATH or in Rust's toolchain directory.
pub fn find_llc() -> Option<PathBuf> {
    which(if cfg!(windows) { "llc.exe" } else { "llc" })
        .or_else(|| {
            let home = env::var("USERPROFILE").or_else(|_| env::var("HOME")).ok()?;
            let toolchains = PathBuf::from(home).join(".rustup").join("toolchains");
            if let Ok(entries) = std::fs::read_dir(toolchains) {
                let bin_name = if cfg!(windows) { "llc.exe" } else { "llc" };
                for e in entries.flatten() {
                    let rustlib_dir = e.path().join("lib").join("rustlib");
                    if let Ok(targets) = std::fs::read_dir(&rustlib_dir) {
                        for t in targets.flatten() {
                            let llc = t.path().join("bin").join(bin_name);
                            if llc.exists() {
                                return Some(llc);
                            }
                        }
                    }
                }
            }
            None
        })
}

/// Compile an LLVM IR file (.ll) using LLC and link into a native executable.
pub fn compile_with_llc(
    llc: &Path,
    ll_path: &Path,
    output_exe: &Path,
    opt_level: &str,
) -> Result<(), String> {
    let opt_flag = match opt_level {
        "0" | "debug" => "-O0",
        "1" => "-O1",
        "2" => "-O2",
        _ => "-O3",
    };
    let opt_bin = llc.with_file_name(if cfg!(windows) { "opt.exe" } else { "opt" });
    let bc_path = output_exe.with_extension("bc");
    let input_for_llc = if opt_bin.exists() {
        let mut opt_cmd = Command::new(&opt_bin);
        opt_cmd.arg(opt_flag);
        opt_cmd.arg(ll_path);
        opt_cmd.arg("-o").arg(&bc_path);
        let opt_res = opt_cmd.output();
        if opt_res.as_ref().map(|r| r.status.success()).unwrap_or(false) {
            bc_path.clone()
        } else {
            ll_path.to_path_buf()
        }
    } else {
        ll_path.to_path_buf()
    };

    let obj_path = output_exe.with_extension("obj");
    let mut cmd = Command::new(llc);
    cmd.arg(opt_flag);
    cmd.arg("-filetype=obj");
    cmd.arg("-relocation-model=pic");
    cmd.arg(&input_for_llc);
    cmd.arg("-o").arg(&obj_path);
    let res = cmd.output().map_err(|e| format!("Failed to run llc: {}", e))?;
    if !res.status.success() {
        return Err(format!(
            "LLC compilation failed:\n{}",
            String::from_utf8_lossy(&res.stderr)
        ));
    }
    let _ = std::fs::remove_file(&bc_path);

    let spec = discover();
    let runtime_lib = crate::runtime::runtime_lib_path();
    if !runtime_lib.exists() {
        return Err(format!(
            "Datara runtime library is missing at '{}'. Rebuild the compiler (`cargo build`).",
            runtime_lib.display()
        ));
    }

    let abs_out = if output_exe.is_absolute() {
        output_exe.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(output_exe)
    };
    let args = link_args(&spec, &obj_path, &runtime_lib, &abs_out, &[]);
    let _guard = linker_lock().lock().unwrap_or_else(|e| e.into_inner());
    let mut link_cmd = Command::new(&spec.program);
    link_cmd.args(&args);
    let link_res = link_cmd.output().map_err(|e| format!("Linker execution failed: {}", e))?;
    if !link_res.status.success() {
        return Err(format!(
            "LLVM Link step failed:\n{}",
            String::from_utf8_lossy(&link_res.stderr)
        ));
    }
    let _ = std::fs::remove_file(&obj_path);
    Ok(())
}

/// Compile an LLVM IR file (.ll) directly into an executable with Clang LTO or LLC.
pub fn compile_with_clang(
    ll_path: &Path,
    runtime_c_path: Option<&Path>,
    output_exe: &Path,
    opt_level: &str,
) -> Result<(), String> {
    if let Some(clang) = find_clang() {
        let opt_flag = match opt_level {
            "0" | "debug" => "-O0",
            "1" => "-O1",
            "2" => "-O2",
            _ => "-O3",
        };
        let mut cmd = Command::new(&clang);
        cmd.arg(opt_flag);
        cmd.arg("-flto");
        cmd.arg("-march=native");
        cmd.arg(ll_path);
        if let Some(rt) = runtime_c_path
            && rt.exists() {
                cmd.arg(rt);
            }
        cmd.arg("-o").arg(output_exe);
        if cfg!(windows) {
            cmd.arg("-lws2_32");
            cmd.arg("-luser32");
            cmd.arg("-lkernel32");
        } else {
            cmd.arg("-lm");
            cmd.arg("-lpthread");
        }
        let res = cmd.output().map_err(|e| format!("Failed to run clang: {}", e))?;
        if !res.status.success() {
            return Err(format!(
                "Clang compilation failed:\n{}",
                String::from_utf8_lossy(&res.stderr)
            ));
        }
        return Ok(());
    }

    if let Some(llc) = find_llc() {
        return compile_with_llc(&llc, ll_path, output_exe, opt_level);
    }

    Err("Neither Clang nor LLC found on system toolchain".to_string())
}

