//! Phase 14 Test Suite: Interprocedural Optimization (IPO / LTO) Engine
//!
//! Validates:
//! 1. Literal argument -> specialized clone function with constant propagation.
//! 2. Single-implementation polymorphic/trait method call devirtualization to direct static call.
//! 3. Cross-module pure inlining at DMIR level (LTO).
//! 4. Dead clone elimination (DCE) for unreferenced specialized functions.
//! 5. End-to-end execution of optimized code producing exact results.

use forgen::dmir::*;
use forgen::driver::ForgenCompiler;
use forgen::optimizer::cost_model::OptimizationDecisionTrace;
use forgen::optimizer::ipo::InterproceduralOptimizer;

#[test]
fn test_constant_argument_specialization_and_cloning() {
    let mut module = Module::new("test_spec");

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

    // Caller: main(val) calls scale(val, 5)
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
                    value: 5,
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

    // Check clone was created: scale__spec_factor_5
    let clone_name = "scale__spec_factor_5";
    assert!(
        module.functions.contains_key(clone_name),
        "Specialized clone '{}' must exist in module",
        clone_name
    );

    // Verify caller now calls the clone
    let main_fn = &module.functions["main"];
    let call_inst = main_fn.blocks[0]
        .instructions
        .iter()
        .find(|i| matches!(i, Inst::Call { .. }))
        .expect("Call instruction must remain");

    if let Inst::Call { func, .. } = call_inst {
        assert_eq!(func, clone_name, "Caller must target specialized clone");
    }

    // Verify clone passes verification
    forgen::dmir::verify_module(&module).expect("Module must be well-formed after specialization");

    // Verify trace record
    let record = trace
        .records
        .iter()
        .find(|r| r.pass == "ArgumentSpecialization")
        .expect("Trace must have ArgumentSpecialization record");
    assert_eq!(record.decision, "Applied");
}

#[test]
fn test_devirtualization_single_impl() {
    let mut module = Module::new("test_devirt");

    // Single implementation: Shape_area(shape_ptr) -> Int
    let shape_area = Function {
        name: "Shape_area".into(),
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
                value: 42,
            }],
            terminator: Terminator::Return {
                value: Some(ValueId(2)),
            },
        }],
    };
    module.functions.insert("Shape_area".into(), shape_area);

    // Caller calling method "area"
    let caller = Function {
        name: "run".into(),
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
                    value: 1,
                },
                Inst::MethodCall {
                    dest: ValueId(11),
                    object: ValueId(10),
                    method: "area".into(),
                    args: Vec::new(),
                    ty: "Int".into(),
                },
            ],
            terminator: Terminator::Return {
                value: Some(ValueId(11)),
            },
        }],
    };
    module.functions.insert("run".into(), caller);

    let mut trace = OptimizationDecisionTrace::new();
    let changed =
        InterproceduralOptimizer::devirtualize_single_impl_methods(&mut module, &mut trace);
    assert!(changed, "Devirtualization must report changed = true");

    let run_fn = &module.functions["run"];
    let first_inst = &run_fn.blocks[0].instructions[1];
    match first_inst {
        Inst::Call { func, args, .. } => {
            assert_eq!(
                func, "Shape_area",
                "Devirtualized call must target Shape_area directly"
            );
            assert_eq!(args.len(), 1, "Receiver must be passed as first argument");
            assert_eq!(args[0], ValueId(10));
        }
        other => panic!(
            "Expected Inst::Call after devirtualization, got {:?}",
            other
        ),
    }

    // Verify trace record
    let record = trace
        .records
        .iter()
        .find(|r| r.pass == "Devirtualization")
        .expect("Trace must have Devirtualization record");
    assert_eq!(record.decision, "Applied");
}

#[test]
fn test_cross_module_pure_inlining_lto() {
    let mut module = Module::new("test_lto");

    // Pure helper: add_one(x) -> x + 1
    let add_one = Function {
        name: "add_one".into(),
        params: vec![("x".into(), "Int".into(), ValueId(1))],
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
                    dest: ValueId(2),
                    value: 1,
                },
                Inst::BinOp {
                    dest: ValueId(3),
                    op: "+".into(),
                    left: ValueId(1),
                    right: ValueId(2),
                    ty: "Int".into(),
                },
            ],
            terminator: Terminator::Return {
                value: Some(ValueId(3)),
            },
        }],
    };
    module.functions.insert("add_one".into(), add_one);

    // Caller
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
                func: "add_one".into(),
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
fn test_dead_clone_elimination() {
    let mut module = Module::new("test_dce");

    // Specialized clone with 0 callers
    let dead_clone = Function {
        name: "helper__spec_x_10".into(),
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
                value: 100,
            }],
            terminator: Terminator::Return {
                value: Some(ValueId(1)),
            },
        }],
    };
    module
        .functions
        .insert("helper__spec_x_10".into(), dead_clone);

    // Active main
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
        !module.functions.contains_key("helper__spec_x_10"),
        "Uncalled specialized clone must be removed"
    );
    assert!(
        module.functions.contains_key("main"),
        "Active functions must be preserved"
    );
}

#[test]
fn test_end_to_end_ipo_llvm_and_native() {
    let source = r#"
fn multiply_by_constant(val: Int, factor: Int) -> Int {
    return val * factor
}

class Calculator {
    base: Int
}

behavior Calculator {
    calc() -> Int => multiply_by_constant(this.base, 4)
}

fn main() {
    mut c = Calculator { base: 25 }
    out c.calc()
}
"#;

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "test_phase14_ipo_e2e.dtr", None);
    assert!(
        res.success,
        "Native release compilation with IPO must succeed: {:?}",
        res.error
    );

    let exe = res.exe_path.expect("Executable must exist");
    let (stdout, stderr, code, _) = compiler.codegen.run_executable(&exe, &[]).unwrap();
    assert_eq!(
        code, 0,
        "Execution returned non-zero code. Stderr: {}",
        stderr
    );
    assert_eq!(stdout.trim(), "100", "Result 25 * 4 must equal 100");
}
