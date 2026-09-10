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
pub extern "C" fn json_get_string(json_str: *const c_char, key: *const c_char) -> *const c_char {
    let __panic_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let json_str = if json_str.is_null() { "" } else { unsafe { CStr::from_ptr(json_str).to_str().unwrap_or("") } };
        let key = if key.is_null() { "" } else { unsafe { CStr::from_ptr(key).to_str().unwrap_or("") } };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) else { return String::new(); };
        val.get(key).and_then(|v| v.as_str()).unwrap_or_default().to_string()
    }));

    match __panic_res {
        Ok(res) => __bridge_ret_str(res.to_string()),
        Err(_) => __bridge_ret_str(String::new()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn json_get_int(json_str: *const c_char, key: *const c_char) -> i64 {
    let __panic_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let json_str = if json_str.is_null() { "" } else { unsafe { CStr::from_ptr(json_str).to_str().unwrap_or("") } };
        let key = if key.is_null() { "" } else { unsafe { CStr::from_ptr(key).to_str().unwrap_or("") } };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) else { return 0; };
        val.get(key).and_then(|v| v.as_i64()).unwrap_or(0)
    }));

    match __panic_res {
        Ok(res) => res as i64,
        Err(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn json_create_pair(k1: *const c_char, v1: *const c_char, k2: *const c_char, v2: i64) -> *const c_char {
    let __panic_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let k1 = if k1.is_null() { "" } else { unsafe { CStr::from_ptr(k1).to_str().unwrap_or("") } };
        let v1 = if v1.is_null() { "" } else { unsafe { CStr::from_ptr(v1).to_str().unwrap_or("") } };
        let k2 = if k2.is_null() { "" } else { unsafe { CStr::from_ptr(k2).to_str().unwrap_or("") } };
        let mut map = serde_json::Map::new();
        map.insert(k1.to_string(), serde_json::Value::String(v1.to_string()));
        map.insert(k2.to_string(), serde_json::Value::Number(v2.into()));
        serde_json::to_string(&serde_json::Value::Object(map)).unwrap_or_default()
    }));

    match __panic_res {
        Ok(res) => __bridge_ret_str(res.to_string()),
        Err(_) => __bridge_ret_str(String::new()),
    }
}

