//! Stage-2 DoD gates for real SSA (block parameters / phis):
//! 1. The verifier mechanically rejects hand-written broken SSA.
//! 2. The verifier accepts valid phi-form IR.
//! 3. Mem2Reg actually promotes named variables on real programs
//!    (evidence in the optimization report, not just a trace line).

use forgen::dmir::{
    BasicBlock, BasicBlockId, BlockParam, Function, Inst, Terminator, ValueId, verify_function,
};
use forgen::driver::ForgenCompiler;

fn block(
    id: usize,
    params: Vec<BlockParam>,
    instructions: Vec<Inst>,
    terminator: Terminator,
) -> BasicBlock {
    BasicBlock {
        id: BasicBlockId(id),
        label: format!("bb{}", id),
        params,
        instructions,
        terminator,
    }
}

fn const_int(dest: usize, value: i64) -> Inst {
    Inst::ConstInt {
        dest: ValueId(dest),
        value,
    }
}

#[test]
fn verifier_accepts_valid_phi_form() {
    // entry: %0 = 5; branch bb1(%0)
    // bb1(%1): return %1
    let f = Function {
        name: "valid".into(),
        params: vec![],
        return_type: "Int".into(),
        entry_block: BasicBlockId(0),
        blocks: vec![
            block(
                0,
                vec![],
                vec![const_int(0, 5)],
                Terminator::Branch {
                    target: BasicBlockId(1),
                    args: vec![ValueId(0)],
                },
            ),
            block(
                1,
                vec![BlockParam {
                    val: ValueId(1),
                    ty: "Int".into(),
                    name: Some("x".into()),
                }],
                vec![],
                Terminator::Return {
                    value: Some(ValueId(1)),
                },
            ),
        ],
    };
    assert_eq!(verify_function(&f), Ok(()));
}

#[test]
fn verifier_rejects_duplicate_definition() {
    let f = Function {
        name: "dup".into(),
        params: vec![],
        return_type: "Int".into(),
        entry_block: BasicBlockId(0),
        blocks: vec![block(
            0,
            vec![],
            vec![const_int(0, 1), const_int(0, 2)],
            Terminator::Return {
                value: Some(ValueId(0)),
            },
        )],
    };
    let err = verify_function(&f).expect_err("duplicate definition must be rejected");
    assert!(err.contains("defined more than once"), "got: {}", err);
}

#[test]
fn verifier_rejects_undefined_use() {
    let f = Function {
        name: "undef".into(),
        params: vec![],
        return_type: "Int".into(),
        entry_block: BasicBlockId(0),
        blocks: vec![block(
            0,
            vec![],
            vec![
                const_int(0, 1),
                Inst::BinOp {
                    dest: ValueId(1),
                    op: "+".into(),
                    left: ValueId(0),
                    right: ValueId(99), // never defined
                    ty: "Int".into(),
                },
            ],
            Terminator::Return {
                value: Some(ValueId(1)),
            },
        )],
    };
    let err = verify_function(&f).expect_err("undefined use must be rejected");
    assert!(err.contains("undefined"), "got: {}", err);
}

#[test]
fn verifier_rejects_branch_arity_mismatch() {
    // entry branches to bb1 with 1 argument, but bb1 declares no parameters.
    let f = Function {
        name: "arity".into(),
        params: vec![],
        return_type: "Int".into(),
        entry_block: BasicBlockId(0),
        blocks: vec![
            block(
                0,
                vec![],
                vec![const_int(0, 1)],
                Terminator::Branch {
                    target: BasicBlockId(1),
                    args: vec![ValueId(0)],
                },
            ),
            block(1, vec![], vec![], Terminator::Return { value: None }),
        ],
    };
    let err = verify_function(&f).expect_err("arity mismatch must be rejected");
    assert!(
        err.contains("arguments but target declares"),
        "got: {}",
        err
    );
}

#[test]
fn verifier_rejects_use_not_dominated_by_definition() {
    // entry: cond -> bb1 / bb2, both join at bb3.
    // %1 is defined only on the bb1 path; bb3 uses it anyway.
    // This is exactly the shape that requires a phi — without one it is broken SSA.
    let f = Function {
        name: "nodom".into(),
        params: vec![],
        return_type: "Int".into(),
        entry_block: BasicBlockId(0),
        blocks: vec![
            block(
                0,
                vec![],
                vec![Inst::ConstBool {
                    dest: ValueId(0),
                    value: true,
                }],
                Terminator::CondBranch {
                    cond: ValueId(0),
                    then_block: BasicBlockId(1),
                    then_args: vec![],
                    else_block: BasicBlockId(2),
                    else_args: vec![],
                },
            ),
            block(
                1,
                vec![],
                vec![const_int(1, 1)],
                Terminator::Branch {
                    target: BasicBlockId(3),
                    args: vec![],
                },
            ),
            block(
                2,
                vec![],
                vec![const_int(2, 2)],
                Terminator::Branch {
                    target: BasicBlockId(3),
                    args: vec![],
                },
            ),
            block(
                3,
                vec![],
                vec![Inst::BinOp {
                    dest: ValueId(3),
                    op: "+".into(),
                    left: ValueId(1), // defined only on the then-path
                    right: ValueId(2),
                    ty: "Int".into(),
                }],
                Terminator::Return {
                    value: Some(ValueId(3)),
                },
            ),
        ],
    };
    let err = verify_function(&f).expect_err("use without dominating definition must be rejected");
    assert!(
        err.contains("before its definition dominates"),
        "got: {}",
        err
    );
}

#[test]
fn mem2reg_actually_promotes_named_variables() {
    // A program whose loop-carried variables (sum, i) are the canonical
    // mem2reg input: LoadVar/AssignVar pairs feeding a while-loop header.
    // In "start" mode the IR is verbatim (no promotion); in "release" mode
    // mem2reg must report real promotions and the binary must still be correct.
    let src = r#"
fn loop_sum(n: Int) -> Int {
    mut sum = 0
    mut i = 0
    while i < n {
        sum = sum + i
        i = i + 1
    }
    return sum
}
fn main() {
    out loop_sum(10)
}
"#;

    let start_compiler = ForgenCompiler::new("start");
    let res_start = start_compiler.compile_source(src, "test_mem2reg_start.dtr", None);
    assert!(
        res_start.success,
        "start compile failed: {:?}",
        res_start.error
    );
    let report_start = res_start.optimization_report.expect("start report missing");
    assert_eq!(
        report_start.variables_promoted, 0,
        "start mode must preserve verbatim IR (no promotion)"
    );

    let release_compiler = ForgenCompiler::new("release");
    let res_rel = release_compiler.compile_source(src, "test_mem2reg_release.dtr", None);
    assert!(
        res_rel.success,
        "release compile failed: {:?}",
        res_rel.error
    );
    let report_rel = res_rel.optimization_report.expect("release report missing");
    assert!(
        report_rel.variables_promoted > 0,
        "mem2reg promoted nothing on a canonical while-loop program — \
         the pass is restoring functions unchanged"
    );

    // Behavioral gate: the promoted binary computes the same answer.
    let exe_rel = res_rel.exe_path.unwrap();
    let (out_rel, err_rel, code_rel, _) = release_compiler
        .codegen
        .run_executable(&exe_rel, &[])
        .unwrap();
    assert_eq!(code_rel, 0, "release exit code non-zero: {}", err_rel);
    assert_eq!(out_rel.trim(), "45", "release output mismatch: {}", out_rel);

    let exe_start = res_start.exe_path.unwrap();
    let (out_start, err_start, code_start, _) = start_compiler
        .codegen
        .run_executable(&exe_start, &[])
        .unwrap();
    assert_eq!(code_start, 0, "start exit code non-zero: {}", err_start);
    assert_eq!(
        out_start.trim(),
        "45",
        "start output mismatch: {}",
        out_start
    );
}
