use forgen::codegen::cranelift::RealCraneliftBackend;
use forgen::codegen::target::TargetInfo;
use forgen::driver::ForgenCompiler;
use object::{Object, ObjectSection};

#[test]
fn audit_dwarf_debug_line_and_info_validity_and_exact_mapping() {
    let source = "// Line 1\n// Line 2\npub fn audited_dwarf(x: Int) -> Int {\n    let val1 = x + 10\n    let val2 = val1 * 3\n    return val2\n}\npub fn main() {\n    out audited_dwarf(5)\n}\n";

    let compiler = ForgenCompiler::new("debug").with_debug(true);
    let res = compiler.compile_source(source, "audit_dwarf.dtr", None);
    assert!(res.success, "Debug compilation must succeed");

    let dmir = res.dmir_module.expect("DMIR module");
    let linux_backend = RealCraneliftBackend::new(TargetInfo::x86_64_linux());
    let elf_bytes = linux_backend
        .compile_to_object_bytes(&dmir)
        .expect("ELF object compilation must succeed");

    let obj = object::File::parse(&*elf_bytes).expect("Parsed ELF object file");

    // 1. .debug_line must exist and be readable
    let line_sec = obj
        .section_by_name(".debug_line")
        .expect(".debug_line section must exist");
    let line_data = line_sec.data().expect("Readable line data");
    assert!(!line_data.is_empty(), ".debug_line must not be empty");

    // 2. .debug_info must exist and be readable
    let info_sec = obj
        .section_by_name(".debug_info")
        .expect(".debug_info section must exist");
    let info_data = info_sec.data().expect("Readable info data");
    assert!(!info_data.is_empty(), ".debug_info must not be empty");

    // 3. Exact line-mapping test using gimli
    let debug_line = gimli::DebugLine::new(line_data, gimli::LittleEndian);
    let program = debug_line
        .program(gimli::DebugLineOffset(0), 8, None, None)
        .expect("Program header valid");

    let mut rows = program.rows();
    let mut mapped_lines = Vec::new();
    while let Ok(Some((_, row))) = rows.next_row() {
        if let Some(l) = row.line() {
            mapped_lines.push(l.get());
        }
    }

    assert!(
        mapped_lines.contains(&3),
        "DWARF must map function decl line 3: {:?}",
        mapped_lines
    );
    assert!(
        mapped_lines.contains(&4),
        "DWARF must map statement line 4: {:?}",
        mapped_lines
    );
    assert!(
        mapped_lines.contains(&5),
        "DWARF must map statement line 5: {:?}",
        mapped_lines
    );
    assert!(
        mapped_lines.contains(&6),
        "DWARF must map return line 6: {:?}",
        mapped_lines
    );
}
