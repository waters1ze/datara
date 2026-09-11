# Datara Unified Technical Glossary

This glossary defines standard, normative terminology used across all Datara specifications, guides, compiler internals, and stdlib documentation.

---

## Language & Type System

### Affine Ownership
A memory management discipline where values have at most one owner. An owned value (`own T` or default `let`/`mut`) cannot be duplicated implicitly. When moved, the previous binding becomes invalid. Borrows are expressed via immutable views (`view T` or `&T`) or exclusive views (`mut-view T` or `&mut T`), eliminating data races at compile-time without a tracing garbage collector.

### Record
A transparent, stack-allocated or flat-memory struct aggregate consisting of named, typed fields. Records have zero runtime overhead, deterministic memory layout, and support structural pattern matching.

### Class
An encapsulated data model supporting methods, behavioral composition (`with`), and role specifications.

### ADT (Algebraic Data Type) & Enum
Sum types declared with `enum` containing typed payload variants. Enums must be exhaustively matched with `match` or `decide`.

### Outcome
The idiomatic Datara error handling construct (`Outcome<T, E>`). Instead of exceptions or unwinding, operations return explicit outcome values that must be inspected.

---

## Security & Concurrency

### Capability Lattice
A strict fine-grained permission lattice enforced at compile-time and runtime. Privileged operations—such as disk I/O, network sockets, child process spawning, and high-resolution timing—require explicit capability tokens (e.g. `SystemCapabilities`). Code without these tokens cannot execute side effects.

### Wavefront Join
In async task execution, a deterministic coordination primitive where multiple parallel branches join along a causal DAG before proceeding.

### Cooperative Cancellation
A cancellation model where asynchronous task regions periodically check for cancellation signals at safe yield points, guaranteeing no resource leaks or half-committed states.

---

## Compiler Architecture & Toolchain

### DMIR (Datara Mid-level Intermediate Representation)
A control-flow graph (CFG) representation in Static Single Assignment (SSA) form with explicit basic blocks, instructions, and terminators. DMIR serves as the canonical representation for dataflow analysis, effect tracking, and optimization.

### Evidence Gates
A suite of formal optimizer validation gates verifying that intermediate transformations preserve program semantics, safety invariants, and effect lattice constraints. If an optimization fails an evidence gate, the compiler safely downgrades or rejects the pass.

### Forgen
The official native AOT and JIT compiler for the Datara programming language. Forgen incorporates Cranelift for ultra-fast incremental development and LLVM for whole-program optimization.

### Sparks Protocol
The decentralized package distribution format for Datara libraries and applications. Every Sparks package features a schema-1 manifest, a capabilities sidecar, SHA-256 Merkle digests, and ed25519 cryptographic signatures.
