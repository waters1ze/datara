# Anti-Pattern: Performance Mistakes

---

## 1. Unnecessary `mut` Bindings (`perf::unnecessary_mut`)

- **Mistake**: Declaring variables with `mut` when they are never reassigned.
- **Consequence**: Prevents the Evidence Gate optimizer from performing Mem2Reg register promotion.
- **Fix**: Replace `mut` with `let`, or run `forgen lint --fix`.

---

## 2. Manual While Index Counting (`style::prefer_for_loop`)

- **Mistake**:
  ```datara
  mut i = 0
  while i < 1000 {
      process(i)
      i = i + 1
  }
  ```
- **Consequence**: Defeats SIMD vectorization and loop-invariant code motion.
- **Fix**:
  ```datara
  for i in 0..1000 {
      process(i)
  }
  ```
