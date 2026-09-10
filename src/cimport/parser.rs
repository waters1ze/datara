use crate::cimport::lexer::{CToken, CTokenKind};
use crate::cimport::types::*;
use std::collections::HashMap;

pub struct CParser {
    tokens: Vec<CToken>,
    pos: usize,
    pub diagnostics: Vec<String>,
    typedefs: HashMap<String, CType>,
}

impl CParser {
    pub fn new(tokens: Vec<CToken>) -> Self {
        let mut typedefs = HashMap::new();
        // Standard C types
        typedefs.insert(
            "int8_t".into(),
            CType::Int {
                name: "int8_t".into(),
                bits: 8,
                is_signed: true,
            },
        );
        typedefs.insert(
            "uint8_t".into(),
            CType::Int {
                name: "uint8_t".into(),
                bits: 8,
                is_signed: false,
            },
        );
        typedefs.insert(
            "int16_t".into(),
            CType::Int {
                name: "int16_t".into(),
                bits: 16,
                is_signed: true,
            },
        );
        typedefs.insert(
            "uint16_t".into(),
            CType::Int {
                name: "uint16_t".into(),
                bits: 16,
                is_signed: false,
            },
        );
        typedefs.insert(
            "int32_t".into(),
            CType::Int {
                name: "int32_t".into(),
                bits: 32,
                is_signed: true,
            },
        );
        typedefs.insert(
            "uint32_t".into(),
            CType::Int {
                name: "uint32_t".into(),
                bits: 32,
                is_signed: false,
            },
        );
        typedefs.insert(
            "int64_t".into(),
            CType::Int {
                name: "int64_t".into(),
                bits: 64,
                is_signed: true,
            },
        );
        typedefs.insert(
            "uint64_t".into(),
            CType::Int {
                name: "uint64_t".into(),
                bits: 64,
                is_signed: false,
            },
        );
        typedefs.insert(
            "size_t".into(),
            CType::Int {
                name: "size_t".into(),
                bits: 64,
                is_signed: false,
            },
        );
        typedefs.insert(
            "ssize_t".into(),
            CType::Int {
                name: "ssize_t".into(),
                bits: 64,
                is_signed: true,
            },
        );
        typedefs.insert(
            "intptr_t".into(),
            CType::Int {
                name: "intptr_t".into(),
                bits: 64,
                is_signed: true,
            },
        );
        typedefs.insert(
            "uintptr_t".into(),
            CType::Int {
                name: "uintptr_t".into(),
                bits: 64,
                is_signed: false,
            },
        );

        Self {
            tokens,
            pos: 0,
            diagnostics: Vec::new(),
            typedefs,
        }
    }

    fn peek(&self) -> &CToken {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek_kind(&self) -> &CTokenKind {
        &self.peek().kind
    }

    fn peek_next(&self) -> &CToken {
        let next_idx = (self.pos + 1).min(self.tokens.len() - 1);
        &self.tokens[next_idx]
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len() || matches!(self.peek_kind(), CTokenKind::Eof)
    }

    fn advance(&mut self) -> &CToken {
        if !self.is_at_end() {
            self.pos += 1;
        }
        &self.tokens[self.pos - 1]
    }

    fn skip_newlines(&mut self) {
        while !self.is_at_end() && matches!(self.peek_kind(), CTokenKind::Newline) {
            self.advance();
        }
    }

    fn sync_to_semicolon(&mut self) {
        while !self.is_at_end() {
            if matches!(self.peek_kind(), CTokenKind::Semicolon) {
                self.advance();
                break;
            }
            if matches!(self.peek_kind(), CTokenKind::Newline) {
                self.advance();
                break;
            }
            self.advance();
        }
    }

    pub fn parse(&mut self) -> Vec<CDecl> {
        let mut decls = Vec::new();

        while !self.is_at_end() {
            self.skip_newlines();
            if self.is_at_end() {
                break;
            }

            // Preprocessor directive
            if matches!(self.peek_kind(), CTokenKind::Hash) {
                if let Some(decl) = self.parse_preprocessor() {
                    decls.push(decl);
                }
                continue;
            }

            // Typedef
            if matches!(self.peek_kind(), CTokenKind::Typedef) {
                if let Some(decl) = self.parse_typedef() {
                    decls.push(decl);
                }
                continue;
            }

            // Enum declaration
            if matches!(self.peek_kind(), CTokenKind::Enum) {
                if let Some(decl) = self.parse_enum() {
                    decls.push(decl);
                }
                continue;
            }

            // Struct forward declaration
            if matches!(self.peek_kind(), CTokenKind::Struct) {
                if let Some(decl) = self.parse_struct_decl() {
                    decls.push(decl);
                }
                continue;
            }

            // Function declaration (or unexpected token)
            if let Some(decl) = self.parse_function_decl() {
                decls.push(decl);
            } else {
                let tok = self.peek().clone();
                if !matches!(
                    tok.kind,
                    CTokenKind::Newline | CTokenKind::Semicolon | CTokenKind::Eof
                ) {
                    self.diagnostics.push(format!(
                        "Unsupported C construct at line {}, col {}: unexpected token {:?}",
                        tok.line, tok.col, tok.kind
                    ));
                    self.sync_to_semicolon();
                } else {
                    self.advance();
                }
            }
        }

        decls
    }

    fn parse_preprocessor(&mut self) -> Option<CDecl> {
        self.advance(); // consume '#'
        let tok = self.peek().clone();
        match &tok.kind {
            CTokenKind::Ident(name) if name == "define" => {
                self.advance(); // consume 'define'
                let id_tok = self.peek().clone();
                if let CTokenKind::Ident(def_name) = &id_tok.kind {
                    let def_name = def_name.clone();
                    self.advance();
                    // Check if followed by integer value
                    let val = self.parse_preprocessor_int_expr();
                    // consume until newline
                    while !self.is_at_end() && !matches!(self.peek_kind(), CTokenKind::Newline) {
                        self.advance();
                    }
                    if let Some(int_val) = val {
                        return Some(CDecl::DefineInt(CDefineInt {
                            name: def_name,
                            value: int_val,
                            line: id_tok.line,
                            col: id_tok.col,
                        }));
                    }
                }
            }
            CTokenKind::Ident(name) if name == "ifdef" || name == "ifndef" || name == "if" => {
                self.advance();
                // If it's `#if 0`, skip to matching `#endif`
                let is_zero = matches!(self.peek_kind(), CTokenKind::IntLit(0));
                while !self.is_at_end() && !matches!(self.peek_kind(), CTokenKind::Newline) {
                    self.advance();
                }
                if is_zero {
                    let mut depth = 1;
                    while !self.is_at_end() && depth > 0 {
                        if matches!(self.peek_kind(), CTokenKind::Hash) {
                            self.advance();
                            if let CTokenKind::Ident(dir) = self.peek_kind() {
                                if dir == "if" || dir == "ifdef" || dir == "ifndef" {
                                    depth += 1;
                                } else if dir == "endif" {
                                    depth -= 1;
                                }
                            }
                        } else {
                            self.advance();
                        }
                    }
                }
            }
            _ => {
                // Skip unhandled directive until newline
                while !self.is_at_end() && !matches!(self.peek_kind(), CTokenKind::Newline) {
                    self.advance();
                }
            }
        }
        None
    }

    fn parse_preprocessor_int_expr(&mut self) -> Option<i64> {
        let is_neg = if matches!(self.peek_kind(), CTokenKind::Minus) {
            self.advance();
            true
        } else {
            false
        };

        if matches!(self.peek_kind(), CTokenKind::LParen) {
            self.advance();
            let val = self.parse_preprocessor_int_expr();
            if matches!(self.peek_kind(), CTokenKind::RParen) {
                self.advance();
            }
            return val.map(|v| if is_neg { -v } else { v });
        }

        if let CTokenKind::IntLit(n) = self.peek_kind() {
            let n = *n;
            self.advance();
            return Some(if is_neg { -n } else { n });
        }

        None
    }

    fn parse_enum(&mut self) -> Option<CDecl> {
        let enum_tok = self.advance().clone(); // consume 'enum'
        let mut name = None;
        if let CTokenKind::Ident(id) = self.peek_kind() {
            name = Some(id.clone());
            self.advance();
        }

        self.skip_newlines();
        if !matches!(self.peek_kind(), CTokenKind::LBrace) {
            return None;
        }
        self.advance(); // consume '{'

        let mut variants = Vec::new();
        let mut current_val: i64 = 0;

        while !self.is_at_end() && !matches!(self.peek_kind(), CTokenKind::RBrace) {
            self.skip_newlines();
            if matches!(self.peek_kind(), CTokenKind::RBrace) {
                break;
            }

            let var_tok = self.peek().clone();
            if let CTokenKind::Ident(var_name) = &var_tok.kind {
                let var_name = var_name.clone();
                self.advance();
                if matches!(self.peek_kind(), CTokenKind::Equal) {
                    self.advance();
                    if let Some(val) = self.parse_preprocessor_int_expr() {
                        current_val = val;
                    }
                }
                variants.push(CEnumVariant {
                    name: var_name,
                    value: current_val,
                    line: var_tok.line,
                    col: var_tok.col,
                });
                current_val += 1;
            }

            self.skip_newlines();
            if matches!(self.peek_kind(), CTokenKind::Comma) {
                self.advance();
            } else {
                self.skip_newlines();
                if !matches!(self.peek_kind(), CTokenKind::RBrace) {
                    break;
                }
            }
        }

        self.skip_newlines();
        if matches!(self.peek_kind(), CTokenKind::RBrace) {
            self.advance();
        }
        self.skip_newlines();
        if matches!(self.peek_kind(), CTokenKind::Semicolon) {
            self.advance();
        }

        Some(CDecl::Enum(CEnum {
            name,
            variants,
            line: enum_tok.line,
            col: enum_tok.col,
        }))
    }

    fn parse_struct_fields(&mut self) -> Option<Vec<CField>> {
        self.skip_newlines();
        if !matches!(self.peek_kind(), CTokenKind::LBrace) {
            return None;
        }
        self.advance(); // consume '{'

        let mut fields = Vec::new();
        while !self.is_at_end() && !matches!(self.peek_kind(), CTokenKind::RBrace) {
            self.skip_newlines();
            if matches!(self.peek_kind(), CTokenKind::RBrace) {
                break;
            }

            let field_line = self.peek().line;
            let field_col = self.peek().col;
            let f_type = match self.parse_type() {
                Some(t) => t,
                None => {
                    self.sync_to_semicolon();
                    continue;
                }
            };

            let f_name = if let CTokenKind::Ident(id) = self.peek_kind() {
                let id = id.clone();
                self.advance();
                id
            } else {
                format!("field_{}", fields.len())
            };

            fields.push(CField {
                name: f_name,
                ty: f_type,
                line: field_line,
                col: field_col,
            });

            self.skip_newlines();
            if matches!(self.peek_kind(), CTokenKind::Semicolon) {
                self.advance();
            }
        }

        self.skip_newlines();
        if matches!(self.peek_kind(), CTokenKind::RBrace) {
            self.advance();
        }

        Some(fields)
    }

    fn parse_struct_decl(&mut self) -> Option<CDecl> {
        let struct_tok = self.advance().clone(); // consume 'struct'
        if let CTokenKind::Ident(name) = self.peek_kind() {
            let name = name.clone();
            self.advance();
            if matches!(self.peek_kind(), CTokenKind::Semicolon) {
                self.advance();
                return Some(CDecl::OpaqueStruct(COpaqueStruct {
                    name,
                    line: struct_tok.line,
                    col: struct_tok.col,
                }));
            }
            if matches!(self.peek_kind(), CTokenKind::LBrace) {
                if let Some(fields) = self.parse_struct_fields() {
                    self.skip_newlines();
                    if matches!(self.peek_kind(), CTokenKind::Semicolon) {
                        self.advance();
                    }
                    self.typedefs
                        .insert(name.clone(), CType::Named(name.clone()));
                    return Some(CDecl::Struct(CStruct {
                        name,
                        fields,
                        line: struct_tok.line,
                        col: struct_tok.col,
                    }));
                }
            }
        }
        self.sync_to_semicolon();
        None
    }

    fn parse_typedef(&mut self) -> Option<CDecl> {
        let typedef_tok = self.advance().clone(); // consume 'typedef'

        // Check for `typedef struct [Tag] { ... } Name;` or `typedef struct X X;`
        if matches!(self.peek_kind(), CTokenKind::Struct) {
            self.advance(); // consume 'struct'
            let mut tag_name = None;
            if let CTokenKind::Ident(tag) = self.peek_kind() {
                tag_name = Some(tag.clone());
                self.advance();
            }
            // Check for opaque struct: `typedef struct X X;`
            if tag_name.is_some() && matches!(self.peek_kind(), CTokenKind::Ident(_)) {
                if let CTokenKind::Ident(alias_name) = self.peek_kind() {
                    let alias_name = alias_name.clone();
                    self.advance();
                    if matches!(self.peek_kind(), CTokenKind::Semicolon) {
                        self.advance();
                        self.typedefs.insert(
                            alias_name.clone(),
                            CType::RawPtr {
                                pointee: Some(alias_name.clone()),
                            },
                        );
                        return Some(CDecl::OpaqueStruct(COpaqueStruct {
                            name: alias_name,
                            line: typedef_tok.line,
                            col: typedef_tok.col,
                        }));
                    }
                }
            }
            // Check for struct with body: `typedef struct [Tag] { ... } Name;`
            if matches!(self.peek_kind(), CTokenKind::LBrace) {
                if let Some(fields) = self.parse_struct_fields() {
                    self.skip_newlines();
                    if let CTokenKind::Ident(alias_name) = self.peek_kind() {
                        let alias_name = alias_name.clone();
                        self.advance();
                        self.skip_newlines();
                        if matches!(self.peek_kind(), CTokenKind::Semicolon) {
                            self.advance();
                        }
                        self.typedefs
                            .insert(alias_name.clone(), CType::Named(alias_name.clone()));
                        if let Some(tag) = tag_name {
                            self.typedefs.insert(tag, CType::Named(alias_name.clone()));
                        }
                        return Some(CDecl::Struct(CStruct {
                            name: alias_name,
                            fields,
                            line: typedef_tok.line,
                            col: typedef_tok.col,
                        }));
                    }
                }
            }
        }

        // Check for `typedef enum { ... } Name;`
        if matches!(self.peek_kind(), CTokenKind::Enum) {
            if let Some(CDecl::Enum(mut e)) = self.parse_enum() {
                if let CTokenKind::Ident(alias_name) = self.peek_kind() {
                    let alias_name = alias_name.clone();
                    self.advance();
                    if matches!(self.peek_kind(), CTokenKind::Semicolon) {
                        self.advance();
                    }
                    e.name = Some(alias_name);
                    return Some(CDecl::Enum(e));
                }
            }
        }

        // Function pointer typedef: `typedef int64_t (*callback_fn)(int64_t);`
        // or scalar / pointer typedef: `typedef int my_int_t;`
        if let Some(target_type) = self.parse_type() {
            if matches!(self.peek_kind(), CTokenKind::LParen)
                && matches!(self.peek_next().kind, CTokenKind::Star)
            {
                self.advance(); // consume '('
                self.advance(); // consume '*'
                let fn_name = if let CTokenKind::Ident(id) = self.peek_kind() {
                    let id = id.clone();
                    self.advance();
                    id
                } else {
                    "callback".to_string()
                };
                if matches!(self.peek_kind(), CTokenKind::RParen) {
                    self.advance(); // consume ')'
                }
                // consume parameter list: `(int64_t, int64_t)`
                if matches!(self.peek_kind(), CTokenKind::LParen) {
                    self.advance();
                    while !self.is_at_end() && !matches!(self.peek_kind(), CTokenKind::RParen) {
                        self.advance();
                    }
                    if matches!(self.peek_kind(), CTokenKind::RParen) {
                        self.advance();
                    }
                }
                self.skip_newlines();
                if matches!(self.peek_kind(), CTokenKind::Semicolon) {
                    self.advance();
                }
                self.typedefs.insert(
                    fn_name.clone(),
                    CType::RawPtr {
                        pointee: Some(fn_name.clone()),
                    },
                );
                return Some(CDecl::Typedef(CTypedef {
                    name: fn_name,
                    target: CType::RawPtr { pointee: None },
                    line: typedef_tok.line,
                    col: typedef_tok.col,
                }));
            }

            if let CTokenKind::Ident(alias_name) = self.peek_kind() {
                let alias_name = alias_name.clone();
                self.advance();
                if matches!(self.peek_kind(), CTokenKind::Semicolon) {
                    self.advance();
                    self.typedefs
                        .insert(alias_name.clone(), target_type.clone());
                    return Some(CDecl::Typedef(CTypedef {
                        name: alias_name,
                        target: target_type,
                        line: typedef_tok.line,
                        col: typedef_tok.col,
                    }));
                }
            }
        }

        self.sync_to_semicolon();
        None
    }

    fn skip_qualifiers(&mut self) {
        while !self.is_at_end() {
            match self.peek_kind() {
                CTokenKind::Const
                | CTokenKind::Volatile
                | CTokenKind::Static
                | CTokenKind::Extern
                | CTokenKind::Inline => {
                    self.advance();
                }
                CTokenKind::Ident(id) => {
                    // Common calling convention macros and API qualifiers
                    if id == "__cdecl"
                        || id == "__stdcall"
                        || id == "__fastcall"
                        || id == "WINAPI"
                        || id == "APIENTRY"
                        || id == "CALLBACK"
                        || id == "SQLITE_API"
                        || id == "DATARA_API"
                        || id == "SQLITE_STDCALL"
                        || id.starts_with("__declspec")
                    {
                        self.advance();
                        if matches!(self.peek_kind(), CTokenKind::LParen) {
                            self.advance();
                            while !self.is_at_end()
                                && !matches!(self.peek_kind(), CTokenKind::RParen)
                            {
                                self.advance();
                            }
                            if matches!(self.peek_kind(), CTokenKind::RParen) {
                                self.advance();
                            }
                        }
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }
    }

    fn parse_type(&mut self) -> Option<CType> {
        self.skip_qualifiers();
        let base_type = match self.peek_kind() {
            CTokenKind::Void => {
                self.advance();
                CType::Void
            }
            CTokenKind::Char => {
                self.advance();
                CType::Char
            }
            CTokenKind::Int => {
                self.advance();
                CType::Int {
                    name: "int".into(),
                    bits: 32,
                    is_signed: true,
                }
            }
            CTokenKind::Short => {
                self.advance();
                if matches!(self.peek_kind(), CTokenKind::Int) {
                    self.advance();
                }
                CType::Int {
                    name: "short".into(),
                    bits: 16,
                    is_signed: true,
                }
            }
            CTokenKind::Long => {
                self.advance();
                if matches!(self.peek_kind(), CTokenKind::Long) {
                    self.advance();
                    if matches!(self.peek_kind(), CTokenKind::Int) {
                        self.advance();
                    }
                    CType::Int {
                        name: "long long".into(),
                        bits: 64,
                        is_signed: true,
                    }
                } else {
                    if matches!(self.peek_kind(), CTokenKind::Int) {
                        self.advance();
                    }
                    CType::Int {
                        name: "long".into(),
                        bits: 64,
                        is_signed: true,
                    }
                }
            }
            CTokenKind::Signed => {
                self.advance();
                if matches!(self.peek_kind(), CTokenKind::Char) {
                    self.advance();
                    CType::Char
                } else {
                    if matches!(self.peek_kind(), CTokenKind::Int) {
                        self.advance();
                    }
                    CType::Int {
                        name: "int".into(),
                        bits: 32,
                        is_signed: true,
                    }
                }
            }
            CTokenKind::Unsigned => {
                self.advance();
                if matches!(self.peek_kind(), CTokenKind::Char) {
                    self.advance();
                    CType::Int {
                        name: "unsigned char".into(),
                        bits: 8,
                        is_signed: false,
                    }
                } else if matches!(self.peek_kind(), CTokenKind::Short) {
                    self.advance();
                    CType::Int {
                        name: "unsigned short".into(),
                        bits: 16,
                        is_signed: false,
                    }
                } else if matches!(self.peek_kind(), CTokenKind::Long) {
                    self.advance();
                    if matches!(self.peek_kind(), CTokenKind::Long) {
                        self.advance();
                    }
                    CType::Int {
                        name: "unsigned long long".into(),
                        bits: 64,
                        is_signed: false,
                    }
                } else {
                    if matches!(self.peek_kind(), CTokenKind::Int) {
                        self.advance();
                    }
                    CType::Int {
                        name: "unsigned int".into(),
                        bits: 32,
                        is_signed: false,
                    }
                }
            }
            CTokenKind::Float => {
                self.advance();
                CType::Float {
                    name: "float".into(),
                    bits: 32,
                }
            }
            CTokenKind::Double => {
                self.advance();
                CType::Float {
                    name: "double".into(),
                    bits: 64,
                }
            }
            CTokenKind::Struct => {
                self.advance();
                if let CTokenKind::Ident(name) = self.peek_kind() {
                    let name = name.clone();
                    self.advance();
                    CType::Named(name)
                } else {
                    return None;
                }
            }
            CTokenKind::Ident(id) => {
                let id = id.clone();
                self.advance();
                if let Some(resolved) = self.typedefs.get(&id) {
                    resolved.clone()
                } else {
                    CType::Named(id)
                }
            }
            _ => return None,
        };

        // Pointer depth
        self.skip_qualifiers();
        let mut pointer_count = 0;
        while matches!(self.peek_kind(), CTokenKind::Star) {
            self.advance();
            pointer_count += 1;
            self.skip_qualifiers();
        }

        if pointer_count == 1 && base_type == CType::Char {
            return Some(CType::String);
        }

        if pointer_count > 0 {
            let pointee_name = match &base_type {
                CType::Named(n) => Some(n.clone()),
                _ => None,
            };
            return Some(CType::RawPtr {
                pointee: pointee_name,
            });
        }

        Some(base_type)
    }

    fn parse_function_decl(&mut self) -> Option<CDecl> {
        let start_pos = self.pos;
        let line = self.peek().line;
        let col = self.peek().col;

        self.skip_qualifiers();
        let return_type = match self.parse_type() {
            Some(t) => t,
            None => {
                self.pos = start_pos;
                return None;
            }
        };

        self.skip_qualifiers();
        let func_name = match self.peek_kind() {
            CTokenKind::Ident(name) => {
                let name = name.clone();
                self.advance();
                name
            }
            _ => {
                self.pos = start_pos;
                return None;
            }
        };

        if !matches!(self.peek_kind(), CTokenKind::LParen) {
            self.pos = start_pos;
            return None;
        }
        self.advance(); // consume '('

        let mut params = Vec::new();
        let mut is_variadic = false;

        // Check for `(void)`
        if matches!(self.peek_kind(), CTokenKind::Void)
            && matches!(self.peek_next().kind, CTokenKind::RParen)
        {
            self.advance(); // consume 'void'
            self.advance(); // consume ')'
        } else {
            while !self.is_at_end() && !matches!(self.peek_kind(), CTokenKind::RParen) {
                self.skip_qualifiers();
                if matches!(self.peek_kind(), CTokenKind::Ellipsis) {
                    self.advance();
                    is_variadic = true;
                    break;
                }

                let param_line = self.peek().line;
                let param_col = self.peek().col;
                let mut param_type = match self.parse_type() {
                    Some(t) => t,
                    None => {
                        self.diagnostics.push(format!(
                            "Unsupported C construct at line {}, col {}: invalid parameter type in function '{}'",
                            param_line, param_col, func_name
                        ));
                        self.sync_to_semicolon();
                        return None;
                    }
                };

                let mut param_name = format!("arg{}", params.len());
                if matches!(self.peek_kind(), CTokenKind::LParen)
                    && matches!(self.peek_next().kind, CTokenKind::Star)
                {
                    self.advance(); // consume '('
                    self.advance(); // consume '*'
                    if let CTokenKind::Ident(cb_name) = self.peek_kind() {
                        param_name = cb_name.clone();
                        self.advance();
                    }
                    if matches!(self.peek_kind(), CTokenKind::RParen) {
                        self.advance(); // consume ')'
                    }
                    if matches!(self.peek_kind(), CTokenKind::LParen) {
                        self.advance(); // consume '('
                        while !self.is_at_end() && !matches!(self.peek_kind(), CTokenKind::RParen) {
                            self.advance();
                        }
                        if matches!(self.peek_kind(), CTokenKind::RParen) {
                            self.advance();
                        }
                    }
                    param_type = CType::RawPtr {
                        pointee: Some("callback".into()),
                    };
                } else if let CTokenKind::Ident(name) = self.peek_kind() {
                    param_name = name.clone();
                    self.advance();
                }

                params.push(CParam {
                    name: param_name,
                    ty: param_type,
                    line: param_line,
                    col: param_col,
                });

                if matches!(self.peek_kind(), CTokenKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }

            if matches!(self.peek_kind(), CTokenKind::RParen) {
                self.advance();
            } else {
                self.diagnostics.push(format!(
                    "Unsupported C construct at line {}, col {}: expected ')' in function '{}'",
                    line, col, func_name
                ));
                self.sync_to_semicolon();
                return None;
            }
        }

        self.skip_qualifiers();
        if matches!(self.peek_kind(), CTokenKind::Semicolon) {
            self.advance();
            Some(CDecl::Function(CFunction {
                name: func_name,
                return_type,
                params,
                is_variadic,
                line,
                col,
            }))
        } else {
            // Function definition with body or unsupported syntax
            self.pos = start_pos;
            None
        }
    }
}
