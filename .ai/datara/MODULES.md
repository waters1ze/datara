# Datara Modules & Multi-File Architecture

Datara organizes code using progressive project levels and structured `use` declarations.

---

## 1. Module Import Resolution Rules

When writing `use a.b.Symbol`:
1. If the first segment is `stdlib`:
   Resolved from embedded standard library (e.g. `use stdlib.math.Math`).
2. If compiling a multi-file project:
   - Within the same directory: `use other_module.Symbol` looks for `other_module.dtr`.
   - From `tests/` targeting `src/`: `use src.module.Symbol` looks for `src/module.dtr`.
3. If importing a library package:
   Looks in `lib/`, `packages/`, or `vendor/`.

---

## 2. Project Hierarchy Levels

### Level 1: Single File
```bash
forgen run script.dtr
```

### Level 2: Directory Project (Zero Manifest)
```
my_cli/
├── main.dtr
├── config.dtr
└── parser.dtr
```
```bash
forgen run my_cli
```

### Level 3: Packaged Project
```
my_app/
├── datara.toml
├── src/
│   ├── main.dtr
│   └── lib.dtr
└── tests/
    └── test_main.dtr
```
```bash
forgen build my_app
forgen test my_app
```
