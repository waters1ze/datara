# Forgen CLI Reference Manual

`forgen` is the official compiler and build tool for Datara.

---

## Complete Subcommand Matrix

### Verification & Compilation
- `forgen check [target]`: Ultra-fast static analysis (types, ownership, effects). Zero binary emission.
- `forgen run [target] [--llvm]`: Compiles and executes project or single script.
- `forgen build [target] [--llvm]`: Emits standalone native executable (`.exe` on Windows, ELF on Linux).
- `forgen test [target]`: Auto-discovers and executes integration tests in `tests/`.
- `forgen bench [target]`: Runs benchmarks in `benches/`.

### Packaging & Enterprise Ecosystem
- `forgen publish [target]`: Calculates SHA-256 Merkle root digest seal (`merkle:sha256:...`) and publishes to HyperGrid registry.
- `forgen install, restore`: Restores and downloads dependencies verified against `datara.lock`.
- `forgen add <pkg>`: Adds dependency to `datara.toml`.
- `forgen tree [--effects]`: Visualizes dependency graph and security capability requirements.
- `forgen vendor [target]`: Bundles dependencies into `vendor/` for 100% offline, air-gapped builds.

### Quality, Formatting & LSP
- `forgen lsp`: Starts official Language Server Protocol (LSP v3.17 with inlay hints, quickfixes, semantic tokens).
- `forgen lint [target] [--fix]`: Linter auditing snake_case, PascalCase, and unnecessary `mut`.
- `forgen audit [target]`: Audits security capability lattice for purity leaks.
- `forgen format [path]`: Source code auto-formatter.
- `forgen explain <code|rule>`: Displays interactive documentation with good/bad examples.

### AI & Compiler Explainability
- `forgen why <symbol>`: Explains compiler optimization decisions (benefits, costs, inlining).
- `forgen context <symbol>`: Structured JSON semantic metadata for AI pair-programmers.
- `forgen inspect <query> <file>`: Inspects semantic graph (`symbol`, `effects`, `optimize`, `ast`, `dmir`, `clif`).
