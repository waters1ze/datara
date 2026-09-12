#![allow(dead_code)]

//! Custom near-memory provider for Cranelift JIT.
//!
//! Allocates JIT code, data, and stack frames within +/- 1GB of runtime
//! symbols (like `datara_rt_out_int`) so that 32-bit PC-relative calls
//! (`Reloc::X86CallPCRel4`) never overflow with `TryFromIntError` on 64-bit
//! architectures (both Windows and Linux/macOS).

use cranelift_jit::{BranchProtection, JITMemoryKind, JITMemoryProvider};
use cranelift_module::{ModuleError, ModuleResult};
use std::io;

struct Allocation {
    ptr: *mut u8,
    len: usize,
}

struct NearMemory {
    allocations: Vec<Allocation>,
    already_finalized: usize,
    current_ptr: *mut u8,
    current_len: usize,
    position: usize,
}

unsafe impl Send for NearMemory {}

impl NearMemory {
    fn new() -> Self {
        Self {
            allocations: Vec::new(),
            already_finalized: 0,
            current_ptr: std::ptr::null_mut(),
            current_len: 0,
            position: 0,
        }
    }

    fn finish_current(&mut self) {
        if self.current_len > 0 && !self.current_ptr.is_null() {
            self.allocations.push(Allocation {
                ptr: self.current_ptr,
                len: self.current_len,
            });
            self.current_ptr = std::ptr::null_mut();
            self.current_len = 0;
            self.position = 0;
        }
    }

    fn allocate(&mut self, size: usize, align: u64) -> io::Result<*mut u8> {
        let align =
            usize::try_from(align).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let align = align.max(1);
        if self.position % align != 0 {
            self.position += align - (self.position % align);
        }

        if self.position + size <= self.current_len {
            let ptr = unsafe { self.current_ptr.add(self.position) };
            self.position += size;
            return Ok(ptr);
        }

        self.finish_current();

        let chunk_size = size.max(2 * 1024 * 1024);
        let (ptr, len) = match unsafe { allocate_near_block(chunk_size) } {
            Ok(res) => res,
            Err(_) => unsafe { allocate_fallback_block(chunk_size)? },
        };
        self.current_ptr = ptr;
        self.current_len = len;
        self.position = size;
        Ok(ptr)
    }

    unsafe fn free_memory(&mut self) {
        self.finish_current();
        for alloc in &self.allocations {
            unsafe { free_near_block(alloc.ptr, alloc.len) };
        }
        self.allocations.clear();
        self.already_finalized = 0;
    }
}

impl Drop for NearMemory {
    fn drop(&mut self) {
        // By default, leak JIT memory so function pointers remain valid
    }
}

pub struct NearMemoryProvider {
    code: NearMemory,
    readonly: NearMemory,
    writable: NearMemory,
}

unsafe impl Send for NearMemoryProvider {}

impl NearMemoryProvider {
    pub fn new() -> Self {
        Self {
            code: NearMemory::new(),
            readonly: NearMemory::new(),
            writable: NearMemory::new(),
        }
    }
}

impl JITMemoryProvider for NearMemoryProvider {
    fn allocate(&mut self, size: usize, align: u64, kind: JITMemoryKind) -> io::Result<*mut u8> {
        match kind {
            JITMemoryKind::Executable => self.code.allocate(size, align),
            JITMemoryKind::ReadOnly => self.readonly.allocate(size, align),
            JITMemoryKind::Writable => self.writable.allocate(size, align),
        }
    }

    unsafe fn free_memory(&mut self) {
        unsafe {
            self.code.free_memory();
            self.readonly.free_memory();
            self.writable.free_memory();
        }
    }

    fn finalize(&mut self, _branch_protection: BranchProtection) -> ModuleResult<()> {
        self.code.finish_current();
        for alloc in &self.code.allocations[self.code.already_finalized..] {
            unsafe {
                make_executable(alloc.ptr, alloc.len).map_err(|e| ModuleError::Allocation {
                    err: io::Error::new(io::ErrorKind::Other, e),
                })?;
            }
        }
        self.code.already_finalized = self.code.allocations.len();

        self.readonly.finish_current();
        for alloc in &self.readonly.allocations[self.readonly.already_finalized..] {
            unsafe {
                make_readonly(alloc.ptr, alloc.len).map_err(|e| ModuleError::Allocation {
                    err: io::Error::new(io::ErrorKind::Other, e),
                })?;
            }
        }
        self.readonly.already_finalized = self.readonly.allocations.len();

        Ok(())
    }
}

#[cfg(windows)]
unsafe fn allocate_near_block(min_size: usize) -> io::Result<(*mut u8, usize)> {
    unsafe extern "system" {
        fn VirtualAlloc(
            lpAddress: *const std::ffi::c_void,
            dwSize: usize,
            flAllocationType: u32,
            flProtect: u32,
        ) -> *mut std::ffi::c_void;
    }
    const MEM_COMMIT: u32 = 0x1000;
    const MEM_RESERVE: u32 = 0x2000;
    const PAGE_READWRITE: u32 = 0x04;

    let gran = 64 * 1024;
    let alloc_size = (min_size + gran - 1) & !(gran - 1);
    let ref_addr = crate::codegen::cranelift::jit::datara_rt_out_int as *const () as usize;

    let max_steps = (800 * 1024 * 1024) / gran;
    for step in 1..max_steps {
        if ref_addr > step * gran + 0x10000 {
            let target = (ref_addr - step * gran) & !(gran - 1);
            let ptr = unsafe {
                VirtualAlloc(
                    target as *const _,
                    alloc_size,
                    MEM_COMMIT | MEM_RESERVE,
                    PAGE_READWRITE,
                )
            };
            if !ptr.is_null() {
                return Ok((ptr as *mut u8, alloc_size));
            }
        }
        let target = (ref_addr + step * gran) & !(gran - 1);
        let ptr = unsafe {
            VirtualAlloc(
                target as *const _,
                alloc_size,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            )
        };
        if !ptr.is_null() {
            return Ok((ptr as *mut u8, alloc_size));
        }
    }

    Err(io::Error::new(
        io::ErrorKind::OutOfMemory,
        "Failed to allocate near JIT memory within 1GB of runtime",
    ))
}

#[cfg(windows)]
unsafe fn make_executable(ptr: *mut u8, len: usize) -> Result<(), String> {
    unsafe extern "system" {
        fn VirtualProtect(
            lpAddress: *mut std::ffi::c_void,
            dwSize: usize,
            flNewProtect: u32,
            lpflOldProtect: *mut u32,
        ) -> i32;
        fn FlushInstructionCache(
            hProcess: *mut std::ffi::c_void,
            lpBaseAddress: *const std::ffi::c_void,
            dwSize: usize,
        ) -> i32;
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
    }
    const PAGE_EXECUTE_READ: u32 = 0x20;
    let mut old_prot = 0u32;
    if unsafe { VirtualProtect(ptr as *mut _, len, PAGE_EXECUTE_READ, &mut old_prot) } == 0 {
        return Err(format!(
            "VirtualProtect failed: {}",
            io::Error::last_os_error()
        ));
    }
    unsafe { FlushInstructionCache(GetCurrentProcess(), ptr as *const _, len) };
    Ok(())
}

#[cfg(windows)]
unsafe fn make_readonly(ptr: *mut u8, len: usize) -> Result<(), String> {
    unsafe extern "system" {
        fn VirtualProtect(
            lpAddress: *mut std::ffi::c_void,
            dwSize: usize,
            flNewProtect: u32,
            lpflOldProtect: *mut u32,
        ) -> i32;
    }
    const PAGE_READONLY: u32 = 0x02;
    let mut old_prot = 0u32;
    if unsafe { VirtualProtect(ptr as *mut _, len, PAGE_READONLY, &mut old_prot) } == 0 {
        return Err(format!(
            "VirtualProtect failed: {}",
            io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(windows)]
unsafe fn free_near_block(ptr: *mut u8, _len: usize) {
    unsafe extern "system" {
        fn VirtualFree(lpAddress: *mut std::ffi::c_void, dwSize: usize, dwFreeType: u32) -> i32;
    }
    const MEM_RELEASE: u32 = 0x8000;
    unsafe { VirtualFree(ptr as *mut _, 0, MEM_RELEASE) };
}

#[cfg(windows)]
unsafe fn allocate_fallback_block(min_size: usize) -> io::Result<(*mut u8, usize)> {
    unsafe extern "system" {
        fn VirtualAlloc(
            lpAddress: *const std::ffi::c_void,
            dwSize: usize,
            flAllocationType: u32,
            flProtect: u32,
        ) -> *mut std::ffi::c_void;
    }
    const MEM_COMMIT: u32 = 0x1000;
    const MEM_RESERVE: u32 = 0x2000;
    const PAGE_READWRITE: u32 = 0x04;

    let gran = 64 * 1024;
    let alloc_size = (min_size + gran - 1) & !(gran - 1);
    let ptr = unsafe {
        VirtualAlloc(
            std::ptr::null(),
            alloc_size,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        )
    };
    if !ptr.is_null() {
        Ok((ptr as *mut u8, alloc_size))
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(unix)]
unsafe fn allocate_fallback_block(min_size: usize) -> io::Result<(*mut u8, usize)> {
    unsafe extern "C" {
        fn mmap(
            addr: *mut std::ffi::c_void,
            len: usize,
            prot: i32,
            flags: i32,
            fd: i32,
            offset: i64,
        ) -> *mut std::ffi::c_void;
    }
    const PROT_READ: i32 = 1;
    const PROT_WRITE: i32 = 2;
    const MAP_PRIVATE: i32 = 2;
    #[cfg(target_os = "macos")]
    const MAP_ANONYMOUS: i32 = 0x1000;
    #[cfg(not(target_os = "macos"))]
    const MAP_ANONYMOUS: i32 = 0x20;

    let page_size = 4096;
    let alloc_size = (min_size + page_size - 1) & !(page_size - 1);
    let ptr = unsafe {
        mmap(
            std::ptr::null_mut(),
            alloc_size,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS,
            -1,
            0,
        )
    };
    if ptr != usize::MAX as *mut std::ffi::c_void {
        Ok((ptr as *mut u8, alloc_size))
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(unix)]
unsafe fn allocate_near_block(min_size: usize) -> io::Result<(*mut u8, usize)> {
    unsafe extern "C" {
        fn mmap(
            addr: *mut std::ffi::c_void,
            len: usize,
            prot: i32,
            flags: i32,
            fd: i32,
            offset: i64,
        ) -> *mut std::ffi::c_void;
        fn munmap(addr: *mut std::ffi::c_void, len: usize) -> i32;
    }
    const PROT_READ: i32 = 1;
    const PROT_WRITE: i32 = 2;
    const MAP_PRIVATE: i32 = 2;
    #[cfg(target_os = "macos")]
    const MAP_ANONYMOUS: i32 = 0x1000;
    #[cfg(not(target_os = "macos"))]
    const MAP_ANONYMOUS: i32 = 0x20;

    let page_size = 4096;
    let alloc_size = (min_size + page_size - 1) & !(page_size - 1);
    let gran = 64 * 1024;
    let ref_addr = crate::codegen::cranelift::jit::datara_rt_out_int as *const () as usize;
    let max_steps = (800 * 1024 * 1024) / gran;

    for step in 1..max_steps {
        if ref_addr > step * gran + 0x10000 {
            let target = (ref_addr - step * gran) & !(gran - 1);
            let ptr = unsafe {
                mmap(
                    target as *mut _,
                    alloc_size,
                    PROT_READ | PROT_WRITE,
                    MAP_PRIVATE | MAP_ANONYMOUS,
                    -1,
                    0,
                )
            };
            if ptr != usize::MAX as *mut std::ffi::c_void {
                let diff = (ptr as usize as isize) - (ref_addr as isize);
                if diff.abs() < 0x3000_0000 {
                    return Ok((ptr as *mut u8, alloc_size));
                } else {
                    unsafe { munmap(ptr, alloc_size) };
                }
            }
        }
        let target = (ref_addr + step * gran) & !(gran - 1);
        let ptr = unsafe {
            mmap(
                target as *mut _,
                alloc_size,
                PROT_READ | PROT_WRITE,
                MAP_PRIVATE | MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        if ptr != usize::MAX as *mut std::ffi::c_void {
            let diff = (ptr as usize as isize) - (ref_addr as isize);
            if diff.abs() < 0x3000_0000 {
                return Ok((ptr as *mut u8, alloc_size));
            } else {
                unsafe { munmap(ptr, alloc_size) };
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::OutOfMemory,
        "Failed to allocate near JIT memory within 1GB of runtime",
    ))
}

#[cfg(unix)]
unsafe fn make_executable(ptr: *mut u8, len: usize) -> Result<(), String> {
    unsafe extern "C" {
        fn mprotect(addr: *mut std::ffi::c_void, len: usize, prot: i32) -> i32;
    }
    const PROT_READ: i32 = 1;
    const PROT_EXEC: i32 = 4;
    if unsafe { mprotect(ptr as *mut _, len, PROT_READ | PROT_EXEC) } != 0 {
        return Err(format!("mprotect failed: {}", io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(unix)]
unsafe fn make_readonly(ptr: *mut u8, len: usize) -> Result<(), String> {
    unsafe extern "C" {
        fn mprotect(addr: *mut std::ffi::c_void, len: usize, prot: i32) -> i32;
    }
    const PROT_READ: i32 = 1;
    if unsafe { mprotect(ptr as *mut _, len, PROT_READ) } != 0 {
        return Err(format!("mprotect failed: {}", io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(unix)]
unsafe fn free_near_block(ptr: *mut u8, len: usize) {
    unsafe extern "C" {
        fn munmap(addr: *mut std::ffi::c_void, len: usize) -> i32;
    }
    unsafe { munmap(ptr as *mut _, len) };
}
