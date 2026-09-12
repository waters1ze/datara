# Cross-Compilation Guide (v1.2.0 «APEX»)

This document details how to cross-compile Datara and Forgen applications across Windows, Linux, macOS, and WebAssembly targets.

---

## 1. Supported Target Triples

Datara v1.2.0 supports the following standard target triples:

| Target Triple | Architecture | OS | Object Format / ABI | C Runtime ABI |
|---|---|---|---|---|
| `x86_64-pc-windows-msvc` | x86_64 | Windows | PE/COFF | MSVC ABI |
| `x86_64-unknown-linux-gnu` | x86_64 | Linux | ELF (SystemV) | glibc |
| `x86_64-unknown-linux-musl` | x86_64 | Linux | ELF (SystemV) | musl (static) |
| `aarch64-unknown-linux-gnu` | AArch64 | Linux | ELF (AArch64) | glibc |
| `aarch64-apple-darwin` | AArch64 | macOS | Mach-O 64-bit | Apple Silicon |
| `x86_64-apple-darwin` | x86_64 | macOS | Mach-O 64-bit | Intel Mac |
| `wasm32-unknown-unknown` | 32-bit WASM | Bare / Web | WebAssembly v1 / SIMD128 | WASM Standard |

---

## 2. Command Line Usage

### Compiling to Specific Targets

To target an explicit platform, pass the `--target` flag to `forgen build`:

```bash
# Cross-compile to Linux x86_64 (GNU)
forgen build --target x86_64-unknown-linux-gnu

# Cross-compile to macOS Apple Silicon
forgen build --target aarch64-apple-darwin

# Target native CPU with host optimizations
forgen build --tune=native
# or
forgen build --native

# Target WebAssembly
forgen build --target wasm32-unknown-unknown
```

### Emitting LLVM IR for Foreign Architectures

When cross-compiling, Forgen lowers source code through DMIR directly into target-adapted LLVM IR:

```bash
forgen build --llvm --target x86_64-unknown-linux-gnu
```

This generates `<project>.ll` configured with the target's data layout, pointer width, calling convention, and vector registers.

---

## 3. Toolchain Requirements & Diagnostics

### Diagnostic `[E0980]` (Cross-Compilation Missing Toolchain)

When generating cross-platform object code, Forgen invokes Clang or LLC with `-mtriple=<target>`.
If the host platform lacks a suitable cross-linker (for instance, compiling an ELF binary on Windows without `lld` or `x86_64-linux-gnu-gcc`), Forgen emits diagnostic **`[E0980]`**:

```
error[E0980]: Cross-compilation toolchain not found for target triple 'x86_64-unknown-linux-gnu'.
   = note: Install lld or a cross-compiler for the target platform.
   = help: Install LLVM with lld or configure PATH with a cross-linker.
```

### Recommended Setup for Universal Cross-Compilation

To cross-compile from Windows to Linux and macOS:
1. Install LLVM with `lld`:
   ```powershell
   winget install LLVM.LLVM
   ```
2. Forgen automatically discovers `lld-link` and `ld.lld` on `PATH` or in standard LLVM directories.

---

## 4. Determinism Across Targets

All Datara arithmetic follows IEEE-754 semantics uniformly across platforms:
- Bitcast list storage (`double` <-> `i64`) ensures exact float preservation regardless of calling convention or register allocation.
- Auto-multiversioning (`FastAvx2` for x86_64, `FastNeon` for AArch64, `Generic` for WASM) selects optimal SIMD paths while preserving identical mathematical output.
