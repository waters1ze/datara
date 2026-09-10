use forgen::codegen::cranelift::RealCraneliftBackend;
use forgen::codegen::target::TargetInfo;
use forgen::driver::ForgenCompiler;
use object::{Object, ObjectSection};

#[test]
fn test_dwarf_line_info_emission_and_parsing() {
    let source = r#"
pub fn add(a: Int, b: Int) -> Int {
    let sum = a + b
    return sum
}

pub fn multiply(x: Int, y: Int) -> Int {
    let prod = x * y
    return prod
}

pub fn main() {
    let a = add(10, 20)
    let b = multiply(a, 2)
    out b
}
"#;

    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "calc_debug.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let dmir = res.dmir_module.expect("DMIR module must be present");
    println!(
        "DMIR functions present: {:?}",
        dmir.functions.keys().collect::<Vec<_>>()
    );

    // 1. Verify DMIR has captured source spans
    assert!(
        dmir.function_spans.contains_key("add"),
        "Function span for 'add' must be recorded"
    );
    assert!(
        dmir.function_spans.contains_key("multiply"),
        "Function span for 'multiply' must be recorded"
    );
    assert!(
        dmir.function_spans.contains_key("main"),
        "Function span for 'main' must be recorded"
    );

    let add_span = &dmir.function_spans["add"];
    assert_eq!(add_span.start_line, 2, "add starts at line 2");

    let mult_span = &dmir.function_spans["multiply"];
    assert_eq!(mult_span.start_line, 7, "multiply starts at line 7");

    let main_span = &dmir.function_spans["main"];
    assert_eq!(main_span.start_line, 12, "main starts at line 12");

    // Verify statement line spans exist for functions
    assert!(
        dmir.function_line_spans
            .get("add")
            .map(|v| !v.is_empty())
            .unwrap_or(false),
        "Line spans for 'add' must be non-empty"
    );
    assert!(
        dmir.function_line_spans
            .get("multiply")
            .map(|v| !v.is_empty())
            .unwrap_or(false),
        "Line spans for 'multiply' must be non-empty"
    );
    assert!(
        dmir.function_line_spans
            .get("main")
            .map(|v| !v.is_empty())
            .unwrap_or(false),
        "Line spans for 'main' must be non-empty"
    );

    // 2. Compile to Linux ELF object bytes
    let linux_backend = RealCraneliftBackend::new(TargetInfo::x86_64_linux());
    let elf_bytes = linux_backend
        .compile_to_object_bytes(&dmir)
        .expect("ELF object compilation must succeed");

    assert!(!elf_bytes.is_empty(), "Object bytes must not be empty");

    // Parse with object crate
    let elf_obj = object::File::parse(&*elf_bytes).expect("Must parse as valid object file");

    let section_names: Vec<String> = elf_obj
        .sections()
        .filter_map(|s| s.name().ok().map(|n| n.to_string()))
        .collect();

    println!("ELF sections generated: {:?}", section_names);

    // Verify all four DWARF sections are embedded
    let has_debug_line = section_names.iter().any(|n| n.contains("debug_line"));
    let has_debug_info = section_names.iter().any(|n| n.contains("debug_info"));
    let has_debug_abbrev = section_names.iter().any(|n| n.contains("debug_abbrev"));
    let has_debug_str = section_names.iter().any(|n| n.contains("debug_str"));

    assert!(has_debug_line, "Must contain .debug_line section");
    assert!(has_debug_info, "Must contain .debug_info section");
    assert!(has_debug_abbrev, "Must contain .debug_abbrev section");
    assert!(has_debug_str, "Must contain .debug_str section");

    // 3. Inspect line table with gimli
    let debug_line_sec = elf_obj
        .section_by_name(".debug_line")
        .expect(".debug_line section must be accessible");
    let line_data = debug_line_sec.data().expect("Line data must be readable");
    assert!(!line_data.is_empty(), ".debug_line data must not be empty");

    let debug_line = gimli::DebugLine::new(line_data, gimli::LittleEndian);

    let program = debug_line
        .program(gimli::DebugLineOffset(0), 8, None, None)
        .expect("Line program header must parse");

    let header = program.header();
    let file_names: Vec<String> = header
        .file_names()
        .iter()
        .filter_map(|f| {
            if let gimli::AttributeValue::String(bytes) = f.path_name() {
                Some(String::from_utf8_lossy(&bytes).to_string())
            } else {
                None
            }
        })
        .collect();

    println!("DWARF file names: {:?}", file_names);
    assert!(
        file_names.iter().any(|f| f.contains("calc_debug.dtr")),
        "File table must contain 'calc_debug.dtr', found: {:?}",
        file_names
    );

    // Run the line number program and collect emitted rows
    let mut rows = program.rows();
    let mut recorded_lines = Vec::new();
    while let Ok(Some((_, row))) = rows.next_row() {
        if let Some(line) = row.line() {
            recorded_lines.push(line.get());
        }
    }
    println!("DWARF recorded lines: {:?}", recorded_lines);
    assert!(
        recorded_lines.contains(&2),
        "DWARF line rows must contain line 2 (add), found: {:?}",
        recorded_lines
    );
    assert!(
        recorded_lines.contains(&7),
        "DWARF line rows must contain line 7 (multiply), found: {:?}",
        recorded_lines
    );
    assert!(
        recorded_lines.contains(&12),
        "DWARF line rows must contain line 12 (main), found: {:?}",
        recorded_lines
    );

    // 4. Also verify Windows COFF object generation
    let win_backend = RealCraneliftBackend::new(TargetInfo::x86_64_windows());
    let coff_bytes = win_backend
        .compile_to_object_bytes(&dmir)
        .expect("COFF object compilation must succeed");
    let coff_obj = object::File::parse(&*coff_bytes).expect("Must parse as valid COFF object file");

    let coff_sections: Vec<String> = coff_obj
        .sections()
        .filter_map(|s| s.name().ok().map(|n| n.to_string()))
        .collect();

    println!("COFF sections generated: {:?}", coff_sections);
    assert!(
        coff_sections.iter().any(|n| n.contains("debug_line")),
        "COFF must contain debug_line section, found: {:?}",
        coff_sections
    );
    assert!(
        coff_sections.iter().any(|n| n.contains("debug_info")),
        "COFF must contain debug_info section, found: {:?}",
        coff_sections
    );
}

#[test]
fn test_dwarf_debug_info_subprograms_and_classes() {
    let source = r#"
class Calculator {
    base: Int

    calc(x: Int) -> Int {
        let res = this.base + x
        return res
    }
}

fn main() {
    let c = Calculator { base: 100 }
    let val = c.calc(50)
    out val
}
"#;

    let compiler = ForgenCompiler::new("debug");
    let res = compiler.compile_source(source, "calc_class.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.error);

    let dmir = res.dmir_module.expect("DMIR module must be present");
    let linux_backend = RealCraneliftBackend::new(TargetInfo::x86_64_linux());
    let elf_bytes = linux_backend
        .compile_to_object_bytes(&dmir)
        .expect("ELF object compilation must succeed");

    let elf_obj = object::File::parse(&*elf_bytes).expect("Must parse as valid object file");

    let debug_info_sec = elf_obj
        .section_by_name(".debug_info")
        .expect(".debug_info section must be accessible");
    let info_data = debug_info_sec.data().expect("Info data must be readable");

    let debug_abbrev_sec = elf_obj
        .section_by_name(".debug_abbrev")
        .expect(".debug_abbrev section must be accessible");
    let abbrev_data = debug_abbrev_sec
        .data()
        .expect("Abbrev data must be readable");

    let debug_str_sec = elf_obj
        .section_by_name(".debug_str")
        .expect(".debug_str section must be accessible");
    let str_data = debug_str_sec.data().expect("Str data must be readable");

    let debug_info = gimli::DebugInfo::new(info_data, gimli::LittleEndian);
    let debug_abbrev = gimli::DebugAbbrev::new(abbrev_data, gimli::LittleEndian);
    let debug_str = gimli::DebugStr::new(str_data, gimli::LittleEndian);

    let mut units = debug_info.units();
    let unit_header = units
        .next()
        .expect("Must have unit")
        .expect("Unit must parse");
    let abbrevs = unit_header
        .abbreviations(&debug_abbrev)
        .expect("Abbrevs must parse");

    let mut entries = unit_header.entries(&abbrevs);
    let root_entry = entries
        .next_dfs()
        .expect("DFS")
        .expect("Root DIE must exist");
    assert_eq!(root_entry.tag(), gimli::DW_TAG_compile_unit);

    let mut subprogram_names = Vec::new();
    while let Ok(Some(entry)) = entries.next_dfs() {
        if entry.tag() == gimli::DW_TAG_subprogram {
            if let Some(attr) = entry.attr(gimli::DW_AT_name) {
                if let Some(s) = attr.string_value(&debug_str) {
                    subprogram_names.push(String::from_utf8_lossy(s.slice()).to_string());
                }
            }
        }
    }

    println!("Subprogram DIE names found: {:?}", subprogram_names);
    assert!(
        subprogram_names.contains(&"Calculator_calc".to_string()),
        "Must contain subprogram 'Calculator_calc', found: {:?}",
        subprogram_names
    );
    assert!(
        subprogram_names.contains(&"main".to_string()),
        "Must contain subprogram 'main', found: {:?}",
        subprogram_names
    );
}
