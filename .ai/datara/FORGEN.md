# Forgen Compiler Architecture & Internals

Forgen is the multi-backend, optimizing native compiler for Datara written in Rust.

---

## Compiler Pipeline

1. **Lexer & Tokens** (`src/lexer/`): Tokenizes UTF-8 input source with span tracking.
2. **Parser** (`src/parser/`): Builds recursive-descent AST (`src/ast/`).
3. **Type Checker** (`src/types/`): Solves generics, nominal classes, enums, refinements.
4. **Ownership & Borrow Engine** (`src/ownership/`): Enforces affine move semantics and zero-copy view scopes.
5. **Security & Evidence Gate** (`src/security/`): Enforces Proof-Carrying Code (PCC), zero-trust capabilities (`E0940`), and data-race checks (`E0943`).
6. **Datara Mid-Level IR (DMIR)** (`src/dmir/`): SSA-based intermediate representation with basic blocks and control-flow dominance.
7. **Code Generation Backends**:
   - Cranelift backend (`src/codegen/cranelift/`): Zero-latency native compilation and JIT execution.
   - LLVM backend (`src/codegen/llvm/`): Whole-program optimization, SIMD, vectorization, and LTO.
