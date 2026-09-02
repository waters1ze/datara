# Datara Language Reference Manual (Specification v1.0 Production)

**Author:** Datara Language & Compiler Team  
**Toolchain:** `forgen` (Native Cranelift AOT Toolchain)  
**Target Architecture:** x86_64 / AArch64 Native (Zero GC, Zero Stop-The-World Pauses)  
**Status:** Canonical Reference Manual & Normative Specification  

---

## Table of Contents

1. [Introduction & Architectural Philosophy](#1-introduction--architectural-philosophy)
2. [Lexical Grammar & Token Structure](#2-lexical-grammar--token-structure)
3. [The Variable Triad: Deterministic State](#3-the-variable-triad-deterministic-state)
4. [Type System & Primitive Representations](#4-type-system--primitive-representations)
5. [Data-Oriented Programming (DOP): Classes & Behaviors](#5-data-oriented-programming-dop-classes--behaviors)
6. [Universal Function Call Syntax (UFCS) & Pipeline Operators](#6-universal-function-call-syntax-ufcs--pipeline-operators)
7. [Affine Ownership, Borrowing & Lifetimes](#7-affine-ownership-borrowing--lifetimes)
8. [The Algebraic Effect Lattice](#8-the-algebraic-effect-lattice)
9. [Pattern Matching, Deciders & Guards](#9-pattern-matching-deciders--guards)
10. [Deterministic Error Handling: Outcome & Question Mark](#10-deterministic-error-handling-outcome--question-mark)
11. [High-Performance Multithreading & Parallelism](#11-high-performance-multithreading--parallelism)
12. [Standard Library Specification](#12-standard-library-specification)
13. [Foreign Function Interface (FFI) & C ABI](#13-foreign-function-interface-ffi--c-abi)
14. [Compiler Internals & The Evidence Gate Optimizer](#14-compiler-internals--the-evidence-gate-optimizer)
15. [The `forgen` Toolchain Command Line Manual](#15-the-forgen-toolchain-command-line-manual)
16. [Diagnostics & Error Index](#16-diagnostics--error-index)

---

## 1. Introduction & Architectural Philosophy

Datara is a statically typed, compiled systems and application programming language designed for mechanical sympathy, high throughput, zero garbage-collection latency, and high developer velocity.

### Core Tenets

1. **Deterministic Affine Memory**: No tracing garbage collector, no automated reference counting overhead. All allocations follow scope-bound ownership, borrow regions, and Scalar Replacement of Aggregates (SROA).
2. **The Evidence Gate Optimizer**: The compiler (`forgen`) does not accept optimization passes based on heuristic guesses. Every SSA transformation pass (Mem2Reg, GVN, SROA, LoopFold, Inliner) is validated against structural invariant fingerprints before machine code emission.
3. **Purity Lattice & Effect Tracking**: Side-effects (`IO`, `Network`, `Database`, `State`, `Time`) are strictly tracked through an algebraic lattice, allowing pure code to be aggressively folded, inlined, and vectorized.
4. **Data-Oriented Separation**: State is held in plain-old-data `class` or `packet` definitions; behavior is decoupled in `behavior` blocks, allowing clean Universal Function Call Syntax (`x.f()` $\equiv$ `f(x)`).
5. **No Silent Fallbacks**: The compiler and code generator strictly enforce complete symbol resolution, type consistency, and SSA dominance. Unresolved calls and invalid mutations trigger immediate compile-time diagnostics.

---

## 2. Lexical Grammar & Token Structure

Datara source code is UTF-8 encoded text stored in `.dtr` files.

### 2.1 Identifiers and Keywords
- **Identifiers**: Match `[a-zA-Z_][a-zA-Z0-9_]*`.
- **Keywords**: `let`, `mut`, `val`, `const`, `fn`, `function`, `class`, `packet`, `behavior`, `component`, `role`, `if`, `else`, `while`, `for`, `parallel`, `with`, `match`, `decide`, `use`, `return`, `extern`, `out`, `err`, `true`, `false`, `null`.

### 2.2 Literals
- **Integers**: Decimal integers (`0`, `42`, `100000`, `-99`). Represented internally as 64-bit signed two's-complement (`i64`).
- **Floats**: Decimal floating point numbers (`3.14159`, `0.0`, `-42.5`). Represented as IEEE 754 double precision (`f64`).
- **Booleans**: `true`, `false`. Represented as `i8` boolean extended to `i64`.
- **Characters**: Single-quoted Unicode scalar literals (`'A'`, `'\n'`, `'\t'`, `'\\'`, `'\''`). Represented as `u32`.
- **Strings**: Double-quoted UTF-8 literals (`"Hello, world!"`). Support escape sequences (`\n`, `\t`, `\r`, `\\`, `\"`, `\0`, `\{`).
- **Interpolated Strings**: When `{expression}` occurs inside double quotes, it is dynamically evaluated and string-formatted at runtime:
  ```datara
  let name = "Alice"
  let age = 30
  out "User: {name}, Age: {age}"
  ```
  To print a literal curly brace inside a string, escape it with a backslash: `"\\{status\\}"`.

### 2.3 Comments
- Single-line comments start with `//` and extend to the end of the line.
- Block comments start with `/*` and terminate with `*/`.

---

## 3. The Variable Triad: Deterministic State

Datara eliminates variable mutation ambiguity through three distinct declaration keywords:

| Keyword | Mutability | Type Dynamics | Reassignment | Primary Usage |
| :--- | :--- | :--- | :--- | :--- |
| `let` | **Immutable** | Static | **Forbidden** | Constants, invariant intermediate values, functional pipelines |
| `mut` | **Mutable** | Type-Locked | **Permitted** (Same type) | Loop counters, accumulators, state machines |
| `val` | **Dynamic** | Polymorphic | **Permitted** (Any type with `mut val`) | Gradual typing, schema evolution, dynamic JSON payload |

### 3.1 Syntax Examples

```datara
// 1. Immutable binding
let host = "127.0.0.1"
let port: Int = 8080
// host = "localhost"  // COMPILE ERROR: E-MUT-001 (Cannot mutate immutable variable)

// 2. Type-locked mutable variable
mut connections: Int = 0
connections = connections + 1
// connections = "many" // COMPILE ERROR: E-TYPE-001 (Cannot assign String to Int variable)

// 3. Dynamic container
val raw_payload = 42
// When declared as mutable dynamic:
mut val flexible = 100
flexible = "now a string" // Valid dynamic transition
```

*(Note: Legacy Go-style `:=` syntax is strictly forbidden and produces a helpful compiler diagnostic directing the developer to use `let` or `mut`).*

---

## 4. Type System & Primitive Representations

Datara is statically typed with local type inference.

### 4.1 Primitive Types

| Primitive Type | CLIF Representation | C ABI Type | Description |
| :--- | :--- | :--- | :--- |
| `Int` | `i64` | `int64_t` | 64-bit signed integer |
| `Float` | `f64` | `double` | 64-bit double precision float |
| `Bool` | `i8` (extended to `i64`) | `int64_t` | Boolean logic |
| `Char` | `i64` | `uint32_t` | Unicode character code point |
| `Str` / `String` | `i64` (pointer) | `const char*` | UTF-8 null-terminated heap string |
| `Unit` | `void` | `void` | Zero-sized unit value |

### 4.2 Compound & Generic Types

1. **Tuples**: Lightweight fixed-size heterogeneous records:
   ```datara
   let pair: (Int, Str) = (42, "Answer")
   let x = pair.0
   let y = pair.1
   ```
2. **Lists**: Contiguous, dynamically resizable arrays allocated on heap:
   ```datara
   let numbers = [10, 20, 30, 40]
   let repeated = [0; 16] // Creates 16 zeroes
   ```
3. **Maps**: Associative hash maps mapping string keys to values:
   ```datara
   let config = { "timeout": 30, "retries": 3 }
   let timeout = config["timeout"]
   ```
4. **Generics**: Parameterized types for classes and behaviors:
   ```datara
   class Box<T> {
       val: T
   }
   ```

---

## 5. Data-Oriented Programming (DOP): Classes & Behaviors

Datara explicitly decouples state structures (`class`, `packet`) from executable operations (`behavior`).

### 5.1 Defining State: `class`

```datara
class Point {
    x: Float
    y: Float
}

class User {
    id: Int
    name: Str
    is_active: Bool
}
```

Structs can be instantiated using standard struct-literal syntax:
```datara
let p = Point { x: 3.0, y: 4.0 }
```

### 5.2 Defining Operations: `behavior`

```datara
behavior Point {
    length_sq() -> Float {
        return this.x * this.x + this.y * this.y
    }

    scale(factor: Float) -> Point {
        return Point { x: this.x * factor, y: this.y * factor }
    }
}
```

### 5.3 Composition Over Inheritance

Datara rejects deep class inheritance hierarchies in favor of flat composition using `using`:

```datara
class Logger {
    prefix: Str
}

class Service {
    using Logger
    port: Int
}
```

All fields and behaviors of `Logger` are flattened directly into `Service` at compile time with zero indirection.

---

## 6. Universal Function Call Syntax (UFCS) & Pipeline Operators

Datara provides full Universal Function Call Syntax: any standalone function `fn process(data: Data, factor: Int)` can be called either as:
1. Standard function call: `process(my_data, 10)`
2. Method syntax: `my_data.process(10)`
3. Pipeline operator syntax: `my_data |> process(10)`

### 6.1 The Pipeline Operator (`|>`)

```datara
fn sanitize(s: Str) -> Str => str_trim(s)
fn quote(s: Str) -> Str => "'" + s + "'"

fn main() {
    let raw = "   clean data   "
    let result = raw |> sanitize() |> quote()
    out result // Prints: 'clean data'
}
```

---

## 7. Affine Ownership, Borrowing & Lifetimes

Datara achieves deterministic memory management without garbage collection through an affine ownership system.

### 7.1 Move Semantics
Every heap-backed resource has a single owner. Passing an owned resource transfers ownership:

```datara
let f = File { path: "data.txt" }
let consumer = consume_file(f)
// out f.read() // COMPILE ERROR: E-OWN-001 (Use of moved resource 'f')
```

### 7.2 Borrowing & Views
Temporary, zero-copy access to data is performed via borrows (`view` or `mut_view`):

```datara
fn inspect_buffer(view buf: Buffer) {
    out buf.length()
}
```

### 7.3 Active Borrow Conflict Detection
The compiler tracks all live borrow scopes. Mutating, re-binding, or moving a variable while an active view is open is rejected at compile-time:

```datara
let orig = Point { x: 10.0, y: 20.0 }
let v = view orig
// let orig = Point { x: 0.0, y: 0.0 } // COMPILE ERROR: E-BORROW-003 (Borrow conflict)
```

---

## 8. The Algebraic Effect Lattice

Every Datara function is analyzed by the compiler's Effect Lattice Engine. Effects form a bounded semi-lattice:

$$\text{Pure} \subset \{\text{State}\} \subset \{\text{IO}\} \subset \{\text{Network}, \text{Database}\} \subset \{\text{Nondeterministic}\}$$

```
                Nondeterministic / Unsafe
                       /         \
                 Database       Network
                       \         /
                          IO
                          |
                        State
                          |
                         Pure
```

### 8.1 Effect Inference
The developer does not need to annotate effects manually. The compiler automatically infers effects:
- Pure arithmetic, string concatenation, and local variable transformations are inferred as `Pure`.
- Console printing (`out`, `err`, `print`), file system access (`file_read`, `file_write`) propagate `IO`.
- Sockets and HTTP calls propagate `Network`.

### 8.2 Effect-Guided Inlining
The `forgen` optimizer prioritizes inlining for `Pure` functions, unrolling loops and performing constant folding without risk of reordering observable side effects.

---

## 9. Pattern Matching, Deciders & Guards

Datara replaces error-prone nested `if/else` ladders with expressive `match` and `decide` constructs:

```datara
fn classify(status_code: Int) -> Str {
    return match status_code {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        404 => "Not Found",
        code if code >= 500 => "Server Error",
        _ => "Unknown Status"
    }
}
```

### 9.1 The `decide` Statement

For multi-branch state evaluation:

```datara
decide {
    score >= 90 => out "Grade: A",
    score >= 80 => out "Grade: B",
    score >= 70 => out "Grade: C",
    _           => out "Grade: F"
}
```

---

## 10. Deterministic Error Handling: Outcome & Question Mark

Datara rejects unchecked runtime exceptions (`try / catch / throw`). Instead, all fallible operations return the algebraic `Outcome<T, E>` (or `Result<T, E>`) type:

```datara
class Outcome<T> {
    is_success: Bool
    value: T
    error_msg: Str
}
```

### 10.1 The `?` (Try/Propagate) Operator
Appending `?` to an `Outcome` or `Option` expression checks for failure:
- If the result is successful, it unwraps the inner `value`.
- If the result is an error, it immediately returns the error from the enclosing function.

```datara
fn load_config(path: Str) -> Outcome<Str> {
    let content = read_file_safe(path)?
    return Outcome<Str> { is_success: true, value: content, error_msg: "" }
}
```

### 10.2 The `or` Fallback Operator
Provide safe default fallbacks without branching:

```datara
let port = parse_port(env_var) or 8080
```

---

## 11. High-Performance Multithreading & Parallelism

Datara features a native work-stealing thread pool built directly into the runtime (`datara_runtime.c`), enabling fork-join parallelism:

### 11.1 `parallel for` Loop
Executes loop iterations concurrently across available CPU cores:

```datara
fn main() {
    mut data = [0; 1000000]
    
    parallel for i in 0..1000000 {
        data[i] = i * 2
    }
}
```

### 11.2 `parallel_invoke`
Executes multiple closures concurrently:

```datara
parallel_invoke(
    fn() { compute_matrices() },
    fn() { fetch_remote_assets() }
)
```

---

## 12. Standard Library Specification

Datara provides a comprehensive, production-grade standard library in `stdlib/`:

### 12.1 High-Performance Fast Math (`stdlib.math.math`)
Native CPU-accelerated mathematical intrinsics:

| Function | Signature | Description |
| :--- | :--- | :--- |
| `math_sqrt(x)` | `Float -> Float` | Single-instruction square root |
| `math_pow(b, e)` | `(Float, Float) -> Float` | Exponential power $b^e$ |
| `math_abs(x)` | `Float -> Float` | Floating-point absolute value |
| `math_sin(x)` | `Float -> Float` | Sine (radians) |
| `math_cos(x)` | `Float -> Float` | Cosine (radians) |
| `math_tan(x)` | `Float -> Float` | Tangent (radians) |
| `math_floor(x)` | `Float -> Float` | Floor rounding |
| `math_ceil(x)` | `Float -> Float` | Ceiling rounding |
| `math_round(x)` | `Float -> Float` | Nearest integer rounding |
| `math_min(a, b)` | `(Float, Float) -> Float` | Minimum of two floats |
| `math_max(a, b)` | `(Float, Float) -> Float` | Maximum of two floats |
| `math_hypot(a, b)` | `(Float, Float) -> Float` | Euclidean distance $\sqrt{a^2 + b^2}$ |
| `math_min_int(a, b)` | `(Int, Int) -> Int` | Minimum of two integers |
| `math_max_int(a, b)` | `(Int, Int) -> Int` | Maximum of two integers |
| `math_abs_int(x)` | `Int -> Int` | Integer absolute value |

### 12.2 String Primitives (`stdlib.text.string`)
- `str_len(s: Str) -> Int`: String byte length.
- `str_trim(s: Str) -> Str`: Strip leading and trailing whitespace.
- `str_contains(s: Str, sub: Str) -> Bool`: Substring check.
- `str_starts_with(s: Str, prefix: Str) -> Bool`: Prefix match.
- `str_ends_with(s: Str, suffix: Str) -> Bool`: Suffix match.
- `str_index_of(s: Str, sub: Str) -> Int`: Substring position (-1 if not found).
- `str_substring(s: Str, start: Int, len: Int) -> Str`: Zero-copy slice.
- `str_to_int(s: Str) -> Int`: Parse integer from string.
- `str_to_float(s: Str) -> Float`: Parse float from string.

### 12.3 File System & I/O (`stdlib.io.fs`)
- `file_read(path: Str) -> Str`: Read entire file into string.
- `file_write(path: Str, content: Str) -> Int`: Write content (overwrites).
- `file_append(path: Str, content: Str) -> Int`: Append content to file.
- `file_exists(path: Str) -> Bool`: Returns `true` if file exists on disk.

### 12.4 JSON Parser (`stdlib.json.parser`)
- `jp.get_string(raw_json: Str, key: Str) -> Str`: Extract string field.
- `jp.get_int(raw_json: Str, key: Str) -> Int`: Extract integer field.
- `jp.get_bool(raw_json: Str, key: Str) -> Bool`: Extract boolean field.

### 12.5 Networking & Sockets (`stdlib.net.socket`)
- `socket_create(is_tcp: Int) -> Int`: Create TCP (`1`) or UDP (`0`) socket.
- `socket_bind(sock: Int, host: Str, port: Int) -> Int`: Bind socket to address.
- `socket_listen(sock: Int, backlog: Int) -> Int`: Listen for incoming connections.
- `socket_accept(sock: Int) -> Int`: Accept client connection.
- `socket_connect(sock: Int, host: Str, port: Int) -> Int`: Connect to server.
- `socket_send(sock: Int, data: Str) -> Int`: Transmit data.
- `socket_recv(sock: Int, max_bytes: Int) -> Str`: Receive incoming data.
- `socket_close(sock: Int)`: Release socket descriptor.

### 12.6 Cryptography (`stdlib.crypto.hash`)
- `sha256(input: Str) -> Str`: Standard SHA-256 hex digest.
- `base64_encode(input: Str) -> Str`: Standard Base64 encoding.
- `base64_decode(input: Str) -> Str`: Decode Base64 string.

### 12.7 Time & System (`stdlib.time.clock`, `stdlib.sys.process`)
- `now_ms() -> Int`: UNIX epoch timestamp in milliseconds.
- `sleep(ms: Int)`: Sleep current thread for `ms` milliseconds.
- `system(cmd: Str) -> Int`: Execute shell command, returns exit code.
- `exec(cmd: Str) -> Str`: Execute command and capture stdout.

---

## 13. Foreign Function Interface (FFI) & C ABI

Datara provides zero-cost C ABI interoperability using the `extern "C"` declaration syntax:

```datara
extern "C" {
    fn puts(str: Str) -> Int
    fn sin(x: Float) -> Float
}

fn main() {
    puts("Calling C runtime directly from Datara!")
}
```

Functions link directly against system dynamic libraries and object files without marshalling overhead.

---

## 14. Compiler Internals & The Evidence Gate Optimizer

The `forgen` compiler compiles Datara source code through a strictly verified pipeline:

```
[Source .dtr]
      |
   [Lexer] -> Tokens
      |
   [Parser] -> Abstract Syntax Tree (AST)
      |
   [TypeChecker] -> Checked AST & Scope Graph
      |
   [Resolver] -> Symbol Graph & Function Signatures
      |
   [Lowering] -> Datara Mid-level IR (DMIR)
      |
   [SSA Optimizer]
      |-- Mem2Reg (Stack-to-Register Promotion)
      |-- SROA (Scalar Replacement of Aggregates)
      |-- Global CSE (Common Subexpression Elimination)
      |-- LICM (Loop Invariant Code Motion)
      |-- LoopFold (Closed-form induction unrolling)
      |-- Inliner (Effect-guided inlining)
      |
   [Evidence Gate] -> Invariant Proof Audit
      |
   [Cranelift Native Backend]
      |
   [Machine Executable (.exe / ELF)]
```

### The Evidence Gate Proof Protocol
Every optimization pass records a cryptographic structural hash of IR before and after execution. Passes that do not produce mathematically verifiable simplifications or reduce loop trip counts are reverted, preventing optimization regressions.

---

## 15. The `forgen` Toolchain Command Line Manual

`forgen` is the unified compiler, package manager, test runner, and language server CLI:

```
Usage: forgen <command> [arguments] [options]
```

### Primary Commands

- `forgen run [file|dir]`: Compile and immediately execute a Datara program.
- `forgen build [file|dir]`: Compile standalone native binary (`.exe` on Windows, ELF on Linux/macOS).
- `forgen check [file|dir]`: Perform fast static verification (types, ownership, effects) with zero code generation.
- `forgen test [dir]`: Discover and execute all integration tests in `tests/`.
- `forgen bench [dir]`: Run benchmark suites in `benches/`.
- `forgen init [name] [--lib]`: Initialize a Level 3 project with `datara.toml`, `src/`, and `tests/`.
- `forgen new <name>`: Create a new project in a new subdirectory.
- `forgen package`: Verify, test, and bundle library package for distribution.
- `forgen lsp`: Launch the official Language Server Protocol daemon (stdio).
- `forgen fmt [file|dir]`: Automatically format Datara source code according to official style guidelines.
- `forgen domain`: Run whole-program specialization and print the Semantic Adaptation Engine (SAE) report.
- `forgen why <symbol>`: Explain why an optimization pass was applied or rejected for a function.
- `forgen inspect <query> <file>`: Inspect intermediate representations (`clif`, `dmir`, `effects`, `ast`).

---

## 16. Diagnostics & Error Index

Datara errors are categorized by phase with unambiguous diagnostic codes:

| Error Code | Category | Meaning | Resolution |
| :--- | :--- | :--- | :--- |
| `E-SYNTAX-001` | Syntax | Unexpected token or malformed expression | Check punctuation, keywords, and brackets |
| `E-MUT-001` | Ownership | Attempt to reassign an immutable `let` variable | Change declaration to `mut` |
| `E-TYPE-001` | Types | Type mismatch in assignment or function return | Align expression type with declared type |
| `E-OWN-001` | Ownership | Use of moved resource | Borrow resource using `view` or clone data |
| `E-BORROW-003` | Ownership | Variable mutated or re-bound during active view | Release or end borrow scope before mutation |
| `E-CODEGEN-001` | Codegen | Unresolved function, method, or SSA value | Ensure function is declared or imported |

---

*Copyright (c) 2026 Datara Language Contributors. Licensed under the MIT or Apache-2.0 License.*
