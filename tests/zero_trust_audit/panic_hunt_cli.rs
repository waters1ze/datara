use std::fs;
use std::process::Command;

fn datara_bin() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("CARGO_BIN_EXE_datara") {
        let pb = std::path::PathBuf::from(p);
        if pb.exists() {
            return pb;
        }
    }
    let ext = if cfg!(windows) { ".exe" } else { "" };
    let candidates = [
        format!("target/release/datara{}", ext),
        format!("target/debug/datara{}", ext),
        format!("target/x86_64-unknown-linux-gnu/release/datara{}", ext),
        format!("target/x86_64-unknown-linux-gnu/debug/datara{}", ext),
    ];
    for c in candidates {
        let p = std::path::PathBuf::from(c);
        if p.exists() {
            return p;
        }
    }
    std::path::PathBuf::from(format!("target/release/datara{}", ext))
}

fn assert_no_panic(out: &std::process::Output, context: &str) {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let code = out.status.code().unwrap_or(-1);

    assert!(
        !stderr.contains("panicked at") && !stdout.contains("panicked at"),
        "PANIC DETECTED on {}:\nSTDOUT:\n{}\nSTDERR:\n{}",
        context,
        stdout,
        stderr
    );

    assert_ne!(
        code, -1073741571,
        "STACK OVERFLOW (0xC00000FD) detected on {}!",
        context
    );
    assert_ne!(
        code, -1073741819,
        "ACCESS VIOLATION (0xC0000005) detected on {}!",
        context
    );
}

#[test]
fn audit_cli_panic_hunt_edge_cases() {
    let bin = datara_bin();
    assert!(
        bin.exists(),
        "datara binary must exist at {}",
        bin.display()
    );

    let temp_dir = std::env::temp_dir().join(format!("zta_panic_hunt_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);

    // 1. Empty file
    {
        let file = temp_dir.join("empty.dtr");
        fs::write(&file, "").unwrap();
        let out = Command::new(&bin).arg("run").arg(&file).output().unwrap();
        assert_no_panic(&out, "empty file");
    }

    // 2. Whitespace and comments only
    {
        let file = temp_dir.join("comments_only.dtr");
        fs::write(
            &file,
            "   \n\t  // line comment\n  /* block \n comment */  \n\n",
        )
        .unwrap();
        let out = Command::new(&bin).arg("run").arg(&file).output().unwrap();
        assert_no_panic(&out, "comments only");
    }

    // 3. UTF-8 BOM
    {
        let file = temp_dir.join("bom.dtr");
        let mut bom_bytes = vec![0xEF, 0xBB, 0xBF];
        bom_bytes.extend_from_slice(b"fn main() { out 42 }\n");
        fs::write(&file, &bom_bytes).unwrap();
        let out = Command::new(&bin).arg("run").arg(&file).output().unwrap();
        assert_no_panic(&out, "BOM file");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            out.status.code().unwrap_or(-1),
            0,
            "BOM file should run: {}",
            stdout
        );
        assert!(stdout.contains("42"), "BOM file should execute cleanly");
    }

    // 4. CRLF line endings
    {
        let file = temp_dir.join("crlf.dtr");
        fs::write(&file, "fn main() {\r\n    out 100\r\n}\r\n").unwrap();
        let out = Command::new(&bin).arg("run").arg(&file).output().unwrap();
        assert_no_panic(&out, "CRLF file");
        assert_eq!(out.status.code().unwrap_or(-1), 0);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("100"));
    }

    // 5. File of 10,000 lines
    {
        let file = temp_dir.join("ten_thousand_lines.dtr");
        let mut big = String::from("fn main() {\n    mut total = 0\n");
        for i in 0..10_000 {
            big.push_str(&format!(
                "    let x{} = {}\n    total = total + {}\n",
                i, 1, 1
            ));
        }
        big.push_str("    out total\n}\n");
        fs::write(&file, &big).unwrap();
        let out = Command::new(&bin).arg("run").arg(&file).output().unwrap();
        assert_no_panic(&out, "10,000 lines");
    }

    // 6. Expression depth 69
    {
        let file = temp_dir.join("depth69.dtr");
        let mut code = String::from("fn main() { out ");
        for _ in 0..69 {
            code.push('(');
        }
        code.push_str("42");
        for _ in 0..69 {
            code.push(')');
        }
        code.push_str(" }\n");
        fs::write(&file, &code).unwrap();
        let out = Command::new(&bin).arg("run").arg(&file).output().unwrap();
        assert_no_panic(&out, "depth 69");
        assert_ne!(
            out.status.code().unwrap_or(-1),
            0,
            "Depth 69 should report error"
        );
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(
            err.contains("nesting")
                || err.contains("E-SYNTAX-001")
                || err.contains("depth")
                || String::from_utf8_lossy(&out.stdout).contains("nesting"),
            "Should cleanly report depth exceeded diagnostic"
        );
    }

    // 7. Binary garbage (1024 random non-UTF-8 bytes)
    {
        let file = temp_dir.join("binary_garbage.dtr");
        let mut garbage = vec![0xFF, 0xFE, 0x00, 0xAA, 0xBB, 0xCC];
        for i in 0..1024 {
            garbage.push(((i * 73 + 17) % 256) as u8);
        }
        fs::write(&file, &garbage).unwrap();
        let out = Command::new(&bin).arg("run").arg(&file).output().unwrap();
        assert_no_panic(&out, "binary garbage");
        assert_ne!(out.status.code().unwrap_or(-1), 0);
    }

    // 8. Cyclic use (a.dtr imports b.dtr, b.dtr imports a.dtr)
    {
        let file_a = temp_dir.join("mod_a.dtr");
        let file_b = temp_dir.join("mod_b.dtr");
        fs::write(
            &file_a,
            "use mod_b\nfn func_a() -> Int { return 1 }\nfn main() {}\n",
        )
        .unwrap();
        fs::write(&file_b, "use mod_a\nfn func_b() -> Int { return 2 }\n").unwrap();
        let out = Command::new(&bin).arg("run").arg(&file_a).output().unwrap();
        assert_no_panic(&out, "cyclic use");
    }

    // 9. Non-existent file
    {
        let file = temp_dir.join("non_existent_file_xyz_987654.dtr");
        let out = Command::new(&bin).arg("run").arg(&file).output().unwrap();
        assert_no_panic(&out, "non-existent file");
        assert_ne!(out.status.code().unwrap_or(-1), 0);
    }

    // 10. Directory passed as file
    {
        let out = Command::new(&bin)
            .arg("run")
            .arg(&temp_dir)
            .output()
            .unwrap();
        assert_no_panic(&out, "directory passed as file");
        assert_ne!(out.status.code().unwrap_or(-1), 0);
    }

    // 11. Stdin argument "-"
    {
        let out = Command::new(&bin).arg("run").arg("-").output().unwrap();
        assert_no_panic(&out, "stdin dash argument");
    }

    // 12. Typo flags
    {
        let out1 = Command::new(&bin)
            .arg("build")
            .arg("--releas")
            .output()
            .unwrap();
        assert_no_panic(&out1, "--releas flag");

        let out2 = Command::new(&bin).arg("build").arg("-O9").output().unwrap();
        assert_no_panic(&out2, "-O9 flag");

        let out3 = Command::new(&bin)
            .arg("build")
            .arg("--target=unknown-triple")
            .output()
            .unwrap();
        assert_no_panic(&out3, "--target=unknown-triple flag");
    }

    let _ = fs::remove_dir_all(&temp_dir);
}
