# Forgen Backend Architecture & Legacy Bootstrap Lowering Status

## 1. Canonical Production Compiler Architecture

The canonical production compiler path for **FORGEN** and **DATARA** is defined as:

```
Datara Source Code (.dtr)
   │
   ▼
Parser & AST Construction
   │
   ▼
Static Resolver & Semantic Scope
   │
   ▼
Static Type System & Monomorphization
   │
   ▼
Effects Lattice (Pure, IO, Network, Unsafe)
   │
   ▼
Ownership, Lifetimes & Borrow Checking
   │
   ▼
Semantic Graph 2.0 (Dependency & Domain Architecture)
   │
   ▼
High-Level IR & SSA (DMIR)
   │
   ▼
Optimizer Engine (SROA, Inlining, DCE, LICM, CSE, Pipeline Fusion)
   │
   ▼
[CANONICAL NATIVE BACKEND]
Cranelift IR Emitter (Multi-Target: x86_64, aarch64, riscv64, wasm32)
   │
   ▼
Target Machine Code / Native Object & Linker
   │
   ▼
Native Executable Binary
```

---

## 2. Legacy Bootstrap Lowering (`LegacyBootstrapCodegen`)

### Status: **LEGACY / BOOTSTRAP REFERENCE ONLY**
- **Location**: `src/codegen/legacy_bootstrap.rs`
- **Exposed Types**: `LegacyBootstrapCodegen`, `BootstrapBackend`, `LegacyBootstrapLowering`
- **Purpose**:
  During early development and vertical slice verification, `LegacyBootstrapCodegen` serves as a reference semantic validator by emitting intermediate C# code to compile via the host .NET compiler (`csc.exe`).
- **Boundaries**:
  - `LegacyBootstrapCodegen` is strictly isolated in `src/codegen/legacy_bootstrap.rs`.
  - It is NOT part of the canonical production Cranelift architecture.
  - All language features (classes, components, roles, functions, lambdas, decide, try/catch, iterators, views, pipelines) are represented directly in DMIR and Cranelift CLIF IR.
  - As Cranelift native code emission matures across all targets, `LegacyBootstrapCodegen` will be maintained exclusively as an automated differential testing reference.

---

## 3. Backend Verification Matrix

| Component | Canonical Path | Legacy Bootstrap Path | Status |
| :--- | :--- | :--- | :--- |
| **Language Frontend** | Rust Lexer/Parser | Rust Lexer/Parser | Production |
| **Semantic & Type System** | Rust Resolver/Types/Effects/Ownership | Rust Resolver/Types/Effects/Ownership | Production |
| **Intermediate Representation**| DMIR SSA | DMIR SSA | Production |
| **Optimizer** | Multi-pass SSA transformations | Multi-pass SSA transformations | Production |
| **Code Generation** | `CraneliftBackend` (`.clif`) | `LegacyBootstrapCodegen` (`.cs` / `csc.exe`) | Active Differential Pair |
| **Direct Host Execution** | Cranelift Native JIT / Object | Host CLR / Native EXE | Verified |
