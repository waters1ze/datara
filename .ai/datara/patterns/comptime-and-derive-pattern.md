# Datara Comptime & Structural @derive Pattern

Datara provides first-class metaprogramming and constant folding directly at the Abstract Syntax Tree (AST) level without macro runtime penalties.

---

## 1. Compile-Time Expression Folding (`comptime { ... }`)

Expressions wrapped in `comptime { ... }` are evaluated deterministically at compile time. The resulting constant literal replaces the entire block in the emitted AST and intermediate representation (DMIR).

```datara
fn main() {
    // 1024 * 64 is folded to 65536 at compile time
    let buffer_size: Int = comptime { 1024 * 64 }
    out fmt"Allocated buffer: {buffer_size} bytes"
}
```

### Guarantees
- Zero runtime calculation overhead.
- Promoted into registers via Mem2Reg.
- Safe for array dimension sizing and hardware buffer configurations.

---

## 2. Structural `@derive(...)` Metaprogramming

Instead of manual boilerplate or reflective runtime overhead, classes can use `@derive` attributes to synthesize methods at compile time.

```datara
@derive(Display, Json, Hash, Clone, Deserialize)
class UserProfile {
    id: Int
    name: Str
    active: Bool
}
```

### Supported Derivations

| Trait | Synthesized Method | Output / Behavior |
|---|---|---|
| `Display` | `to_string() -> Str` | Formats as `ClassName(f1=val1, f2=val2)` |
| `Json` / `Serialize` | `to_json() -> Str` | Serializes into JSON string `{"id": 1, ...}` |
| `Deserialize` | `from_json(s: Str) -> Self` | Parses JSON string into a concrete instance |
| `Hash` | `hash() -> Int` | High-speed FNV-1a non-cryptographic hash |
| `Clone` | `clone() -> Self` | Deep structural value duplicate |

### Example Usage
```datara
fn main() {
    let u1 = UserProfile { id: 1, name: "Alice", active: true }
    let u2 = u1.clone()

    out fmt"Display: {u1.to_string()}"
    out fmt"JSON: {u1.to_json()}"
    out fmt"Hash match: {u1.hash() == u2.hash()}"
}
```
