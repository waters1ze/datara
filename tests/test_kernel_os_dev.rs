use forgen::driver::ForgenCompiler;

#[test]
fn test_kernel_mode_and_mmu_bitfields() {
    let source = r#"
@kernel_mode
@no_std
module kernel.arch.x86_64.mmu

class PageTableEntry {
    present: Bool in bit 0
    writable: Bool in bit 1
    user_accessible: Bool in bit 2
    physical_frame: UInt64 in bits 12..=51
    no_execute: Bool in bit 63
}

behavior PageTableEntry {
    fn new(frame: UInt64) -> PageTableEntry {
        return PageTableEntry {
            present: true,
            writable: true,
            user_accessible: false,
            physical_frame: frame,
            no_execute: false
        }
    }
}

fn main() -> Int {
    let pte = PageTableEntry.new(0x0010_0000)
    return 0
}
"#;
    let compiler = ForgenCompiler::new("check");
    let res = compiler.check_source(source, "kernel_mmu_test.dtr");
    assert!(
        res.success,
        "@kernel_mode and bitfields should parse and check cleanly: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_naked_function_and_inline_asm_ports() {
    let source = r#"
@kernel_mode
@no_std
module kernel.ports

class Port8 {
    port: UInt16
}

behavior Port8 {
    @naked
    fn inb(self) -> UInt8 {
        asm! {
            "in al, dx",
            options: [nostack]
        }
        return 0
    }

    @naked
    fn outb(self, val: UInt8) -> Void {
        asm! {
            "out dx, al",
            options: [nostack]
        }
    }
}

fn main() -> Int {
    let p = Port8 { port: 0x03F8 }
    p.outb(65)
    return 0
}
"#;
    let compiler = ForgenCompiler::new("check");
    let res = compiler.check_source(source, "kernel_ports_test.dtr");
    assert!(
        res.success,
        "@naked and asm! should check cleanly: {:?}",
        res.diagnostics
    );
}
