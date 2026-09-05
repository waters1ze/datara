# Anti-Pattern: Type System Mistakes

Datara is strictly typed. It never guesses or performs silent lossy conversions.

---

## 1. Silent Numeric Coercion

- **Mistake**: Passing a `Float` to an `Int` parameter or assigning directly.
- **Bad Code**:
  ```datara
  let x: Int = 10.5 // Compile Error: TypeMismatch
  ```
- **Correct Code**:
  ```datara
  let x: Int = 10.5 as Int
  ```

---

## 2. Integer Conditionals (Truthiness Fallacy)

- **Mistake**: Expecting non-zero integers or pointers to evaluate as boolean in `if` statements.
- **Bad Code**:
  ```datara
  let count = 5
  if count { // Compile Error: expected Bool, found Int
      out "positive"
  }
  ```
- **Correct Code**:
  ```datara
  let count = 5
  if count > 0 {
      out "positive"
  }
  ```

---

## 3. Missing Explicit Function Return Value

- **Mistake**: Forgetting `return` on certain branch paths in a non-void function.
- **Bad Code**:
  ```datara
  fn get_label(code: Int) -> Str {
      if code == 1 {
          return "OK"
      }
      // Missing return in else path triggers E-TYPE-003
  }
  ```
- **Correct Code**:
  ```datara
  fn get_label(code: Int) -> Str {
      if code == 1 {
          return "OK"
      }
      return "ERROR"
  }
  ```
