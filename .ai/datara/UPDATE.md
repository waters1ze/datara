# Datara Evolution & Migration Guide

This guide details recent language changes, removals, and migration steps from older prototype drafts.

---

## Deprecated & Removed Features

| Old / Prototype Syntax | Status | Modern Datara Replacement |
|---|---|---|
| `:=` operator | **REMOVED** | `let x = 10` or `mut x = 10` |
| `try / catch` | **REMOVED** | `Outcome<T>`, `?` operator, `or` fallback |
| `extends / from` (Inheritance) | **REMOVED** | `using Component` flat composition & `role` |
| Lone `&` and `|` | **REMOVED** | Bitwise `and(a, b)`, `or(a, b)`, `xor(a, b)` |
| `{var}` in `"..."` | **REMOVED** | `fmt"..."`, `$"..."`, or `f"..."` |
| Untyped raw pointers | **RESTRICTED** | Zero-copy views `view x` and safe references |
