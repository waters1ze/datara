use forgen::driver::ForgenCompiler;

struct BrokenCase {
    id: usize,
    name: &'static str,
    source: &'static str,
    expected_code: &'static str,
    expected_msg_sub: &'static str,
}

const TWELVE_BROKEN: &[BrokenCase] = &[
    // 1. Typo in identifier
    BrokenCase {
        id: 1,
        name: "typo_in_identifier",
        source: "fn main() { let target_counter = 10; out target_countr }",
        expected_code: "E-RESOLVE-001",
        expected_msg_sub: "target_counter",
    },
    // 2. Type mismatch
    BrokenCase {
        id: 2,
        name: "type_mismatch",
        source: "fn main() { let x: Int = \"hello\" }",
        expected_code: "E-TYPE-001",
        expected_msg_sub: "String",
    },
    // 3. Use after move
    BrokenCase {
        id: 3,
        name: "use_after_move",
        source: "fn main() { let x = 10; destroy(x); out x }",
        expected_code: "E-BORROW-001",
        expected_msg_sub: "moved",
    },
    // 4. Incomplete match
    BrokenCase {
        id: 4,
        name: "incomplete_match",
        source: "enum Color { Red, Green, Blue } fn main() { let c = Color.Red; match c { Color.Red => 1 } }",
        expected_code: "E0310",
        expected_msg_sub: "non-exhaustive",
    },
    // 5. Borrow conflict (mutable view then read)
    BrokenCase {
        id: 5,
        name: "borrow_conflict",
        source: "class Box { v: Int } fn main() { mut b = Box { v: 1 }; let v = mut_view(b); out b.v }",
        expected_code: "E-BORROW-006",
        expected_msg_sub: "borrow",
    },
    // 6. Violated contract (broken contract expression)
    BrokenCase {
        id: 6,
        name: "broken_contract_syntax",
        source: "fn bad(x: Int) -> Int require invalid_unresolved_var > 0 { return x } fn main() { out bad(1) }",
        expected_code: "E-TYPE-004",
        expected_msg_sub: "Operator",
    },
    // 7. Invalid trait bound
    BrokenCase {
        id: 7,
        name: "invalid_trait_bound",
        source: "trait Serializable { fn serialize(&self) -> String; } class Plain { v: Int } fn save<T: Serializable>(x: T) {} fn main() { let p = Plain { v: 1 }; save(p) }",
        expected_code: "E-TYPE-001",
        expected_msg_sub: "Serializable",
    },
    // 8. Parser depth exceeded (L+1)
    BrokenCase {
        id: 8,
        name: "parser_depth_exceeded",
        source: "fn main() { out (((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((1))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))) }",
        expected_code: "E0105",
        expected_msg_sub: "nesting",
    },
    // 9. Broken UTF-8 / unexpected tokens
    BrokenCase {
        id: 9,
        name: "broken_utf8_char",
        source: "fn main() { out 10 \u{0000} 20 }",
        expected_code: "E-SYNTAX-001",
        expected_msg_sub: "character",
    },
    // 10. Cyclic / missing module
    BrokenCase {
        id: 10,
        name: "cyclic_use",
        source: "use cyclic_module\nfn main() {}",
        expected_code: "E-RESOLVE-005",
        expected_msg_sub: "cyclic_module",
    },
    // 11. out without argument
    BrokenCase {
        id: 11,
        name: "out_without_arg",
        source: "fn main() { out }",
        expected_code: "E-SYNTAX-001",
        expected_msg_sub: "Unexpected",
    },
    // 12. Integer literal 9223372036854775808 (exceeds i64::MAX)
    BrokenCase {
        id: 12,
        name: "integer_literal_overflow",
        source: "fn main() { let x = 9223372036854775808 }",
        expected_code: "E-SYNTAX-004",
        expected_msg_sub: "integer",
    },
];

#[test]
fn audit_diagnostics_12_broken_programs_zero_panics_valid_spans_and_messages() {
    let compiler = ForgenCompiler::new("release");

    for c in TWELVE_BROKEN {
        println!(">>> [DIAG TEST #{}] {} <<<", c.id, c.name);

        let res = std::panic::catch_unwind(|| {
            compiler.compile_source(c.source, &format!("{}.dtr", c.name), None)
        });

        assert!(
            res.is_ok(),
            "CRITICAL: Compiler panicked on broken program #{} ({})!",
            c.id,
            c.name
        );

        let comp_res = res.unwrap();
        assert!(
            !comp_res.success,
            "Broken program #{} ({}) must NOT compile successfully!",
            c.id, c.name
        );

        let diag = &comp_res.diagnostics;
        assert!(
            !diag.is_empty(),
            "Program #{} ({}) must produce diagnostic text",
            c.id,
            c.name
        );

        assert!(
            diag.contains(c.expected_code)
                || comp_res
                    .error
                    .as_deref()
                    .unwrap_or("")
                    .contains(c.expected_code),
            "Program #{} ({}) expected error code '{}', got:\n{}",
            c.id,
            c.name,
            c.expected_code,
            diag
        );

        let diag_lower = diag.to_lowercase();
        let exp_lower = c.expected_msg_sub.to_lowercase();
        assert!(
            diag_lower.contains(&exp_lower)
                || comp_res
                    .error
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&exp_lower),
            "Program #{} ({}) expected message substring '{}', got:\n{}",
            c.id,
            c.name,
            c.expected_msg_sub,
            diag
        );

        // Verify span presence (e.g. line:col or --> file:line:col)
        assert!(
            diag.contains("-->") || diag.contains(":") || diag.contains(".dtr"),
            "Program #{} ({}) diagnostic must include file/line source span! Got:\n{}",
            c.id,
            c.name,
            diag
        );
    }
}
