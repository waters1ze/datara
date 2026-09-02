# FORGEN Native Backend Architecture Report

**Version**: 1.0 (Native Backend Phase)  
**Status**: ACTIVE & VERIFIED  
**Authors**: Lead Compiler Engineer / Systems Architect  

---

## 1. Executive Summary

In this phase, Forgen transitions from a pure bootstrap translation prototype into a dual-backend native compiler. The architecture introduces:
1. **Target Architecture & TargetInfo Abstraction** (`src/codegen/target.rs`): Explicit support for target triples, CPU architectures (`x86_64`, `aarch64`, `riscv64`), operating systems (`windows`, `linux`, `macos`), calling conventions (`WindowsFastcall`, `SystemV`, `Aarch64Standard`), vector instruction sets (`SSE2`, `AVX`, `AVX2`, `AVX512`, `Neon`), and pointer sizes.
2. **Direct Cranelift IR (CLIF) Emitter** (`src/codegen/cranelift/`): Direct, verifiable lowering from SSA-form DMIR into standard Cranelift Intermediate Representation (CLIF).
3. **Pluggable Backend Interface** (`CodegenBackend`): Decoupled backend driver enabling both native machine code emission (Cranelift) and verified standalone executable generation (`BootstrapBackend`).

---

## 2. Target Abstraction (`TargetInfo`)

The target model provides compile-time configuration and multi-target code generation capabilities:

```rust
pub struct TargetInfo {
    pub arch: Arch,
    pub os: Os,
    pub abi: Abi,
    pub calling_convention: CallingConvention,
    pub vector_support: Vec<VectorExtension>,
    pub endianness: Endianness,
    pub pointer_width_bits: usize,
}
```

### Standard Target Configurations

| Target Triple | Architecture | OS | Calling Convention | Vector Extensions |
| :--- | :--- | :--- | :--- | :--- |
| `x86_64-pc-windows-msvc` | `x86_64` | `Windows` | `WindowsFastcall` | SSE2, AVX, AVX2 |
| `x86_64-unknown-linux-gnu` | `x86_64` | `Linux` | `SystemV` | SSE2, AVX, AVX2, AVX512 |
| `aarch64-unknown-linux-gnu` | `Aarch64` | `Linux` | `Aarch64Standard` | Neon |
| `aarch64-apple-darwin` | `Aarch64` | `MacOS` | `Aarch64Standard` | Neon |

---

## 3. Cranelift IR (CLIF) Emission Architecture

The `ClifEmitter` translates high-level semantic SSA definitions in DMIR to explicit low-level Cranelift basic blocks and instructions:

1. **Explicit Stack Slots**: Allocations for non-scalar or local mutable slots (`ss0 = explicit_slot 8 ; var 'res'`).
2. **SSA Register Mapping**: Value IDs are emitted as Cranelift SSA value numbers (`v0`, `v1`, ...).
3. **Control Flow**: Conditionals and loops are lowered into distinct labeled blocks (`entry()`, `loop_header:`, `loop_body:`, `loop_exit:`).
4. **Platform ABI Compliance**: Calling conventions (`windows_fastcall`, `system_v`) are tagged directly onto function declarations.

### Example Generated CLIF IR

```clif
; Auto-generated Cranelift IR (CLIF) by Forgen Native Backend
; Target: x86_64-pc-windows-msvc

test compile
target x86_64-pc-windows-msvc

function u0:main() windows_fastcall {
    ss0 = explicit_slot 8 ; var 'v_1012'
    ss1 = explicit_slot 8 ; var 'v_1013'
    ss2 = explicit_slot 8 ; var 'v_9'
    ss3 = explicit_slot 8 ; var 'res'
  entry():
    v7 = iconst.i64 10
    v8 = iconst.i64 20
    v1014 = iconst.i64 2
    v1015 = imul v1013, v1014
    v1016 = iadd v1012, v1015
    call fn$rt_out(v10)
    return
}
```

---

## 4. Verification & Backend Test Coverage

The native backend implementation is verified via automated test suites:
- `tests/test_target_info.rs`: Target description predicates, multi-arch triples, and version variant selection.
- `tests/test_cranelift_backend.rs`: Multi-target CLIF generation for Windows, Linux x86_64, and Linux Aarch64 with calling convention verification.
