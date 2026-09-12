//! Zero-Stall Live Code Hot-Reloading (< 100 us) for Cranelift JIT
//!
//! Provides atomic jump trampolines (JIT VTable) allowing individual functions
//! to be recompiled and swapped in-place mid-frame without restarting the game
//! or losing any scene, world, or player state.

use std::collections::HashMap;
use std::sync::RwLock;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::time::Instant;

/// Thread-safe table of atomic function pointers.
/// Each slot corresponds to an exported or internal function called via indirection.
pub struct JitTrampolineTable {
    trampolines: RwLock<Vec<AtomicPtr<u8>>>,
    name_to_index: RwLock<HashMap<String, usize>>,
}

impl Default for JitTrampolineTable {
    fn default() -> Self {
        Self::new()
    }
}

impl JitTrampolineTable {
    pub fn new() -> Self {
        Self {
            trampolines: RwLock::new(Vec::new()),
            name_to_index: RwLock::new(HashMap::new()),
        }
    }

    /// Registers a function entry in the trampoline table.
    pub fn register(&self, name: &str, initial_ptr: *const u8) -> usize {
        let mut names = self.name_to_index.write().unwrap();
        if let Some(&idx) = names.get(name) {
            let tramps = self.trampolines.read().unwrap();
            tramps[idx].store(initial_ptr as *mut u8, Ordering::Release);
            return idx;
        }

        let mut tramps = self.trampolines.write().unwrap();
        let idx = tramps.len();
        tramps.push(AtomicPtr::new(initial_ptr as *mut u8));
        names.insert(name.to_string(), idx);
        idx
    }

    /// Gets the current finalized executable function pointer by name.
    pub fn get_ptr(&self, name: &str) -> Option<*const u8> {
        let names = self.name_to_index.read().unwrap();
        let idx = *names.get(name)?;
        let tramps = self.trampolines.read().unwrap();
        let ptr = tramps[idx].load(Ordering::Acquire);
        if ptr.is_null() {
            None
        } else {
            Some(ptr as *const u8)
        }
    }

    /// Gets the current finalized executable function pointer by index (O(1)).
    pub fn get_ptr_by_index(&self, index: usize) -> Option<*const u8> {
        let tramps = self.trampolines.read().unwrap();
        if index >= tramps.len() {
            return None;
        }
        let ptr = tramps[index].load(Ordering::Acquire);
        if ptr.is_null() {
            None
        } else {
            Some(ptr as *const u8)
        }
    }

    /// Atomically swaps the function entry pointer in-place.
    /// Returns the exact swap latency in nanoseconds.
    pub fn hot_swap(&self, name: &str, new_ptr: *const u8) -> Result<u128, String> {
        let start = Instant::now();
        let names = self.name_to_index.read().unwrap();
        let idx = names.get(name).copied().ok_or_else(|| {
            format!(
                "Cannot hot-swap: function '{}' not found in trampoline table",
                name
            )
        })?;

        let tramps = self.trampolines.read().unwrap();
        tramps[idx].store(new_ptr as *mut u8, Ordering::Release);
        let elapsed_nanos = start.elapsed().as_nanos();
        Ok(elapsed_nanos)
    }

    /// Returns the number of registered function trampolines.
    pub fn len(&self) -> usize {
        self.trampolines.read().unwrap().len()
    }

    /// Returns true if the trampoline table is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
