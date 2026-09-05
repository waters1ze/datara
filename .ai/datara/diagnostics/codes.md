# Datara Compiler Diagnostics & Error Code Reference

This document is the authoritative, compiler-grounded catalog of all diagnostic codes, error conditions, safety gates, and linter rules in **Forgen** (the optimizing native compiler for Datara).

---

## 1. Syntax Errors (`E-SYNTAX-*`)

### `E-SYNTAX-001`: SyntaxUnexpectedToken
- **Meaning**: The parser encountered a token that violates the Datara grammar.
- **Common Trigger**: Using deprecated syntax such as `:=`, lone `&` / `|`, `try/catch`, or `extends`.
- **Compiler Command**: `forgen explain E-SYNTAX-001`
- **Bad Code**:
  ```datara
  mut x := 10        // Error: ':=' is deprecated
  let mask = a & b   // Error: lone '&' is not allowed
  ```
- **Good Code**:
  ```datara
  mut x: Int = 10
  let mask = and(a, b) // Use bitwise function 'and'
  ```

### `E-SYNTAX-002`: SyntaxUnterminatedString
- **Meaning**: A string literal was opened with `"` or `fmt"` but never closed before end-of-line or file.
- **Bad Code**:
  ```datara
  let msg = "Hello world
  ```
- **Good Code**:
  ```datara
  let msg = "Hello world"
  ```

### `E-SYNTAX-003`: SyntaxUnterminatedComment
- **Meaning**: Multi-line comment `/*` has no matching closing `*/`.

### `E-SYNTAX-004`: SyntaxInvalidNumber
- **Meaning**: Malformed integer or floating-point literal (e.g., `12.34.56` or invalid base prefix).

### `E-SYNTAX-005`: SyntaxInvalidChar
- **Meaning**: An unrecognized character in the input source stream.

### `E-SYNTAX-006`: SyntaxExpectedExpression
- **Meaning**: An operator or statement expected an expression on its right-hand side, but none was provided.

### `E-SYNTAX-007`: SyntaxExpectedIdentifier
- **Meaning**: A declaration keyword (`let`, `mut`, `fn`, `class`) was followed by a token other than a valid identifier.

### `E-SYNTAX-008`: SyntaxExpectedType
- **Meaning**: Type annotation `:` was followed by an invalid token instead of a type name.

---

## 2. Resolution Errors (`E-RESOLVE-*`)

### `E-RESOLVE-001`: ResolveUndefinedSymbol
- **Meaning**: An identifier was referenced that is not present in the current lexical scope or imported modules.
- **Compiler Command**: `forgen explain E-RESOLVE-001`
- **Common Triggers**:
  1. Typo in variable or function name.
  2. Missing `use` import (e.g. `use stdlib.math.Math`).
  3. Scope leak (accessing a variable declared inside an inner `if` or `for` block).
- **Fix**: Verify symbol name, add proper `use` import, or widen the declaration scope.

### `E-RESOLVE-002`: ResolveDuplicateSymbol
- **Meaning**: Two declarations in the same scope share the exact same identifier.
- **Bad Code**:
  ```datara
  let count = 10
  let count = 20  // Error: duplicate declaration
  ```
- **Good Code**:
  ```datara
  let count = 10
  let second_count = 20
  ```

### `E-RESOLVE-003`: ResolveUnknownType
- **Meaning**: A type annotation refers to a type name that cannot be resolved in the project or stdlib.

### `E-RESOLVE-004`: ResolveCircularDependency
- **Meaning**: Two or more modules import each other cyclically without a clear architectural hierarchy.

### `E-RESOLVE-005`: ResolveUnreachableModule
- **Meaning**: A `use` path cannot be located on disk or in the embedded standard library catalog.

---

## 3. Type Errors (`E-TYPE-*`)

### `E-TYPE-001`: TypeMismatch
- **Meaning**: An expression type does not match the expected static type in assignment, return, or parameter passing.
- **Compiler Command**: `forgen explain E-TYPE-001`
- **Core Rule**: Datara is strictly typed. There are NO silent implicit numeric conversions.
- **Bad Code**:
  ```datara
  let x: Int = 3.14   // Float assigned to Int
  if count { ... }    // Int used where Bool is required
  ```
- **Good Code**:
  ```datara
  let x: Int = 3.14 as Int
  if count != 0 { ... }
  ```

### `E-TYPE-002`: TypeCannotInfer
- **Meaning**: The type inference engine cannot deduce the exact type without an explicit type annotation.

### `E-TYPE-003`: TypeMissingReturn
- **Meaning**: A non-void function has execution paths that do not end in a `return` or terminal expression.

### `E-TYPE-004`: TypeInvalidBinaryOp
- **Meaning**: An operator is not defined for the provided operand types (e.g., adding a `Bool` to a `Str`).

### `E-TYPE-005`: TypeInvalidUnaryOp
- **Meaning**: Unary negation `-` or logical not `!` applied to an incompatible type.

### `E-TYPE-006`: TypeInvalidMemberAccess
- **Meaning**: Accessing `obj.field` or `obj.method()` where the member does not exist in the class, entity, or behavior.

### `E-TYPE-007`: TypeGenericMismatch
- **Meaning**: Number or constraints of generic type arguments do not match definition (e.g. `List<Int, Str>`).

---

## 4. Borrow & Ownership Errors (`E-BORROW-*`)

### `E-BORROW-001`: BorrowUseAfterMove
- **Meaning**: An owned value was moved into another variable or function parameter, and subsequently accessed.
- **Compiler Command**: `forgen explain E-BORROW-002`
- **Bad Code**:
  ```datara
  let b = a
  out a    // Error: 'a' was moved into 'b'
  ```
- **Good Code**:
  ```datara
  let b = view a
  out a    // Valid: 'a' was borrowed, not consumed
  ```

### `E-BORROW-002`: BorrowCannotMutateImmutable
- **Meaning**: Attempting to reassign or mutate an immutable variable declared with `let` or `val`.
- **Compiler Command**: `forgen explain E-BORROW-001`
- **Bad Code**:
  ```datara
  let total = 0
  total = total + 1
  ```
- **Good Code**:
  ```datara
  mut total = 0
  total = total + 1
  ```

### `E-BORROW-003`: BorrowConflictActiveView
- **Meaning**: Mutating a source variable while an immutable `view` borrow is currently active.
- **Fix**: End the scope of the `view` before mutating the source variable.

### `E-BORROW-004`: BorrowMultipleMutableViews
- **Meaning**: Creating more than one mutable view simultaneously to the same variable.

### `E-BORROW-005`: BorrowEscapingView
- **Meaning**: Returning or storing a `view` that outlives the local variable it borrows from.

---

## 5. Effect Errors (`E-EFFECT-*`)

### `E-EFFECT-001`: EffectImpureInPureContext
- **Meaning**: A function declared as `pure` attempts to perform side effects (file I/O, networking, or global mutation).
- **Compiler Command**: `forgen explain E-EFFECT-001`
- **Fix**: Remove `pure` if side effects are required, or isolate effects in non-pure callers.

### `E-EFFECT-002`: EffectUnsafeOperation
- **Meaning**: Executing an operation marked unsafe without an enclosing `unsafe(justification: "...") { ... }` block.

### `E-EFFECT-003`: EffectUnhandledIO
- **Meaning**: Direct unhandled I/O executed without required capability or effect context.

---

## 6. Security & Zero-Trust Safety Gates (`E0940 - E0951`)

### `E0940`: SecurityViolation (Capability Required)
- **Meaning**: An OS-level operation (`fs_open`, `fs_write`, `net_connect`, `proc_spawn`) requires a capability token.
- **Bad Code**:
  ```datara
  fn steal(path: String) -> String {
      let handle = fs_open(path) // Error[E0940]
      return "stolen"
  }
  ```
- **Good Code**:
  ```datara
  fn read_config(path: String, token: Capability<FileRead>) -> String {
      let handle = token.open(path)
      return handle.read_all()
  }

  fn main(sys_caps: SystemCapabilities) {
      let safe_token = sys_caps.files.grant_readonly("config.json")
      let content = read_config("config.json", safe_token)
      out content
  }
  ```

### `E0941`: ProofCarryingCodeViolation (Unproven Divisor / Bounds)
- **Meaning**: An arithmetic division `/` has a divisor that the compiler cannot mathematically prove is non-zero.
- **Compiler Command**: `forgen explain E0941`
- **Bad Code**:
  ```datara
  fn calc_avg(total: Float, count: Float) -> Float {
      return total / count // Error[E0941]: unproven divisor count
  }
  ```
- **Good Code (Precondition Contract)**:
  ```datara
  fn calc_avg(total: Float, count: Float) -> Float
      require count != 0.0, "Count cannot be zero"
  {
      return total / count
  }
  ```
- **Good Code (Guarded Branch)**:
  ```datara
  fn calc_avg(total: Float, count: Float) -> Float {
      if count != 0.0 {
          return total / count
      }
      return 0.0
  }
  ```
- **Good Code (Refinement Type)**:
  ```datara
  type NonZeroFloat = Float where val != 0.0
  fn calc_avg(total: Float, count: NonZeroFloat) -> Float {
      return total / count
  }
  ```

### `E0942`: UncheckedFFIViolation
- **Meaning**: Calling an external C/Rust FFI function without an explicit `unsafe(justification: "...")` block.

### `E0943`: DataRaceViolation
- **Meaning**: A `parallel for` or `parallel` block accesses and mutates a variable shared across threads without thread-local isolation.
- **Bad Code**:
  ```datara
  mut total = 0
  parallel for i in 1..10 {
      total = total + i // Error[E0943]: data race on total
  }
  ```
- **Good Code**:
  ```datara
  parallel for i in 1..10 {
      mut local = 0
      local = local + i
  }
  ```

### `E0945`: InvariantViolation
- **Meaning**: Class or entity invariant condition failed validation upon method exit.

### `E0946`: TerminationViolation
- **Meaning**: Pure function contains a recursive call or loop whose termination cannot be statically proven.

### `E0947`: RangeViolation
- **Meaning**: Value statically proven to exceed variable interval refinement or fixed array bounds.

### `E0950`: AllocationViolation
- **Meaning**: Heap allocation detected in an `@no_alloc` hard real-time execution context.

### `E0951`: PanicViolation
- **Meaning**: An unhandled panic path exists in an `@no_panic` hard real-time execution context.

### `E0310`: NonExhaustiveMatch
- **Meaning**: A `match` statement fails to cover all variants of an `enum` ADT and lacks a wildcard `_` arm.

### `E0311`: UnreachablePattern
- **Meaning**: A pattern branch in `match` is preceded by an identical or wider pattern that renders it dead code.

### `E0420`: DimensionMismatch
- **Meaning**: Incompatible Units of Measure in physical arithmetic expressions (e.g. adding Meters to Seconds).

---

## 7. Linter & Style Rules

### `style::non_snake_case`
- **Enforces**: `snake_case` for variables, function names, method names, and parameters.
- **Auto-Fixable**: Yes (`forgen lint --fix`).

### `style::non_camel_case_types`
- **Enforces**: `PascalCase` for classes, entities, components, roles, packets, and enums.
- **Auto-Fixable**: Yes (`forgen lint --fix`).

### `perf::unnecessary_mut`
- **Enforces**: Variables declared with `mut` that are never mutated must use `let` to enable Mem2Reg register allocation.
- **Auto-Fixable**: Yes (`forgen lint --fix`).

### `style::unused_variable`
- **Enforces**: Declared variables must be read. If deliberately unused, prefix with underscore `_unused`.

### `style::prefer_for_loop`
- **Enforces**: Replacing manual index increment `while i < N { ... i = i + 1 }` with zero-cost vectorized `for i in 0..N`.

### `style::bool_comparison`
- **Enforces**: Simplifying `if cond == true` to `if cond`, and `if cond == false` to `if !cond`.
- **Auto-Fixable**: Yes (`forgen lint --fix`).
