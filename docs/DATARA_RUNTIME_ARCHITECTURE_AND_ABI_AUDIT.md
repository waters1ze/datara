# Datara Runtime Architecture & ABI Audit

**Date**: August 30, 2026  
**Status**: **APPROVED ARCHITECTURAL SPECIFICATION**

---

## 1. Architectural Philosophy & Hard Runtime Boundaries

The Datara runtime follows a strict, layered design with clear ABI boundaries:

\text{Datara Application} \longrightarrow \text{Forgen / Rust Compiler} \longrightarrow \text{Native Machine Code} \longrightarrow \text{Datara Runtime ABI}

### Core Principles:
1. **Zero Runtime Bloat**: C is reserved strictly for low-level platform glue and C-compatible ABI primitives. C must never become a giant runtime.
2. **Rust Core Foundation**: High-level runtime systems (concurrency schedulers, async reactors, type descriptors, memory pool managers) are implemented in Rust.
3. **Compiler Awareness of Effects & Allocations**: The Forgen optimizer must explicitly model the side-effects and allocation behaviors of every runtime symbol (Alloc, Read, Write, Pure, IO) to enable aggressive dead-code elimination, hoisting (LICM), and stack promotion.
4. **Hot-Path Lowering & Inlining**: Forgen is engineered to progressively replace runtime function calls with direct Cranelift/LLVM SSA instructions on hot paths (e.g. SROA for structs, Small String Optimization (SSO) stack buffers, fast integer-to-string conversion).

---

## 2. Modular Runtime Directory Structure

`
runtime/
├── core/                  # [Rust] High-level runtime orchestration & task queues
├── memory/                # [Rust + Allocator Glue] Bump allocators, arenas, GC-free region tracking
├── platform/              # [Rust / C] Platform-specific OS interfaces (Windows, Linux, macOS)
├── io/                    # [Rust / C] Fast unbuffered/buffered console and file descriptors
├── strings/               # [Rust / C] UTF-8 slice manipulations, small string optimizations (SSO)
└── abi/                   # [C-Compatible Boundary] Exported symbols with stable C linkage
    ├── datara_runtime.h   # C ABI header definition
    └── datara_runtime.c   # Low-level ABI implementation
`

---

## 3. Comprehensive Runtime ABI Audit Matrix

Every runtime function currently exported by datara_runtime.c is audited below:

| Symbol Name | Purpose & Justification | Current Implementation | Memory Allocations | Compiler Effects | Can/Should be Rust? | Forgen Hot-Path Optimization / Bypass Strategy |
|---|---|---|---|---|---|---|
| datara_rt_out_int | Prints 64-bit integer + newline to stdout | C (printf(%lld\n, v)) | **None** | IO, Write | Can be Rust/C | Lowerable to fast itoa + write syscall; bypasses libc printf overhead. |
| datara_rt_out_float | Prints 64-bit double + newline to stdout | C (printf(%g\n, v)) | **None** | IO, Write | Can be Rust/C | Inlinable via fast floating-point dtoa routines (e.g. Ryu algorithm). |
| datara_rt_out_str | Prints null-terminated or sliced string | C (printf(%s\n, s)) | **None** | IO, Read | Can be Rust/C | Lowerable directly to WriteFile (Win32) or write (POSIX) syscall. |
| datara_rt_err | Prints error string to stderr | C (printf(stderr, ...)) | **None** | IO, Read | Can be Rust/C | Direct OS descriptor write. |
| datara_rt_exit | Terminates process with exit code | C (exit(code)) | **None** | Terminates | Platform C/OS | Replaced by direct ExitProcess / exit_group syscall. |
| datara_rt_str_concat | Concatenates two string literals/buffers | C (malloc + memcpy) | **Heap Alloc** (malloc) | Alloc, Read | **Rust / Forgen** | **High-priority bypass**: For short strings ($\le 24$ bytes), Forgen emits SSO stack buffer copies without calling runtime malloc. |
| datara_rt_int_to_str | Formats integer to newly allocated string | C (malloc + snprintf) | **Heap Alloc** (malloc) | Alloc | **Rust / Forgen** | Forgen stack-allocates 32-byte scratch buffer and runs inlined integer-to-ascii conversion. |
| datara_rt_str_eq | Fast lexicographical string equality | C (strcmp) | **None** | Pure, Read | Rust/C | Inlined as SIMD 64-bit integer vector comparisons for short fixed lengths. |
| datara_rt_str_len | Computes string byte length | C (strlen) | **None** | Pure, Read | Rust/C | When string length is known at compile time, folded to constant integer. |
| datara_rt_list_create_4| Creates fixed-size list buffer | C (malloc) | **Heap Alloc** (malloc) | Alloc | Rust | SROA pass completely scalarizes lists used locally, eliminating heap allocation. |
| datara_rt_list_get | Reads element at index with bounds check | C pointer indexing | **None** | Pure, Read | Rust/C | Inlined directly to SSA load(base + 8 + idx * 8) with loop invariant bounds hoisting. |
| datara_rt_map_create_2 | Creates key-value tuple store | C (malloc) | **Heap Alloc** (malloc) | Alloc | Rust | SROA converts map entries to SSA registers. |
| datara_rt_map_get | Looks up value by key | C linear scan | **None** | Pure, Read | Rust | Inlined to perfect hash or direct struct field load for static keys. |
| 
ow_ms / datara_rt_now_ms | Retrieves monotonic epoch time in milliseconds | Platform C (GetSystemTimeAsFileTime) | **None** | Read, Time | Platform C/Rust | Emits direct dtsc / monotonic clock syscall. |

---

## 4. Effect Lattice & Optimizer Integration

In the Forgen optimizer (src/optimizer/), each runtime call is tagged with an effect signature:

`ust
pub enum RuntimeEffect {
    Pure,                    // Can be CSE'd, constant-folded, or eliminated if unused
    Read(MemoryRegion),     // Does not mutate memory; safe to LICM hoist out of loops
    Alloc(AllocSize),       // Allocates memory; eligible for SROA / Escape Analysis stack promotion
    Write(MemoryRegion),    // Mutates memory; enforces barrier ordering
    IO,                     // Interacts with OS / Console; cannot be moved across volatile boundaries
    Terminates,             // Never returns (e.g. exit)
}
`

This ensures that while the runtime implementation remains clean, minimal, and portable, the compiler maintains full visibility into execution semantics for maximum optimization.
