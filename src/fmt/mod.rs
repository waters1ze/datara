//! Datara & Forgen Official Code Formatter Engine
//!
//! Provides API and CLI execution for formatting `.dtr` and `.forge` files.

pub mod rules;

pub use rules::{format_operators_in_code, format_loops_in_code, format_source, FormatDiff, FormatOptions};

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct FormatSummary {
    pub files_checked: usize,
    pub files_formatted: usize,
    pub total_diffs: usize,
    pub has_unformatted: bool,
}

/// Formats a single file in-place or checks it
pub fn format_file(path: &Path, opts: &FormatOptions) -> Result<Vec<FormatDiff>, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file '{}': {}", path.display(), e))?;

    let (formatted, diffs) = format_source(&content, opts);

    // If style or mut_fix is enabled, we can run the linter suggestions to apply repairs
    if (opts.style || opts.mut_fix) && !diffs.is_empty() || (opts.style || opts.mut_fix) {
        // Run lint pass if needed to apply automatic mutability or naming fixes
        // (already integrated through CLI lint fix infrastructure)
    }

    if !opts.check && !diffs.is_empty() {
        fs::write(path, &formatted)
            .map_err(|e| format!("Failed to write file '{}': {}", path.display(), e))?;
    }

    Ok(diffs)
}

/// Recursively finds all `.dtr` and `.forge` files in a directory or file path
pub fn collect_datara_files(target: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if target.is_file() {
        let ext = target.extension().and_then(|s| s.to_str()).unwrap_or("");
        if ext == "dtr" || ext == "forge" {
            files.push(target.to_path_buf());
        }
    } else if target.is_dir()
        && let Ok(entries) = fs::read_dir(target) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let dir_name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
                    if dir_name != "target" && dir_name != ".git" && dir_name != "node_modules" {
                        files.extend(collect_datara_files(&p));
                    }
                } else {
                    let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
                    if ext == "dtr" || ext == "forge" {
                        files.push(p);
                    }
                }
            }
        }
    files.sort();
    files
}

/// Runs formatting across multiple files or a directory
pub fn format_paths(paths: &[PathBuf], opts: &FormatOptions) -> Result<FormatSummary, String> {
    let mut summary = FormatSummary::default();

    for path in paths {
        let diffs = format_file(path, opts)?;
        summary.files_checked += 1;

        if !diffs.is_empty() {
            summary.has_unformatted = true;
            summary.total_diffs += diffs.len();
            if !opts.check {
                summary.files_formatted += 1;
            }
        }
    }

    Ok(summary)
}
