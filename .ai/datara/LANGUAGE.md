# Datara Language Specification Overview

Datara is a modern, statically typed, data-oriented systems programming language created to unify developer productivity with bare-metal C/Rust-grade performance and mathematical safety guarantees.

---

## 1. Core Language Pillars

1. **Zero-Vtable Monomorphic Dispatch**:
   Traditional OOP dynamic dispatch (`vtable` pointer chasing) is replaced by flat composition (`component`), contract interfaces (`role`), and monomorphic direct dispatch.

2. **Evidence Gate & Proof-Carrying Code (PCC)**:
   Safety checks are mathematically verified at compile time. Division-by-zero, out-of-bounds indexing, and data races are caught statically before binaries are generated.

3. **Affine Ownership with Zero-Copy Views**:
   Data has a single clear owner. Borrowing is achieved through zero-copy `view` bindings, preventing memory corruption and dangling pointers without garbage collection pauses.

4. **Zero-Trust Security Capability Lattice**:
   External effects (disk I/O, sockets, process execution) cannot happen silently. Functions require explicit, zero-cost `Capability` tokens originating from `SystemCapabilities`.

5. **Dataflow Stream Fusion**:
   Pipelines written with `|>` or `then` are fused into tight SIMD-vectorized loops with zero intermediate allocations.

---

## 2. Compilation Target Architecture

```
Datara Source (.dtr)
       │
       ▼
   Lexer & Parser
       │
       ▼
   Abstract Syntax Tree (AST)
       │
       ▼
   Type & Ownership Checker
   (Affine Borrowing, PCC Gate, Effect Lattice)
       │
       ▼
   Datara Mid-level IR (DMIR)
       │
   ┌───┴────────────────────────┐
   ▼                            ▼
Cranelift JIT/AOT           LLVM AOT Pipeline
(Zero-latency, ~10ms)       (Full LTO, SIMD, AVX-512)
   │                            │
   └───────────┬────────────────┘
               ▼
   Standalone Native Binary (.exe / ELF)
```
