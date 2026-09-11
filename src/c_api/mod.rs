//! C-compatible Embed API for `libforgen`.
//!
//! Provides a stable C ABI for embedding the Datara compiler and executing
//! compiled functions in-process via Cranelift JIT or native shared libraries.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::codegen::TargetInfo;
use crate::codegen::cranelift::backend::RealCraneliftBackend;
use crate::codegen::cranelift::jit::create_jit_module;
use crate::driver::ForgenCompiler;
use cranelift_jit::JITModule;

thread_local! {
    static LAST_ERROR_TL: RefCell<CString> = RefCell::new(CString::new("").unwrap());
}

fn set_error(msg: &str) {
    LAST_ERROR_TL.with(|cell| {
        *cell.borrow_mut() =
            CString::new(msg).unwrap_or_else(|_| CString::new("Unknown error").unwrap());
    });
}

fn clear_error() {
    LAST_ERROR_TL.with(|cell| {
        *cell.borrow_mut() = CString::new("").unwrap();
    });
}

struct LoadedModule {
    _jit_module: Option<JITModule>,
    functions: HashMap<String, *const u8>,
}

unsafe impl Send for LoadedModule {}
unsafe impl Sync for LoadedModule {}

struct GlobalState {
    _initialized: bool,
    modules: HashMap<PathBuf, LoadedModule>,
    active_module: Option<PathBuf>,
}

static STATE: Mutex<Option<GlobalState>> = Mutex::new(None);

/// Initialize the Forgen embed runtime.
///
/// Returns 0 on success, or a negative error code on failure.
#[unsafe(no_mangle)]
pub extern "C" fn forgen_init() -> i32 {
    let mut lock = match STATE.lock() {
        Ok(l) => l,
        Err(e) => e.into_inner(),
    };
    *lock = Some(GlobalState {
        _initialized: true,
        modules: HashMap::new(),
        active_module: None,
    });
    clear_error();
    0
}

/// Load and compile a Datara module (.dtr) into the process memory.
///
/// Returns 0 on success, or -1 on failure (inspect via `forgen_last_error`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn forgen_load_module(module_path: *const c_char) -> i32 {
    if module_path.is_null() {
        set_error("forgen_load_module: module_path cannot be NULL");
        return -1;
    }

    let c_str = unsafe { CStr::from_ptr(module_path) };
    let path_str = match c_str.to_str() {
        Ok(s) => s,
        Err(e) => {
            set_error(&format!("forgen_load_module: invalid UTF-8 path: {}", e));
            return -1;
        }
    };

    let path = PathBuf::from(path_str);
    if !path.exists() {
        set_error(&format!(
            "forgen_load_module: file not found: {}",
            path.display()
        ));
        return -1;
    }

    let compiler = ForgenCompiler::new("release");
    let dmir_module = match compiler.compile_file_to_dmir(&path) {
        Ok(m) => m,
        Err(e) => {
            set_error(&format!("forgen_load_module: compilation error: {}", e));
            return -1;
        }
    };

    let backend = RealCraneliftBackend::new(TargetInfo::host());
    let (isa, call_conv, frontend_config) = match backend.build_target_isa(true) {
        Ok(cfg) => cfg,
        Err(e) => {
            set_error(&format!(
                "forgen_load_module: failed to build target ISA: {}",
                e
            ));
            return -1;
        }
    };

    let mut jit_module = match create_jit_module(isa) {
        Ok(m) => m,
        Err(e) => {
            set_error(&format!(
                "forgen_load_module: failed to create JIT module: {}",
                e
            ));
            return -1;
        }
    };

    let artifacts = match backend.compile_into_module_opt(
        &mut jit_module,
        &dmir_module,
        frontend_config,
        call_conv,
        true,
    ) {
        Ok(a) => a,
        Err(e) => {
            set_error(&format!(
                "forgen_load_module: backend compile failed: {}",
                e
            ));
            return -1;
        }
    };

    if let Err(e) = jit_module.finalize_definitions() {
        set_error(&format!(
            "forgen_load_module: JIT finalization failed: {}",
            e
        ));
        return -1;
    }

    let mut functions = HashMap::new();
    for name in dmir_module.functions.keys() {
        if let Some(&func_id) = artifacts.func_ids.get(name) {
            let code_ptr = jit_module.get_finalized_function(func_id);
            functions.insert(name.clone(), code_ptr);
        }
    }

    let loaded = LoadedModule {
        _jit_module: Some(jit_module),
        functions,
    };

    let mut lock = match STATE.lock() {
        Ok(l) => l,
        Err(e) => e.into_inner(),
    };

    let state = lock.get_or_insert_with(|| GlobalState {
        _initialized: true,
        modules: HashMap::new(),
        active_module: None,
    });

    state.modules.insert(path.clone(), loaded);
    state.active_module = Some(path);
    clear_error();

    0
}

/// Call a function exported by the loaded module.
///
/// Parameters:
/// - `func_name`: null-terminated name of the function to call.
/// - `args`: pointer to an array of 64-bit integer / pointer arguments.
/// - `arg_count`: number of arguments in `args`.
/// - `out_result`: pointer where the 64-bit return value will be stored.
///
/// Returns 0 on success, or -1 on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn forgen_call_fn(
    func_name: *const c_char,
    args: *const i64,
    arg_count: usize,
    out_result: *mut i64,
) -> i32 {
    if func_name.is_null() {
        set_error("forgen_call_fn: func_name is NULL");
        return -1;
    }
    if out_result.is_null() {
        set_error("forgen_call_fn: out_result is NULL");
        return -1;
    }
    if arg_count > 0 && args.is_null() {
        set_error("forgen_call_fn: args is NULL with arg_count > 0");
        return -1;
    }

    let c_str = unsafe { CStr::from_ptr(func_name) };
    let fn_name = match c_str.to_str() {
        Ok(s) => s,
        Err(e) => {
            set_error(&format!(
                "forgen_call_fn: invalid UTF-8 in func_name: {}",
                e
            ));
            return -1;
        }
    };

    let code_ptr = {
        let lock = match STATE.lock() {
            Ok(l) => l,
            Err(e) => e.into_inner(),
        };
        let state = match lock.as_ref() {
            Some(s) => s,
            None => {
                set_error("forgen_call_fn: runtime not initialized; call forgen_init first");
                return -1;
            }
        };
        let active_path = match state.active_module.as_ref() {
            Some(p) => p,
            None => {
                set_error("forgen_call_fn: no module loaded; call forgen_load_module first");
                return -1;
            }
        };
        let module = match state.modules.get(active_path) {
            Some(m) => m,
            None => {
                set_error("forgen_call_fn: active module not found");
                return -1;
            }
        };
        match module.functions.get(fn_name) {
            Some(&ptr) => ptr,
            None => {
                set_error(&format!(
                    "forgen_call_fn: function '{}' not found in module",
                    fn_name
                ));
                return -1;
            }
        }
    };

    let arg_slice = if arg_count > 0 && !args.is_null() {
        unsafe { std::slice::from_raw_parts(args, arg_count) }
    } else {
        &[]
    };

    if arg_count > 8 {
        set_error("forgen_call_fn: [E0904] more than 8 arguments are not supported");
        return -1;
    }

    let call_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        match arg_count {
            0 => {
                let f: extern "C" fn() -> i64 = std::mem::transmute(code_ptr);
                f()
            }
            1 => {
                let f: extern "C" fn(i64) -> i64 = std::mem::transmute(code_ptr);
                f(arg_slice[0])
            }
            2 => {
                let f: extern "C" fn(i64, i64) -> i64 = std::mem::transmute(code_ptr);
                f(arg_slice[0], arg_slice[1])
            }
            3 => {
                let f: extern "C" fn(i64, i64, i64) -> i64 = std::mem::transmute(code_ptr);
                f(arg_slice[0], arg_slice[1], arg_slice[2])
            }
            4 => {
                let f: extern "C" fn(i64, i64, i64, i64) -> i64 = std::mem::transmute(code_ptr);
                f(arg_slice[0], arg_slice[1], arg_slice[2], arg_slice[3])
            }
            5 => {
                let f: extern "C" fn(i64, i64, i64, i64, i64) -> i64 =
                    std::mem::transmute(code_ptr);
                f(
                    arg_slice[0],
                    arg_slice[1],
                    arg_slice[2],
                    arg_slice[3],
                    arg_slice[4],
                )
            }
            6 => {
                let f: extern "C" fn(i64, i64, i64, i64, i64, i64) -> i64 =
                    std::mem::transmute(code_ptr);
                f(
                    arg_slice[0],
                    arg_slice[1],
                    arg_slice[2],
                    arg_slice[3],
                    arg_slice[4],
                    arg_slice[5],
                )
            }
            7 => {
                let f: extern "C" fn(i64, i64, i64, i64, i64, i64, i64) -> i64 =
                    std::mem::transmute(code_ptr);
                f(
                    arg_slice[0],
                    arg_slice[1],
                    arg_slice[2],
                    arg_slice[3],
                    arg_slice[4],
                    arg_slice[5],
                    arg_slice[6],
                )
            }
            8 => {
                let f: extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64) -> i64 =
                    std::mem::transmute(code_ptr);
                f(
                    arg_slice[0],
                    arg_slice[1],
                    arg_slice[2],
                    arg_slice[3],
                    arg_slice[4],
                    arg_slice[5],
                    arg_slice[6],
                    arg_slice[7],
                )
            }
            _ => {
                set_error("forgen_call_fn: [E0904] invalid argument count");
                0
            }
        }
    }));

    match call_res {
        Ok(ret_val) => {
            unsafe {
                *out_result = ret_val;
            }
            0
        }
        Err(e) => {
            set_error(&format!(
                "forgen_call_fn: panic during function invocation: {:?}",
                e
            ));
            -1
        }
    }
}

/// Shuts down the embed runtime, freeing all loaded modules and cached JIT memory.
#[unsafe(no_mangle)]
pub extern "C" fn forgen_shutdown() -> i32 {
    let mut lock = match STATE.lock() {
        Ok(l) => l,
        Err(e) => e.into_inner(),
    };
    *lock = None;
    clear_error();
    0
}

/// Returns the last error message as a null-terminated UTF-8 string.
/// The returned pointer is thread-local and remains valid until the next forgen call
/// on the current thread.
#[unsafe(no_mangle)]
pub extern "C" fn forgen_last_error() -> *const c_char {
    LAST_ERROR_TL.with(|cell| cell.borrow().as_ptr())
}
