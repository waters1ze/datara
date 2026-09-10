# Datara v1.0.0 Release Notes: The Production Release

We are proud to announce the official General Availability release of **Datara v1.0.0** and the **`forgen`** native compiler toolchain!

Datara is a high-performance, compiled systems and application programming language combining ergonomic syntax with bare-metal speed, zero-cost abstractions, deterministic execution, and seamless polyglot interoperability (C, CPython, Node/JS, and Rust).

---

## What's New in v1.0.0

### 1. SemVer 2.0 Stability Guarantees
- Formally defined language stability contract in `docs/canonical/DATARA_LANGUAGE_SPEC.md` Section 8:
  - **Syntax & Grammar Invariance**: Backwards-compatible across all 1.x releases.
  - **Deterministic Semantics**: Well-defined arithmetic overflow traps and type resolution.
  - **C ABI Stability**: Unaltered symbol naming and calling conventions for `extern c` declarations.
  - **DPM Lockfile Reproducibility**: Merkle-tree rooted hash guarantees bit-identical builds.

### 2. Sparks Decentralized Package Registry (`dpm`)
- Specification defined in `docs/sparks.md` and integrated into `dpm`:
  - Decentralized sparse-index protocol (`sparks/<package>`).
  - Schema: 1 metadata manifests with Ed25519-compact cryptographic signatures.
  - Compile-time capability lattice sidecars protecting system resources (`io.fs`, `net.connect`, `sys.env`, `ffi.c`).
  - SHA256 integrity verification preventing supply-chain tampering.

### 3. Production Showcases (`examples/showcase/`)
- Three verified, deterministic end-to-end showcases:
  - **`json_parser`**: High-performance recursive descent JSON parser with full structural validation.
  - **`http_server`**: Deterministic HTTP/1.1 request router with JSON response generation.
  - **`toy_kv`**: Append-only commit log storage engine featuring WAL replay, rolling checksums, and compaction.

### 4. Robust Compiler Diagnostics & Tooling
- Standardized machine-readable `E-XXXX-YYY` diagnostic codes across all compiler stages.
- Interactive CLI command `forgen explain <E-CODE>` for instant diagnosis and suggestions.
- DWARF & PDB native debug information emission.

### 5. Multi-Target Backends & Performance Parity
- **Dual AOT/JIT Backends**: Cranelift for 30–50ms developer iteration, LLVM for production `-O3` binaries with SIMD auto-vectorization.
- **Tail Call Optimization (TCO)**: Deep recursion runs in $O(1)$ stack space via `fastcc` calling conventions.
- **Evidence-Gated Optimizations**: Every optimization pass verified by physical IR fingerprinted deltas.

### 6. GameDev, Microcontrollers & Operating System Kernels
- **Game Engines & Simulation**: Deterministic lockstep physics netcode (`tests/test_lockstep_sim.rs`), linear Frame Arena Allocator ($O(1)$ bump allocation with zero GC pauses), hardware SIMD vectors (`float4`, `int4`, `dot`, `min4`, `max4`), and cache-friendly data-oriented structs for ECS.
- **Embedded & Microcontrollers (STM32, ESP32, RISC-V)**: Zero GC footprint, predictable static stack frames, bitwise intrinsics (`clz`, `ctz`, `popcnt`), fixed-width integers (`UInt8`..`UInt64`), and MMIO register mapping replacing unsafe C++.
- **OS Kernels & Zero-Trust Security**: Capability Lattice preventing unauthorized syscalls or privileged code execution at compile time, `RawPtr` for hardware memory mapping, and zero-cost `extern "C"` ABI for interrupts and context switching.

---

## Installation & Upgrades

### Windows (64-bit)
- **PowerShell One-Liner:**
  ```powershell
  irm https://raw.githubusercontent.com/waters1ze/datara/main/install.ps1 | iex
  ```
- **GUI Installer:** [Datara-v1.0.0-Setup.exe](https://github.com/waters1ze/datara/releases/download/v1.0.0/Datara-v1.0.0-Setup.exe)
- **Winget:** `winget install waters1ze.Datara`
- **Scoop:** `scoop install https://raw.githubusercontent.com/waters1ze/datara/main/packaging/scoop/datara.json`

### Linux & macOS
- **Unix Shell One-Liner:**
  ```bash
  curl -fsSL https://raw.githubusercontent.com/waters1ze/datara/main/install.sh | bash
  ```
- **Homebrew:** `brew install waters1ze/tap/datara`
- **Arch Linux (AUR):** `yay -S datara-bin`
- **Debian/Ubuntu:** `dpkg -i datara_1.0.0_amd64.deb`
- **Fedora/RHEL:** `rpm -i datara-1.0.0-1.x86_64.rpm`
