# Datara & Forgen Documentation Portal

Welcome to the official documentation for the **Datara** systems programming language and the **Forgen** native AOT compiler toolchain.

---

## Documentation Map

### 1. Tutorials & Guides
* **[Official 10-Step Tutorial](TUTORIAL.md)** — Step-by-step hands-on guide from fresh installation to production CLI application.
* **[Unified Technical Glossary](GLOSSARY.md)** — Core terminology, memory models, and formal concept definitions.
* **[Sparks Decentralized Package Protocol](sparks.md)** — Specification for Sparks packages, capabilities, and cryptographic signatures.
* **[Полное руководство по языку Datara (RU)](DATARA_LANGUAGE_GUIDE_RU.md)** — Исчерпывающий академический справочник по синтаксису, парадигмам и архитектуре на русском языке.

### 2. Normative Language References & Specifications (v1.0+)
* **[Language Reference Manual](DATARA_LANGUAGE_REFERENCE_MANUAL.md)** — Complete syntax, type system, affine ownership, pattern matching, traits, and error handling.
* **[Project & Package Model](DATARA_PROJECT_MODEL.md)** — Working with `datara.toml`, `datara.lock`, workspace modules, and `dpm`.
* **[Core vs Standard Library](CORE_VS_STDLIB.md)** — Architectural boundary between compiler builtins and standard library modules (`stdlib.*`).
* **[Conformance Matrix](CONFORMANCE_MATRIX.md)** — Status and validation matrix across all language features.
* **[Normative Specification (v1.0)](SPEC_V1.md)** — Formal definition of syntax rules, memory model, and 13 Evidence Gates.
* **[Complete Technical Specification (v2.0)](DATARA_FORGEN_COMPLETE_TECHNICAL_SPECIFICATION_V2.md)** — Detailed specification of compiler passes, IR, and lowering.

### 3. Historical & Archival Specifications (v0.1 Archive)
* **[Canonical Specifications Archive](canonical/)** — Authoritative archival specs for backwards-compatibility auditing:
  * [Datara Language Spec v0.1 (Archival)](canonical/DATARA_LANGUAGE_SPEC_v0.1.md)
  * [Forgen Compiler Architecture v0.1 (Archival)](canonical/FORGEN_COMPILER_ARCHITECTURE_v0.1.md)
  * [Datara & Forgen Core Concept v0.1 (Archival)](canonical/DATARA_FORGEN_CONCEPT_v0.1.md)

---

### 4. Practical Guides & Interop
* **[Game Development with Datara](gamedev.md)** — Data-oriented design (DOP), zero-pause memory management, cache-efficient layouts, and high-frequency loops.
* **[Rust Interop Bridge Guide](rust_bridge.md)** — Calling Rust crates directly from Datara and embedding Datara in existing native workflows.

---

### 3. Architecture & Compiler Guarantees
* **[Current Architecture](CURRENT_ARCHITECTURE.md)** — High-level overview of the Forgen toolchain (Parser, Semantics, DMIR, Cranelift/LLVM backends).
* **[Optimizer Contract](OPTIMIZER_CONTRACT.md)** — Evidence Gate optimizer, Bounds Check Elimination (BCE), and zero-cost abstractions.
* **[Semantic Contract](SEMANTIC_CONTRACT.md)** — Soundness invariants, affine ownership rules, and borrow checking guarantees.
* **[Semantic Invariants](SEMANTIC_INVARIANTS.md)** — Mathematical proofs of safety and determinism.
* **[Semantic Adaptation Engine](SEMANTIC_ADAPTATION_ENGINE.md)** — Cross-platform ABI alignment and context propagation.
* **[LSP & IDE Architecture](DATARA_IDE_LSP_ARCHITECTURE.md)** — Language Server Protocol engine design, completion, and diagnostics.
* **[Shell & REPL Architecture](DATARA_SHELL_ARCHITECTURE.md)** — Interactive evaluator and JIT execution loop.

---

### 4. Benchmarks & Roadmap
* **[Performance Benchmarks & Methodology](PERFORMANCE.md)** — Compilation speed, runtime throughput, memory latency, and determinism benchmarks.
* **[Interactive Benchmark Showcase](index.html)** — Interactive HTML dashboard with charts and real-time metric visualizations.
* **[Project Roadmap](../ROADMAP.md)** — Immediate priorities, feature tracking, and long-term milestones.
