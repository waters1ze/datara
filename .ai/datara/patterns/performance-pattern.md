# Datara Performance Engineering & Evidence Gate

Forgen uses the **Evidence Gate** optimizer to perform mathematical transformations, loop vectorization, and register promotion.

---

## 1. Immutable Bindings & Mem2Reg (`let` vs `mut`)

- Use `let` for all variables whose value does not change after initialization.
- **Why**: `let` variables are automatically promoted into CPU registers via Mem2Reg, enabling constant folding, dead-store elimination, and lock-free thread safety.
- **Rule**: If `mut` is unused, the compiler emits `perf::unnecessary_mut`. Run `forgen lint --fix` to auto-repair.

---

## 2. Idiomatic Loops & Closed-Form Vectorization

### Prefer Range `for` over `while`
- **Manual While**:
  ```datara
  mut i = 0
  while i < 1000 {
      process(i)
      i = i + 1
  }
  ```
  *Result*: Emits `style::prefer_for_loop`.

- **Idiomatic Range For**:
  ```datara
  for i in 0..1000 {
      process(i)
  }
  ```
  *Result*: The compiler recognizes the constant trip count, unrolls loops, and vectorizes operations via SIMD (AVX2 / NEON).

---

## 3. String Templates: Stream Fusion

Regular string concatenation creates multiple temporary heap allocations:
```datara
// Suboptimal:
let msg = "User " + name + " is " + age + " years old"

// Optimal (Stream Fusion):
let msg = fmt"User {name} is {age} years old"
```
The compiler calculates total required buffer capacity and writes values directly in a single pass.

---

## 4. Explainability & Why Commands

To inspect compiler decisions:
```bash
# Ask the compiler why optimizations were applied or rejected
forgen why compute_total

# Get machine-readable structured semantic metadata
forgen context compute_total
```
