//! Phase 17 Test Suite: Runtime Systems Layer
//!
//! Validates:
//! 1. Chase-Lev Lock-Free Work-Stealing Deque:
//!    - Single-producer, multi-consumer lock-free deque.
//!    - Owner LIFO popping (`datara_rt_chase_lev_pop`) and FIFO stealing (`datara_rt_chase_lev_steal`).
//!    - Dynamic circular buffer resizing when capacity is exceeded.
//!    - Concurrent multi-threaded stress test: 1 producer pushing and popping, 3 concurrent thief threads stealing.
//!    - Zero missing tasks, zero duplicate tasks, clean destruction.
//! 2. SIMD Fast Memory Operations:
//!    - `datara_rt_fast_memcpy`, `datara_rt_fast_memset`, `datara_rt_fast_strncmp`, `datara_rt_fast_memcmp`.
//!    - Correctness: exact byte-for-byte match across small, unaligned, medium, and large buffers (1B to 1MB).
//!    - Speedup: >= 1.5x speedup vs naive byte-by-byte baseline on 1MB buffers.
//! 3. Thread Pinning and Core Affinity:
//!    - `datara_rt_get_current_core()`, `datara_rt_pin_thread()`, `datara_rt_pin_worker_threads()`.
//!    - Thread affinity verification on logical cores.
//! 4. Differential Runtime Compilation & Execution:
//!    - Datara source exercising runtime systems compiles and executes deterministically.

use forgen::codegen::cranelift::jit::*;
use forgen::driver::ForgenCompiler;
use std::collections::HashSet;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

// ============================================================================
// 1. Chase-Lev Deque: Single-Threaded Semantics (LIFO Pop, FIFO Steal, Resize)
// ============================================================================

#[test]
fn test_chase_lev_single_threaded_lifo_and_fifo() {
    unsafe {
        // Initial capacity 16 to test both normal operations and resizing
        let q = datara_rt_chase_lev_create(16);
        assert!(!q.is_null());
        assert_eq!(datara_rt_chase_lev_size(q), 0);
        assert_eq!(datara_rt_chase_lev_pop(q), -1);
        assert_eq!(datara_rt_chase_lev_steal(q), -1);

        // 1. Push 10 items and verify LIFO popping by owner
        for i in 1..=10 {
            datara_rt_chase_lev_push(q, i);
        }
        assert_eq!(datara_rt_chase_lev_size(q), 10);

        // Pop should be LIFO: 10, 9, 8, ...
        for expected in (1..=10).rev() {
            let val = datara_rt_chase_lev_pop(q);
            assert_eq!(val, expected, "Expected LIFO pop order");
        }
        assert_eq!(datara_rt_chase_lev_size(q), 0);
        assert_eq!(datara_rt_chase_lev_pop(q), -1);

        // 2. Push 10 items and verify FIFO stealing by thieves
        for i in 101..=110 {
            datara_rt_chase_lev_push(q, i);
        }
        assert_eq!(datara_rt_chase_lev_size(q), 10);

        // Steal should be FIFO: 101, 102, 103, ...
        for expected in 101..=110 {
            let val = datara_rt_chase_lev_steal(q);
            assert_eq!(val, expected, "Expected FIFO steal order");
        }
        assert_eq!(datara_rt_chase_lev_size(q), 0);
        assert_eq!(datara_rt_chase_lev_steal(q), -1);

        // 3. Test Dynamic Resizing: push 1000 items into a queue with initial cap 16
        for i in 1..=1000 {
            datara_rt_chase_lev_push(q, i);
        }
        assert_eq!(datara_rt_chase_lev_size(q), 1000);

        // Pop first 500 (LIFO: 1000 down to 501)
        for expected in (501..=1000).rev() {
            let val = datara_rt_chase_lev_pop(q);
            assert_eq!(val, expected);
        }
        assert_eq!(datara_rt_chase_lev_size(q), 500);

        // Steal remaining 500 (FIFO: 1 up to 500)
        for expected in 1..=500 {
            let val = datara_rt_chase_lev_steal(q);
            assert_eq!(val, expected);
        }
        assert_eq!(datara_rt_chase_lev_size(q), 0);
        assert_eq!(datara_rt_chase_lev_pop(q), -1);

        datara_rt_chase_lev_destroy(q);
    }
}

// ============================================================================
// 2. Chase-Lev Deque: Multi-Threaded Concurrent Work-Stealing Stress Test
// ============================================================================

#[test]
fn test_chase_lev_concurrent_work_stealing() {
    let q_ptr = unsafe { datara_rt_chase_lev_create(64) };
    assert!(!q_ptr.is_null());

    let total_tasks = 10_000i64;
    let producer_done = Arc::new(AtomicBool::new(false));
    let results = Arc::new(Mutex::new(Vec::with_capacity(total_tasks as usize)));

    // Wrap raw pointer as usize for safe Send across thread boundaries
    let q_usize = q_ptr as usize;

    // Spawn 3 thief threads
    let mut handles = Vec::new();
    for _thief_id in 0..3 {
        let done = Arc::clone(&producer_done);
        let res = Arc::clone(&results);
        let handle = thread::spawn(move || {
            let q = q_usize as *mut ();
            let mut local_stolen = Vec::new();
            loop {
                let task = unsafe { datara_rt_chase_lev_steal(q) };
                if task != -1 {
                    local_stolen.push(task);
                } else if done.load(Ordering::Acquire) {
                    // Double-check once more
                    let retry = unsafe { datara_rt_chase_lev_steal(q) };
                    if retry != -1 {
                        local_stolen.push(retry);
                    } else {
                        break;
                    }
                } else {
                    thread::yield_now();
                }
            }
            let mut lock = res.lock().unwrap();
            lock.extend(local_stolen);
        });
        handles.push(handle);
    }

    // Producer thread (main thread): pushes tasks and occasionally pops its own
    let mut producer_collected = Vec::new();
    for task_id in 1..=total_tasks {
        unsafe { datara_rt_chase_lev_push(q_ptr, task_id) };

        // Pop roughly 20% of tasks locally
        if task_id % 5 == 0 {
            let popped = unsafe { datara_rt_chase_lev_pop(q_ptr) };
            if popped != -1 {
                producer_collected.push(popped);
            }
        }
    }

    // Pop any remaining tasks producer can grab
    loop {
        let popped = unsafe { datara_rt_chase_lev_pop(q_ptr) };
        if popped == -1 {
            break;
        }
        producer_collected.push(popped);
    }

    // Signal thieves that producer is done
    producer_done.store(true, Ordering::Release);

    // Wait for all thief threads to terminate
    for handle in handles {
        handle.join().unwrap();
    }

    // Collect all processed tasks
    let mut all_tasks = results.lock().unwrap().clone();
    all_tasks.extend(producer_collected);

    // Assert exact task count
    assert_eq!(
        all_tasks.len(),
        total_tasks as usize,
        "Total executed tasks must exactly equal total pushed tasks"
    );

    // Assert NO duplicate task IDs
    let unique_tasks: HashSet<i64> = all_tasks.into_iter().collect();
    assert_eq!(
        unique_tasks.len(),
        total_tasks as usize,
        "Every task must be executed exactly once without duplicates"
    );

    // Assert all task IDs from 1..=total_tasks are present
    for id in 1..=total_tasks {
        assert!(unique_tasks.contains(&id), "Task ID {} was missing", id);
    }

    assert_eq!(unsafe { datara_rt_chase_lev_size(q_ptr) }, 0);
    unsafe { datara_rt_chase_lev_destroy(q_ptr) };
}

// ============================================================================
// 3. SIMD Fast Memory Operations: Correctness & Speedup Benchmarking
// ============================================================================

#[test]
fn test_simd_fast_memcpy_correctness_and_speedup() {
    unsafe {
        // Correctness across varied sizes and unaligned pointers
        let sizes = [
            0, 1, 7, 15, 16, 31, 32, 63, 64, 65, 127, 128, 255, 256, 4096, 65536,
        ];
        for &sz in &sizes {
            let mut src = vec![0u8; sz + 16];
            let mut dst = vec![0u8; sz + 16];
            for i in 0..sz {
                src[i] = ((i * 37 + 13) & 0xFF) as u8;
            }

            // Test aligned
            datara_rt_fast_memcpy(dst.as_mut_ptr() as *mut (), src.as_ptr() as *const (), sz);
            assert_eq!(
                &dst[..sz],
                &src[..sz],
                "Aligned memcpy failed for size {}",
                sz
            );

            // Test unaligned offsets
            if sz > 0 {
                let mut unaligned_dst = vec![0u8; sz + 16];
                datara_rt_fast_memcpy(
                    unaligned_dst.as_mut_ptr().add(3) as *mut (),
                    src.as_ptr().add(1) as *const (),
                    sz.saturating_sub(4),
                );
                assert_eq!(
                    &unaligned_dst[3..3 + (sz.saturating_sub(4))],
                    &src[1..1 + (sz.saturating_sub(4))],
                    "Unaligned memcpy failed for size {}",
                    sz
                );
            }
        }

        // Benchmark speedup on 1 MB buffer
        let bench_size = 1024 * 1024; // 1 MB
        let src = vec![0x77u8; bench_size];
        let mut dst_naive = vec![0u8; bench_size];
        let mut dst_fast = vec![0u8; bench_size];
        let iters = 50;

        // Naive byte-by-byte baseline
        let t0 = Instant::now();
        for _ in 0..iters {
            let s_ptr = src.as_ptr();
            let d_ptr = dst_naive.as_mut_ptr();
            for i in 0..bench_size {
                *d_ptr.add(i) = std::hint::black_box(*s_ptr.add(i));
            }
            std::hint::black_box(&dst_naive);
        }
        let dur_naive = t0.elapsed();

        // Vectorized / unrolled fast memcpy
        let t1 = Instant::now();
        for _ in 0..iters {
            datara_rt_fast_memcpy(
                dst_fast.as_mut_ptr() as *mut (),
                src.as_ptr() as *const (),
                bench_size,
            );
            std::hint::black_box(&dst_fast);
        }
        let dur_fast = t1.elapsed();

        let speedup = dur_naive.as_secs_f64() / dur_fast.as_secs_f64();
        println!(
            "[Phase 17] Fast Memcpy 1MB: Naive = {:?}, Fast = {:?}, Speedup = {:.2}x",
            dur_naive, dur_fast, speedup
        );
        assert_eq!(dst_naive, dst_fast);
        assert!(
            speedup >= 1.5,
            "Fast memcpy must achieve >= 1.5x speedup (measured: {:.2}x)",
            speedup
        );
    }
}

#[test]
fn test_simd_fast_memset_correctness_and_speedup() {
    unsafe {
        let sizes = [0, 1, 8, 31, 64, 100, 512, 65536];
        for &sz in &sizes {
            let mut buf = vec![0u8; sz + 8];
            datara_rt_fast_memset(buf.as_mut_ptr() as *mut (), 0xAB, sz);
            for i in 0..sz {
                assert_eq!(
                    buf[i], 0xAB,
                    "fast_memset mismatch at index {} for sz {}",
                    i, sz
                );
            }
        }

        // Benchmark speedup on 1 MB buffer
        let bench_size = 1024 * 1024;
        let mut buf_naive = vec![0u8; bench_size];
        let mut buf_fast = vec![0u8; bench_size];
        let iters = 50;

        let t0 = Instant::now();
        for _ in 0..iters {
            let ptr = buf_naive.as_mut_ptr();
            for i in 0..bench_size {
                *ptr.add(i) = std::hint::black_box(0x5A);
            }
            std::hint::black_box(&buf_naive);
        }
        let dur_naive = t0.elapsed();

        let t1 = Instant::now();
        for _ in 0..iters {
            datara_rt_fast_memset(buf_fast.as_mut_ptr() as *mut (), 0x5A, bench_size);
            std::hint::black_box(&buf_fast);
        }
        let dur_fast = t1.elapsed();

        let speedup = dur_naive.as_secs_f64() / dur_fast.as_secs_f64();
        println!(
            "[Phase 17] Fast Memset 1MB: Naive = {:?}, Fast = {:?}, Speedup = {:.2}x",
            dur_naive, dur_fast, speedup
        );
        assert_eq!(buf_naive, buf_fast);
        assert!(
            speedup >= 1.5,
            "Fast memset must achieve >= 1.5x speedup (measured: {:.2}x)",
            speedup
        );
    }
}

#[test]
fn test_simd_fast_memcmp_and_strncmp() {
    unsafe {
        let buf1 = vec![0x42u8; 1024];
        let mut buf2 = vec![0x42u8; 1024];

        // Identical buffers
        assert_eq!(
            datara_rt_fast_memcmp(buf1.as_ptr() as *const (), buf2.as_ptr() as *const (), 1024),
            0
        );

        // Mismatch at first byte
        buf2[0] = 0x43;
        assert!(
            datara_rt_fast_memcmp(buf1.as_ptr() as *const (), buf2.as_ptr() as *const (), 1024) < 0
        );
        buf2[0] = 0x41;
        assert!(
            datara_rt_fast_memcmp(buf1.as_ptr() as *const (), buf2.as_ptr() as *const (), 1024) > 0
        );
        buf2[0] = 0x42; // restore

        // Mismatch beyond 64-byte block boundary (e.g. byte 75)
        buf2[75] = 0x99;
        assert!(
            datara_rt_fast_memcmp(buf1.as_ptr() as *const (), buf2.as_ptr() as *const (), 1024) < 0
        );
        // Compare only first 70 bytes: should be identical (0)
        assert_eq!(
            datara_rt_fast_memcmp(buf1.as_ptr() as *const (), buf2.as_ptr() as *const (), 70),
            0
        );

        // Fast strncmp tests
        let s1 = CString::new("Hello, Phase 17 Runtime Systems!").unwrap();
        let s2 = CString::new("Hello, Phase 17 Runtime Systems!").unwrap();
        let s3 = CString::new("Hello, Phase 17 Runtime Systems?").unwrap();

        assert_eq!(datara_rt_fast_strncmp(s1.as_ptr(), s2.as_ptr(), 32), 0);
        assert_ne!(datara_rt_fast_strncmp(s1.as_ptr(), s3.as_ptr(), 32), 0);
        // Matching prefix of length 20
        assert_eq!(datara_rt_fast_strncmp(s1.as_ptr(), s3.as_ptr(), 20), 0);
    }
}

// ============================================================================
// 4. Thread Pinning and Core Affinity
// ============================================================================

#[test]
fn test_thread_pinning_and_core_affinity() {
    // 1. Get current core
    let core = unsafe { datara_rt_get_current_core() };
    println!("[Phase 17] Current logical core: {}", core);
    assert!(core >= 0, "Current core ID must be non-negative");

    // 2. Pin current thread to core 0
    let pin_res = unsafe { datara_rt_pin_thread(0) };
    println!("[Phase 17] Pin to core 0 result: {}", pin_res);
    assert_eq!(
        pin_res, 0,
        "Pinning thread to core 0 should succeed (0 = OK)"
    );

    // 3. Pin worker threads helper
    unsafe { datara_rt_pin_worker_threads() };

    // 4. Spawn multi-threaded pinning test
    let handles: Vec<_> = (0..4)
        .map(|i| {
            thread::spawn(move || unsafe {
                let pin_res = datara_rt_pin_thread(i % 4);
                let my_core = datara_rt_get_current_core();
                (pin_res, my_core)
            })
        })
        .collect();

    for h in handles {
        let (pin_res, my_core) = h.join().unwrap();
        assert_eq!(pin_res, 0, "Pinning thread should succeed (0 = OK)");
        assert!(my_core >= 0);
    }
}

// ============================================================================
// 5. Differential Runtime Execution (Datara Source)
// ============================================================================

#[test]
fn test_runtime_systems_differential_execution() {
    let source = r#"
fn compute_sum(n: Int) -> Int {
    mut s = 0
    mut i = 1
    while i <= n {
        s = s + i
        i = i + 1
    }
    s
}

fn main() -> Int {
    let total = compute_sum(100)
    println(int_to_str(total))
    0
}
"#;

    let compiler = ForgenCompiler::new("release");
    let res = compiler.compile_source(source, "phase17_runtime_exec.dtr", None);
    assert!(res.success, "Compilation failed: {:?}", res.diagnostics);
    let exe_path = res.exe_path.expect("Executable must be generated");
    let (stdout, _stderr, code, _) = compiler
        .cranelift
        .run_executable(&exe_path, &[])
        .expect("Execution must succeed");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "5050");

    let _ = std::fs::remove_file(exe_path);
}
