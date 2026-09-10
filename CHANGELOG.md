# Changelog

All notable changes to the Datara compiler and toolchain (`forgen`) are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

