use std::collections::HashMap;
use std::path::{Path, PathBuf};

use cranelift_codegen::entity::SecondaryMap;
use cranelift_module::FuncId;
use gimli::write::{
    Address, AttributeValue, DirectoryId, Dwarf, EndianVec, FileId, LineProgram, LineString,
    Sections, Unit,
};
use gimli::{
    DW_AT_comp_dir, DW_AT_decl_column, DW_AT_decl_file, DW_AT_decl_line, DW_AT_high_pc,
    DW_AT_low_pc, DW_AT_name, DW_AT_producer, DW_TAG_subprogram, Encoding, Format, LittleEndian,
};
use object::write::{Object, SectionKind, StandardSegment, SymbolId};

use crate::codegen::cranelift::backend::ModuleCompileArtifacts;
use crate::dmir::Module;

pub struct DwarfLineEmitter;

impl DwarfLineEmitter {
    /// Emits complete DWARF 4 line tables (.debug_line) and debug info (.debug_info,
    /// .debug_abbrev, .debug_str) into the Cranelift target object before emission.
    pub fn emit_dwarf_to_object(
        obj: &mut Object,
        dmir_module: &Module,
        artifacts: &ModuleCompileArtifacts,
        functions: &SecondaryMap<FuncId, Option<(SymbolId, bool)>>,
    ) -> Result<(), String> {
        let address_size = match obj.architecture() {
            object::Architecture::X86_64
            | object::Architecture::Aarch64
            | object::Architecture::Riscv64 => 8,
            _ => 4,
        };

        let encoding = Encoding {
            format: Format::Dwarf32,
            version: 4,
            address_size,
        };

        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let comp_dir_str = cwd.to_string_lossy().to_string();
        let fallback_file = format!("{}.dtr", dmir_module.name);

        let mut line_program = LineProgram::new(
            encoding,
            gimli::LineEncoding::default(),
            LineString::String(comp_dir_str.as_bytes().to_vec()),
            None,
            LineString::String(fallback_file.as_bytes().to_vec()),
            None,
        );
        let default_dir_id = line_program.default_directory();

        // Collect all unique source files referenced in module spans
        let mut file_ids: HashMap<String, FileId> = HashMap::new();
        fn register_file(
            file_ids: &mut HashMap<String, FileId>,
            path_str: &str,
            lp: &mut LineProgram,
            default_dir_id: DirectoryId,
        ) -> FileId {
            if let Some(&fid) = file_ids.get(path_str) {
                return fid;
            }
            let p = Path::new(path_str);
            let dir_id = if let Some(parent) = p.parent() {
                if !parent.as_os_str().is_empty() {
                    lp.add_directory(LineString::String(
                        parent.to_string_lossy().as_bytes().to_vec(),
                    ))
                } else {
                    default_dir_id
                }
            } else {
                default_dir_id
            };
            let file_name = p
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| path_str.to_string());
            let fid = lp.add_file(
                LineString::String(file_name.as_bytes().to_vec()),
                dir_id,
                None,
            );
            file_ids.insert(path_str.to_string(), fid);
            fid
        }

        for span in dmir_module.function_spans.values() {
            if !span.file.is_empty() {
                register_file(&mut file_ids, &span.file, &mut line_program, default_dir_id);
            }
        }
        for spans in dmir_module.function_line_spans.values() {
            for span in spans {
                if !span.file.is_empty() {
                    register_file(&mut file_ids, &span.file, &mut line_program, default_dir_id);
                }
            }
        }

        let default_fid = if file_ids.is_empty() {
            register_file(
                &mut file_ids,
                &fallback_file,
                &mut line_program,
                default_dir_id,
            )
        } else {
            *file_ids.values().next().unwrap()
        };

        let mut dwarf = Dwarf::new();
        let unit_id = dwarf.units.add(Unit::new(encoding, line_program));
        let unit = dwarf.units.get_mut(unit_id);
        let root = unit.root();

        let producer_id = dwarf.strings.add(b"Datara Forgen Cranelift AOT".to_vec());
        let unit_name_id = dwarf.strings.add(fallback_file.as_bytes().to_vec());
        let comp_dir_id = dwarf.strings.add(comp_dir_str.as_bytes().to_vec());

        // Populate compilation unit DIE
        unit.get_mut(root)
            .set(DW_AT_producer, AttributeValue::StringRef(producer_id));
        unit.get_mut(root)
            .set(DW_AT_name, AttributeValue::StringRef(unit_name_id));
        unit.get_mut(root)
            .set(DW_AT_comp_dir, AttributeValue::StringRef(comp_dir_id));
        unit.get_mut(root)
            .set(DW_AT_low_pc, AttributeValue::Address(Address::Constant(0)));

        // Collect only functions defined in this DMIR module (exclude runtime imports)
        let mut defined_funcs = Vec::new();
        for (fn_name, _) in &dmir_module.functions {
            if let Some(&func_id) = artifacts.func_ids.get(fn_name) {
                if let Some(Some((symbol_id, _))) = functions.get(func_id) {
                    let sym = obj.symbol(*symbol_id);
                    defined_funcs.push((fn_name, func_id, *symbol_id, sym.value, sym.size));
                }
            }
        }
        // Also include synthesized main entry if present
        if let Some(main_id) = artifacts.main_entry_id {
            if let Some(Some((symbol_id, _))) = functions.get(main_id) {
                let sym = obj.symbol(*symbol_id);
                // Only if not already present
                if !defined_funcs
                    .iter()
                    .any(|(_, fid, _, _, _)| *fid == main_id)
                {
                    static ENTRY_NAME: String = String::new();
                    defined_funcs.push((&ENTRY_NAME, main_id, *symbol_id, sym.value, sym.size));
                }
            }
        }

        defined_funcs.sort_by_key(|&(_, _, _, addr, _)| addr);

        for (fn_name, _func_id, _symbol_id, fn_addr, fn_size_raw) in defined_funcs {
            if fn_name.is_empty() {
                continue;
            }
            let fn_size = fn_size_raw.max(1);
            let fn_span = dmir_module.function_spans.get(fn_name);
            let line_spans = dmir_module.function_line_spans.get(fn_name);

            let fn_fid = fn_span
                .and_then(|sp| file_ids.get(&sp.file).copied())
                .unwrap_or(default_fid);
            let fn_start_line = fn_span.map(|sp| sp.start_line as u64).unwrap_or(1);
            let fn_start_col = fn_span.map(|sp| sp.start_col as u64).unwrap_or(1);

            // 1. Line Program sequence for this function
            unit.line_program
                .begin_sequence(Some(Address::Constant(fn_addr)));

            // Function header line row
            let row = unit.line_program.row();
            row.file = fn_fid;
            row.line = fn_start_line;
            row.column = fn_start_col;
            row.is_statement = true;
            unit.line_program.generate_row();

            // Distinct statement line rows
            if let Some(spans) = line_spans {
                let mut last_line = fn_start_line;
                for (idx, sp) in spans.iter().enumerate() {
                    let s_line = sp.start_line as u64;
                    if s_line != last_line && s_line > 0 {
                        // Distribute offset evenly across statements inside function
                        let stmt_offset = (fn_size * (idx + 1) as u64) / (spans.len() + 1) as u64;
                        let target_addr = fn_addr + stmt_offset.min(fn_size.saturating_sub(1));

                        let row = unit.line_program.row();
                        row.address_offset = target_addr;
                        row.file = file_ids.get(&sp.file).copied().unwrap_or(fn_fid);
                        row.line = s_line;
                        row.column = sp.start_col as u64;
                        row.is_statement = true;
                        unit.line_program.generate_row();
                        last_line = s_line;
                    }
                }
            }

            unit.line_program.end_sequence(fn_addr + fn_size);

            // 2. Add DW_TAG_subprogram DIE
            let fn_name_id = dwarf.strings.add(fn_name.as_bytes().to_vec());
            let sub = unit.add(root, DW_TAG_subprogram);
            let sub_entry = unit.get_mut(sub);
            sub_entry.set(DW_AT_name, AttributeValue::StringRef(fn_name_id));
            sub_entry.set(DW_AT_decl_file, AttributeValue::FileIndex(Some(fn_fid)));
            sub_entry.set(DW_AT_decl_line, AttributeValue::Udata(fn_start_line));
            sub_entry.set(DW_AT_decl_column, AttributeValue::Udata(fn_start_col));
            sub_entry.set(
                DW_AT_low_pc,
                AttributeValue::Address(Address::Constant(fn_addr)),
            );
            sub_entry.set(DW_AT_high_pc, AttributeValue::Udata(fn_size));
        }

        // 3. Serialize DWARF sections
        let mut sections = Sections::new(EndianVec::new(LittleEndian));
        dwarf
            .write(&mut sections)
            .map_err(|e| format!("DWARF serialization error: {:?}", e))?;

        // 4. Append non-empty DWARF sections to Object
        let debug_segment = obj.segment_name(StandardSegment::Debug).to_vec();
        sections
            .for_each(|id, data| {
                if !data.slice().is_empty() {
                    let sec_name = id.name();
                    let section_id = obj.add_section(
                        debug_segment.clone(),
                        sec_name.as_bytes().to_vec(),
                        SectionKind::Debug,
                    );
                    obj.append_section_data(section_id, data.slice(), 1);
                }
                Ok::<(), ()>(())
            })
            .map_err(|_| "Failed to write DWARF sections to object".to_string())?;

        Ok(())
    }
}
