//! Phase 4: 3-Tier Zero-Lock Allocator Tests
//!
//! Verifies:
//! 1. Tier 0: Compile-time escape analysis & stack promotion of non-escaping collections.
//! 2. Tier 1: 2MB Thread-Local Ephemeral Bump Arena with checkpoint & reset.
//! 3. Tier 2: 64-bit Bitmask Slab Cache with O(1) tzcnt bit-scan and zero lock contention.
//! 4. Tier 3: 2MB Huge Pages with graceful OS fallback.
//! 5. Unified 3-Tier dynamic router (datara_rt_tier_alloc / datara_rt_tier_free).
//! 6. Multi-threaded benchmark: zero-lock thread-local scaling vs malloc/free lock contention.

use forgen::driver::ForgenCompiler;

unsafe extern "C" {
    fn datara_rt_arena_alloc(bytes: i64) -> *mut u8;
    fn datara_rt_arena_checkpoint() -> i64;
    fn datara_rt_arena_reset(saved_top: i64);
    fn datara_rt_arena_remaining() -> i64;

    fn datara_rt_slab_alloc(bytes: usize) -> *mut u8;
    fn datara_rt_slab_free(ptr: *mut u8, bytes: usize);

    fn datara_rt_huge_page_alloc(bytes: usize) -> *mut u8;
    fn datara_rt_huge_page_free(ptr: *mut u8, bytes: usize);

    fn datara_rt_tier_alloc(bytes: usize) -> *mut u8;
    fn datara_rt_tier_free(ptr: *mut u8, bytes: usize);

    fn datara_rt_benchmark_tier_allocator(threads: i64, iters: i64, size: i64) -> f64;
    fn datara_rt_benchmark_malloc_free(threads: i64, iters: i64, size: i64) -> f64;
}

#[test]
fn test_v120_tier1_bump_arena() {
    unsafe {
        let cp_start = datara_rt_arena_checkpoint();
        let rem_start = datara_rt_arena_remaining();

        let ptr1 = datara_rt_arena_alloc(256);
        assert!(!ptr1.is_null(), "Arena allocation must succeed");
        std::ptr::write_bytes(ptr1, 0xAA, 256);

        let cp_mid = datara_rt_arena_checkpoint();
        assert!(
            cp_mid > cp_start,
            "Checkpoint should advance after allocation"
        );

        let ptr2 = datara_rt_arena_alloc(512);
        assert!(!ptr2.is_null());
        std::ptr::write_bytes(ptr2, 0xBB, 512);

        // Reset back to cp_mid
        datara_rt_arena_reset(cp_mid);
        assert_eq!(datara_rt_arena_checkpoint(), cp_mid);

        // Reset back to start
        datara_rt_arena_reset(cp_start);
        assert_eq!(datara_rt_arena_checkpoint(), cp_start);
        assert_eq!(datara_rt_arena_remaining(), rem_start);
    }
}

#[test]
fn test_v120_tier2_bitmask_slab_cache() {
    unsafe {
        const SLOTS: usize = 64;
        let mut ptrs: Vec<*mut u8> = Vec::with_capacity(SLOTS);

        // 1. Allocate all 64 slots of 32-byte class
        for i in 0..SLOTS {
            let p = datara_rt_slab_alloc(32);
            assert!(!p.is_null());
            *p = (i & 0xFF) as u8;
            ptrs.push(p);
        }

        // Verify pointers are all distinct
        for i in 0..SLOTS {
            for j in (i + 1)..SLOTS {
                assert_ne!(ptrs[i], ptrs[j], "Slab slots must be distinct");
            }
        }

        // 2. Free all slots
        for &p in &ptrs {
            datara_rt_slab_free(p, 32);
        }

        // 3. Re-allocate 64 slots: bitmask tzcnt should reuse the freed slots
        let mut ptrs_reused = Vec::with_capacity(SLOTS);
        for _ in 0..SLOTS {
            let p = datara_rt_slab_alloc(32);
            assert!(!p.is_null());
            ptrs_reused.push(p);
        }

        // Clean up
        for p in ptrs_reused {
            datara_rt_slab_free(p, 32);
        }
    }
}

#[test]
fn test_v120_tier3_huge_pages() {
    unsafe {
        let size = 2 * 1024 * 1024; // 2MB
        let p = datara_rt_huge_page_alloc(size);
        assert!(
            !p.is_null(),
            "Huge page allocation must succeed (or fallback)"
        );

        // Touch the beginning and end of the 2MB page
        *p = 0x42;
        *p.add(size - 1) = 0x24;
        assert_eq!(*p, 0x42);
        assert_eq!(*p.add(size - 1), 0x24);

        datara_rt_huge_page_free(p, size);
    }
}

#[test]
fn test_v120_unified_tier_allocator() {
    unsafe {
        // Small -> Tier 2 Slab (32 bytes)
        let p_small = datara_rt_tier_alloc(32);
        assert!(!p_small.is_null());
        *p_small = 10;
        datara_rt_tier_free(p_small, 32);

        // Medium -> Pool (4096 bytes)
        let p_med = datara_rt_tier_alloc(4096);
        assert!(!p_med.is_null());
        *p_med = 20;
        datara_rt_tier_free(p_med, 4096);

        // Large -> Tier 3 Huge Page (2MB)
        let p_large = datara_rt_tier_alloc(2 * 1024 * 1024);
        assert!(!p_large.is_null());
        *p_large = 30;
        datara_rt_tier_free(p_large, 2 * 1024 * 1024);
    }
}

#[test]
fn test_v120_zero_lock_multithreaded_speedup() {
    unsafe {
        let threads = 4i64;
        let iters = 200_000i64;
        let alloc_size = 64i64;

        let time_tier = datara_rt_benchmark_tier_allocator(threads, iters, alloc_size);
        let time_malloc = datara_rt_benchmark_malloc_free(threads, iters, alloc_size);

        let speedup = time_malloc / time_tier;
        println!(
            "[Zero-Lock Multi-Threaded Allocator] Malloc: {:.4}s, Tier Allocator: {:.4}s, Speedup: {:.2}x",
            time_malloc, time_tier, speedup
        );

        // Zero-lock thread-local slab must be significantly faster than global malloc under thread contention
        assert!(
            speedup >= 1.20,
            "Zero-lock tier allocator should outperform global lock-contended malloc (got {:.2}x)",
            speedup
        );
    }
}

#[test]
fn test_v120_tier0_escape_analysis_stack_promotion() {
    let source = r#"
fn non_escaping_local() -> Int {
    val items = [10, 20, 30, 40]
    return items[0] + items[1]
}

fn main() {
    val r = non_escaping_local()
    out r
}
"#;

    let compiler = ForgenCompiler::new("domain").with_llvm(true);
    let res = compiler.compile_source(source, "stack_promote.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.diagnostics);

    let report = res
        .optimization_report
        .expect("Optimization report must be present");
    let has_stack_promoted = report
        .adaptation_records
        .iter()
        .any(|r| r.decision == "StackPromotedCollection");
    assert!(
        has_stack_promoted,
        "Tier 0 escape analysis must promote non-escaping collection to stack-local memory"
    );
}
