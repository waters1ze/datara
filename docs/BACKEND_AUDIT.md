# Forgen Backend Architecture & Verification Audit

## 1. Executive Summary

This document provides a forensic technical audit of the **Forgen** compiler backend, intermediate representation, and execution pipeline. 

The goal of this audit is to guarantee complete architectural transparency:
- Proving what is executed by the 100% pure **Rust compiler core**.
- Demystifying the codegen layer.
- Formally detailing the evolution path from the current bootstrapping native backend to the direct LLVM/Cranelift machine code generator.

---

## 2. Canonical Compiler Pipeline

The compilation process in Forgen follows a strict, layered 12-subsystem pipeline:

```
                  ┌───────────────────────────────┐
                  │    Datara Source (*.dtr)      │
                  └──────────────┬────────────────┘
                                 │
                        1. Lexer (Rust)
                                 │
                        2. Parser (Rust)
                                 │
                        3. Resolver (Rust)
                                 │
                      4. Type Checker (Rust)
                                 │
                     5. Effect Lattice (Rust)
                                 │
                    6. Ownership Safety (Rust)
                                 │
                    7. Semantic Graph (Rust)
                                 │
                   8. DMIR Lowering (SSA IR, Rust)
                                 │
                   9. Domain Optimizer (Rust)
             (Inlining, SROA, DCE, Const Folding)
                                 │
                  10. Backend Native Generation
                                 │
                                 ▼
                     Windows Native Executable (.exe)
```

Every stage from (1) to (9) is written **exclusively in Rust** (`src/`).

---

## 3. Backend Implementation Reality

### Current Bootstrapping Backend (`src/codegen/mod.rs`)
In Phase 1 & 2, to achieve instantaneous Windows PE executable generation with zero external C++ toolchain dependencies on developer machines, Forgen uses a **structured intermediate C# emission backend**:
- DMIR instructions are lowered into clean, strongly-typed scalar C# code.
- Windows built-in `csc.exe` (Roslyn / .NET Framework compiler present on every Windows machine) is invoked by Rust to produce a standalone Windows `.exe` PE binary.
- All optimizations (Dead Symbol Elimination, Inlining, SROA allocation scalarization, Constant folding) are performed **before** backend emission in **Rust DMIR**.

### Why This Was Chosen for Bootstrap
1. **Instant Portability**: Works immediately on any Windows machine out-of-the-box without requiring a 20GB LLVM or MSVC C++ toolchain installation.
2. **Deterministic Verification**: Enables end-to-end verification of language semantics, classes, behaviors, decide blocks, pipeline operators, and ownership diagnostics.
3. **Pure Rust Pipeline Control**: The Rust compiler core retains 100% control over parsing, type checking, ownership, effects, semantic graphs, optimization, and whole-program reachability.

---

## 4. Native Code Generation Evolution Roadmap

The transition from bootstrap emission to direct machine code generation is structured in three milestones:

```
[Phase 2 - Current]
DMIR -> Structured Native Backend -> PE Executable (Working & Verified)

[Phase 3 - Near Term]
DMIR -> Cranelift IR -> Direct Object Code (.obj) -> LLD-Link -> PE Executable

[Phase 4 - Long Term / Domain Release]
DMIR -> LLVM IR -> LLVM LTO / Vectorizer -> Machine Code (.exe, ELF, Mach-O)
```

### Architectural Contract:
- **Zero Language Leakage**: Datara syntax and semantics have zero dependency on any specific target language.
- **Frontend / Middle-end Agnostic**: Lexer, Parser, Resolver, TypeChecker, OwnershipTracker, EffectAnalyzer, DMIR, and Optimizer are completely target-agnostic and will remain unchanged when switching from Cranelift to LLVM.
