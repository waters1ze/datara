# Anti-Pattern: Common Syntax Mistakes in Datara

This guide catalogs the most common syntax errors developers and AI agents make when transitioning to Datara from languages like Python, TypeScript, Go, or Rust.

---

## 1. Using Deprecated Operator `:=`

- **Mistake**: Using `:=` for assignment or initialization.
- **Compiler Error**: `SyntaxError: Operator ':=' is deprecated. Use 'let' or 'mut'`.
- **Bad Code**:
  ```datara
  count := 10
  mut name := "Alice"
  ```
- **Correct Code**:
  ```datara
  let count = 10
  mut name = "Alice"
  ```

---

## 2. Using `try / catch` Blocks

- **Mistake**: Wrapping failing calls in `try { ... } catch { ... }`.
- **Compiler Error**: `SyntaxUnexpectedToken: 'try' is not recognized as a valid keyword`.
- **Fact**: `try/catch` was completely eliminated from Datara.
- **Correct Code**:
  ```datara
  // Use postfix ? for early return propagation
  let result = risky_operation()?

  // Or use postfix 'or' for fallback
  let fallback = risky_operation() or default_val
  ```

---

## 3. Expecting String Interpolation in Regular Quotes

- **Mistake**: Expecting `"Hello {name}"` to interpolate variables.
- **Fact**: In Datara, `"..."` is 100% literal text. Braces `{name}` are never evaluated in regular strings.
- **Bad Code**:
  ```datara
  let greeting = "Hello {name}" // Prints literal "Hello {name}"!
  ```
- **Correct Code**:
  ```datara
  let greeting = fmt"Hello {name}" // Stream Fusion activates
  ```

---

## 4. Using Lone `&` or `|` Operators

- **Mistake**: Using single `&` or `|` for bitwise arithmetic or boolean logic.
- **Compiler Error**: `Datara does not support lone '&' or '|' operators. Did you mean '&&', '||', or '|>'?`
- **Bad Code**:
  ```datara
  let bitmask = a & b
  let flags = a | b
  ```
- **Correct Code**:
  ```datara
  let bitmask = and(a, b)
  let flags = or(a, b)
  ```
