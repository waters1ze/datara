//! Integration and verification test suite for Phase 3: Gamedev Layer (Honest Minimum).
//!
//! Verifies:
//! 1. High-resolution time APIs: monotonicity and non-negative delta-time.
//! 2. Documentation compilation: every single Datara code block in `docs/gamedev.md` compiles cleanly.
//! 3. Deterministic game loop showcase: 20 consecutive runs of `examples/showcase/game_loop/main.dtr`
//!    produce 100% byte-for-byte identical outputs and physics checksums.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use forgen::driver::ForgenCompiler;
use forgen::runtime::{
    datara_rt_time_delta_ms, datara_rt_time_precise_ms, datara_rt_time_reset_delta,
};

#[test]
fn test_time_api_monotonicity_and_delta() {
    unsafe {
        datara_rt_time_reset_delta();
    }

    let t0 = unsafe { datara_rt_time_precise_ms() };
    assert!(t0 > 0.0, "Initial precise timestamp must be positive");

    let mut prev_t = t0;
    for i in 0..100 {
        // Perform small work
        let mut _sink = 0i64;
        for j in 0..1000 {
            _sink = _sink.wrapping_add(j ^ (i + 1));
        }

        let curr_t = unsafe { datara_rt_time_precise_ms() };
        let dt = unsafe { datara_rt_time_delta_ms() };

        assert!(
            curr_t >= prev_t,
            "datara_rt_time_precise_ms must be monotonic non-decreasing: prev={}, curr={}",
            prev_t,
            curr_t
        );
        assert!(
            dt >= 0.0,
            "datara_rt_time_delta_ms must be non-negative: dt={}",
            dt
        );

        prev_t = curr_t;
    }
}

#[test]
fn test_docs_gamedev_all_examples_compile() {
    let doc_path = Path::new("docs/gamedev.md");
    assert!(
        doc_path.exists(),
        "docs/gamedev.md must exist at {}",
        doc_path.display()
    );

    let content = fs::read_to_string(doc_path).expect("must read docs/gamedev.md");
    let mut code_blocks: Vec<String> = Vec::new();

    let mut in_datara = false;
    let mut current_block = String::new();

    for line in content.lines() {
        if line.trim_start().starts_with("```datara") {
            in_datara = true;
            current_block.clear();
        } else if in_datara && line.trim_start().starts_with("```") {
            in_datara = false;
            if !current_block.trim().is_empty() {
                code_blocks.push(current_block.clone());
            }
            current_block.clear();
        } else if in_datara {
            current_block.push_str(line);
            current_block.push('\n');
        }
    }

    assert!(
        code_blocks.len() >= 5,
        "docs/gamedev.md must contain at least 5 datara examples, found {}",
        code_blocks.len()
    );

    let compiler = ForgenCompiler::new("debug");
    for (idx, code) in code_blocks.iter().enumerate() {
        let snippet_name = format!("gamedev_doc_snippet_{}.dtr", idx + 1);
        let res = compiler.compile_source(code, &snippet_name, None);
        assert!(
            res.success,
            "Snippet #{} in docs/gamedev.md failed to compile:\nCode:\n{}\nDiagnostics:\n{:?}\nError:\n{:?}",
            idx + 1,
            code,
            res.diagnostics,
            res.error
        );
    }
}

#[test]
fn test_game_loop_showcase_20_runs_byte_for_byte_determinism() {
    let project_dir = PathBuf::from("examples/showcase/game_loop");
    let dtr_file = project_dir.join("main.dtr");
    assert!(
        dtr_file.exists(),
        "Game loop source file must exist at {}",
        dtr_file.display()
    );

    let temp_dir = std::env::temp_dir().join(format!("forgen_game_loop_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);
    let exe_file = temp_dir.join(if cfg!(windows) {
        "game_loop_test.exe"
    } else {
        "game_loop_test"
    });

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_file(&dtr_file, Some(&exe_file));
    assert!(
        res.success,
        "Compilation of game_loop/main.dtr failed: {:?}",
        res.error
    );
    assert!(
        exe_file.exists(),
        "Compiled binary must exist at {}",
        exe_file.display()
    );

    let mut first_output: Option<String> = None;
    let total_runs = 20;

    for run_idx in 1..=total_runs {
        let output = Command::new(&exe_file)
            .output()
            .unwrap_or_else(|e| panic!("Failed to execute run {}: {}", run_idx, e));

        assert!(
            output.status.success(),
            "Run {} failed with exit status {:?}",
            run_idx,
            output.status
        );

        let stdout = String::from_utf8_lossy(&output.stdout)
            .trim()
            .replace("\r\n", "\n");
        assert!(
            stdout.contains("GAME_LOOP_OK: frames=60 ticks=82 entities=16"),
            "Run {} did not report GAME_LOOP_OK format: {}",
            run_idx,
            stdout
        );
        assert!(
            stdout.contains("CHECKSUM:-273743660") || stdout.contains("CHECKSUM:-582514539"),
            "Run {} did not report expected checksum: {}",
            run_idx,
            stdout
        );

        if let Some(ref expected) = first_output {
            assert_eq!(
                &stdout, expected,
                "Run {} output differed from run 1 (determinism violation)!\nRun 1:\n{}\nRun {}:\n{}",
                run_idx, expected, run_idx, stdout
            );
        } else {
            first_output = Some(stdout);
        }
    }

    let _ = fs::remove_file(&exe_file);
    let _ = fs::remove_file(exe_file.with_extension("obj"));
    let _ = fs::remove_dir_all(&temp_dir);
}
