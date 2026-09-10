pub mod lexer;
pub mod parser;
pub mod types;

use crate::ast::*;
use crate::cimport::lexer::CLexer;
use crate::cimport::parser::CParser;
use crate::cimport::types::*;
use crate::diagnostics::{DiagnosticEngine, ErrorCode, SourceSpan};
use std::path::{Path, PathBuf};

pub fn expand_c_imports(
    program: &mut Program,
    base_dir: Option<&Path>,
    diag: &mut DiagnosticEngine,
) {
    let mut new_declarations = Vec::new();
    let mut extra_libs = Vec::new();

    for decl in program.declarations.drain(..) {
        match decl {
            Decl::CImport(cimport) => {
                // Record link libraries
                for lib in &cimport.link_libs {
                    if !extra_libs.contains(lib) {
                        extra_libs.push(lib.clone());
                    }
                }

                // Resolve header path
                let header_path_buf = PathBuf::from(&cimport.header_path);
                let resolved_path = if header_path_buf.is_absolute() && header_path_buf.exists() {
                    Some(header_path_buf.clone())
                } else if let Some(base) = base_dir {
                    let cand = base.join(&cimport.header_path);
                    if cand.exists() { Some(cand) } else { None }
                } else {
                    None
                };

                let resolved_path = resolved_path.or_else(|| {
                    if let Ok(cwd) = std::env::current_dir() {
                        let cand = cwd.join(&cimport.header_path);
                        if cand.exists() {
                            return Some(cand);
                        }
                    }
                    if header_path_buf.exists() {
                        return Some(header_path_buf.clone());
                    }
                    None
                });

                let header_file_path = match resolved_path {
                    Some(p) => p,
                    None => {
                        diag.error(
                            ErrorCode::CImportHeaderNotFound,
                            format!(
                                "C header file '{}' not found (searched relative to source directory and working directory)",
                                cimport.header_path
                            ),
                            Some(cimport.span.clone()),
                        );
                        continue;
                    }
                };

                let header_content = match std::fs::read_to_string(&header_file_path) {
                    Ok(s) => s,
                    Err(e) => {
                        diag.error(
                            ErrorCode::CImportReadFailed,
                            format!(
                                "Failed to read C header file '{}': {}",
                                header_file_path.display(),
                                e
                            ),
                            Some(cimport.span.clone()),
                        );
                        continue;
                    }
                };

                let header_str_path = header_file_path.to_string_lossy().to_string();
                let mut lexer = CLexer::new(&header_content);
                let tokens = lexer.tokenize();
                let mut parser = CParser::new(tokens);
                let c_decls = parser.parse();

                for parser_diag in parser.diagnostics {
                    diag.warning(
                        ErrorCode::CImportUnsupportedConstruct,
                        parser_diag,
                        Some(cimport.span.clone()),
                    );
                }

                for c_decl in c_decls {
                    match c_decl {
                        CDecl::Function(cf) => {
                            let f_span = SourceSpan::new(
                                cf.line,
                                cf.col,
                                cf.line,
                                cf.col + cf.name.len(),
                                header_str_path.clone(),
                            );
                            let params: Vec<Param> = cf
                                .params
                                .iter()
                                .map(|p| {
                                    let p_span = SourceSpan::new(
                                        p.line,
                                        p.col,
                                        p.line,
                                        p.col + p.name.len(),
                                        header_str_path.clone(),
                                    );
                                    Param {
                                        name: p.name.clone(),
                                        type_node: p.ty.to_datara_type_node(
                                            &header_str_path,
                                            p.line,
                                            p.col,
                                        ),
                                        ownership_mode: "val".into(),
                                        span: p_span,
                                    }
                                })
                                .collect();

                            let return_type = cf.return_type.to_datara_type_node(
                                &header_str_path,
                                cf.line,
                                cf.col,
                            );

                            new_declarations.push(Decl::ExternFn(ExternFnDecl {
                                abi: "C".into(),
                                name: cf.name,
                                params,
                                return_type,
                                span: f_span,
                            }));
                        }
                        CDecl::Enum(ce) => {
                            let e_span = SourceSpan::new(
                                ce.line,
                                ce.col,
                                ce.line,
                                ce.col + 4,
                                header_str_path.clone(),
                            );
                            if let Some(enum_name) = ce.name {
                                if !new_declarations.iter().any(|d| match d {
                                    Decl::Type(t) => t.name == enum_name,
                                    Decl::Class(c) => c.name == enum_name,
                                    _ => false,
                                }) {
                                    new_declarations.push(Decl::Type(TypeDecl {
                                        name: enum_name,
                                        base_type: TypeNode::new("Int", e_span.clone()),
                                        is_export: true,
                                        span: e_span.clone(),
                                    }));
                                }
                            }

                            // Emit integer functions for each variant value for ergonomic constant access
                            for v in &ce.variants {
                                if !new_declarations.iter().any(|d| match d {
                                    Decl::Function(f) => f.name == v.name,
                                    _ => false,
                                }) {
                                    let v_span = SourceSpan::new(
                                        v.line,
                                        v.col,
                                        v.line,
                                        v.col + v.name.len(),
                                        header_str_path.clone(),
                                    );
                                    new_declarations.push(Decl::Function(FunctionDecl {
                                        name: v.name.clone(),
                                        attributes: Vec::new(),
                                        generic_params: Vec::new(),
                                        generic_constraints: Vec::new(),
                                        params: Vec::new(),
                                        return_type: Some(TypeNode::new("Int", v_span.clone())),
                                        requires: Vec::new(),
                                        ensures: Vec::new(),
                                        decreases: None,
                                        body: Box::new(Stmt::Return(
                                            Some(Expr::Literal(
                                                LiteralValue::Int(v.value),
                                                v_span.clone(),
                                            )),
                                            v_span.clone(),
                                        )),
                                        is_expression_body: true,
                                        is_export: true,
                                        span: v_span,
                                    }));
                                }
                            }
                        }
                        CDecl::DefineInt(cd) => {
                            let d_span = SourceSpan::new(
                                cd.line,
                                cd.col,
                                cd.line,
                                cd.col + cd.name.len(),
                                header_str_path.clone(),
                            );
                            new_declarations.push(Decl::Function(FunctionDecl {
                                name: cd.name,
                                attributes: Vec::new(),
                                generic_params: Vec::new(),
                                generic_constraints: Vec::new(),
                                params: Vec::new(),
                                return_type: Some(TypeNode::new("Int", d_span.clone())),
                                requires: Vec::new(),
                                ensures: Vec::new(),
                                decreases: None,
                                body: Box::new(Stmt::Return(
                                    Some(Expr::Literal(
                                        LiteralValue::Int(cd.value),
                                        d_span.clone(),
                                    )),
                                    d_span.clone(),
                                )),
                                is_expression_body: true,
                                is_export: true,
                                span: d_span,
                            }));
                        }
                        CDecl::OpaqueStruct(cs) => {
                            if !new_declarations.iter().any(|d| match d {
                                Decl::Type(t) => t.name == cs.name,
                                Decl::Class(c) => c.name == cs.name,
                                _ => false,
                            }) {
                                let s_span = SourceSpan::new(
                                    cs.line,
                                    cs.col,
                                    cs.line,
                                    cs.col + cs.name.len(),
                                    header_str_path.clone(),
                                );
                                new_declarations.push(Decl::Type(TypeDecl {
                                    name: cs.name,
                                    base_type: TypeNode::new("RawPtr", s_span.clone()),
                                    is_export: true,
                                    span: s_span,
                                }));
                            }
                        }
                        CDecl::Struct(cs) => {
                            if !new_declarations.iter().any(|d| match d {
                                Decl::Type(t) => t.name == cs.name,
                                Decl::Class(c) => c.name == cs.name,
                                _ => false,
                            }) {
                                let s_span = SourceSpan::new(
                                    cs.line,
                                    cs.col,
                                    cs.line,
                                    cs.col + cs.name.len(),
                                    header_str_path.clone(),
                                );
                                let mut body_items = Vec::new();
                                for f in &cs.fields {
                                    let f_span = SourceSpan::new(
                                        f.line,
                                        f.col,
                                        f.line,
                                        f.col + f.name.len(),
                                        header_str_path.clone(),
                                    );
                                    let type_node =
                                        f.ty.to_datara_type_node(&header_str_path, f.line, f.col);
                                    body_items.push(ClassItem::Field(FieldDecl {
                                        name: f.name.clone(),
                                        type_node,
                                        bit_field: None,
                                        default_value: None,
                                        is_mut: false,
                                        span: f_span,
                                    }));
                                }
                                new_declarations.push(Decl::Class(ClassDecl {
                                    name: cs.name.clone(),
                                    attributes: Vec::new(),
                                    generic_params: Vec::new(),
                                    base_type: None,
                                    compositions: Vec::new(),
                                    body_items,
                                    invariants: Vec::new(),
                                    is_export: true,
                                    span: s_span,
                                }));
                            }
                        }
                        CDecl::Typedef(ct) => {
                            if !new_declarations.iter().any(|d| match d {
                                Decl::Type(t) => t.name == ct.name,
                                Decl::Class(c) => c.name == ct.name,
                                _ => false,
                            }) {
                                let t_span = SourceSpan::new(
                                    ct.line,
                                    ct.col,
                                    ct.line,
                                    ct.col + ct.name.len(),
                                    header_str_path.clone(),
                                );
                                if let Some(base_type) =
                                    ct.target
                                        .to_datara_type_node(&header_str_path, ct.line, ct.col)
                                {
                                    new_declarations.push(Decl::Type(TypeDecl {
                                        name: ct.name,
                                        base_type,
                                        is_export: true,
                                        span: t_span,
                                    }));
                                }
                            }
                        }
                    }
                }
            }
            other => new_declarations.push(other),
        }
    }

    program.declarations = new_declarations;
    for lib in extra_libs {
        if !program.link_libraries.contains(&lib) {
            program.link_libraries.push(lib);
        }
    }
}
