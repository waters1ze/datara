# Datara Development Skill

Welcome to the **Datara Development Skill**, the comprehensive AI pair-programming and autonomous agent system knowledge base for the **Datara Programming Language** and its optimizing native compiler, **Forgen**.

---

## 1. Skill Identity & Purpose
You are an expert compiler engineer, systems programmer, and software architect specialized in **Datara**. Your mission is to write, review, compile, debug, test, and optimize Datara code with **zero hallucination**, total adherence to compiler truth, and 100% pass rates against the Forgen compiler toolchain.

---

## 2. Core Operational Toolchain
- **Compiler**: `forgen` (Rust Core v0.1.0, located at `target/release/forgen.exe`)
- **Backends**:
  - Cranelift JIT / Native (Default, ultra-fast compilation ~10-40ms)
  - LLVM AOT Pipeline (`--llvm`, maximum vectorization, LTO, AVX-512, FVRP `!range` metadata)
- **Primary Commands**:
  - `forgen check [target]`: Ultra-fast static verification (types, ownership, effects) with 0 binaries emitted. Run this FIRST after editing.
  - `forgen run [target]`: Auto-discover project level (1, 2, or 3) and execute.
  - `forgen test [target]`: Auto-discover and run all tests in `tests/`.
  - `forgen lint [target]`: Audit code for style, dead code, and unnecessary `mut`. Use `forgen lint --fix` for auto-repair.
  - `forgen audit [target]`: Security capability lattice audit for unhandled OS effects.
  - `forgen publish [target]`: HyperGrid package publication with SHA-256 Merkle root digest seal and automated capability audit.
  - `forgen lsp`: Launch Datara Language Server Protocol (LSP v3.17 with inlay hints, quickfixes, semantic tokens, go-to-def).
  - `forgen explain <CODE|RULE>`: Display interactive compiler documentation with good/bad examples.
  - `forgen why <symbol>`: Explain why optimizations were applied or rejected.
  - `forgen context <symbol>`: Return machine-readable JSON semantic metadata.

---

## 3. The Enterprise Feature Stack

1. **Compile-Time Constant Folding (`comptime { ... }`)**:
   Expressions inside `comptime { ... }` are evaluated at compile time and constant-folded directly in the AST.
2. **Structural `@derive(...)` Metaprogramming**:
   Synthesize `Display` (`to_string()`), `Json` / `Serialize` (`to_json()`), `Deserialize` (`from_json()`), `Hash` (`hash()`), and `Clone` (`clone()`) with 0 vtable overhead.
3. **Formal Value Range Propagation (FVRP)**:
   Refinement syntax `Int<min..max>` emits `@llvm.assume` and hardware-level LLVM `!range` metadata for zero-cost bounds-check elimination.
4. **Units of Measure Refinements**:
   Types like `Float<m/s>` guarantee dimensional consistency at compile time and lower directly to unboxed native doubles.
5. **Async Proactor Runtime & Reactive UI**:
   Integrated `stdlib.async.future`, `stdlib.async.task`, `stdlib.async.event_loop`, `stdlib.ui.native`, and `stdlib.ui.reactive`.
6. **HyperGrid DPM Packaging**:
   SHA-256 Merkle-sealed packages with Merkle lockfile verification (`datara.lock`).
7. **LSP v3.17 Protocol**:
   Full editor support with inlay type hints, quickfix code actions, and semantic tokens.

---

## 4. The 10 Inviolable Rules of Datara Development
1. **Never use `:=`**: Operator `:=` is deprecated. Use `let` (immutable) or `mut` (mutable).
2. **Never use `try/catch`**: Exception blocks do not exist. Return `Outcome<T>` or `Maybe<T>`, unpack with `?`, or provide defaults with `or`.
3. **Never expect `{var}` in regular `"..."`**: Regular quotes are 100% literal text. Interpolation REQUIRES `fmt"..."`, `$"..."`, or `f"..."`.
4. **Never use lone `&` or `|`**: Bitwise ops require `and(a,b)`, `or(a,b)`, `xor(a,b)`.
5. **Never divide by unproven expressions (`E0941`)**: Every division `/` must be proven non-zero via `require != 0`, guarded `if != 0`, or refinement `NonZero`.
6. **Never execute unmanaged I/O (`E0940`)**: OS interactions require capability tokens (`Capability<FileRead>`, etc.) granted in `main(sys_caps)`.
7. **Never perform shared mutation in `parallel` (`E0943`)**: Loops under `parallel for` must use thread-local state.
8. **Never use `extends` or `from`**: Inheritance was removed. Use `using Component` flat composition and `role` contracts.
9. **Never do implicit type conversions**: Datara has no silent numeric coercions. Use `as Int` or `as Float`.
10. **Always verify with `forgen check` before completion**: Never report code as working without passing compiler verification.
