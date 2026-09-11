# Datara v1.1.0 Performance Goals & Verification Contract

## 1. Core Philosophy: Honest, Evidence-Backed Performance

Datara is architected for zero-overhead safety and deterministic high performance.
Unlike languages that rely on dynamic tracing or unpredictable garbage collection, Datara couples an
**affine ownership type system**, **algebraic effect typing**, and an **Evidence Gate optimizer** to
unlock transformations that traditional C and Rust compilers cannot perform without manual annotations.

All benchmark results are published with **100% provenance integrity**:
- **Same-Algorithm Equivalence**: Every benchmark compares strictly equivalent algorithmic implementations.
- **Median of >= 7 Runs**: Fluke spikes and cache priming anomalies are discarded; metrics reflect stable medians across at least 7 timed iterations preceded by warm-up runs.
- **Frozen Benchmark Suite**: Benchmark workloads and baseline measurements are permanently recorded in `docs/data/baseline_*.json`. Modifications to the suite require formal architectural justification.

---

## 2. Three Verifiable Tiers of Performance Criteria

Performance superiority in Datara is formally specified across three immutable tiers:

### Tier 1: Universal Parity Everywhere (LLVM Backend <= 1.03x of clang -O3)
Across every standard same-algorithm computational benchmark, Datara compiled via the LLVM release backend must execute in **<= 1.03x** the runtime of equivalent C code compiled with `clang -O3` (or MSVC `/O2`).
*(Note: The Cranelift backend is designated for iterative development and instant JIT evaluation, maintaining a <= 1.10x ceiling).* 

### Tier 2: Targeted Wins Where C Lacks Information (Datara >= 1.15x vs C)
Where Datara's type system provides mathematical guarantees absent in C/C++, Datara must achieve **>= 1.15x performance superiority** over honest C baselines without manual `restrict` or vendor-specific pragmas:
1. **Bounds-Check Elimination (BCE)**: Range analysis proven by the Evidence Gate compiles index-heavy loops without boundary checks. Safe Datara must run at <= 1.05x of unsafe C.
2. **Ownership-Derived IR Attributes**: Affine uniqueness maps to LLVM `noalias`, pure functions map to `readonly`/`readnone`, and non-escaping pointers map to `nocapture` and `dereferenceable`. Eliminates alias shadows on pointer-heavy structures.
3. **Interprocedural Engine (IPO / Devirtualization)**: Devirtualizes single-implementation traits and auto-specializes constant arguments into cloned fast-paths.
4. **Closed-Form Loop Folding**: LoopFold O(1) reduction transforms affine induction loops into closed-form arithmetic.
5. **Memory Layout & Value Representation**: SoA (Structure of Arrays) field reordering, 64-byte alignment, thread-local pool allocation, and Small String Optimization (SSO for <= 22 bytes) guaranteeing zero heap allocations.

### Tier 3: Zero Regressions (<= 5% Stability Invariant)
No existing benchmark may degrade by > 5% across compiler releases without an explicit architectural record, rationale, and documented mitigation path.

---

## 3. Frozen Benchmark Matrix (12 Canonical Workloads)

| # | Workload ID | Category | Primary Optimization Lever | Target Baseline |
|---|---|---|---|---|
| 1 | `fib_35` | Recursion / Control Flow | Tail-call optimization & stack reduction | O(N) vs recursion |
| 2 | `sum_1e8` | Loop Induction | Closed-form fold & vectorization | O(1) LoopFold |
| 3 | `dot_4m_float4` | SIMD Linear Algebra | Native AVX2/NEON vector instructions | >= 1.0x C |
| 4 | `matmul_naive` | Cache Locality | Loop interchange & tiling (Loop Engine v2) | >= 1.15x naive C |
| 5 | `sort_100k` | Branch / Memory | Branch-predictor hints & bounds elimination | Parity with Rust/C |
| 6 | `json_parse` | Streaming I/O | Zero-copy slicing & SIMD whitespace skip | >= 1.15x C |
| 7 | `string_builder_sso` | Allocation / Memory | SSO <= 22 bytes (zero heap allocations) | >= 1.30x C |
| 8 | `alloc_heavy` | Heap / Allocator | Size-class pool allocator | >= 1.30x malloc |
| 9 | `hashmap_chain` | Dynamic Dispatch | Trait devirtualization & inline caches | Parity with C |
| 10 | `branchy_match` | Profile Guided | PGO edge weights & cold-block separation | >= 1.05x unprofiled |
| 11 | `nbody_sim` | Floating-Point | Fused multiply-add & register allocation | Parity with C |
| 12 | `quaternion_norm` | Vector Mathematics | Evidence Gate vectorization proof | Parity with C |

---

## 4. Hardware and Environment Provenance

Baseline metrics recorded in `docs/data/baseline_*.json` are pinned to the following host platform:
- **Processor**: AMD Ryzen 5 7600 6-Core Processor (12 logical cores)
- **RAM**: 31.1 GB DDR5
- **Operating System**: Windows 11 Pro x86_64
- **Rust Toolchain**: rustc 1.98.0 / cargo 1.98.0
- **C/C++ Toolchain**: MSVC 19.50.35727 / Clang LLVM native
