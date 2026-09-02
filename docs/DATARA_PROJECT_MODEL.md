# Datara Progressive Project Model (Zero-Config to Full-Config)

Datara and Forgen use a **Progressive Project Model** designed to eliminate boilerplate for small scripts while smoothly scaling to multi-module applications and enterprise packages.

---

## 1. The Three Project Levels

```
                     ┌──────────────────────────────────────────────┐
                     │   LEVEL 3 — Full Manifest Project            │
                     │   datara.toml + src/ + tests/ + examples/   │
                     │   (Dependencies, Profiles, Advanced Targets) │
                     └──────────────────────▲───────────────────────┘
                                            │
                     ┌──────────────────────┴───────────────────────┐
                     │   LEVEL 2 — Directory Project                │
                     │   myapp/ (main.dtr + modules...)             │
                     │   (Auto-Discovered Dependency & Semantic)    │
                     └──────────────────────▲───────────────────────┘
                                            │
                     ┌──────────────────────┴───────────────────────┐
                     │   LEVEL 1 — Single File                      │
                     │   hello.dtr                                  │
                     │   (Zero Manifest, Instant Run & Build)       │
                     └──────────────────────────────────────────────┘
```

---

### Level 1 — Single File (Zero Configuration)
For scripts, algorithms, competitive programming, and quick prototypes:

```bash
# hello.dtr
fn main() {
    out "Hello, World!"
}
```

```bash
forgen run hello.dtr
forgen build hello.dtr
```
* **No `datara.toml` needed.**
* Output executable `hello.exe` is created directly alongside the script.
* Full compiler optimizations (Cranelift native, SROA, inlining) apply out-of-the-box.

---

### Level 2 — Directory Project (Automatic Discovery)
When your program grows into multiple files and folders, simply organize them in a folder:

```
myapp/
├── main.dtr
├── math.dtr
└── utils/
    └── string_ops.dtr
```

```bash
cd myapp
forgen run
```

Forgen automatically:
1. Detects `main.dtr` (or `src/main.dtr`) as the project entry point.
2. Discovers all `.dtr` source files in the folder hierarchy.
3. Constructs the cross-file symbol table and dependency graph.
4. Performs whole-program type inference, effect checking, and ownership verification.
5. Builds and launches the native application.

---

### Level 3 — Full Manifest Project (`forgen init`)
When your project requires external dependencies, target customization, testing suites, or custom release profiles:

```bash
forgen init myapp
```

Creates the canonical Datara project layout:
```
myapp/
├── datara.toml         # Project Manifest & configuration
├── src/
│   └── main.dtr        # Application entry point
├── tests/
│   └── test_main.dtr   # Integration test suites
├── examples/
│   └── demo.dtr        # Sample and usage examples
└── .gitignore          # Default ignore patterns (target/, *.exe, etc.)
```

#### `datara.toml` Reference

```toml
[package]
name = "myapp"
version = "0.1.0"
entry = "src/main.dtr"
authors = ["Team <team@example.com>"]
description = "High-performance native service in Datara"
edition = "2026"
license = "MIT"

[dependencies]
# std = "1.0"
# network = { path = "../network" }

[target]
binary_name = "myapp"
system_allocator = true
simd_level = "avx2"
link_crt = "dynamic"

[profile.dev]
opt_level = 0
debug_info = true
incremental = true

[profile.release]
opt_level = 3
lto = true
inline_threshold = 250
devirtualize = true

[profile.domain]
opt_level = 3
whole_program_specialization = true
semantic_adaptation = true
dead_symbol_elimination = true
strip_unused_runtime = true
```

---

## 2. CLI Command Suite with Progressive Auto-Discovery

All `forgen` commands auto-discover the project context from the current working directory or an explicit path:

| Command | Purpose | Discovery Behavior |
| :--- | :--- | :--- |
| `forgen run [target] [args...]` | Run project or single file | Auto-discovers entry point + incremental caching |
| `forgen build [target]` | Compile standalone native binary | Resolves all sources into `<binary_name>.exe` |
| `forgen test [target]` | Run integration tests | Executes all tests in `tests/` and test modules |
| `forgen bench [target]` | Run benchmarks | Compiles & executes all benchmarks in `benches/` |
| `forgen check [target]` | Fast static verification | Full semantic, type, effect & borrow checks without linking |
| `forgen domain [target]` | Whole-program domain specialization | Monomorphizes generics, removes dead code, applies PGO |
| `forgen sae [target]` | Semantic Adaptation Engine report | Inspects physical representation decisions (WHAT $\to$ HOW) |
| `forgen fmt [target]` | Canonical code formatting | Auto-formats all `.dtr` files in project |
| `forgen why <symbol> [target]` | Optimization explainability | Details cost-model decisions and pass justifications |
| `forgen context <symbol> [tgt]`| AI Semantic Context API | Emits machine-readable JSON for agentic IDE integration |

---

## 3. Incremental Build Caching

Forgen tracks file modification timestamps across all project sources and `datara.toml`:
* If `<target>.exe` exists and is newer than all `.dtr` sources and the manifest, `forgen run` skips recompilation and launches the binary immediately (turnaround $< 2\text{ms}$).
* Touching any source file automatically triggers incremental native recompilation via Cranelift and MSVC linker.
