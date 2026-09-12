//! Phase 10 (v1.2.0): Runtime Systems Layer
//!
//! Validates:
//! 1. SIMD fast memory operations (`fast_memcpy`, `fast_memset`, `fast_memcmp`, `fast_strncmp`)
//!    with speedup >= 1.5x vs naive baseline and byte-for-byte correctness.
//! 2. SSO strings <= 22 bytes with verified 0 heap allocations.
//! 3. Chase-Lev lock-free work-stealing deque with LIFO owner pop, FIFO thief steal, and dynamic resizing.
//! 4. Multi-threaded work-stealing stress test (1 producer + 3 thieves, 0 lost/duplicate tasks).
//! 5. Thread pinning and core affinity APIs.
//! 6. End-to-end compilation and deterministic execution.

use forgen::codegen::cranelift::jit::*;
use forgen::driver::ForgenCompiler;
use std::collections::HashSet;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

#[test]
fn test_v120_runtime_simd_fast_memory_ops() {
    unsafe {
        // Correctness across multiple buffer sizes
        let sizes = [0, 1, 7, 16, 33, 64, 128, 255, 1024, 65536];
        for &sz in &sizes {
            let src: Vec<u8> = (0..sz).map(|i| (i * 7 + 13) as u8).collect();
            let mut dst = vec![0u8; sz];
            if sz > 0 {
                datara_rt_fast_memcpy(dst.as_mut_ptr() as *mut (), src.as_ptr() as *const (), sz);
                assert_eq!(dst, src, "fast_memcpy failed for size {}", sz);

                let cmp =
                    datara_rt_fast_memcmp(dst.as_ptr() as *const (), src.as_ptr() as *const (), sz);
                assert_eq!(cmp, 0, "fast_memcmp must report identical for size {}", sz);
            }
        }

        // Benchmark speedup on 1 MB buffer
        let bench_size = 1024 * 1024;
        let src: Vec<u8> = (0..bench_size).map(|i| (i ^ 0x5A) as u8).collect();
        let mut dst_naive = vec![0u8; bench_size];
        let mut dst_fast = vec![0u8; bench_size];
        let iters = 40;

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
            "[Phase 10] Fast Memcpy 1MB: Naive = {:?}, Fast = {:?}, Speedup = {:.2}x",
            dur_naive, dur_fast, speedup
        );
        assert_eq!(dst_naive, dst_fast);
        assert!(
            speedup >= 1.5,
            "Fast memcpy must achieve >= 1.5x speedup (measured: {:.2}x)",
            speedup
        );

        // Fast memset test
        let mut memset_buf = vec![0u8; bench_size];
        datara_rt_fast_memset(memset_buf.as_mut_ptr() as *mut (), 0xEF, bench_size);
        for (idx, &b) in memset_buf.iter().enumerate() {
            assert_eq!(b, 0xEF, "fast_memset mismatch at byte {}", idx);
        }
    }
}

#[test]
fn test_v120_runtime_sso_strings_zero_heap() {
    unsafe {
        datara_rt_reset_heap_alloc_count();
        assert_eq!(datara_rt_heap_alloc_count(), 0);

        let sso_samples = [
            "",
            "a",
            "hello",
            "datara-v120",
            "123456789012345678901",
            "1234567890123456789012", // exactly 22 bytes
        ];

        for &s in &sso_samples {
            assert!(s.len() <= 22);
            let c_str = CString::new(s).unwrap();
            let is_sso = datara_rt_str_is_sso(c_str.as_ptr());
            assert_eq!(
                is_sso,
                1,
                "String '{}' (len {}) must be SSO eligible",
                s,
                s.len()
            );

            let sso_ptr = datara_rt_str_sso(c_str.as_ptr());
            assert!(!sso_ptr.is_null());

            let roundtrip = std::ffi::CStr::from_ptr(sso_ptr as *const std::os::raw::c_char)
                .to_str()
                .unwrap();
            assert_eq!(roundtrip, s, "SSO roundtrip must match original content");
        }

        // Must produce 0 heap allocations
        let heap_count = datara_rt_heap_alloc_count();
        assert_eq!(
            heap_count, 0,
            "SSO strings <= 22 bytes must produce 0 heap allocations (got {})",
            heap_count
        );

        // Non-SSO string > 22 bytes
        let long_str = "this string is definitely longer than twenty-two characters";
        let c_long = CString::new(long_str).unwrap();
        let is_sso_long = datara_rt_str_is_sso(c_long.as_ptr());
        assert_eq!(is_sso_long, 0, "String > 22 bytes must not be SSO");

        let long_ptr = datara_rt_str_sso(c_long.as_ptr());
        assert!(!long_ptr.is_null());
        let heap_after_long = datara_rt_heap_alloc_count();
        assert!(
            heap_after_long > 0,
            "Non-SSO string must trigger heap allocation"
        );
    }
}

#[test]
fn test_v120_runtime_chase_lev_deque_lifo_fifo_and_resizing() {
    unsafe {
        let q = datara_rt_chase_lev_create(16);
        assert!(!q.is_null());
        assert_eq!(datara_rt_chase_lev_size(q), 0);
        assert_eq!(datara_rt_chase_lev_pop(q), -1);
        assert_eq!(datara_rt_chase_lev_steal(q), -1);

        // LIFO push & pop
        for i in 1..=20 {
            datara_rt_chase_lev_push(q, i);
        }
        assert_eq!(datara_rt_chase_lev_size(q), 20);

        for expected in (1..=20).rev() {
            let val = datara_rt_chase_lev_pop(q);
            assert_eq!(val, expected, "LIFO pop must yield newest element");
        }
        assert_eq!(datara_rt_chase_lev_size(q), 0);

        // FIFO stealing
        for i in 100..=120 {
            datara_rt_chase_lev_push(q, i);
        }
        for expected in 100..=120 {
            let val = datara_rt_chase_lev_steal(q);
            assert_eq!(val, expected, "FIFO steal must yield oldest element");
        }
        assert_eq!(datara_rt_chase_lev_size(q), 0);

        // Dynamic resizing: push 2000 items with initial capacity 16
        for i in 1..=2000 {
            datara_rt_chase_lev_push(q, i);
        }
        assert_eq!(datara_rt_chase_lev_size(q), 2000);

        // Pop first 1000
        for expected in (1001..=2000).rev() {
            let val = datara_rt_chase_lev_pop(q);
            assert_eq!(val, expected);
        }

        // Steal remaining 1000
        for expected in 1..=1000 {
            let val = datara_rt_chase_lev_steal(q);
            assert_eq!(val, expected);
        }
        assert_eq!(datara_rt_chase_lev_size(q), 0);

        datara_rt_chase_lev_destroy(q);
    }
}

#[test]
fn test_v120_runtime_chase_lev_multithreaded_work_stealing() {
    unsafe {
        let total_tasks: i64 = 20_000;
        let q = datara_rt_chase_lev_create(64);
        let q_ptr = q as usize;

        let stolen_tasks = Arc::new(Mutex::new(Vec::new()));
        let popped_tasks = Arc::new(Mutex::new(Vec::new()));
        let done_flag = Arc::new(AtomicBool::new(false));

        // Spawn 3 thief threads
        let mut thieves = Vec::new();
        for _ in 0..3 {
            let stolen_clone = Arc::clone(&stolen_tasks);
            let done_clone = Arc::clone(&done_flag);
            let handle = thread::spawn(move || {
                let mut local_stolen = Vec::new();
                let queue = q_ptr as *mut ();
                while !done_clone.load(Ordering::Acquire) {
                    let task = datara_rt_chase_lev_steal(queue);
                    if task >= 0 {
                        local_stolen.push(task);
                    } else {
                        thread::yield_now();
                    }
                }
                // Drain any residual tasks
                loop {
                    let task = datara_rt_chase_lev_steal(queue);
                    if task >= 0 {
                        local_stolen.push(task);
                    } else {
                        break;
                    }
                }
                let mut guard = stolen_clone.lock().unwrap();
                guard.extend(local_stolen);
            });
            thieves.push(handle);
        }

        // Producer thread pushes all tasks and periodically pops
        let mut local_popped = Vec::new();
        for task_id in 0..total_tasks {
            datara_rt_chase_lev_push(q, task_id);
            if task_id % 4 == 0 {
                let popped = datara_rt_chase_lev_pop(q);
                if popped >= 0 {
                    local_popped.push(popped);
                }
            }
        }

        // Drain remainder via pop
        loop {
            let popped = datara_rt_chase_lev_pop(q);
            if popped >= 0 {
                local_popped.push(popped);
            } else {
                break;
            }
        }
        popped_tasks.lock().unwrap().extend(local_popped);

        done_flag.store(true, Ordering::Release);
        for handle in thieves {
            handle.join().unwrap();
        }

        let all_stolen = stolen_tasks.lock().unwrap().clone();
        let all_popped = popped_tasks.lock().unwrap().clone();
        let total_processed = all_stolen.len() + all_popped.len();

        println!(
            "[Phase 10] Chase-Lev Concurrent: Total = {}, Popped = {}, Stolen = {}",
            total_processed,
            all_popped.len(),
            all_stolen.len()
        );

        assert_eq!(
            total_processed as i64, total_tasks,
            "Total processed tasks must equal pushed tasks"
        );

        let mut seen = HashSet::new();
        for &t in all_stolen.iter().chain(all_popped.iter()) {
            assert!(seen.insert(t), "Duplicate task {} detected in execution", t);
        }

        datara_rt_chase_lev_destroy(q);
    }
}

#[test]
fn test_v120_runtime_thread_pinning_and_affinity() {
    unsafe {
        let core = datara_rt_get_current_core();
        assert!(core >= 0, "Core index must be non-negative");

        let pin_res = datara_rt_pin_thread(0);
        assert!(
            pin_res == 0 || pin_res == 1,
            "Thread pinning should return valid status code"
        );

        datara_rt_pin_worker_threads();
    }
}

#[test]
fn test_v120_runtime_e2e_compilation_and_execution() {
    let source = r#"
fn compute_sum(n: Int) -> Int {
    mut total = 0
    mut i = 1
    while i <= n {
        total = total + i
        i = i + 1
    }
    return total
}

fn main() {
    let s = "short_str"
    let n = 100
    let res = compute_sum(n)
    println(res)
}
"#;

    let compiler = ForgenCompiler::new("release").with_llvm(true);
    let res = compiler.compile_source(source, "v120_runtime_e2e.dtr", None);
    assert!(res.success, "Compilation must succeed: {:?}", res.error);

    let exe = res.exe_path.expect("Executable must exist");
    for run in 1..=5 {
        let (stdout, stderr, code, _) = compiler.cranelift.run_executable(&exe, &[]).unwrap();
        assert_eq!(code, 0, "Run {} failed: {}", run, stderr);
        assert_eq!(
            stdout.trim(),
            "5050",
            "Sum 1..100 must be 5050 on run {}",
            run
        );
    }
}
