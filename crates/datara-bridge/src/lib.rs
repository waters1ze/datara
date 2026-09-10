//! Datara 1.0 Zero-Copy Bridge & FFI Bindings.
//!
//! Provides zero-copy buffer passing (slice views, no memmove),
//! UTF-8 string views, and Outcome<T, E> sum types conforming to Datara C ABI.

use std::ffi::CStr;
use std::os::raw::c_char;
use std::slice;
use std::str;

/// C-ABI compatible zero-copy immutable buffer view.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct DataraSlice<'a, T> {
    pub data: *const T,
    pub len: usize,
    _marker: std::marker::PhantomData<&'a [T]>,
}

unsafe impl<'a, T: Sync> Sync for DataraSlice<'a, T> {}
unsafe impl<'a, T: Send> Send for DataraSlice<'a, T> {}

impl<'a, T> DataraSlice<'a, T> {
    /// Create a zero-copy DataraSlice from a standard Rust slice (zero allocations).
    #[inline(always)]
    pub fn from_slice(s: &'a [T]) -> Self {
        Self {
            data: s.as_ptr(),
            len: s.len(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Access the underlying memory as a safe Rust slice without copying.
    #[inline(always)]
    pub fn as_slice(&self) -> &'a [T] {
        if self.data.is_null() || self.len == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(self.data, self.len) }
        }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// C-ABI compatible zero-copy mutable buffer view.
#[repr(C)]
#[derive(Debug)]
pub struct DataraSliceMut<'a, T> {
    pub data: *mut T,
    pub len: usize,
    _marker: std::marker::PhantomData<&'a mut [T]>,
}

unsafe impl<'a, T: Sync> Sync for DataraSliceMut<'a, T> {}
unsafe impl<'a, T: Send> Send for DataraSliceMut<'a, T> {}

impl<'a, T> DataraSliceMut<'a, T> {
    /// Create a mutable zero-copy DataraSliceMut from a mutable Rust slice.
    #[inline(always)]
    pub fn from_slice_mut(s: &'a mut [T]) -> Self {
        Self {
            data: s.as_mut_ptr(),
            len: s.len(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Access the underlying memory as a mutable Rust slice without copying.
    #[inline(always)]
    pub fn as_slice_mut(&mut self) -> &'a mut [T] {
        if self.data.is_null() || self.len == 0 {
            &mut []
        } else {
            unsafe { slice::from_raw_parts_mut(self.data, self.len) }
        }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// C-ABI compatible zero-copy UTF-8 string view.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct DataraString<'a> {
    pub ptr: *const c_char,
    pub len: usize,
    _marker: std::marker::PhantomData<&'a str>,
}

impl<'a> DataraString<'a> {
    #[inline(always)]
    pub fn from_str(s: &'a str) -> Self {
        Self {
            ptr: s.as_ptr() as *const c_char,
            len: s.len(),
            _marker: std::marker::PhantomData,
        }
    }

    #[inline(always)]
    pub fn as_str(&self) -> Result<&'a str, str::Utf8Error> {
        if self.ptr.is_null() || self.len == 0 {
            Ok("")
        } else {
            let u8_slice = unsafe { slice::from_raw_parts(self.ptr as *const u8, self.len) };
            str::from_utf8(u8_slice)
        }
    }
}

/// C-ABI compatible Outcome<T, E> sum-type representation.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct DataraOutcome<T: Copy> {
    pub is_ok: i32,
    pub pad: i32,
    pub val: T,
}

impl<T: Copy> DataraOutcome<T> {
    pub fn ok(value: T) -> Self {
        Self {
            is_ok: 1,
            pad: 0,
            val: value,
        }
    }

    pub fn err(fallback: T) -> Self {
        Self {
            is_ok: 0,
            pad: 0,
            val: fallback,
        }
    }

    pub fn is_ok(&self) -> bool {
        self.is_ok != 0
    }

    pub fn is_err(&self) -> bool {
        self.is_ok == 0
    }

    pub fn unwrap(&self) -> T {
        assert!(self.is_ok(), "Called unwrap on failed DataraOutcome");
        self.val
    }
}

unsafe extern "C" {
    pub fn forgen_init() -> i32;
    pub fn forgen_load_module(module_path: *const c_char) -> i32;
    pub fn forgen_call_fn(
        func_name: *const c_char,
        args: *const i64,
        arg_count: usize,
        out_result: *mut i64,
    ) -> i32;
    pub fn forgen_shutdown() -> i32;
    pub fn forgen_last_error() -> *const c_char;
}

/// Safe RAII wrapper around the Datara compiler and runtime.
pub struct DataraModule {
    _path: String,
    loaded: bool,
}

impl DataraModule {
    pub fn load(path: &str) -> Result<Self, String> {
        let rc = unsafe { forgen_init() };
        if rc != 0 {
            let err = unsafe { CStr::from_ptr(forgen_last_error()) };
            return Err(err.to_string_lossy().into_owned());
        }

        let c_path = std::ffi::CString::new(path).map_err(|e| e.to_string())?;
        let rc = unsafe { forgen_load_module(c_path.as_ptr()) };
        if rc != 0 {
            let err = unsafe { CStr::from_ptr(forgen_last_error()) };
            return Err(err.to_string_lossy().into_owned());
        }

        Ok(Self {
            _path: path.to_string(),
            loaded: true,
        })
    }

    pub fn call(&self, func_name: &str, args: &[i64]) -> Result<i64, String> {
        if !self.loaded {
            return Err("Module not loaded".into());
        }
        let c_name = std::ffi::CString::new(func_name).map_err(|e| e.to_string())?;
        let mut result = 0i64;
        let rc = unsafe {
            forgen_call_fn(
                c_name.as_ptr(),
                if args.is_empty() {
                    std::ptr::null()
                } else {
                    args.as_ptr()
                },
                args.len(),
                &mut result,
            )
        };
        if rc != 0 {
            let err = unsafe { CStr::from_ptr(forgen_last_error()) };
            return Err(err.to_string_lossy().into_owned());
        }
        Ok(result)
    }
}

impl Drop for DataraModule {
    fn drop(&mut self) {
        if self.loaded {
            unsafe {
                forgen_shutdown();
            }
            self.loaded = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[unsafe(no_mangle)]
    pub extern "C" fn forgen_init() -> i32 { 0 }
    #[unsafe(no_mangle)]
    pub extern "C" fn forgen_load_module(_: *const c_char) -> i32 { 0 }
    #[unsafe(no_mangle)]
    pub extern "C" fn forgen_call_fn(_: *const c_char, _: *const i64, _: usize, out: *mut i64) -> i32 {
        unsafe { *out = 42; }
        0
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn forgen_shutdown() -> i32 { 0 }
    #[unsafe(no_mangle)]
    pub extern "C" fn forgen_last_error() -> *const c_char {
        c"".as_ptr()
    }

    #[test]
    fn test_zero_copy_slice_address_identity() {
        let original_data = vec![1.0f64, 2.0, 3.0, 4.0, 5.0];
        let slice_view = DataraSlice::from_slice(&original_data);

        // Prove exact pointer equality (zero copy, no allocation, no memmove)
        assert_eq!(
            original_data.as_ptr(),
            slice_view.as_slice().as_ptr(),
            "DataraSlice MUST point to the identical memory address (zero-copy)"
        );
        assert_eq!(slice_view.len(), 5);
        assert_eq!(slice_view.as_slice()[2], 3.0);
    }

    #[test]
    fn test_zero_copy_slice_mut_mutation() {
        let mut data = vec![10i64, 20, 30];
        let original_ptr = data.as_ptr();
        {
            let mut view = DataraSliceMut::from_slice_mut(&mut data);
            assert_eq!(view.as_slice_mut().as_mut_ptr(), original_ptr as *mut i64);
            view.as_slice_mut()[1] = 999;
        }
        assert_eq!(data[1], 999);
    }

    #[test]
    fn test_zero_copy_string_view() {
        let s = "Hello Datara 1.0";
        let str_view = DataraString::from_str(s);
        assert_eq!(s.as_ptr() as *const c_char, str_view.ptr);
        assert_eq!(str_view.as_str().unwrap(), s);
    }

    #[test]
    fn test_outcome_ok_and_err() {
        let ok_res = DataraOutcome::ok(42i64);
        assert!(ok_res.is_ok());
        assert_eq!(ok_res.unwrap(), 42);

        let err_res = DataraOutcome::err(0i64);
        assert!(err_res.is_err());
    }

    #[test]
    fn test_datara_module_raii_call() {
        let module = DataraModule::load("dummy.dtr").expect("Module load");
        let res = module.call("test_fn", &[10, 20]).expect("Call test_fn");
        assert_eq!(res, 42);
    }
}
