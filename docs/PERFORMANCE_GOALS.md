# Datara v1.2.0 Performance Goals & Verification Contract

## 1. Core Philosophy: Honest, Evidence-Backed Performance («APEX»)

Datara is architected for zero-overhead safety, byte-for-byte reproducibility, and deterministic high performance.
Unlike languages that rely on dynamic tracing, undefined behavior loopholes, or unpredictable garbage collection, Datara couples an
**affine ownership type system**, **algebraic effect typing**, and an **Evidence Gate optimizer** to
unlock transformations that traditional C, Rust, Fortran, and Zig compilers cannot perform without manual annotations or unsafe flags.

All benchmark results are published with **100% provenance integrity**:
- **Determinism is Identity**: IEEE-754 floating-point semantics by default; Fast-Math is strictly opt-in (`--fast-math`, `@fast_math`). Lockstep checksums (20/20 runs byte-for-byte) are an invariant.
- **Benchmark as Sensor, Not Goal**: Benchmarks diagnose where the compiler failed to exploit available invariants (aliasing, bounds, purity, constants, cache layout). Fixes must be general optimization passes, verified to improve random programs generated via proptest (±20% Applied-rate parity).
- **Same-Algorithm Equivalence**: Every benchmark compares strictly equivalent algorithmic implementations with identical data representations.
- **Median of >= 15 Runs**: Metrics reflect stable medians across at least 15 timed iterations preceded by warm-up runs, with noise floor measurement (delta = noise, threshold = max(1%, noise * 2)).
- **Frozen Benchmark Suite**: 12 canonical workloads + 4 RealWorld applications permanently recorded in `docs/data/`.

---

## 2. Three Verifiable Tiers of Performance Criteria

Performance superiority in Datara is formally specified across three immutable tiers:

### Tier 1: Universal Parity Everywhere (LLVM Backend <= 1.01x of clang -O3 / rustc -O3+lto)
Across every standard same-algorithm computational benchmark, Datara compiled via the LLVM release backend must execute in **<= 1.01x** the runtime of equivalent C/Rust code (or at the measured CPU noise floor).
*(Note: The Cranelift backend is designated for instant developer builds and REPL/JIT evaluation, maintaining a <= 1.10x ceiling).*

### Tier 2: Targeted Wins Where C Lacks Information (Datara >= 1.15x vs C/Rust/Fortran)
Where Datara's mathematical guarantees provide information absent in C/C++/Fortran without brittle manual pragmas, Datara achieves **>= 1.15x performance superiority**:
1. **Ownership-Derived IR Attributes (Zero-Alias)**: Affine uniqueness maps to LLVM `noalias`, pure functions map to `readonly`/`readnone`, and non-escaping references map to `nocapture` and `dereferenceable(n)` with alignments up to `align 64`. Eliminates alias barriers in multi-array loops (e.g., SAXPY/MatVec >= 1.15x vs `clang -O3` without `restrict`).
2. **Bounds-Check Elimination (BCE)**: Range analysis proven by the Evidence Gate compiles index-heavy loops without boundary checks. Safe Datara executes at <= 1.05x of unsafe C.
3. **Polyhedral Loop Engine & Fortran-Style Fusion**: Direction-vector dependency analysis, cache blocking/tiling (32KB), loop interchange, induction sanitation, and whole-array expression fusion (C = A + B * D in 1 loop without temporary buffers).
4. **Interprocedural Engine (IPO / LTO)**: Single-implementation trait devirtualization, cross-module pure inlining, and automatic constant argument specialization into cloned fast paths.
5. **Zero-Lock 3-Tier Allocator & Value Representation**: Tier 0 escape-analysis stack promotion, Tier 1 2MB bump arena, Tier 2 size-class slab cache with tzcnt, Tier 3 2MB huge pages with graceful fallback, and Small String Optimization (SSO <= 22 bytes, zero heap allocations).
6. **Auto SoA Transformer**: Layout adapter converting arrays-of-records to parallel structure-of-arrays based on access patterns or @soa.

### Tier 3: Zero Regressions (<= 5% Stability Invariant)
No existing benchmark may degrade by > 5% across compiler releases without an explicit architectural record, rationale, and documented mitigation path.

---

## 3. Frozen Benchmark Matrix (12 Canonical Workloads + 4 RealWorld)

### Canonical Micro/Macro Workloads
| # | Workload ID | Category | Primary Optimization Lever | Target Baseline |
|---|---|---|---|---|
| 1 | `fib_35` | Recursion / Control Flow | Tail-call optimization & stack reduction | Parity with C / O(N) |
| 2 | `sum_1e8` | Loop Induction | Closed-form fold & SIMD vectorization | >= 1.15x C |
| 3 | `dot_4m_float4` | SIMD Linear Algebra | Native AVX2/AVX-512/NEON vector intrinsics | >= 1.0x C |
| 4 | `matmul_naive` | Cache Locality | Polyhedral interchange & 32KB tiling | >= 1.15x naive C |
| 5 | `sort_100k` | Branch / Memory | Branch-predictor hints & bounds elimination | Parity with Rust/C |
| 6 | `json_parse` | Streaming I/O | Zero-copy slicing & SIMD whitespace skip | >= 1.15x C |
| 7 | `string_builder_sso` | Allocation / Memory | SSO <= 22 bytes (zero heap allocations) | >= 1.30x C |
| 8 | `alloc_heavy` | Heap / Allocator | 3-tier zero-lock pool allocator | >= 1.30x malloc |
| 9 | `hashmap_chain` | Dynamic Dispatch | Trait devirtualization & inline caches | Parity with C |
| 10 | `branchy_match` | Profile Guided | Autonomous PGO edge weights & cold splitting | >= 1.05x unprofiled |
| 11 | `nbody_sim` | Floating-Point / Layout | Auto SoA layout & FMA instruction fusion | >= 1.15x C |
| 12 | `quaternion_norm` | Vector Mathematics | Evidence Gate SIMD vectorization proof | Parity with C |

### RealWorld Applications (Anti-Laundering, 200–600 lines each)
| # | Application ID | Domain | Architecture | Performance Criterion |
|---|---|---|---|---|
| 13 | `realworld_json_rest` | Backend Services | HTTP routing, JSON parser, buffer builder | >= 1.0x Rust/C, >= 1.10x avg |
| 14 | `realworld_grep_cli` | CLI Systems | Directory walker, line parser, regex matcher | >= 1.0x Rust/C, >= 1.10x avg |
| 15 | `realworld_physics_2d` | Game Engine / Simulation | Particle integrator, broadphase, collision SoA | >= 1.0x Rust/C, >= 1.10x avg |
| 16 | `realworld_image_blur` | Media / Vision | Box blur, 2D convolution buffer pass | >= 1.0x Rust/C, >= 1.10x avg |

---

## 4. Hardware and Environment Provenance

Baseline metrics recorded in `docs/data/baseline_*.json` are pinned to the following host platform:
- **Processor**: AMD Ryzen 5 7600 6-Core Processor (12 logical cores)
- **RAM**: 31.1 GB DDR5
- **Operating System**: Windows 11 Pro x86_64
- **Rust Toolchain**: rustc 1.98.0 / cargo 1.98.0
- **C/C++ Toolchains**: MSVC 19.50.35727 / Clang LLVM native
