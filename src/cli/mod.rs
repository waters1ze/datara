//! CLI entry point: shared argument-parsing helpers plus the thin `run_cli`
//! dispatch. Command implementations live in the group modules under `cli/`:
//!
//! - [`build`]   — check, run, test, bench, build, domain, profile
//! - [`inspect`] — sae, why, context, inspect (semantic graph / DMIR / codegen)
//! - [`pkg`]     — dpm, add, remove, install, publish, search, info, list,
//!   package, update, vendor
//! - [`project`] — init/new, clean, tree
//! - [`tools`]   — lsp, ui, fmt, lint/audit, doc, export
//! - [`misc`]    — explain, watch, completions, setup-tools, help, write_zip

mod build;
mod inspect;
mod misc;
mod pkg;
mod project;
mod tools;

pub use misc::write_zip;

use std::env;
use std::path::Path;

/// Serializes a value to pretty JSON for CLI output. Serialization of the
/// compiler's own types cannot fail in practice, but a panic here would abort
/// the CLI *after* a successful compilation, so fall back to a stub instead.
pub(crate) fn to_pretty_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string())
}

/// Same as [`to_pretty_json`] but produces a `serde_json::Value` for embedding
/// into a larger JSON document. Round-tripping through `serde_json::Value`
/// (a `BTreeMap`-backed object representation) also yields deterministic,
/// key-sorted output for values that contain `HashMap`s.
pub(crate) fn to_json_value<T: serde::Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or(serde_json::Value::Null)
}

/// Python keywords that must not be used as generated function names.
pub(crate) fn keyword_set() -> std::collections::HashSet<&'static str> {
    [
        "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class",
        "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global",
        "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return",
        "try", "while", "with", "yield",
    ]
    .into_iter()
    .collect()
}

pub(crate) fn extract_target_arg(args: &[String], skip: usize) -> Option<&Path> {
    let mut i = skip;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--" {
            break;
        }
        if arg == "-o" || arg == "--out" || arg == "--pgo" || arg == "--target" {
            i += 2;
            continue;
        }
        if arg.starts_with("-") {
            i += 1;
            continue;
        }
        return Some(Path::new(arg));
    }
    None
}

pub fn run_cli() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        // Just like Python, running datara or forgen without arguments launches the interactive REPL
        crate::repl::ReplSession::run_interactive();
        return;
    }
    if args[1] == "--help"
        || args[1] == "-h"
        || args[1] == "help"
        || (args.iter().skip(1).any(|a| a == "--help" || a == "-h") && args[1] != "run")
    {
        misc::print_help();
        return;
    }

    if args.len() >= 2 && (args[1] == "--version" || args[1] == "-v" || args[1] == "version") {
        let triple = crate::codegen::target::TargetInfo::host().triple_string();
        println!(
            "Datara Toolchain & Forgen AOT Native Compiler v{}",
            env!("CARGO_PKG_VERSION")
        );
        println!("Target Architecture: {} (Cranelift Backend)", triple);
        println!("Datara Language Specification 2026 Edition");
        crate::update::notify_if_update_available();
        return;
    }

    let command = &args[1];

    if args.iter().any(|a| a == "--auto-install" || a == "-y") {
        unsafe {
            std::env::set_var("FORGEN_AUTO_INSTALL", "1");
        }
    }

    // Each handler returns `true` when the CLI should run the post-command
    // update notification; handlers that short-circuit (as the original arms
    // did with an early `return`) report `false`.
    let notify = match command.as_str() {
        "check-update" | "self-update" => {
            crate::update::run_check_update_command();
            false
        }

        "setup-tools" | "install-tools" => {
            misc::run_setup_tools();
            true
        }

        "init" | "new" => project::cmd_init(&args),

        "check" => build::cmd_check(&args),

        "lsp" | "language-server" => tools::cmd_lsp(),

        "ui" => tools::cmd_ui(&args),

        "quick" | "run" | "start" => build::cmd_run(command, &args),

        "test" => build::cmd_test(&args),

        "bench" => build::cmd_bench(&args),

        "build" | "release" | "debug" | "verify" => build::cmd_build(command, &args),

        "sae" => inspect::cmd_sae(&args),

        "domain" => build::cmd_domain(&args),

        "why" => inspect::cmd_why(&args),

        "context" => inspect::cmd_context(&args),

        "profile" => build::cmd_profile(&args),

        "inspect" => inspect::cmd_inspect(&args),

        "pkg" | "pm" | "dpm" => pkg::cmd_dpm(&args),

        "add" => pkg::cmd_add(&args),

        "remove" | "rm" => pkg::cmd_remove(&args),

        "install" | "restore" => pkg::cmd_install(&args),

        "publish" => pkg::cmd_publish(&args),

        "search" => pkg::cmd_search(&args),

        "info" => pkg::cmd_info(&args),

        "list" | "ls" | "verify-pkg" => pkg::cmd_list(&args),

        "package" => pkg::cmd_package(&args),

        "fmt" | "format" => tools::cmd_fmt(&args),

        "clean" => project::cmd_clean(&args),

        "lint" | "audit" => tools::cmd_lint(command, &args),

        "explain" => misc::cmd_explain(&args),

        "watch" => misc::cmd_watch(&args),

        "tree" => project::cmd_tree(&args),

        "repl" => {
            crate::repl::ReplSession::run_interactive();
            true
        }

        "doc" => tools::cmd_doc(&args),

        "export" => tools::cmd_export(&args),

        "update" | "upgrade" => pkg::cmd_update(&args),

        "vendor" => pkg::cmd_vendor(&args),

        "completions" => misc::cmd_completions(&args),

        _ => {
            eprintln!("Unknown command: {}", command);
            misc::print_help();
            std::process::exit(1);
        }
    };

    if notify {
        crate::update::notify_if_update_available();
    }
}
