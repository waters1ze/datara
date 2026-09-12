//! Phase 9 (v1.2.0): Interprocedural Optimization (IPO / LTO) & Specialization Engine
//!
//! Validates:
//! 1. Constant argument specialization with function cloning and constant propagation.
//! 2. Devirtualization of single-implementation polymorphic/behavior method calls to direct calls.
//! 3. Cross-module pure inlining at DMIR level.
//! 4. Dead clone elimination (DCE) for unreferenced specialized functions.
//! 5. End-to-end execution of optimized code producing exact deterministic results across runs.

use forgen::dmir::*;
use forgen::driver::ForgenCompiler;
use forgen::optimizer::cost_model::OptimizationDecisionTrace;
use forgen::optimizer::ipo::InterproceduralOptimizer;

#[test]
fn test_v120_ipo_constant_argument_specialization_cloning() {
    let mut module = Module::new("test_v120_spec");

    // Callee: scale(x, factor) -> x * factor
    let callee = Function {
        name: "scale".into(),
        params: vec![
            ("x".into(), "Int".into(), ValueId(1)),
            ("factor".into(), "Int".into(), ValueId(2)),
        ],
        param_refinements: Vec::new(),
        requires: Vec::new(),
        return_type: "Int".into(),
        entry_block: BasicBlockId(0),
        blocks: vec![BasicBlock {
            id: BasicBlockId(0),
            label: "bb0".into(),
            params: Vec::new(),
            instructions: vec![Inst::BinOp {
                dest: ValueId(3),
                op: "*".into(),
                left: ValueId(1),
                right: ValueId(2),
                ty: "Int".into(),
            }],
            terminator: Terminator::Return {
                value: Some(ValueId(3)),
            },
        }],
    };
    module.functions.insert("scale".into(), callee);

    // Caller: main(val) calls scale(val, 8)
    let caller = Function {
        name: "main".into(),
        params: vec![("val".into(), "Int".into(), ValueId(10))],
        param_refinements: Vec::new(),
        requires: Vec::new(),
        return_type: "Int".into(),
        entry_block: BasicBlockId(0),
        blocks: vec![BasicBlock {
            id: BasicBlockId(0),
            label: "bb0".into(),
            params: Vec::new(),
            instructions: vec![
                Inst::ConstInt {
                    dest: ValueId(11),
                    value: 8,
                },
                Inst::Call {
                    dest: ValueId(12),
                    func: "scale".into(),
                    args: vec![ValueId(10), ValueId(11)],
                    ty: "Int".into(),
                },
            ],
            terminator: Terminator::Return {
                value: Some(ValueId(12)),
            },
        }],
    };
    module.functions.insert("main".into(), caller);

    let mut trace = OptimizationDecisionTrace::new();
    let changed = InterproceduralOptimizer::specialize_constant_arguments(&mut module, &mut trace);
    assert!(changed, "Specialization must report changed = true");

    let clone_name = "scale__spec_factor_8";
    assert!(
        module.functions.contains_key(clone_name),
        "Specialized clone '{}' must exist in module",
        clone_name
    );

    let main_fn = &module.functions["main"];
    let call_inst = main_fn.blocks[0]
        .instructions
        .iter()
        .find(|i| matches!(i, Inst::Call { .. }))
        .expect("Call instruction must remain");

    if let Inst::Call { func, .. } = call_inst {
        assert_eq!(func, clone_name, "Caller must target specialized clone");
    }

    forgen::dmir::verify_module(&module).expect("Module must be well-formed after specialization");

    let record = trace
        .records
        .iter()
        .find(|r| r.pass == "ArgumentSpecialization")
        .expect("Trace must have ArgumentSpecialization record");
    assert_eq!(record.decision, "Applied");
}

#[test]
fn test_v120_ipo_trait_devirtualization() {
    let mut module = Module::new("test_v120_devirt");

    // Single implementation: Renderer_draw(rend_ptr) -> Int
    let renderer_draw = Function {
        name: "Renderer_draw".into(),
        params: vec![("self".into(), "Int".into(), ValueId(1))],
        param_refinements: Vec::new(),
        requires: Vec::new(),
        return_type: "Int".into(),
        entry_block: BasicBlockId(0),
        blocks: vec![BasicBlock {
            id: BasicBlockId(0),
            label: "bb0".into(),
            params: Vec::new(),
            instructions: vec![Inst::ConstInt {
                dest: ValueId(2),
                value: 99,
            }],
            terminator: Terminator::Return {
                value: Some(ValueId(2)),
            },
        }],
    };
    module
        .functions
        .insert("Renderer_draw".into(), renderer_draw);

    let caller = Function {
        name: "execute".into(),
        params: Vec::new(),
        param_refinements: Vec::new(),
        requires: Vec::new(),
        return_type: "Int".into(),
        entry_block: BasicBlockId(0),
        blocks: vec![BasicBlock {
            id: BasicBlockId(0),
            label: "bb0".into(),
            params: Vec::new(),
            instructions: vec![
                Inst::ConstInt {
                    dest: ValueId(10),
                    value: 42,
                },
                Inst::MethodCall {
                    dest: ValueId(11),
                    object: ValueId(10),
                    method: "draw".into(),
                    args: Vec::new(),
                    ty: "Int".into(),
                },
            ],
            terminator: Terminator::Return {
                value: Some(ValueId(11)),
            },
        }],
    };
    module.functions.insert("execute".into(), caller);

    let mut trace = OptimizationDecisionTrace::new();
    let changed =
        InterproceduralOptimizer::devirtualize_single_impl_methods(&mut module, &mut trace);
    assert!(changed, "Devirtualization must report changed = true");

    let exec_fn = &module.functions["execute"];
    let call_inst = &exec_fn.blocks[0].instructions[1];
    match call_inst {
        Inst::Call { func, args, .. } => {
            assert_eq!(
                func, "Renderer_draw",
                "Devirtualized call must target Renderer_draw directly"
            );
            assert_eq!(args.len(), 1, "Receiver must be passed as first argument");
            assert_eq!(args[0], ValueId(10));
        }
        other => panic!(
            "Expected Inst::Call after devirtualization, got {:?}",
            other
        ),
    }

    let record = trace
        .records
        .iter()
        .find(|r| r.pass == "Devirtualization")
        .expect("Trace must have Devirtualization record");
    assert_eq!(record.decision, "Applied");
}

#[test]
fn test_v120_ipo_cross_module_pure_inlining() {
    let mut module = Module::new("test_v120_pure_lto");

    // Pure helper: square(x) -> x * x
    let square = Function {
        name: "square".into(),
        params: vec![("x".into(), "Int".into(), ValueId(1))],
        param_refinements: Vec::new(),
        requires: Vec::new(),
        return_type: "Int".into(),
        entry_block: BasicBlockId(0),
        blocks: vec![BasicBlock {
            id: BasicBlockId(0),
            label: "bb0".into(),
            params: Vec::new(),
            instructions: vec![Inst::BinOp {
                dest: ValueId(2),
                op: "*".into(),
                left: ValueId(1),
                right: ValueId(1),
                ty: "Int".into(),
            }],
            terminator: Terminator::Return {
                value: Some(ValueId(2)),
            },
        }],
    };
    module.functions.insert("square".into(), square);

    // Caller: compute(v) calls square(v)
    let caller = Function {
        name: "compute".into(),
        params: vec![("v".into(), "Int".into(), ValueId(10))],
        param_refinements: Vec::new(),
        requires: Vec::new(),
        return_type: "Int".into(),
        entry_block: BasicBlockId(0),
        blocks: vec![BasicBlock {
            id: BasicBlockId(0),
            label: "bb0".into(),
            params: Vec::new(),
            instructions: vec![Inst::Call {
                dest: ValueId(11),
                func: "square".into(),
                args: vec![ValueId(10)],
                ty: "Int".into(),
            }],
            terminator: Terminator::Return {
                value: Some(ValueId(11)),
            },
        }],
    };
    module.functions.insert("compute".into(), caller);

    let mut trace = OptimizationDecisionTrace::new();
    let changed = InterproceduralOptimizer::inline_cross_module_pure(&mut module, &mut trace);
    assert!(changed, "Cross-module pure inlining must succeed");

    let compute_fn = &module.functions["compute"];
    let has_call = compute_fn.blocks[0]
        .instructions
        .iter()
        .any(|i| matches!(i, Inst::Call { .. }));
    assert!(
        !has_call,
        "Call to pure function must be eliminated by cross-module inlining"
    );

    forgen::dmir::verify_module(&module).expect("Module must verify after pure LTO inlining");
}

#[test]
fn test_v120_ipo_dead_clone_elimination() {
    let mut module = Module::new("test_v120_dce");

    let dead_clone = Function {
        name: "helper__spec_x_42".into(),
        params: Vec::new(),
        param_refinements: Vec::new(),
        requires: Vec::new(),
        return_type: "Int".into(),
        entry_block: BasicBlockId(0),
        blocks: vec![BasicBlock {
            id: BasicBlockId(0),
            label: "bb0".into(),
            params: Vec::new(),
            instructions: vec![Inst::ConstInt {
                dest: ValueId(1),
                value: 420,
            }],
            terminator: Terminator::Return {
                value: Some(ValueId(1)),
            },
        }],
    };
    module
        .functions
        .insert("helper__spec_x_42".into(), dead_clone);

    let main_fn = Function {
        name: "main".into(),
        params: Vec::new(),
        param_refinements: Vec::new(),
        requires: Vec::new(),
        return_type: "Int".into(),
        entry_block: BasicBlockId(0),
        blocks: vec![BasicBlock {
            id: BasicBlockId(0),
            label: "bb0".into(),
            params: Vec::new(),
            instructions: vec![Inst::ConstInt {
                dest: ValueId(1),
                value: 0,
            }],
            terminator: Terminator::Return {
                value: Some(ValueId(1)),
            },
        }],
    };
    module.functions.insert("main".into(), main_fn);

    let mut trace = OptimizationDecisionTrace::new();
    let changed = InterproceduralOptimizer::eliminate_dead_clones(&mut module, &mut trace);
    assert!(changed, "Dead clone must be eliminated");
    assert!(
        !module.functions.contains_key("helper__spec_x_42"),
        "Uncalled specialized clone must be removed"
    );
    assert!(
        module.functions.contains_key("main"),
        "Active functions must be preserved"
    );
}

#[test]
fn test_v120_ipo_end_to_end_execution_determinism() {
    let source = r#"
fn multiply_by_constant(val: Int, factor: Int) -> Int {
    return val * factor
}

class Pipeline {
    multiplier: Int
}

behavior Pipeline {
    run(input: Int) -> Int => multiply_by_constant(input, this.multiplier)
}

fn main() {
    mut p = Pipeline { multiplier: 7 }
    let res = p.run(6)
    println(res)
}
"#;

    let compiler = ForgenCompiler::new("release").with_llvm(true);
    let res = compiler.compile_source(source, "v120_ipo_pipeline.dtr", None);
    assert!(
        res.success,
        "Native compilation with IPO must succeed: {:?}",
        res.error
    );

    let exe = res.exe_path.expect("Executable must exist");
    // Run 5 consecutive times to verify determinism
    for run in 1..=5 {
        let (stdout, stderr, code, _) = compiler.cranelift.run_executable(&exe, &[]).unwrap();
        assert_eq!(
            code, 0,
            "Run {} failed with code {}. Stderr: {}",
            run, code, stderr
        );
        assert_eq!(
            stdout.trim(),
            "42",
            "Result of 6 * 7 must equal 42 on run {}",
            run
        );
    }
}
