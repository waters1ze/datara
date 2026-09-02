# IMPLEMENTATION DECISIONS (ADRs) — DATARA + FORGEN

**Status:** Approved  
**Decider:** Lead Compiler Engineer / Systems Architect

---

## ADR-001: Strict Layered Compiler Pipeline
- **Decision:** Separate Frontend (Lexer, Parser, DAST) from Semantic Core (Resolver, Types, Effects, Ownership, Semantic Graph), Middle-end (HIR, DMIR, Optimizer), and Backend (Codegen, Runtime).
- **Consequence:** Clean compiler boundaries allow independent verification, AST query tools, and multiple target backends (Native PE executable, LLVM, future embedded targets).

## ADR-002: Dual-Pass Scope & Behavior Slicing Resolution
- **Decision:** Name resolution operates in two passes:
  1. Declaration collection (registers all classes, behaviors, roles, components across all project files).
  2. Member merging and body resolution (merges split `behavior Target { ... }` blocks into target class symbols before typechecking).
- **Consequence:** Files do not form artificial barriers to type definitions. Programmers can cleanly organize methods into separate files without runtime overhead.

## ADR-003: Compact Declaration Normalization in AST
- **Decision:** Surface syntactic sugars (`x := 10`, `name String`, `add() -> Int => a + b`) are parsed into first-class AST nodes that normalize directly into canonical typed symbols.
- **Consequence:** Ergonomic syntax is 100% equivalent in optimization and layout capabilities to explicit syntax.

## ADR-004: Pure SSA Model in DMIR with Effect Monotonicity
- **Decision:** DMIR uses a linear SSA instruction format with explicit basic blocks, typed operands, and attached effect sets. Every optimization pass validates SSA invariants and effect monotonicity.
- **Consequence:** Guarantees optimization safety and allows precise analysis of pure expressions and parallel regions.

## ADR-005: Standalone Native Executable Generation Backend
- **Decision:** Implement native binary generation producing standalone Windows `.exe` executables with stripped runtime support.
- **Consequence:** Immediate capability to build, execute, and benchmark compiled Datara binaries without requiring heavy external VM installations.

## ADR-006: Bilingual Machine-Readable Diagnostics
- **Decision:** Diagnostic engine uses standard error codes (`E-SYNTAX-001`, `E-TYPE-001`, `E-BORROW-001`, etc.) with message formatters in English and Russian.
- **Consequence:** AI agents and CI systems parse stable error codes; Russian-speaking developers get native-language compiler diagnostics.
