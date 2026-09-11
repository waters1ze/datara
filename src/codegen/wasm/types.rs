use serde::{Deserialize, Serialize};

pub fn encode_u32_leb128(mut val: u32, buf: &mut Vec<u8>) {
    loop {
        let mut byte = (val & 0x7F) as u8;
        val >>= 7;
        if val != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if val == 0 {
            break;
        }
    }
}

/// Encodes a signed 32-bit integer as signed LEB128.
pub fn encode_i32_leb128(mut val: i32, buf: &mut Vec<u8>) {
    let mut more = true;
    while more {
        let mut byte = (val & 0x7F) as u8;
        val >>= 7;
        let sign_bit = (byte & 0x40) != 0;
        if (val == 0 && !sign_bit) || (val == -1 && sign_bit) {
            more = false;
        } else {
            byte |= 0x80;
        }
        buf.push(byte);
    }
}

/// Encodes a signed 64-bit integer as signed LEB128.
pub fn encode_i64_leb128(mut val: i64, buf: &mut Vec<u8>) {
    let mut more = true;
    while more {
        let mut byte = (val & 0x7F) as u8;
        val >>= 7;
        let sign_bit = (byte & 0x40) != 0;
        if (val == 0 && !sign_bit) || (val == -1 && sign_bit) {
            more = false;
        } else {
            byte |= 0x80;
        }
        buf.push(byte);
    }
}

/// Encodes a 64-bit float in IEEE-754 little-endian format.
pub fn encode_f64(val: f64, buf: &mut Vec<u8>) {
    buf.extend_from_slice(&val.to_le_bytes());
}

/// Encodes a 32-bit float in IEEE-754 little-endian format.
pub fn encode_f32(val: f32, buf: &mut Vec<u8>) {
    buf.extend_from_slice(&val.to_le_bytes());
}

/// Encodes a UTF-8 string with a leading unsigned LEB128 length prefix.
pub fn encode_str(s: &str, buf: &mut Vec<u8>) {
    let bytes = s.as_bytes();
    encode_u32_leb128(bytes.len() as u32, buf);
    buf.extend_from_slice(bytes);
}

/// Emits a WebAssembly binary section with ID and length prefix.
pub(crate) fn emit_section(id: u8, content: &[u8], buf: &mut Vec<u8>) {
    buf.push(id);
    encode_u32_leb128(content.len() as u32, buf);
    buf.extend_from_slice(content);
}

// ---------------------------------------------------------------------------
// Capability Specification & Transitive Effect Analysis
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]

pub enum WasmValType {
    I32 = 0x7F,
    I64 = 0x7E,
    F32 = 0x7D,
    F64 = 0x7C,
    V128 = 0x7B,
}

impl WasmValType {
    pub fn wat_name(&self) -> &'static str {
        match self {
            WasmValType::I32 => "i32",
            WasmValType::I64 => "i64",
            WasmValType::F32 => "f32",
            WasmValType::F64 => "f64",
            WasmValType::V128 => "v128",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WasmFuncType {
    pub params: Vec<WasmValType>,
    pub results: Vec<WasmValType>,
}

impl WasmFuncType {
    pub fn encode(&self, buf: &mut Vec<u8>) {
        buf.push(0x60); // func form
        encode_u32_leb128(self.params.len() as u32, buf);
        for p in &self.params {
            buf.push(*p as u8);
        }
        encode_u32_leb128(self.results.len() as u32, buf);
        for r in &self.results {
            buf.push(*r as u8);
        }
    }
}

pub(crate) fn map_dmir_type_to_wasm(ty: &str) -> Option<WasmValType> {
    match ty {
        "Int" | "Bool" | "Int64" | "Char" | "Unit" => Some(WasmValType::I64),
        "Float" | "Float64" => Some(WasmValType::F64),
        "Float32" => Some(WasmValType::F32),
        "Int32" => Some(WasmValType::I32),
        "Float4" | "Int4" | "Vector4" => Some(WasmValType::V128),
        "String" | "Str" | "List" | "Map" | "Dynamic" => Some(WasmValType::I64), // linear-memory pointer
        _ if ty.starts_with("List<") || ty.starts_with("Map<") => Some(WasmValType::I64),
        _ => Some(WasmValType::I64),
    }
}

// ---------------------------------------------------------------------------
// Wasm Emitter Implementation
// ---------------------------------------------------------------------------
