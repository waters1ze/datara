# Changelog

All notable changes to the Datara compiler and toolchain (`forgen`) are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.2.0] - 2026-09-12 «APEX PERFORMANCE»

### Added
- **СТОЛП 1: Zero-Alias Ownership IR**: Derivation of `align 64` for SIMD/vectors, LLVM TBAA type-based alias analysis metadata (`!tbaa`), `!invariant.load !{}` for immutable record field reads, proving 3-array Saxpy vectorization.
- **СТОЛП 2: std.simd & Polyhedral Loop Engine**: Standard vector intrinsics (`f32x4`, `f32x8`, `f32x16`, `i32x4`, `i32x8`, `f64x2`, `f64x4`) across LLVM, Cranelift, and WASM; Fused Multiply-Add (FMA) pattern lowering to hardware `@llvm.fma`; 2D cache tiling with block size $B=32$; stencil wavefront time-skewing `(t, i) -> (t, i + 2*t)`; loop interchange; affine vectorization metadata `!llvm.loop.vectorize.width = 4`.
- **СТОЛП 3: 3-Tier Zero-Lock Memory Architecture**: Tier 0 stack promotion via escape analysis; Tier 1 2MB thread-local ephemeral bump arena (`datara_rt_arena_alloc`, `checkpoint`, `reset`); Tier 2 64-bit bitmask slab cache with `_BitScanForward64` / `__builtin_ctzll` slot discovery; Tier 3 2MB huge pages with seamless OS fallback.
- **СТОЛП 4: Autonomous Profile-Guided Optimization (PGO)**: `--pgo-train` CFG edge probes and `.prof.json` profiling flush; `--pgo-use` expanding inlining budget 2x on hot functions, splitting cold blocks into `.text.cold` sections, and emitting `!prof` branch weights.
- **СТОЛП 5: Auto SoA Layout Transformer**: Automatic transformation of array-of-records (e.g. N-body `{x, y, z, vx, vy, vz, mass}`) to structure-of-arrays based on field selectivity threshold <= 0.60 or `@soa` attribute; differential bit-for-bit execution parity.
- **СТОЛП 6: Ultra-Compact Embed Profile**: `--tiny` compiler flag emitting minimal footprint binaries (50.0 KB <= 60 KB budget, cold start <= 0.5 ms); `--embed` exporting shared libraries (`.dll`, `.so`, `.dylib`) and C header with host runtime C API (`forgen_init`, `forgen_load_module`, `forgen_call_fn`, `forgen_shutdown`).
- **Bounds-Check Elimination (BCE)**: Inductive range analysis and condition dominator proofs hoisting and eliminating array bounds checks (`datara_rt_list_get_unchecked`); emitting LLVM `@llvm.assume(idx >= 0)`.
- **IPO/LTO & Specialization Engine**: Constant argument specialization with function cloning, devirtualization of single-implementation trait methods to direct static calls, DMIR-level cross-module pure inlining, dead clone elimination (DCE).
- **Redundant Call Elimination**: Interprocedural dominance-based CSE for pure functions and known runtime math operations.
- **Runtime Systems Layer**: SIMD fast memory primitives (`fast_memcpy` >= 1.5x speedup, `fast_memset`, `fast_memcmp`, `fast_strncmp`), Small String Optimization (SSO) for strings <= 22 bytes with 0 heap allocations, Chase-Lev SPMC work-stealing deque, core affinity thread pinning.
- **Benchmark Matrix & RealWorld Applications**: 16 frozen workloads (12 canonical + 4 RealWorld applications: JSON REST, Grep CLI, 2D Physics, 2D Box Blur) with 100% passed, 11 targeted wins >= 1.15x, 5 universal parity <= 1.03x, 0 regressions; scale stress suite (1k..100k lines) quasi-linear compilation scaling ($O(N \log N)$), hot function runtime stability <= 3%.
- **Cross-Platform & Cross-Compilation**: Target triple parsing and models for Windows (MSVC/GNU), Linux (GNU/musl), macOS (Apple Silicon / Intel), and WASM32; auto-multiversioning runtime CPUID dispatch; `--tune=native` compilation; diagnostic `[E0980]` (CrossCompilationMissingToolchain) in EN/RU locales; comprehensive `docs/CROSS_COMPILE.md` guide.

### Changed
- Retained strict IEEE-754 identity determinism by default across all platforms; fast-math remains strictly opt-in via `--fast-math` / `@fast_math`.
- Hardened LLC cross-compilation on Windows host by strictly guarding `-mcpu=native` when cross-compiling.
- Verified 100% data provenance and anti-tamper integrity via `scripts/verify_charts.py --test-tamper`.

## [1.1.0] - 2026-09-11

### Added
- **Native Async/Await to Completion**: Real asynchronous runtime scheduling with PCS DAG wavefront joins, cooperative task cancellation, deterministic timer priority queues (1000 concurrent timers with identical FNV-1a checksums across 20 runs), and fail-closed WASM error diagnostics.
- **Extended Stdlib & DX**: Implemented `Set<T>`, `Deque<T>`, `PriorityQueue<T>`, lazy `Iterator` chains (`take`, `skip`, `zip`, `map`, `filter`), `StringBuilder`, formatted strings (`f"..."`), and unified test runner (`--list`, filtering, standard exit codes).
- **Beginner Error Diagnostics**: 10 automated error suggestions ("did you mean...") with structured E-codes and 0 compiler crashes.
- **Gamedev Layer**: Monotonic microsecond-precision time APIs (`datara_rt_time_precise_ms`, `datara_rt_time_delta_ms`), 60 Hz fixed timestep game loop showcase with byte-identical output across 20 consecutive runs, and SoA ECS patterns.
- **Official 10-Step Tutorial**: Hands-on progressive guide in `docs/TUTORIAL.md` and `examples/tutorial/` (steps 1–10) with automated verification via `scripts/verify_quickstart.ps1` and `.sh`.
- **Sparks Seed Packages**: Created `packages/sparks/mathx`, `strx`, and `jsonx` with schema 1 manifests and security capability sidecars.
- **Brand & Visual Identity**: Authentic Datara Spark icon with clean transparent vector variant and golden-yellow squircle wrapper badge across repository, VS Code extension, and Windows system shortcuts.
- **Unified Documentation**: `docs/GLOSSARY.md`, updated `docs/README.md` navigation separating canonical specs from archival v0.1 specs, and automated consistency verification in `scripts/check_docs_consistency.py`.

### Fixed
- Fixed Cranelift struct return escape analysis in `compile_func.rs` ensuring structs escaping via return are allocated on the heap.
- Fixed DMIR lowering type inference in `infer.rs` to respect explicit type signatures rather than substring name heuristics.
- Fixed Windows `file:///` drive-letter prefix handling in package manager HTTP transport.
- Fixed IPO constant argument specialization in `ipo.rs` to avoid dismantling recursive functions, preserving tail-call optimization (TCO).
- Fixed WASM capability classifier in `wasm/capabilities.rs` adding `datara_rt_list_get_unchecked` and `datara_rt_list_set_unchecked` mapping for loop BCE parity.
- Fixed DMIR inlining of void-returning functions in `ipo.rs` and `inline.rs` to emit `Inst::ConstInt { dest, value: 0 }` for Unit returns instead of leaving caller `dest` undefined in SSA verifier.
- Fixed SROA pass ordering in `src/optimizer/mod.rs` to execute before loop optimization preventing unrolled struct binding duplicates.

## [1.0.0] - 2026-09-11

### Added
- **Production v1.0.0 Release**: Complete, audited, production-grade release of the Datara programming language compiler and toolchain (`forgen`).
- **SemVer 2.0 Stability Guarantees**: Formally specified backwards-compatibility contracts in `docs/canonical/DATARA_LANGUAGE_SPEC.md` covering grammar invariance, arithmetic overflow traps, C ABI stability, and DPM lockfile determinism.
- **Sparks Decentralized Package Protocol**: Implemented `sparks/` package namespace support in `dpm`, backed by `docs/sparks.md` specification with schema: 1 JSON metadata, ed25519-compact signature verification, capability manifest auditing, and SHA256 integrity checks.
- **Showcase Applications**: Added verified real-world Datara showcases in `examples/showcase/`:
  - `json_parser`: High-throughput deterministic recursive descent JSON parser.
  - `http_server`: Deterministic HTTP/1.1 request router and JSON response generator.
  - `toy_kv`: Append-only key-value storage engine with WAL replay, rolling checksum, and compaction.
- **Machine-Readable Diagnostics**: Standardized `E-XXXX-YYY` diagnostic codes across parser, typechecker, and optimizer with actionable suggestions and `forgen explain` support.
- **DWARF & PDB Debug Information**: Native debug symbols on Windows x86_64, Linux ELF, and macOS Mach-O.

### Changed
- Promoted compiler engine status from alpha/beta to 1.0.0 General Availability.
- Cleaned and retired legacy `0.1.0` release artifacts and outdated roadmaps.
- Synchronized all workspace manifests (`Cargo.toml`, `VERSION`, `datara.toml`) to `1.0.0`.

### Fixed
- Fixed SSA block merging bug in optimizer pass `merge_blocks` (`src/optimizer/mod.rs`), ensuring block parameter substitutions propagate to all downstream blocks in the function.
- Fixed Cranelift backend string concatenation lowering for built-in string functions (`src/codegen/cranelift/backend.rs`).
- Fixed capability lattice validation for nested Sparks dependencies.

### Performance
- Full empirical verification against Rust, C, and Go benchmarks (details and raw metrics in [`docs/PERFORMANCE.md`](docs/PERFORMANCE.md)):
  - Binary compilation speed: 30–50ms incremental JIT/AOT cycles.
  - Constant tail recursion elimination (TCO) with `fastcc` calling convention.
  - 100% chart data provenance verified via `scripts/verify_charts.py`.

### Security
- Cryptographic ed25519 package verification in DPM.
- Fail-closed DMIR optimizer verification gates rejecting invalid SSA transformations.
- Memory safety certified with zero AddressSanitizer defects.

