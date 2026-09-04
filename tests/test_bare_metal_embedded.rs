use forgen::driver::ForgenCompiler;

#[test]
fn test_register_block_mmio() {
    let source = r#"
@bare_metal
module embedded.timer

register Timer2 at 0x4000_0000 {
    control: UInt16 at 0x00
    status: UInt16 at 0x02
    counter: UInt32 at 0x08
    reload: UInt32 at 0x0C
}

fn init_timer() -> Int {
    return 0
}

fn main() -> Int {
    init_timer()
    return 0
}
"#;
    let compiler = ForgenCompiler::new("check");
    let res = compiler.check_source(source, "embedded_timer_test.dtr");
    assert!(
        res.success,
        "register block declaration and @bare_metal should check cleanly: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_interrupt_handler_vector() {
    let source = r#"
@bare_metal
module embedded.isr

class SystemTimer {
    ticks: Int
}

behavior SystemTimer {
    @interrupt_handler(vector: 0x001C)
    fn on_tick() -> Void {
        asm! {
            "cli",
            "nop",
            options: [nostack]
        }
    }
}

fn main() -> Int {
    return 0
}
"#;
    let compiler = ForgenCompiler::new("check");
    let res = compiler.check_source(source, "embedded_isr_test.dtr");
    assert!(
        res.success,
        "@interrupt_handler should check cleanly: {:?}",
        res.diagnostics
    );
}
