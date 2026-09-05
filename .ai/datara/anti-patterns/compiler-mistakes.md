# Anti-Pattern: Safety Gate Violations (E0940 - E0943)

The Forgen compiler enforces four strict safety gates that cannot be bypassed silently.

---

## 1. Proof-Carrying Code Violation (`E0941`): Unproven Divisor

- **Mistake**: Dividing by a variable or expression without proving it cannot be zero.
- **Bad Code**:
  ```datara
  fn calc_ratio(a: Float, b: Float) -> Float {
      return a / b // Fatal Error[E0941]: Unproven divisor 'b' may be zero
  }
  ```
- **Fix 1 (Contract Precondition)**:
  ```datara
  fn calc_ratio(a: Float, b: Float) -> Float
      require b != 0.0, "Divisor cannot be 0"
  {
      return a / b
  }
  ```
- **Fix 2 (Guarded Branch)**:
  ```datara
  fn calc_ratio(a: Float, b: Float) -> Float {
      if b != 0.0 {
          return a / b
      }
      return 0.0
  }
  ```

---

## 2. Security Capability Violation (`E0940`): Unproven I/O

- **Mistake**: Direct file or network access without a capability token witness.
- **Bad Code**:
  ```datara
  let data = fs_read("passwords.txt") // Fatal Error[E0940]
  ```
- **Fix**: Receive `Capability<FileRead>` passed from `main(sys_caps: SystemCapabilities)`.

---

## 3. Unchecked FFI Violation (`E0942`)

- **Mistake**: Calling external C/Rust FFI functions without justification.
- **Bad Code**:
  ```datara
  foreign_c_function() // Fatal Error[E0942]
  ```
- **Fix**:
  ```datara
  unsafe(justification: "Calling validated hardware timer C library") {
      foreign_c_function()
  }
  ```

---

## 4. Data Race Violation (`E0943`)

- **Mistake**: Mutating shared outer state inside `parallel for`.
- **Bad Code**:
  ```datara
  mut acc = 0
  parallel for i in 0..10 {
      acc = acc + i // Fatal Error[E0943]: data race on 'acc'
  }
  ```
- **Fix**: Use thread-local accumulators.
