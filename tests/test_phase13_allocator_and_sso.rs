//! Phase 13 Test Suite: Allocators & Value Representation (C-runtime)
//!
//! Validates:
//! 1. Thread-local size-class pool allocator and Box runtime functions.
//! 2. SSO (Small String Optimization): strings <= 22 bytes cause 0 heap allocations.
//! 3. Small-vector inline segment and stack-promoted collections.
//! 4. Adaptive layout optimizer: aggregate alignment (16/32/64) and field reordering.
//! 5. Microbenchmark: Alloc-heavy workload (List/Map/Box) achieves >= 1.30x speedup.

use forgen::dmir::Module;
use forgen::optimizer::adaptive::{AdaptationCategory, LayoutAdapter, SemanticAdaptationEngine};
use forgen::runtime::*;
use std::ffi::{CStr, CString};
use std::time::Instant;

#[test]
fn test_thread_local_size_class_pool_allocator_and_box() {
    // Test size-class pool allocator across all 10 size classes
    let sizes = [16, 32, 48, 64, 96, 128, 192, 256, 512, 1024];
    for &sz in &sizes {
        let ptr1 = unsafe { datara_rt_pool_alloc(sz) };
        assert!(!ptr1.is_null(), "Pool alloc for size {} failed", sz);
        unsafe { datara_rt_pool_free(ptr1, sz) };

        // Second alloc should recycle the block from free-list
        let ptr2 = unsafe { datara_rt_pool_alloc(sz) };
        assert!(!ptr2.is_null());
        assert_eq!(
            ptr1, ptr2,
            "Pool free-list must recycle block for size {}",
            sz
        );
        unsafe { datara_rt_pool_free(ptr2, sz) };
    }

    // Test Box allocation
    let b = unsafe { datara_rt_box_alloc(424242) };
    assert!(!b.is_null());
    let val = unsafe { datara_rt_box_get(b) };
    assert_eq!(val, 424242);
    unsafe { datara_rt_box_free(b) };
}

#[test]
fn test_sso_strings_zero_heap_allocations() {
    unsafe { datara_rt_reset_heap_alloc_count() };
    let initial_heap = unsafe { datara_rt_heap_alloc_count() };
    assert_eq!(initial_heap, 0);

    // Strings <= 22 bytes
    let small_strings = [
        "",
        "a",
        "hello",
        "1234567890",
        "datara_lang_fast",
        "twenty-two-bytes-exact", // exactly 22 bytes
    ];

    for s in &small_strings {
        let c_s = CString::new(*s).unwrap();
        let sso_ptr = unsafe { datara_rt_str_sso(c_s.as_ptr()) };
        assert!(!sso_ptr.is_null());
        let res_str = unsafe { CStr::from_ptr(sso_ptr) }.to_str().unwrap();
        assert_eq!(res_str, *s);
        let is_sso = unsafe { datara_rt_str_is_sso(c_s.as_ptr()) };
        assert_eq!(is_sso, 1, "String {:?} (len {}) must be SSO", s, s.len());
    }

    // Verify 0 heap allocations occurred for all strings <= 22 bytes
    let heap_after_sso = unsafe { datara_rt_heap_alloc_count() };
    assert_eq!(
        heap_after_sso, 0,
        "SSO strings <= 22 bytes must produce 0 heap allocations (got {})",
        heap_after_sso
    );

    // Non-SSO string > 22 bytes
    let large_s = "this-string-is-definitely-longer-than-twenty-two-bytes";
    let c_large = CString::new(large_s).unwrap();
    let is_sso = unsafe { datara_rt_str_is_sso(c_large.as_ptr()) };
    assert_eq!(is_sso, 0, "String > 22 bytes must not be SSO");

    let non_sso_ptr = unsafe { datara_rt_str_sso(c_large.as_ptr()) };
    assert!(!non_sso_ptr.is_null());
    let heap_after_large = unsafe { datara_rt_heap_alloc_count() };
    assert!(
        heap_after_large > 0,
        "Non-SSO string must trigger heap allocation"
    );
}

#[test]
fn test_small_vector_and_stack_collection() {
    // 1. Verify small-vector List
    let small_list = unsafe { forgen::codegen::cranelift::jit::datara_rt_list_create(8) };
    assert!(!small_list.is_null());
    let is_small = unsafe { datara_rt_list_is_small_vec(small_list) };
    assert_eq!(is_small, 1, "List of cap 8 must have small-vector flag");
    unsafe { forgen::codegen::cranelift::jit::datara_rt_list_free(small_list as *mut ()) };

    // 2. Verify stack-promoted List on a stack buffer
    let mut stack_buffer = [0u8; 256];
    let stack_list = unsafe { datara_rt_list_init_stack(stack_buffer.as_mut_ptr() as *mut (), 16) };
    assert!(!stack_list.is_null());

    // Check len
    unsafe {
        assert_eq!(*stack_list, 0);
        // Append elements
        for i in 1..=5 {
            forgen::codegen::cranelift::jit::datara_rt_list_append(stack_list, i * 10);
        }
        assert_eq!(*stack_list, 5);
        for i in 1..=5 {
            let elem = *stack_list.add(i as usize);
            assert_eq!(elem, i * 10);
        }
        // Freeing stack list is a safe no-op
        forgen::codegen::cranelift::jit::datara_rt_list_free(stack_list as *mut ());
    }
}

#[test]
fn test_layout_adapter_alignment_and_field_reordering() {
    let mut module = Module::new("particle_mod");

    // Define a class with unordered fields of different sizes
    // Bool (1B), Int (8B), Byte (1B), Float4 (16B SIMD), Float (8B)
    module.class_fields.insert(
        "Particle".to_string(),
        vec![
            "active".to_string(),
            "id".to_string(),
            "flag".to_string(),
            "position".to_string(),
            "velocity".to_string(),
        ],
    );

    module
        .class_field_types
        .insert("Particle.active".to_string(), "Bool".to_string());
    module
        .class_field_types
        .insert("Particle.id".to_string(), "Int".to_string());
    module
        .class_field_types
        .insert("Particle.flag".to_string(), "Byte".to_string());
    module
        .class_field_types
        .insert("Particle.position".to_string(), "Float4".to_string());
    module
        .class_field_types
        .insert("Particle.velocity".to_string(), "Float".to_string());

    let mut sae = SemanticAdaptationEngine::new("release");
    let layouts = LayoutAdapter::adapt_layout(&mut module, &mut sae.log);

    let particle_layout = layouts
        .get("Particle")
        .expect("Layout for Particle must exist");

    // Must be 64-byte aligned because of Float4 SIMD field
    assert_eq!(
        particle_layout.alignment, 64,
        "Particle with Float4 must be 64-byte aligned (got {})",
        particle_layout.alignment
    );

    // Fields must be reordered: 16B (position) -> 8B (id, velocity) -> 1B (active, flag)
    let reordered = &particle_layout.ordered_fields;
    assert_eq!(
        reordered[0], "position",
        "Float4 field must be ranked first"
    );
    assert!(reordered[1] == "id" || reordered[1] == "velocity");
    assert!(reordered[2] == "id" || reordered[2] == "velocity");
    assert!(reordered[3] == "active" || reordered[3] == "flag");
    assert!(reordered[4] == "active" || reordered[4] == "flag");

    // Verify AdaptationRecord logged
    let has_layout_record = sae.log.records.iter().any(|r| {
        r.category == AdaptationCategory::Layout
            && r.candidate == "Particle"
            && r.decision.contains("Aligned 64 bytes")
    });
    assert!(
        has_layout_record,
        "SAE log must record 64-byte layout adaptation"
    );
}

#[test]
fn test_alloc_heavy_microbenchmark() {
    const ITERATIONS: usize = 200_000;

    // 1. Baseline: libc malloc + free
    let t0 = Instant::now();
    for _ in 0..ITERATIONS {
        let p = unsafe { libc_malloc(32) };
        unsafe { libc_free(p) };
        let m = unsafe { libc_malloc(96) };
        unsafe { libc_free(m) };
    }
    let baseline_duration = t0.elapsed();

    // 2. Thread-local size-class pool allocator (Datara C-runtime)
    let t1 = Instant::now();
    for _ in 0..ITERATIONS {
        let b = unsafe { datara_rt_box_alloc(99) };
        unsafe { datara_rt_box_free(b) };
        let p = unsafe { datara_rt_pool_alloc(96) };
        unsafe { datara_rt_pool_free(p, 96) };
    }
    let pool_duration = t1.elapsed();

    let baseline_ms = baseline_duration.as_secs_f64() * 1000.0;
    let pool_ms = pool_duration.as_secs_f64() * 1000.0;
    let speedup = baseline_ms / pool_ms.max(0.0001);

    println!(
        "[Alloc-Heavy Benchmark] {} ops | Baseline malloc: {:.2} ms | Datara pool: {:.2} ms | Speedup: {:.2}x",
        ITERATIONS * 2,
        baseline_ms,
        pool_ms,
        speedup
    );

    assert!(
        speedup >= 1.30,
        "Alloc-heavy benchmark must be >= 1.30x faster than baseline (got {:.2}x)",
        speedup
    );
}

// Helpers for raw libc malloc/free comparison
unsafe extern "C" {
    #[link_name = "malloc"]
    fn libc_malloc(size: usize) -> *mut u8;
    #[link_name = "free"]
    fn libc_free(ptr: *mut u8);
}
