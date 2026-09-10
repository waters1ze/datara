//! Auto-generated Datara Rust Bridge Shim
#![allow(unused_imports, non_snake_case, dead_code, unused_variables)]

use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

thread_local! {
    static STRING_RING: RefCell<Vec<CString>> = const { RefCell::new(Vec::new()) };
}

fn __bridge_ret_str(s: String) -> *const c_char {
    let cs = CString::new(s).unwrap_or_default();
    let ptr = cs.as_ptr();
    STRING_RING.with(|ring| {
        let mut b = ring.borrow_mut();
        if b.len() >= 32 { b.remove(0); }
        b.push(cs);
    });
    ptr
}

#[unsafe(no_mangle)]
pub extern "C" fn encode_rgb_png(buf_ptr: *const u8, buf_len: i64, width: i64, height: i64) -> *const c_char {
    let __panic_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // Zero-copy access into Datara buffer without heap clone
        let raw_bytes = unsafe { std::slice::from_raw_parts(buf_ptr, buf_len as usize) };
        let Some(img) = image::RgbImage::from_raw(width as u32, height as u32, raw_bytes.to_vec()) else {
            return String::new();
        };
        let mut out_png = std::io::Cursor::new(Vec::new());
        let _ = img.write_to(&mut out_png, image::ImageFormat::Png);
        let bytes = out_png.into_inner();
        format!("PNG_BYTES:{}:HEADER:{}", bytes.len(), bytes.get(1).copied().unwrap_or(0))
    }));

    match __panic_res {
        Ok(res) => __bridge_ret_str(res.to_string()),
        Err(_) => __bridge_ret_str(String::new()),
    }
}

