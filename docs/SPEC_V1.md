# Datara Language Specification V1 (Frozen)

**Document Status:** Normative Specification  
**Edition:** 2026.1 (September 2026)  
**Compiler Implementation:** Forgen (`forgen`)

---

## 1. Core Principles

Datara is designed as a high-performance compiled systems and application programming language prioritizing:
1. **Verifiable Zero-Cost Abstractions**: High-level ergonomics (OOP, composition, generics) compile to identical machine code as hand-written scalar C/Rust without garbage collection or hidden heap allocations.
2. **Fail-Closed Semantics**: Type mismatches, cycle imports, unhandled error states, and unverified optimizations fail compilation rather than producing silent fallback or undefined behavior.
3. **Minimal Core, Maximal Standard Library**: Language primitives remain lean and closed; non-primitive abstractions live in `.dtr` standard library modules.

---

## 2. Canonical Decisions & Language Grammar (The 13 Gates)

### Gate 1: Function Declarations (`fn` vs `function`)
- **Canonical:** `fn name(param Type) -> ReturnType { ... }`
- **Compatibility:** `function` is recognized by the lexer and parser as an alias, lowering into the identical AST node.
- **Normative Rule:** New code and standard libraries MUST use `fn`.

### Gate 2: Module Imports (`use` vs `import`)
- **Canonical:** `use module.path` or `use module.{ItemA, ItemB}` or `use module as Alias`.
- **Compatibility:** `import` is recognized at the frontend and maps to `Decl::Use`.
- **Normative Rule:** Circular module dependencies are strictly prohibited and MUST produce a compile-time error detailing the exact dependency cycle chain.

### Gate 3: Object-Oriented Composition (`with` vs `from`)
- **Canonical Composition:** `with` combines roles, behaviors, and components (`class Service with Logger, Metrics`).
- **Canonical Inheritance:** `from` specifies single base class inheritance (`class Dog from Animal`).
- **Normative Rule:** Multiple inheritance is forbidden; composition with behavioral roles via `with` is the sole mechanism for code reuse across hierarchies.

### Gate 4: Error Handling Model (Result/Outcome vs try/catch)
- **Canonical Model:** `Outcome<T>` (aliased to `Result<T, String>`) with `?` postfix propagation.
- **Normative Rule:** The error channel of `Outcome<T>` is strictly typed (defaulting to `String`). Functions utilizing `?` must declare an `Outcome<T>` return signature. Imperative `try/catch` syntax is excluded from the V1 core specification; all recoverable error flow is value-based.

### Gate 5: Boolean Coercion
- **Normative Rule:** Strictly typed. Conditions in `if`, `while`, `decide`, and logical expressions (`&&`, `||`, `!`) must evaluate strictly to type `Bool`. No integer (0/1), null, pointer, or string "truthy/falsy" coercions are permitted.

### Gate 6: Integer Overflow Semantics
- **Normative Rule:** Standard integer operations on `Int` (signed 64-bit) execute two's complement wrapping arithmetic by default ($Z/2^{64}$).
- **Checked Arithmetic:** For algorithms requiring overflow detection, checked operations (`checked_add`, `checked_mul`) are provided in the standard library.
- **Optimization Guard:** Loop optimizations (such as `LoopFold`) must use parity-split arithmetic to ensure mathematical identity with wrapping integers across all ranges.

### Gate 7: Numeric Widening and Promotion
- **Normative Rule:** No implicit widening between `Int` and `Float`. Mixing `Int` and `Float` in binary operations without an explicit cast (`.to_float()` / `.to_int()`) is a compile-time type mismatch error.

### Gate 8: Pattern Matching and `decide` Exhaustiveness
- **Normative Rule:** `decide` constructs over tagged unions (`Outcome`, `Maybe`) must either exhaustively match all variant tags (`is_success: true` and `is_success: false`) or supply an unconditional `else` branch. Unhandled variants cause a compile-time error.

### Gate 9: Role and Component Method Conflicts
- **Normative Rule:** When a class composes multiple roles or components providing identical method signatures, the composing class MUST explicitly provide a method body overriding the signature. Ambient or order-dependent resolution is disallowed.

### Gate 10: Domain Contracts and Data Models
- **Normative Rule:** Domain models are expressed via standard `class`, `struct`, and `entity` declarations with typed fields. The compiler performs no implicit ORM mapping, dynamic reflection, or code generation without explicit DAST/DMIR representation.

### Gate 11: Concurrency and Cancellation
- **Normative Rule:** Cooperative cancellation via explicit cancellation tokens or channel closure across actor tasks. Unbounded asynchronous reactors are excluded from the V1 minimal core.

### Gate 12: Parallel Execution and Error Boundaries
- **Normative Rule:** `parallel { ... }` blocks dispatch concurrent workloads. If any branch raises an unhandled error or panic, the block initiates fail-fast termination: active tasks are joined, and the first error is propagated.

### Gate 13: ABI and Memory Layout
- **Normative Rule:** All scalar types and struct fields adhere to the target host C ABI (x86_64 MSVC on Windows, System V on Linux). Struct fields are laid out in declaration order with natural alignment padding, guaranteeing direct C-interop capability without marshalling overhead.

---

## 3. Type System Summary

| Type | Representation | Description |
|---|---|---|
| `Int` | 64-bit signed integer | Two's complement integer |
| `Float` | 64-bit IEEE 754 | Double-precision floating point |
| `Bool` | 1-bit / 64-bit native register | Strict boolean (`true` / `false`) |
| `String` | Pointer + Length | UTF-8 encoded string |
| `Unit` | 0-bit | Empty return / tuple |
| `List<T>` | Contiguous dynamic array | Managed heap slice |
| `Map<K, V>` | Hash map | Key-value associative table |
| `Outcome<T>` | Struct `{ is_success: Bool, value: T, error_msg: String }` | Canonical error wrapper |
| `Maybe<T>` | Struct `{ has_value: Bool, value: T }` | Canonical optional wrapper |
| `View<T>` | Read-only slice | Zero-copy non-allocating borrow |
| `mut View<T>` | Exclusive mutable slice | Safe non-aliased mutable borrow |
