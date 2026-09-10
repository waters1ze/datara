use crate::ast::TypeNode;
use crate::diagnostics::SourceSpan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CType {
    Void,
    Int {
        name: String,
        bits: usize,
        is_signed: bool,
    },
    Float {
        name: String,
        bits: usize,
    },
    Char,
    String, // char* or const char*
    RawPtr {
        pointee: Option<String>,
    },
    Named(String),
}

impl CType {
    pub fn to_datara_type_node(&self, file: &str, line: usize, col: usize) -> Option<TypeNode> {
        let span = SourceSpan::new(line, col, line, col, file.to_string());
        match self {
            CType::Void => None,
            CType::Int { .. } => Some(TypeNode::new("Int", span)),
            CType::Float { .. } => Some(TypeNode::new("Float", span)),
            CType::Char => Some(TypeNode::new("Int", span)),
            CType::String => Some(TypeNode::new("String", span)),
            CType::RawPtr { .. } => Some(TypeNode::new("RawPtr", span)),
            CType::Named(name) => match name.as_str() {
                "int" | "long" | "short" | "size_t" | "ssize_t" | "int64_t" | "uint64_t"
                | "int32_t" | "uint32_t" | "int16_t" | "uint16_t" | "int8_t" | "uint8_t"
                | "intptr_t" | "uintptr_t" | "ptrdiff_t" => Some(TypeNode::new("Int", span)),
                "float" | "double" => Some(TypeNode::new("Float", span)),
                "char" => Some(TypeNode::new("Int", span)),
                _ => Some(TypeNode::new(name, span)),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct CField {
    pub name: String,
    pub ty: CType,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct CStruct {
    pub name: String,
    pub fields: Vec<CField>,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct CParam {
    pub name: String,
    pub ty: CType,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct CFunction {
    pub name: String,
    pub return_type: CType,
    pub params: Vec<CParam>,
    pub is_variadic: bool,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct CEnumVariant {
    pub name: String,
    pub value: i64,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct CEnum {
    pub name: Option<String>,
    pub variants: Vec<CEnumVariant>,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct CDefineInt {
    pub name: String,
    pub value: i64,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct COpaqueStruct {
    pub name: String,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct CTypedef {
    pub name: String,
    pub target: CType,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub enum CDecl {
    Function(CFunction),
    Enum(CEnum),
    DefineInt(CDefineInt),
    OpaqueStruct(COpaqueStruct),
    Struct(CStruct),
    Typedef(CTypedef),
}
