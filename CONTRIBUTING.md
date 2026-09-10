# Contributing to Datara & Forgen

Thank you for your interest in contributing to Datara! We welcome contributions to the compiler, standard library, documentation, and tooling.

---

## Code of Conduct

All contributors and maintainers are expected to adhere to our [Code of Conduct](CODE_OF_CONDUCT.md).

---

## Development Environment Setup

### Prerequisites
- **Rust Toolchain**: 1.85+ stable (`rustup default stable`).
- **C/C++ Build Tools**:
  - Windows: Visual Studio Build Tools (C++ Desktop development)
  - Linux: `build-essential` (Ubuntu/Debian) or `base-devel` (Arch)
  - macOS: `xcode-select --install`

### Getting the Code
```bash
git clone https://github.com/waters1ze/datara.git
cd datara
cargo check
cargo test
```

---

## Architecture Guidelines

1. **Compiler Pipeline**:
   - `src/lexer`: Tokenization and span tracking.
   - `src/parser`: Recursive-descent AST construction with error recovery.
   - `src/resolver`: Scope resolution and name binding.
   - `src/types`: Hindley-Milner type inference and effect checking.
   - `src/dmir`: Conversion from AST to SSA intermediate representation.
   - `src/optimizer`: Proof-carrying passes governed by the Evidence Gate.
   - `src/codegen`: Cranelift native machine code lowering and linking.
   - `src/runtime`: Minimal C runtime for I/O, memory, and OS interop.

2. **The Evidence Gate Principle**:
   - Every mutating optimization pass MUST be verifiably provable. Do not add heuristic passes that report success without an intermediate representation structural delta.

3. **Strict Invariants**:
   - Always write regression tests in `tests/` for any bug fix.
   - Code must pass `cargo fmt --all -- --check` and `cargo clippy -- -D warnings`.

---

## Submitting Pull Requests

1. Fork the repository and create a feature branch (`git checkout -b feature/my-feature`).
2. Implement your changes following standard Rust conventions.
3. Verify that all tests pass (`cargo test`).
4. Commit with clear, descriptive commit messages.
5. Push to your fork and submit a Pull Request.
