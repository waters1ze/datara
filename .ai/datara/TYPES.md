# Datara Type System & Memory Architecture

Datara provides a strong, static, affine type system designed to enforce zero-overhead abstractions and machine-level memory safety.

---

## 1. Primitive Scalar Types

| Type | Bits | Native (Cranelift/LLVM) | Description |
|---|---|---|---|
| `Int` | 64 | `i64` | Signed two's complement 64-bit integer |
| `Float` | 64 | `f64` / `double` | IEEE-754 double precision floating point |
| `Bool` | 8 | `i8` | Boolean (`true` or `false`) |
| `Str` / `String` | 128 | `{ ptr: i64, len: i64 }` | UTF-8 fat-pointer byte slice |
| `Void` / `Unit` | 0 | `void` | Zero-sized unit return type |

---

## 2. Formal Value Range Propagation (FVRP) Refinements

Datara allows types to carry compile-time mathematical constraints:
- `Int<min..max>`: Value guaranteed within interval. Lowered with `@llvm.assume` and LLVM `!range` metadata (`!{i64 min, i64 max}`).
- `Float<unit>`: Physical unit verification (e.g. `Float<m/s>`, `Float<kg>`). Erased to zero-overhead `double` in native machine code.

---

## 3. Structural Derivations (`@derive`)

Classes annotated with `@derive(...)` automatically receive synthesized methods:
- `to_string() -> Str` (Display)
- `to_json() -> Str` (Json / Serialize)
- `from_json(s: Str) -> Self` (Deserialize)
- `hash() -> Int` (FNV-1a Hash)
- `clone() -> Self` (Deep clone)
