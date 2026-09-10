use forgen::codegen::cranelift::RealCraneliftBackend;
use forgen::codegen::target::TargetInfo;
use forgen::driver::ForgenCompiler;
use object::{Object, ObjectSection};

#[test]
fn audit_dwarf_line_mapping_pinpoints_exact_statement() {
    // Exact source where line numbers are strictly defined:
    // Line 1: // Comment header
    // Line 2:
    // Line 3: pub fn compute_exact(a: Int) -> Int {
    // Line 4:     let target_mult = a * 42
    // Line 5:     let target_statement = target_mult + 100
    // Line 6:     return target_statement
    // Line 7: }
    // Line 8: pub fn main() {
    // Line 9:     out compute_exact(10)
    // Line 10: }
    let source = "// Comment header\n\npub fn compute_exact(a: Int) -> Int {\n    let target_mult = a * 42\n    let target_statement = target_mult + 100\n    return target_statement\n}\npub fn main() {\n    out compute_exact(10)\n}\n";

    let compiler = ForgenCompiler::new("debug").with_debug(true);
    let res = compiler.compile_source(source, "audit_dwarf_exact.dtr", None);
    assert!(
        res.success,
        "Debug compilation must succeed: {:?}",
        res.error
    );

    let dmir = res.dmir_module.expect("DMIR module must be present");

    // 1. Compile to Linux ELF object bytes to inspect standard DWARF sections
    let linux_backend = RealCraneliftBackend::new(TargetInfo::x86_64_linux());
    let elf_bytes = linux_backend
        .compile_to_object_bytes(&dmir)
        .expect("ELF object compilation with DWARF must succeed");

    let elf_obj = object::File::parse(&*elf_bytes).expect("Must parse as valid ELF object");

    let line_sec = elf_obj
        .section_by_name(".debug_line")
        .expect(".debug_line section MUST exist in object file");
    let line_data = line_sec.data().expect("Line data readable");
    assert!(!line_data.is_empty(), ".debug_line must not be empty");

    let info_sec = elf_obj
        .section_by_name(".debug_info")
        .expect(".debug_info section MUST exist in object file");
    let info_data = info_sec.data().expect("Info data readable");
    assert!(!info_data.is_empty(), ".debug_info must not be empty");

    // 2. Parse line number program with gimli
    let debug_line = gimli::DebugLine::new(line_data, gimli::LittleEndian);
    let program = debug_line
        .program(gimli::DebugLineOffset(0), 8, None, None)
        .expect("Line program header must parse");

    let mut rows = program.rows();
    let mut recorded_lines = Vec::new();
    while let Ok(Some((_, row))) = rows.next_row() {
        if let Some(line) = row.line() {
            recorded_lines.push(line.get());
        }
    }

    println!("Audit DWARF recorded lines: {:?}", recorded_lines);

    // Verify exact line mapping:
    // Function starts at line 3
    assert!(
        recorded_lines.contains(&3),
        "DWARF line table must contain function declaration line 3, got: {:?}",
        recorded_lines
    );
    // target_mult is at line 4
    assert!(
        recorded_lines.contains(&4),
        "DWARF line table must contain first statement line 4, got: {:?}",
        recorded_lines
    );
    // target_statement is at line 5
    assert!(
        recorded_lines.contains(&5),
        "DWARF line table must contain target_statement line 5, got: {:?}",
        recorded_lines
    );
    // return is at line 6
    assert!(
        recorded_lines.contains(&6),
        "DWARF line table must contain return statement line 6, got: {:?}",
        recorded_lines
    );

    // 3. Inspect .debug_str / .debug_info to confirm symbol naming in DWARF
    let str_sec = elf_obj
        .section_by_name(".debug_str")
        .expect(".debug_str section must exist");
    let str_data = str_sec.data().expect("String data readable");
    let str_content = String::from_utf8_lossy(str_data);
    assert!(
        str_content.contains("compute_exact"),
        ".debug_str must contain function name 'compute_exact'"
    );
}
