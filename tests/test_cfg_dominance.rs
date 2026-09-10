use forgen::dmir::cfg::ControlFlowGraph;
use forgen::driver::ForgenCompiler;

#[test]
fn test_cfg_dominator_tree_and_loops() {
    let source = r#"
fn loop_compute(limit: Int) -> Int {
    mut i = 0
    mut sum = 0
    while i < limit {
        sum = sum + i
        i = i + 1
    }
    return sum
}

fn main() {
    out loop_compute(10)
}
"#;
    let compiler = ForgenCompiler::new("quick");
    let res = compiler.compile_source(source, "cfg_test.dtr", None);
    assert!(res.success, "Compilation should succeed: {:?}", res.error);

    let dmir = res.dmir_module.expect("DMIR module must exist");
    let loop_fn = dmir
        .functions
        .get("loop_compute")
        .expect("loop_compute must exist");

    assert!(
        loop_fn.blocks.len() > 1,
        "Function must contain multiple basic blocks in CFG representation"
    );

    let cfg = ControlFlowGraph::build(loop_fn);
    assert_eq!(cfg.entry, loop_fn.entry_block);
    assert!(
        !cfg.loops.is_empty(),
        "Must detect at least one natural loop in while loop"
    );

    let natural_loop = &cfg.loops[0];
    assert!(
        cfg.dominates(natural_loop.header, natural_loop.back_edges[0]),
        "Loop header must dominate back-edge block"
    );
}

#[test]
fn test_cfg_if_branch_dominance() {
    let source = r#"
fn branch_fn(x: Int) -> Int {
    mut result = 0
    if x > 100 {
        result = 1
    } else {
        result = 2
    }
    return result
}

fn main() {
    out branch_fn(50)
}
"#;
    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "cfg_branch_test.dtr", None);
    assert!(res.success);

    let dmir = res.dmir_module.unwrap();
    let branch_fn = dmir.functions.get("branch_fn").unwrap();

    assert!(
        branch_fn.blocks.len() >= 3,
        "If-else construct must create separate then/else/merge blocks"
    );

    println!("branch_fn blocks: {:#?}", branch_fn.blocks);
    let cfg = ControlFlowGraph::build(branch_fn);
    println!("CFG entry: {:?}", cfg.entry);
    println!("CFG blocks: {:?}", cfg.blocks);
    println!("CFG idom: {:?}", cfg.idom);
    println!("CFG predecessors: {:?}", cfg.predecessors);
    println!("CFG successors: {:?}", cfg.successors);
    for &b in &cfg.blocks {
        if b != cfg.entry {
            assert!(
                cfg.dominates(cfg.entry, b),
                "Entry block {:?} must dominate {:?}",
                cfg.entry,
                b
            );
        }
    }
}
