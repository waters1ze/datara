//! Regression tests for correctness fixes applied after the v0.1.0 audit.

use forgen::dmir::{BasicBlock, BasicBlockId, Function, Inst, Module, Terminator, ValueId};
use forgen::driver::ForgenCompiler;
use forgen::optimizer::Optimizer;

/// Diamond CFG whose arm blocks are stored BEFORE the head block. This order
/// used to panic in `convert_branches_to_select` (the head index shifted after
/// the two arm removals) or report the wrong block id in the decision trace.
fn diamond_module(float_arms: bool) -> Module {
    let mut module = Module::new("regress");
    let mut then_inst = Inst::ConstInt {
        dest: ValueId(10),
        value: 1,
    };
    let mut else_inst = Inst::ConstInt {
        dest: ValueId(11),
        value: 2,
    };
    if float_arms {
        then_inst = Inst::ConstFloat {
            dest: ValueId(10),
            value: 1.0,
        };
        else_inst = Inst::ConstFloat {
            dest: ValueId(11),
            value: 2.0,
        };
    }
    module.functions.insert(
        "pick".to_string(),
        Function {
            name: "pick".into(),
            params: vec![("cond".into(), "Bool".into(), ValueId(1))],
            return_type: "Int".into(),
            entry_block: BasicBlockId(0),
            blocks: vec![
                BasicBlock {
                    id: BasicBlockId(1),
                    label: "if_then".into(),
                    params: vec![],
                    instructions: vec![then_inst],
                    terminator: Terminator::Branch {
                        target: BasicBlockId(3),
                        args: vec![ValueId(10)],
                    },
                },
                BasicBlock {
                    id: BasicBlockId(2),
                    label: "if_else".into(),
                    params: vec![],
                    instructions: vec![else_inst],
                    terminator: Terminator::Branch {
                        target: BasicBlockId(3),
                        args: vec![ValueId(11)],
                    },
                },
                BasicBlock {
                    id: BasicBlockId(0),
                    label: "entry".into(),
                    params: vec![],
                    instructions: vec![],
                    terminator: Terminator::CondBranch {
                        cond: ValueId(1),
                        then_block: BasicBlockId(1),
                        then_args: vec![],
                        else_block: BasicBlockId(2),
                        else_args: vec![],
                    },
                },
                BasicBlock {
                    id: BasicBlockId(3),
                    label: "if_merge".into(),
                    params: vec![forgen::dmir::BlockParam {
                        val: ValueId(12),
                        ty: "Int".into(),
                        name: None,
                    }],
                    instructions: vec![],
                    terminator: Terminator::Return {
                        value: Some(ValueId(12)),
                    },
                },
            ],
        },
    );
    module
}

#[test]
fn if_conversion_arms_before_head_does_not_panic() {
    let mut module = diamond_module(false);
    let mut opt = Optimizer::new("release");
    // Must not panic and must remove the two arm blocks.
    opt.optimize_module(&mut module);

    let f = &module.functions["pick"];
    assert!(
        f.blocks
            .iter()
            .all(|b| b.id != BasicBlockId(1) && b.id != BasicBlockId(2)),
        "then/else arm blocks must be removed"
    );
}

#[test]
fn if_conversion_select_infers_float_type() {
    let mut module = diamond_module(true);
    let mut opt = Optimizer::new("release");
    opt.optimize_module(&mut module);

    let f = &module.functions["pick"];
    let sel_ty = f
        .blocks
        .iter()
        .flat_map(|b| b.instructions.iter())
        .find_map(|i| match i {
            Inst::Select { ty, .. } => Some(ty.clone()),
            _ => None,
        });
    assert_eq!(
        sel_ty.as_deref(),
        Some("Float"),
        "select must infer Float from ConstFloat arms, not hardcode Int"
    );
}

#[test]
fn llvm_float_interpolation_uses_real_runtime_symbol() {
    let source = r#"
fn main() {
    out(fmt"v={1.5}")
}
"#;
    let compiler = ForgenCompiler::new("release").with_llvm(true);
    let res = compiler.compile_source(source, "fmt_float.dtr", None);
    assert!(res.success, "Compilation must succeed: {:?}", res.error);
    let llvm = res.llvm_source.expect("LLVM IR must be generated");
    assert!(
        llvm.contains("datara_rt_float_to_str"),
        "must call the real runtime symbol datara_rt_float_to_str"
    );
    assert!(
        !llvm.contains("datara_rt_flt_to_str"),
        "old undefined symbol datara_rt_flt_to_str must be gone"
    );
}

#[test]
fn llvm_select_stays_float_typed_for_float_diamonds() {
    let source = r#"
fn pick(cond: Bool) -> Float {
    mut x = 0.0
    if cond { x = 1.5 } else { x = 2.5 }
    x
}

fn main() {
    out(pick(true))
    out(pick(false))
}
"#;
    let compiler = ForgenCompiler::new("release").with_llvm(true);
    let res = compiler.compile_source(source, "select_float.dtr", None);
    assert!(res.success, "Compilation must succeed: {:?}", res.error);
    let llvm = res.llvm_source.expect("LLVM IR must be generated");
    assert!(
        llvm.contains("select i1"),
        "if-conversion select must be present"
    );
    let bad_select = llvm
        .lines()
        .any(|l| l.contains("select i1") && l.contains("i64 %v"));
    assert!(
        !bad_select,
        "float select must keep double operands, never i64:\n{}",
        llvm
    );
}

#[test]
fn cranelift_rejects_simd_with_clear_error() {
    let source = r#"
fn main() {
    let a = float4(1.0, 2.0, 3.0, 4.0)
    let b = float4(4.0, 3.0, 2.0, 1.0)
    out(dot(a, b))
}
"#;
    let compiler = ForgenCompiler::new("quick");
    let res = compiler.compile_source(source, "simd_cranelift.dtr", None);
    let err = format!("{:?}", res.error);
    assert!(
        !res.success,
        "Cranelift must reject SIMD instead of silently emitting garbage"
    );
    assert!(
        err.contains("--llvm"),
        "error must point the user to --llvm, got: {}",
        err
    );
}

#[test]
fn llvm_simd_dot_end_to_end() {
    if forgen::codegen::linker::find_clang().is_none() {
        // The LLVM pipeline requires clang; the DMIR-level tests above cover
        // the rest of the behaviour on toolchains without it.
        return;
    }
    let dir = std::env::temp_dir().join("forgen_simd_test");
    let _ = std::fs::create_dir_all(&dir);
    let src = dir.join("simd_app.dtr");
    let exe = dir.join("simd_app.exe");
    std::fs::write(
        &src,
        r#"
fn main() {
    let a = float4(1.0, 2.0, 3.0, 4.0)
    let b = float4(4.0, 3.0, 2.0, 1.0)
    out(dot(a, b))
}
"#,
    )
    .unwrap();

    let compiler = ForgenCompiler::new("release").with_llvm(true);
    let res = compiler.compile_file(&src, Some(&exe));
    assert!(
        res.success,
        "LLVM SIMD compilation must succeed: {:?}",
        res.error
    );
    let (stdout, _stderr, code, _) = compiler
        .codegen
        .run_executable(&exe, &[])
        .expect("run failed");
    assert_eq!(code, 0, "executable must exit 0, stderr: {}", _stderr);
    assert!(
        stdout.contains("20"),
        "dot(a,b) == 20, got stdout: {}",
        stdout
    );

    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&exe);
    let _ = std::fs::remove_file(dir.join("simd_app.ll"));
    let _ = std::fs::remove_dir(&dir);
}
