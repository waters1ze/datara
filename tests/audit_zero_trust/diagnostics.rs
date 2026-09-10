use forgen::driver::ForgenCompiler;
use std::fs;

#[test]
fn audit_diagnostics_ten_broken_programs() {
    let compiler = ForgenCompiler::new("debug");
    let temp_dir = std::env::temp_dir().join("audit_diagnostics_10_programs");
    let _ = fs::create_dir_all(&temp_dir);

    // 1. Typo in variable name
    {
        let src = "fn main() {\n    let my_special_counter = 42\n    out my_special_countr\n}\n";
        let p = temp_dir.join("prog1_typo.dtr");
        fs::write(&p, src).unwrap();
        let res = compiler.compile_file(&p, None);
        assert!(!res.success, "[Prog 1] Typo must fail compilation");
        let err = res.error.expect("Error message present");
        assert!(!err.contains("panicked"), "[Prog 1] Compiler panicked!");
        assert!(
            err.contains("E-RESOLVE-001") || err.contains("my_special_counter"),
            "[Prog 1] Expected E-RESOLVE-001 or typo suggestion 'my_special_counter', got: {}",
            err
        );
        println!(
            ">>> [DIAG 1: Typo] PASS: {}",
            err.lines().next().unwrap_or("")
        );
    }

    // 2. Type mismatch
    {
        let src = "fn main() {\n    let x: Int = \"hello world\"\n    out x\n}\n";
        let p = temp_dir.join("prog2_type_mismatch.dtr");
        fs::write(&p, src).unwrap();
        let res = compiler.compile_file(&p, None);
        assert!(!res.success, "[Prog 2] Type mismatch must fail compilation");
        let err = res.error.expect("Error message present");
        assert!(!err.contains("panicked"), "[Prog 2] Compiler panicked!");
        assert!(
            err.contains("E-TYPE-001") || err.contains("Type mismatch") || err.contains("Int"),
            "[Prog 2] Expected type mismatch error code or explanation, got: {}",
            err
        );
        println!(
            ">>> [DIAG 2: Type Mismatch] PASS: {}",
            err.lines().next().unwrap_or("")
        );
    }

    // 3. Definite use-after-move
    {
        let src = "fn main() {\n    let val = 100\n    destroy(val)\n    out val\n}\n";
        let p = temp_dir.join("prog3_use_after_move.dtr");
        fs::write(&p, src).unwrap();
        let res = compiler.compile_file(&p, None);
        assert!(
            !res.success,
            "[Prog 3] Use-after-move must fail compilation"
        );
        let err = res.error.expect("Error message present");
        assert!(!err.contains("panicked"), "[Prog 3] Compiler panicked!");
        assert!(
            err.contains("E0301")
                || err.contains("E-BORROW-001")
                || err.contains("Use of moved value"),
            "[Prog 3] Expected E0301/E-BORROW-001, got: {}",
            err
        );
        println!(
            ">>> [DIAG 3: Use-After-Move] PASS: {}",
            err.lines().next().unwrap_or("")
        );
    }

    // 4. Non-exhaustive match
    {
        let src = "enum Status { Active, Pending, Closed }\nfn check(s: Status) -> Int {\n    match s {\n        Status.Active => 1,\n        Status.Pending => 2,\n    }\n}\nfn main() {\n    out check(Status.Active)\n}\n";
        let p = temp_dir.join("prog4_non_exhaustive_match.dtr");
        fs::write(&p, src).unwrap();
        let res = compiler.compile_file(&p, None);
        if !res.success {
            let err = res.error.as_deref().unwrap_or("");
            assert!(!err.contains("panicked"), "[Prog 4] Compiler panicked!");
            println!(
                ">>> [DIAG 4: Non-exhaustive match rejected] Code: {}",
                err.lines().next().unwrap_or("")
            );
        } else {
            println!(
                ">>> [DIAG 4: Non-exhaustive match WARNING/FINDING]: compiler permitted missing Closed variant!"
            );
        }
    }

    // 5. Deep nesting recursion limit (Depth 70)
    {
        let mut deep = String::from("1");
        for _ in 0..70 {
            deep = format!("({})", deep);
        }
        let src = format!("fn main() {{ let x = {}; out x; }}\n", deep);
        let p = temp_dir.join("prog5_deep_nesting.dtr");
        fs::write(&p, src).unwrap();
        let res = compiler.compile_file(&p, None);
        assert!(
            !res.success,
            "[Prog 5] Deep nesting beyond MAX_PARSE_DEPTH must fail"
        );
        let err = res.error.expect("Error message present");
        assert!(
            !err.contains("panicked"),
            "[Prog 5] Parser panicked on deep nesting!"
        );
        assert!(
            err.contains("recursion")
                || err.contains("depth")
                || err.contains("deep")
                || err.contains("E0105")
                || err.contains("E-SYNTAX-001"),
            "[Prog 5] Expected recursion limit error, got: {}",
            err
        );
        println!(">>> [DIAG 5: Deep Nesting] PASS (clean limit rejection, no stack overflow)");
    }

    // 6. Invalid UTF-8 in source file
    {
        let p = temp_dir.join("prog6_invalid_utf8.dtr");
        let bad_bytes: Vec<u8> = vec![
            b'f', b'n', b' ', b'm', b'a', b'i', b'n', b'(', b')', b' ', b'{', b' ', b'l', b'e',
            b't', b' ', b's', b' ', b'=', b' ', b'"', 0xFF, 0xFE, 0x80, b'"', b' ', b'}',
        ];
        fs::write(&p, bad_bytes).unwrap();
        let res = compiler.compile_file(&p, None);
        assert!(
            !res.success,
            "[Prog 6] Invalid UTF-8 file must fail compilation"
        );
        let err = res.error.expect("Error message present");
        assert!(
            !err.contains("panicked"),
            "[Prog 6] Compiler panicked on invalid UTF-8!"
        );
        assert!(
            err.contains("UTF-8")
                || err.contains("utf-8")
                || err.contains("stream did not contain valid UTF-8"),
            "[Prog 6] Expected UTF-8 decoding error, got: {}",
            err
        );
        println!(
            ">>> [DIAG 6: Invalid UTF-8] PASS: {}",
            err.lines().next().unwrap_or("")
        );
    }

    // 7. Cyclic use imports
    {
        let mod_a_path = temp_dir.join("mod_a.dtr");
        let mod_b_path = temp_dir.join("mod_b.dtr");

        let mod_a_src = "use mod_b;\npub fn foo() -> Int => 1\n";
        let mod_b_src = "use mod_a;\npub fn bar() -> Int => 2\n";

        fs::write(&mod_a_path, mod_a_src).unwrap();
        fs::write(&mod_b_path, mod_b_src).unwrap();

        let res = compiler.compile_file(&mod_a_path, None);
        let err = res.error.as_deref().unwrap_or("");
        assert!(
            !err.contains("panicked"),
            "[Prog 7] Compiler panicked on cycle!"
        );
        println!(">>> [DIAG 7: Cyclic Use] Handled gracefully without infinite loop");
    }

    // 8. Compile-time division by zero in @no_panic context
    {
        let src = "@no_panic\nfn danger() -> Int {\n    let x = 100 / 0\n    return x\n}\nfn main() {\n    out danger()\n}\n";
        let p = temp_dir.join("prog8_div_zero_no_panic.dtr");
        fs::write(&p, src).unwrap();
        let res = compiler.compile_file(&p, None);
        assert!(
            !res.success,
            "[Prog 8] Division by zero in @no_panic must fail compilation"
        );
        let err = res.error.expect("Error message present");
        assert!(
            !err.contains("panicked"),
            "[Prog 8] Compiler panicked on div-by-zero!"
        );
        assert!(
            err.contains("E0951") || err.contains("Division by zero"),
            "[Prog 8] Expected E0951 Panic Violation, got: {}",
            err
        );
        println!(
            ">>> [DIAG 8: Comptime Div-by-Zero in @no_panic] PASS: {}",
            err.lines().next().unwrap_or("")
        );
    }

    // 9. Mutating immutable let variable
    {
        let src = "fn main() {\n    let immutable_val = 10\n    immutable_val = 20\n    out immutable_val\n}\n";
        let p = temp_dir.join("prog9_mutate_immutable.dtr");
        fs::write(&p, src).unwrap();
        let res = compiler.compile_file(&p, None);
        assert!(
            !res.success,
            "[Prog 9] Mutating immutable variable must fail compilation"
        );
        let err = res.error.expect("Error message present");
        assert!(
            !err.contains("panicked"),
            "[Prog 9] Compiler panicked on mutating immutable!"
        );
        assert!(
            err.contains("E-BORROW-002") || err.contains("Cannot mutate immutable variable"),
            "[Prog 9] Expected E-BORROW-002, got: {}",
            err
        );
        println!(
            ">>> [DIAG 9: Mutate Immutable] PASS: {}",
            err.lines().next().unwrap_or("")
        );
    }

    // 10. Missing return in non-unit function
    {
        let src = "fn compute_result(x: Int) -> Int {\n    let dummy = x + 1\n}\nfn main() {\n    out compute_result(5)\n}\n";
        let p = temp_dir.join("prog10_missing_return.dtr");
        fs::write(&p, src).unwrap();
        let res = compiler.compile_file(&p, None);
        assert!(
            !res.success,
            "[Prog 10] Non-unit function without return must fail compilation!"
        );
        let err = res.error.as_deref().unwrap_or("");
        assert!(!err.contains("panicked"), "[Prog 10] Compiler panicked!");
        assert!(
            err.contains("E-TYPE-003") || err.contains("missing a return value"),
            "[Prog 10] Expected E-TYPE-003, got: {}",
            err
        );
        println!(
            ">>> [DIAG 10: Missing Return] PASS: {}",
            err.lines().next().unwrap_or("")
        );
    }

    let _ = fs::remove_dir_all(&temp_dir);
}
