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
pub extern "C" fn regex_is_match(pattern: *const c_char, text: *const c_char) -> bool {
    let __panic_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let pattern = if pattern.is_null() { "" } else { unsafe { CStr::from_ptr(pattern).to_str().unwrap_or("") } };
        let text = if text.is_null() { "" } else { unsafe { CStr::from_ptr(text).to_str().unwrap_or("") } };
        let Ok(re) = regex::Regex::new(pattern) else { return false; };
        re.is_match(text)
    }));

    match __panic_res {
        Ok(res) => res,
        Err(_) => false,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn regex_find(pattern: *const c_char, text: *const c_char) -> *const c_char {
    let __panic_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let pattern = if pattern.is_null() { "" } else { unsafe { CStr::from_ptr(pattern).to_str().unwrap_or("") } };
        let text = if text.is_null() { "" } else { unsafe { CStr::from_ptr(text).to_str().unwrap_or("") } };
        let Ok(re) = regex::Regex::new(pattern) else { return String::new(); };
        re.find(text).map(|m| m.as_str().to_string()).unwrap_or_default()
    }));

    match __panic_res {
        Ok(res) => __bridge_ret_str(res.to_string()),
        Err(_) => __bridge_ret_str(String::new()),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn regex_replace_all(pattern: *const c_char, text: *const c_char, rep: *const c_char) -> *const c_char {
    let __panic_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let pattern = if pattern.is_null() { "" } else { unsafe { CStr::from_ptr(pattern).to_str().unwrap_or("") } };
        let text = if text.is_null() { "" } else { unsafe { CStr::from_ptr(text).to_str().unwrap_or("") } };
        let rep = if rep.is_null() { "" } else { unsafe { CStr::from_ptr(rep).to_str().unwrap_or("") } };
        let Ok(re) = regex::Regex::new(pattern) else { return text.to_string(); };
        re.replace_all(text, rep).into_owned()
    }));

    match __panic_res {
        Ok(res) => __bridge_ret_str(res.to_string()),
        Err(_) => __bridge_ret_str(String::new()),
    }
}

