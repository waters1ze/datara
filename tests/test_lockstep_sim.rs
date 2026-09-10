//! Integration test for Wave 5.5: Gamedev Showcase (Lockstep Simulation).
//!
//! Compiles `examples/showcase/lockstep_sim/lockstep.dtr` into a native binary,
//! executes it 4 separate times, and verifies that all 4 runs produce bit-for-bit
//! identical 64-bit checksum outputs across multi-threaded SIMD physics execution.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use forgen::driver::ForgenCompiler;

#[test]
fn test_lockstep_sim_deterministic_replay_4_runs() {
    let project_dir = PathBuf::from("examples/showcase/lockstep_sim");
    let dtr_file = project_dir.join("lockstep.dtr");
    assert!(
        dtr_file.exists(),
        "Simulation source file must exist at {}",
        dtr_file.display()
    );

    let temp_dir = std::env::temp_dir().join(format!("forgen_lockstep_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);
    let exe_file = temp_dir.join(if cfg!(windows) {
        "lockstep_sim.exe"
    } else {
        "lockstep_sim"
    });

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_file(&dtr_file, Some(&exe_file));
    assert!(
        res.success,
        "Compilation of lockstep.dtr failed: {:?}",
        res.error
    );
    assert!(
        exe_file.exists(),
        "Compiled binary must exist at {}",
        exe_file.display()
    );

    let mut checksums = Vec::new();

    for run_idx in 1..=4 {
        let output = Command::new(&exe_file)
            .output()
            .unwrap_or_else(|e| panic!("Failed to execute run {}: {}", run_idx, e));

        assert!(
            output.status.success(),
            "Run {} failed with status {:?}",
            run_idx,
            output.status
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("LOCKSTEP_SIM_OK: frames=50 entities=64"),
            "Run {} did not report success in stdout: {}",
            run_idx,
            stdout
        );

        let checksum_line = stdout
            .lines()
            .find(|l| l.contains("CHECKSUM:"))
            .unwrap_or_else(|| panic!("Run {} missing CHECKSUM line: {}", run_idx, stdout));

        let checksum_val: i64 = checksum_line
            .split("CHECKSUM:")
            .nth(1)
            .unwrap()
            .trim()
            .parse()
            .unwrap_or_else(|e| panic!("Failed to parse checksum on run {}: {}", run_idx, e));

        assert_ne!(
            checksum_val, 0,
            "Run {} generated a trivial zero checksum",
            run_idx
        );

        checksums.push(checksum_val);
    }

    // Verify all 4 runs yielded bit-for-bit identical checksum
    for i in 1..checksums.len() {
        assert_eq!(
            checksums[0],
            checksums[i],
            "Bit-for-bit mismatch between run 1 ({}) and run {} ({})",
            checksums[0],
            i + 1,
            checksums[i]
        );
    }

    println!(
        "[Lockstep Simulation Verified] All 4 runs yielded identical checksum: {}",
        checksums[0]
    );

    let _ = fs::remove_dir_all(&temp_dir);
}
