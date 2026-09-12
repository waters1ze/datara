use forgen::codegen::cranelift::{CraneliftBackend, JitCompilationTier};
use forgen::driver::ForgenCompiler;

#[test]
fn test_cranelift_live_code_hot_reloading() {
    let compiler = ForgenCompiler::new("debug");
    let backend = CraneliftBackend::for_host();
    let mut session = backend
        .create_jit_session(JitCompilationTier::FastCompile)
        .expect("Must create JIT session");

    // 1. Initial gameplay module: get_damage() -> 45
    let code_v1 = r#"
pub fn get_damage() -> Int {
    return 45
}

fn main() {
    let d = get_damage()
    println(fmt"DAMAGE: {d}")
}
"#;
    let dmir_v1 = compiler
        .compile_source_to_dmir(code_v1, "gameplay.dtr")
        .expect("Must compile DMIR v1");
    session.load_module(&dmir_v1).expect("Must load module v1");

    let (out1, _, code1, _) = session.run_entry(None, &[], true).expect("Must run v1");
    assert_eq!(code1, 0);
    assert!(
        out1.contains("DAMAGE: 45"),
        "Expected DAMAGE: 45, got: {}",
        out1
    );

    // 2. Live code edit during gameplay: get_damage() -> 180 (e.g. buff/balance patch)
    let code_v2 = r#"
pub fn get_damage() -> Int {
    return 180
}

fn main() {
    let d = get_damage()
    println(fmt"DAMAGE: {d}")
}
"#;
    let dmir_v2 = compiler
        .compile_source_to_dmir(code_v2, "gameplay.dtr")
        .expect("Must compile DMIR v2");
    println!(
        "DMIR v2 functions: {:?}",
        dmir_v2.functions.keys().collect::<Vec<_>>()
    );

    let swap_latency_ns = session
        .hot_reload_function(&dmir_v2, "get_damage")
        .expect("Must hot-reload get_damage");

    println!(
        "Hot reload swap completed in {} nanoseconds ({} us)",
        swap_latency_ns,
        swap_latency_ns / 1000
    );
    // Hot swap latency budget check: allow tolerance for CI virtualized environments and AddressSanitizer overhead
    let max_budget_ns = if std::env::var("CI").is_ok() {
        2_000_000_000 // 2s in virtualized/ASan CI runners
    } else {
        100_000_000 // 100ms in local environments
    };
    assert!(
        swap_latency_ns < max_budget_ns,
        "Hot swap latency must be within budget, took {} ns",
        swap_latency_ns
    );

    // 3. Immediately run next frame: must execute new code without process restart
    let (out2, _, code2, _) = session.run_entry(None, &[], true).expect("Must run v2");
    assert_eq!(code2, 0);
    assert!(
        out2.contains("DAMAGE: 180"),
        "Expected DAMAGE: 180, got: {}",
        out2
    );
}
