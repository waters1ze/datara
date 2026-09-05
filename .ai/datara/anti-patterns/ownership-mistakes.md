# Anti-Pattern: Ownership & Borrowing Mistakes

Datara uses an affine type system with zero-copy views. Understanding ownership boundaries prevents compile errors.

---

## 1. Use After Move (`E-BORROW-001` / `E-BORROW-002`)

- **Mistake**: Passing an owned resource into another variable and then reading it again.
- **Bad Code**:
  ```datara
  let data = load_large_dataset()
  let processor = DataProcessor { records: data } // 'data' moved here!
  out data.len() // Compile Error: use of moved value 'data'
  ```
- **Correct Code (Borrow via View)**:
  ```datara
  let data = load_large_dataset()
  let processor = DataProcessor { records: view data }
  out data.len() // Valid: 'data' was only borrowed
  ```

---

## 2. Reassigning an Immutable Variable (`E-BORROW-002`)

- **Mistake**: Mutating a variable declared with `let` or `val`.
- **Bad Code**:
  ```datara
  let counter = 0
  counter = counter + 1 // Compile Error: cannot mutate immutable binding
  ```
- **Correct Code**:
  ```datara
  mut counter = 0
  counter = counter + 1
  ```

---

## 3. Mutating Data During an Active View (`E-BORROW-003`)

- **Mistake**: Modifying the original variable while a `view` is still in scope.
- **Bad Code**:
  ```datara
  mut buffer = [1, 2, 3]
  let v = buffer.view()
  buffer = [4, 5, 6] // Compile Error: buffer is borrowed by 'v'
  out v
  ```
- **Correct Code**:
  ```datara
  mut buffer = [1, 2, 3]
  {
      let v = buffer.view()
      out v
  } // 'v' goes out of scope here
  buffer = [4, 5, 6] // Now legal
  ```
