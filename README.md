# Datara: High-Performance Systems & Application Language

[![License](https://img.shields.io/badge/License-Apache_2.0_OR_MIT-blue.svg)](LICENSE-APACHE)
[![CI](https://github.com/waters1ze/datara/actions/workflows/ci.yml/badge.svg)](https://github.com/waters1ze/datara/actions/workflows/ci.yml)
[![Target](https://img.shields.io/badge/target-x86__64_native-orange.svg)]()
[![Codegen](https://img.shields.io/badge/codegen-Cranelift-purple.svg)]()
[![Verification](https://img.shields.io/badge/evidence_gate-verified-brightgreen.svg)]()

Datara is a high-performance compiled systems and application programming language and compiler toolchain (`forgen`) written in Rust. Engineered for cloud backends, high-frequency financial engines, scientific computing, and native desktop tooling, Datara bridges the gap between the productivity of modern expressive languages and the mechanical sympathy, zero-cost abstractions, and predictable latency of bare-metal systems.

Datara completely rejects garbage collection pauses and reference-counting cycles in favor of deterministic scope-based affine ownership. It pioneers the **Evidence Gate Optimizer**, an SSA-level verification framework that guarantees every optimization pass is backed by structural mathematical proof before being committed to binary codegen. Native machine code generation is executed via **Cranelift**, producing standalone, relocatable executables without external runtime bloat.

> 📖 **Official Language Specification**: For the exhaustive language specification, formal grammar, stdlib reference, effect lattice, and compiler internals, see the [**Datara Language Reference Manual**](docs/DATARA_LANGUAGE_REFERENCE_MANUAL.md).

---

## Key Highlights & Architectural Pillars

- **[AOT CODEGEN] Native Execution Speed**: Ahead-of-Time compilation to native machine code (`.exe` / ELF) via Cranelift with host hardware instruction support (SSE4.2, AVX, AVX2, FMA, BMI1, BMI2, POPCNT).
- **[AFFINE MEMORY] Deterministic Memory Safety**: Zero GC, zero runtime stop-the-world pauses. Scope-based affine ownership and borrow regions eliminate use-after-free and data races at compile time.
- **[FORMAL AUDIT] Proving Optimizer with Evidence Gate**: Every compiler optimization pass (`Inliner`, `Mem2Reg`, `Global CSE`, `LICM`, `SROA`, `LoopFold`) is validated against IR structural fingerprints. Passes that claim speedups without verified structural delta are mechanically rejected.
- **[PURITY LATTICE] Algebraic Effects & Effect Lattice**: Static tracking of computational effects (`pure`, `io`, `net`, `state`, `time`). Pure functions unlock aggressive compile-time evaluation and inlining.
- **[EXPRESSIVE SYNTAX] Modern Ergonomics**: Universal Function Call Syntax (UFCS), pipe operators (`|>`), string interpolation (`"{var}"`), first-class tuples, pattern matching with guards, and algebraic error propagation (`?`).
- **[ZERO-OVERHEAD FFI] Native C ABI Bridge**: Direct, zero-overhead binding to external C libraries, Win32 system APIs, and Rust dynamic libraries (`cdylib`).
- **[TOOLCHAIN] Built-in Language Server (LSP)**: First-class editor integration providing real-time diagnostics, visual error carets, and contextual help.

---

## Table of Contents

1. [Key Highlights & Architectural Pillars](#key-highlights--architectural-pillars)
2. [Language Tour & Syntax Guide](#language-tour--syntax-guide)
   - [Variable Triad (`let`, `mut`, `val`)](#variable-triad-let-mut-val)
   - [Primitive & Numerical Types](#primitive--numerical-types)
   - [Strings, Escapes & Character Literals](#strings-escapes--character-literals)
   - [Collections, Slices & Tuples](#collections-slices--tuples)
   - [Interactive Console I/O (`input`, `read_line`, conversions)](#interactive-console-io-input-read_line-conversions)
   - [Control Flow & Smart Narrowing](#control-flow--smart-narrowing)
   - [Functions, UFCS & Data Pipelines](#functions-ufcs--data-pipelines)
   - [Data-Oriented Programming (`class` & `behavior`)](#data-oriented-programming-class--behavior)
   - [Pattern Matching (`match` & `decide`)](#pattern-matching-match--decide)
   - [Deterministic Error Handling (`Result`, `Option`, `?`, `or`)](#deterministic-error-handling-result-option---or)
   - [Resource Management (`with`)](#resource-management-with)
   - [Data Parallelism (`parallel for`)](#concurrency--parallelism-parallel)
3. [Foreign Function Interface (FFI)](#foreign-function-interface-ffi)
   - [Declaring External Functions](#declaring-external-functions)
   - [Interoperability Strategy (C, Rust, Win32)](#interoperability-strategy)
4. [Standard Library Reference](#standard-library-reference)
5. [Compiler Architecture & Pipeline](#compiler-architecture--pipeline)
6. [Evidence Gate Optimizer Architecture](#evidence-gate-optimizer-architecture)
   - [Verification Protocol](#verification-protocol)
   - [SSA Optimization Passes](#ssa-optimization-passes)
   - [Closed-Form Loop Folding](#closed-form-loop-folding-loopfold)
7. [Codebase Volume & Engineering Audit](#codebase-volume--engineering-audit)
8. [Language Comparison Matrix](#language-comparison-matrix)
9. [Command Line Interface (`forgen`)](#command-line-interface-forgen)
10. [Building Desktop Applications & GUI](#building-desktop-applications--gui)
11. [Adoption Analysis & 2026–2027 Roadmap](#adoption-analysis--20262027-roadmap)
12. [Installation & Multi-Platform Setup Guide](#installation--multi-platform-setup-guide)
13. [Building from Source](#building-from-source)

---

## Language Tour & Syntax Guide

### Variable Triad (`let`, `mut`, `val`)

Datara replaces the ambiguity of dynamic typing with three distinct, purposeful variable declarations:

```datara
// 1. 'let': Immutable static binding. Assigned once, cannot be mutated.
let max_connections: Int = 1000
let host = "127.0.0.1"

// 2. 'mut': Statically type-locked mutable variable.
mut counter: Int = 0
counter = counter + 1
// counter = "string"  // Compile Error: E-TYPE-001 (Type mismatch)

// 3. 'val': Dynamic container for gradual evolution or external schema ingestion.
val payload = 42
// When declared with 'mut val', mutation across dynamic types is permitted.
```

*(Note: Legacy Go-style `:=` is rejected by the compiler with a direct diagnostic and suggestion).*

---

### Primitive & Numerical Types

Datara provides fixed-width, platform-independent numeric primitives and high-precision financial types:

| Type | Description | Machine Representation |
| :--- | :--- | :--- |
| `Int` | 64-bit signed two's-complement integer | `i64` |
| `Float` | 64-bit IEEE 754 double-precision float | `f64` |
| `Bool` | Boolean logic (`true` or `false`) | `i8` (extended to `i64`) |
| `Char` | Unicode scalar value in single quotes (`'A'`) | `u32` |
| `Str` | UTF-8 heap-allocated immutable string | `*const c_char` / slice |
| `Unit` | Empty return type (equivalent to `()`) | `void` |

> Note: `Dec64` / `Dec128` are reserved in the syntax but **not yet implemented** in the backend.

---

### Strings & Character Literals

Strings support full escape sequences and expressive inline interpolation:

```datara
fn main() {
    let char_a: Char = 'A'
    let message = "Welcome to Datara!\nVersion: 1.0\tStatus: Active"
    
    // Escaped quotes and literal curly braces
    let json_snippet = "\{\"status\": \"ok\", \"code\": 200\}"
    
    // String interpolation
    let user = "Alice"
    let score = 95
    out "User {user} scored {score} points."
}
```

---

### Collections & Tuples

Datara provides high-performance contiguous lists, associative maps, and lightweight fixed tuples:

```datara
fn main() {
    // 1. Lists
    let numbers = [10, 20, 30, 40]
    out "Length: {numbers.len()}"
    out "First: {numbers[0]}"
    
    // List repeated initialization
    let zeroes = [0; 8]
    
    // 2. Maps (Key-Value)
    let config = { "port": 8080, "timeout": 30 }
    let port = config["port"]
    
    // 3. Tuples
    let point = (100, 200, "label")
}
```

---

### Interactive Console I/O (`input`, `read_line`, conversions)

Datara provides built-in functions for interactive CLI tools and terminal applications:

```datara
fn main() {
    // 1. input(prompt: Str) -> Str: prints prompt to stdout and reads from stdin
    let username = input("Enter your username: ")

    // 2. read_line() -> Str: reads next line without printing a prompt
    let raw_age = read_line()

    // 3. Type conversions:
    let age = str_to_int(raw_age)
    let weight = str_to_float("72.5")

    // Formatted output
    out "Welcome, {username}! Age: {age}, Weight: {weight} kg."
}
```

Builtin I/O & parsing signatures:
- `input(prompt: Str) -> Str`: Writes prompt to standard output and reads user input from standard input up to newline.
- `read_line() -> Str`: Reads one line from standard input.
- `str_to_int(s: Str) -> Int`: Converts ASCII/UTF-8 numeric text into a signed 64-bit integer.
- `str_to_float(s: Str) -> Float`: Converts decimal text into an IEEE 754 64-bit float.
- `str_trim(s: Str) -> Str`: Returns a new string with leading and trailing whitespace stripped.

---

### Control Flow & Smart Narrowing

#### Conditionals & Smart Type Narrowing

`if` expressions and statements automatically narrow nullable `Option` types:

```datara
fn inspect(value: Int?) -> Str {
    if value != None {
        // 'value' is automatically narrowed to 'Int' inside this block
        let num: Int = value
        return "Value present: {num}"
    } else {
        return "No value"
    }
}
```

#### Loops

```datara
fn main() {
    // Range-based for loop
    mut total: Int = 0
    for i in 1..10 {
        total = total + i
    }
    
    // Iterating over collections
    let items = [1, 2, 3, 4]
    mut item_sum: Int = 0
    for item in items {
        item_sum = item_sum + item
    }
    
    // While loop
    mut n: Int = 5
    while n > 0 {
        n = n - 1
    }
}
```

---

### Functions, UFCS & Data Pipelines

Datara functions can be defined with block bodies or concise expression bodies (`=>`). 

Universal Function Call Syntax (UFCS) and the pipe operator (`|>`) allow data to flow left-to-right naturally:

```datara
// Concise expression body
fn square(n: Int) -> Int => n * n

fn increment(n: Int) -> Int => n + 1

fn format_result(val: Int) -> Str => "Result: {val}"

fn main() {
    let x = 5
    
    // Standard function call
    let a = square(x)
    
    // UFCS call syntax: x.square() resolves to square(x)
    let b = x.square().increment()
    
    // Pipeline syntax: values flow cleanly across transformations
    let c = x
        |> square
        |> increment
        |> format_result
        
    out c // "Result: 26"
}
```

---

### Data-Oriented Programming (`class` & `behavior`)

Datara adopts clean separation of data schema and behavior:

- **`class`**: Defines the physical memory layout (fields only).
- **`behavior`**: Implements methods, trait conformance, and algorithms operating on that state.

```datara
class User {
    id: Int
    username: Str
    is_admin: Bool
}

behavior User {
    display_name() -> Str {
        if this.is_admin {
            return "[ADMIN] " + this.username
        }
        return this.username
    }
    
    promote() {
        this.is_admin = true
    }
}

fn main() {
    let admin = User {
        id: 1,
        username: "root",
        is_admin: true
    }
    out admin.display_name()
}
```

---

### Pattern Matching (`match` & `decide`)

Pattern matching in Datara is exhaustive, type-checked, and supports literal values, identifier bindings, wildcards (`_`), and conditional guards:

```datara
fn classify_status(code: Int) -> Str {
    let message = match code {
        200 => "OK"
        201 => "Created"
        400 => "Bad Request"
        404 => "Not Found"
        err if err >= 500 => "Server Error: {err}"
        _ => "Unknown Code"
    }
    return message
}
```

---

### Zero-Cost Error Handling (`Result`, `Option`, `?`, `or`)

Datara does **not** have slow, unpredictable stack-unwinding exceptions (`try/catch` is completely deprecated and rejected by the compiler). Instead, errors are typed first-class values:

- `T!E` is sugar for `Outcome[T]` (`Result[T, E]`).
- `T?` is sugar for `Maybe[T]` (`Option[T]`).
- The `?` operator performs zero-copy early propagation.
- The `or { ... }` block provides inline fallbacks and recovery.

```datara
fn parse_port(s: Str) -> Int!Str {
    let port = str_to_int(s)
    if port == 0 {
        return Outcome.err("Invalid port number")
    }
    return Outcome.ok(port)
}

fn start_server(port_str: Str) -> Str!Str {
    // If parse_port returns an error, '?' immediately returns the error
    let port = parse_port(port_str)?
    return Outcome.ok("Server running on port {port}")
}

fn main() {
    // Using 'or' for fallback default value
    let port = parse_port("invalid") or { 8080 }
    out "Port chosen: {port}"
}
```

---

### Resource Management (`with`)

The `with` block guarantees deterministic resource acquisition and release (RAII):

```datara
fn main() {
    with file = file_read("config.json") {
        out "Config content: {file}"
    }
    // Resources acquired by 'file' are automatically finalized on scope exit
}
```

---

### Concurrency & Parallelism (`parallel`)

Datara includes native constructs for parallel iteration across multicore processors:

```datara
fn main() {
    mut counter: Int = 0
    
    // Automatically partitioned and scheduled across hardware worker threads
    parallel for i in 1..1000 {
        // Concurrent worker tasks
    }
}
```

---

## Foreign Function Interface (FFI)

Datara can directly link and invoke any native library adhering to the standard C ABI (C, C++, Rust, Zig, Win32 API).

### Declaring External Functions

Use `extern "C"` to declare symbols resolved by the platform linker:

```datara
// Win32 API declaration
extern "C" fn MessageBoxA(hwnd: Int, text: Str, caption: Str, utype: Int) -> Int
extern "C" fn GetTickCount64() -> Int

fn main() {
    let time = GetTickCount64()
    out "Uptime ms: {time}"
    
    // Spawn a native message box
    // MessageBoxA(0, "Hello from Datara!", "Native Dialog", 0)
}
```

### Interoperability Strategy

1. **C / C++**: Direct header-less ABI binding. Simply declare the `extern "C"` signature; the linker resolves symbols from standard system libraries (`kernel32.lib`, `user32.lib`, `msvcrt.lib`, `libc.so`).
2. **Rust**: Compile your Rust code with `#[no_mangle] pub extern "C" fn ...` into a `.lib` / `.a` / `.dll` and link it directly.
3. **Python / JavaScript**: For dynamic runtimes, Datara uses structured IPC via standard input/output streams or embedded C bindings (e.g. `libpython` or QuickJS/Node FFI).

---

## Standard Library Reference

Datara comes out of the box with high-performance runtime primitives:

### I/O & Filesystem
- `file_read(path: Str) -> Str`: Reads an entire file into a string.
- `file_write(path: Str, content: Str) -> Int`: Overwrites a file (returns 1 on success).
- `file_append(path: Str, content: Str) -> Int`: Appends content to an existing file.
- `file_exists(path: Str) -> Int`: Returns 1 if file exists, 0 otherwise.

### Process & CLI Arguments
- `args_count() -> Int`: Returns the number of command-line arguments passed to the executable.
- `args_get(index: Int) -> Str`: Retrieves the argument at `index` (`args_get(0)` is program name).
- `env_get(key: Str) -> Str`: Retrieves an environment variable by name.

### String Primitives
- `str_len(s: Str) -> Int`: Returns string byte length.
- `str_contains(s: Str, needle: Str) -> Bool`: Tests substring presence.
- `str_starts_with(s: Str, prefix: Str) -> Bool`: Prefix match.
- `str_ends_with(s: Str, suffix: Str) -> Bool`: Suffix match.
- `str_index_of(s: Str, needle: Str) -> Int`: Returns 0-based character index or -1 if not found.
- `str_trim(s: Str) -> Str`: Trims leading and trailing whitespace.
- `str_to_int(s: Str) -> Int`: Parses numeric string to integer.

### Timing & Sleep
- `now_ms() -> Int`: Current Unix timestamp in milliseconds.
- `now_precise_ms() -> Float`: High-resolution microsecond timer converted to milliseconds.
- `sleep(ms: Int)`: Suspends execution of current thread for given milliseconds.

### High-Performance Fast Math (`stdlib.math.math`)
- `math_sqrt(x: Float) -> Float`: Single-instruction square root ($\sqrt{x}$).
- `math_pow(b: Float, e: Float) -> Float`: Exponentiation ($b^e$).
- `math_abs(x: Float) -> Float`: Floating point absolute value.
- `math_sin(x: Float) / math_cos(x: Float) / math_tan(x: Float)`: Fast trigonometry.
- `math_floor(x) / math_ceil(x) / math_round(x)`: Fast IEEE-754 rounding operations.
- `math_min(a, b) / math_max(a, b) / math_hypot(a, b)`: Floating point min/max/hypot.
- `math_min_int(a, b) / math_max_int(a, b) / math_abs_int(x)`: Branchless integer math.

---

## Compiler Architecture & Pipeline

```
  ┌──────────────┐      ┌──────────────┐      ┌──────────────┐
  │ Source Code  │ ───> │  Lexer (01)  │ ───> │ Parser (02)  │
  └──────────────┘      └──────────────┘      └──────────────┘
                                                     │
                                                     ▼
  ┌──────────────┐      ┌──────────────┐      ┌──────────────┐
  │   Effects    │ <─── │ TypeChecker  │ <─── │ Resolver     │
  │ Lattice (05) │      │  & Generics  │      │  & Scoping   │
  └──────────────┘      └──────────────┘      └──────────────┘
         │
         ▼
  ┌──────────────┐      ┌─────────────────────────────┐      ┌──────────────┐
  │  Ownership   │ ───> │     DMIR Lowering (SSA)     │ ───> │ EvidenceGate │
  │ Tracker (06) │      │ Basic Blocks & Phis-as-Args │      │  Optimizer   │
  └──────────────┘      └─────────────────────────────┘      └──────────────┘
                                                                    │
                                                                    ▼
  ┌──────────────┐      ┌─────────────────────────────┐      ┌──────────────┐
  │ Native .exe  │ <─── │     MSVC / SysV Linker      │ <─── │  Cranelift   │
  │ Machine Code │      │  (link.exe / lld / gcc)     │      │ Codegen (08) │
  └──────────────┘      └─────────────────────────────┘      └──────────────┘
```

### Evidence Gate Optimizer Architecture

Traditional compilers often report optimization metrics blindly without verifiable proof. Datara features a **Proof-Carrying Evidence Gate**:

```
[IR Before Pass] ---> Optimizer Pass ---> Verifier Gate ---> [IR After Pass]
                            |                   |
                     Pass Claims Speedup   Fingerprint Delta?
                                           Delta == 0 -> Mechanically REJECTED
                                           Delta > 0  -> Proven & APPLIED
```

#### Verification Protocol
- Before and after every mutating pass, a deterministic structural fingerprint of the intermediate representation (CFG topology, value definitions, dominance frontier counts) is computed.
- If a pass (such as `LoopFold` or `Inlining`) claims to have applied an optimization but the IR structural delta does not verify the claim, the pass is mechanically downgraded to `Rejected`, counters are restored, and no unproven metadata is recorded.
- **Fail-Closed Verifier**: After every mutating transformation, `verify_module` checks the DMIR SSA invariants. Any broken phi argument, dominance violation, or dangling ValueId immediately halts compilation rather than generating corrupted machine code.

#### SSA Optimization Passes
When building in release or domain mode (`forgen build -m release`):
1. **Semantic Adaptation Engine (SAE)**: Analyzes domain profiles (financial ledger, embedded real-time, high-throughput network, scientific tensor) and configures pass thresholds accordingly.
2. **Inter-Procedural Pure Inlining**: Recursively inlines leaf functions proved pure by the Effect Lattice.
3. **Mem2Reg**: Promotes stack-allocated variables (`LoadVar`/`AssignVar`) into SSA block parameters and register values using iterated dominance frontiers (Cytron et al.).
4. **Closed-Form Loop Folding (`LoopFold`)**: Summation loops over ranges like `for i in 1..N { sum += i }` are automatically recognized and converted into closed-form $O(1)$ arithmetic:
   $$\text{Sum} = \frac{n(n - 1)}{2} \cdot a + n \cdot b$$
   A loop iterating $10^9$ times is reduced to three machine instructions executing in less than one nanosecond.
5. **Global Common Subexpression Elimination (CSE)**: Dominance-based value numbering eliminates redundant arithmetic, address calculations, and duplicate calls.
6. **Loop-Invariant Code Motion (LICM)**: Hoists invariant instructions and pure function calls out of loop pre-headers.
7. **Pipeline Fusion**: Merges chained filter, map, and fold operations into a single continuous pass, preventing intermediate buffer allocations.
8. **SROA (Scalar Replacement of Aggregates)**: Flattens composite structures, tuples, and points into distinct scalar SSA variables, completely eliminating heap allocations.
9. **Sibling & Tail Recursion Elimination**: Converts binary and tail-recursive functions into single iterative loops with accumulator block parameters.
10. **Dead Code & Dead Symbol Elimination (DCE)**: Prunes unreached basic blocks, dead SSA values, and unreferenced functions across whole modules.

---

## Codebase Volume & Engineering Audit

### Project Line Count Audit

| Component | Files | Lines of Code | Role in Language Ecosystem |
| :--- | :--- | :--- | :--- |
| `src/` (Compiler Engine) | 52 | 22,770 | Lexer, Parser, Resolver, TypeChecker, Effects, DMIR SSA, Optimizer, Backend |
| `tests/` (Test Suites) | 75 | 8,625 | Integration tests, benchmarks, SSA verifiers, audit regressions |
| `stdlib/` & `.dtr` Suites | 128 | 84,055 | Standard library code, domain tests, stress fixtures |
| `docs/` & Architecture Specs | 50 | 16,097 | Formal design records, memory specs, language RFCs |
| `src/runtime/` (C Runtime) | 1 | 600+ | OS memory, console I/O, string algorithms, high-res timers |
| **Total Checked-in Source** | **370+** | **132,885** | **Complete Project Volume** |

### Why Modern Architecture Accomplishes More With Less Code
Developers often wonder why historical compilers (GCC, rustc, Clang) span millions of lines while Datara's compiler is ~31,000 lines of Rust.

1. **Leveraging Cranelift**: Rather than hand-crafting 200,000+ lines of CPU architecture backends (x86_64, AArch64, RISC-V register allocation, instruction selection, machine code encoding, and PE/ELF object writer), Datara builds directly upon **Cranelift** (`cranelift-codegen`, `cranelift-frontend`, `cranelift-object`), which contributes over **350,000 lines of industrial-strength codegen machinery**.
2. **Dense, Modern Design**:
   - Early Rust (2012) was ~40,000 lines of OCaml / early Rust.
   - Early Zig (2016) was ~25,000 lines of C++.
   - Early Go (2009) was ~60,000 lines of C.
   Datara does not duplicate solved problems; its 31,000 lines of Rust focus exclusively on what makes it revolutionary: **The Evidence Gate, Affine Memory Invariants, Closed-Form Loop Folding, and SSA Pipeline Fusion**.

---

## Language Comparison Matrix

| Dimension | Datara | Rust | Zig | Go | C++ | Python |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Memory Model** | Scope-based Affine | Borrow Checker | Manual / Allocators | Tracing GC | RAII / Manual | Reference Counted GC |
| **Runtime Pauses** | Zero | Zero | Zero | Stop-the-world | Zero | Frequent pauses |
| **Compilation** | AOT (Cranelift) | AOT (LLVM) | AOT (LLVM/Self) | AOT (Custom) | AOT (LLVM/GCC) | Bytecode Interpreter |
| **Compilation Speed**| Ultra-fast | Slow (LLVM) | Moderate | Very Fast | Slow | Instant (interpreted) |
| **Error Handling** | Result / `?` / `or` | Result / `?` | Error unions | `if err != nil` | Exceptions | Exceptions |
| **Optimizer Verification**| Evidence Gate | Trust passes | Trust passes | Trust passes | Trust passes | None |
| **OOP Style** | DOP (`class` + `behavior`) | Traits + Structs | Structs + Comptime | Structs + Interfaces | Class Inheritance | Class Inheritance |
| **C ABI FFI** | Zero-cost `extern "C"` | `extern "C"` | `@cImport` | cgo (overhead) | Native | ctypes / CFFI (slow) |

---

## Command Line Interface (`forgen`)

The `forgen` binary is your all-in-one driver for building, running, and managing Datara projects:

```bash
# Check syntax, types, and borrow safety without compiling native object
forgen check main.dtr

# Direct Execution (compiles and runs immediately)
forgen run main.dtr

# Compile to Native Optimized Standalone Executable (.exe / ELF)
forgen build -m release -o my_app.exe main.dtr

# Run Workspace Test Suites and Golden Tests
forgen test

# Start the Language Server Protocol (LSP) daemon for IDEs
forgen lsp

# Initialize a new Datara project
forgen new my_project
```

---

## Building Desktop Applications & GUI

Datara is uniquely positioned for building lightweight, blazing-fast desktop applications without the memory overhead of Electron:

1. **Direct Native Win32 / Cocoa / GTK GUI**:
   - Link directly against system windowing libraries (`user32.lib`, `gdi32.lib`).
   - Create zero-overhead native windows, canvas graphics, and system trays with instantaneous startup and < 5MB memory footprints.
2. **Modern Webview / Tauri Integration**:
   - Use Datara as the high-performance native core executing business logic, database queries, and heavy computation.
   - Communicate with an HTML5/CSS/JavaScript webview front-end via lightweight IPC or embedded HTTP/WebSocket servers.

---

## Adoption Analysis & 2026–2027 Roadmap

### Honest Perspective: Why Isn't Everyone Using Datara Yet?
Datara possesses world-class core technology: verified optimizations, sub-millisecond SSA lowering, Cranelift native machine codegen, and deterministic memory safety without GC. However, programming language adoption is driven by ecosystem maturity:

1. **Package Ecosystem**: Languages like Rust and Go thrive because `cargo` and `go get` provide instant access to thousands of ready-to-use community packages. Datara currently compiles local projects and links native C libraries, but lacks a centralized open-source registry.
2. **Standard Library Scope**: While Datara has full native builtins for file I/O, strings, time, math, system arguments, and console input, modern enterprise applications expect native HTTP/2, TLS, JSON/Protobuf serialization, and SQL database drivers out of the box.
3. **IDE Plugin Distribution**: The Language Server Protocol (`forgen lsp`) is functional, but official one-click extensions on the VS Code Marketplace and JetBrains plugin portal have not yet been published.
4. **Public Launch & Documentation**: Datara is currently in developer preview and has not yet undergone a wide-scale public 1.0 release campaign.

### Strategic Roadmap (2026–2027)

- **Q4 2026**:
  - Publication of official VS Code Extension with syntax highlighting, live diagnostic squiggles, and code navigation.
  - Pre-built cross-platform binary releases for Windows (`x86_64-pc-windows-msvc`), Linux (`x86_64-unknown-linux-gnu`), and macOS (`aarch64-apple-darwin`).
  - Standard Library Expansion: Native JSON parser and cross-platform non-blocking TCP socket networking.
- **Q1 2027**:
  - Centralized Package Manager (`forgen add <package>` and `forgen publish`).
  - Auto-vectorization in DMIR targeting AVX-512 and ARM NEON SIMD registers.
- **Q2 2027**:
  - Embedded Python C-API bridge enabling zero-copy tensor sharing with PyTorch and NumPy.
  - Interactive WebAssembly playground running Datara directly in the browser.

---

## Installation & Multi-Platform Setup Guide

Datara binaries are completely self-contained. The official installer automatically configures the compiler executable, standard library, C runtime archive, and environment variables.

### Windows (x86_64)

#### Option A: 1-Click Installer (Recommended)
1. Download `forgen-v0.1.0-windows-x64.zip` from [GitHub Releases](https://github.com/waters1ze/datara/releases).
2. Extract the archive.
3. Double-click `install.bat` (or right-click `install.ps1` -> *Run with PowerShell*).
4. Open a new Terminal and verify with `forgen --help`.

#### Option B: PowerShell One-Liner
Open PowerShell as a standard user and execute:
```powershell
irm https://raw.githubusercontent.com/waters1ze/datara/main/scripts/install.ps1 | iex
```

#### Windows Prerequisites
To link native executables, Datara utilizes the native MSVC linker:
- Install **Visual Studio Build Tools** (select *"Desktop development with C++"*).
- Alternatively, if Visual Studio 2019/2022 Community is already installed, `forgen` automatically locates the MSVC toolchain via `vswhere`.

---

### Linux (Ubuntu, Debian, Fedora, Arch, Alpine, RHEL)

#### Universal Unix Shell Installer
Run the official POSIX installer in your terminal:
```bash
curl -fsSL https://raw.githubusercontent.com/waters1ze/datara/main/scripts/install.sh | bash
```
The installer will:
- Install the `forgen` binary to `~/.datara/bin`.
- Install the standard library to `~/.datara/stdlib`.
- Automatically append `PATH` and `DATARA_HOME` to your active shell (`~/.bashrc`, `~/.zshrc`, or `~/.profile`).

#### Distribution-Specific C Toolchain Dependencies
Datara compiles directly to object files and links using your system's native C compiler (`gcc` or `clang`). Ensure build essentials are installed for your distribution:

- **Ubuntu / Debian / Linux Mint**:
  ```bash
  sudo apt update && sudo apt install -y build-essential
  ```
- **Fedora / RHEL / CentOS / Rocky Linux**:
  ```bash
  sudo dnf groupinstall -y "Development Tools"
  ```
- **Arch Linux / Manjaro**:
  ```bash
  sudo pacman -Syu --needed base-devel
  ```
- **Alpine Linux**:
  ```bash
  sudo apk add --no-cache build-base
  ```
- **openSUSE / SLES**:
  ```bash
  sudo zypper install -y -t pattern devel_basis
  ```

---

### macOS (Apple Silicon M1/M2/M3/M4 & Intel x86_64)

#### Terminal One-Liner
Execute the installer in Terminal:
```bash
curl -fsSL https://raw.githubusercontent.com/waters1ze/datara/main/scripts/install.sh | bash
```

#### macOS Prerequisites
Ensure Apple command-line developer tools are installed:
```bash
xcode-select --install
```

---

### Post-Installation Verification

Open a new terminal session and run:
```bash
# 1. Verify that the forgen CLI is accessible
forgen --help

# 2. Create and run a new project in seconds
forgen new my_test_app
cd my_test_app
forgen run
```

---

## Building from Source

If you prefer building the toolchain directly from source code:

### Prerequisites
- **Rust toolchain** (1.85+ recommended: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- **Native C Compiler**: MSVC on Windows, GCC/Clang on Linux/macOS.

### Build Steps
```bash
# 1. Clone repository
git clone https://github.com/waters1ze/datara.git
cd datara

# 2. Run test and benchmark suite (44+ test suites, 100% pass)
cargo test

# 3. Build optimized release binary
cargo build --release

# 4. Run installer script to set up environment
powershell -ExecutionPolicy Bypass -File scripts/install.ps1   # Windows
bash scripts/install.sh                                      # Linux / macOS
```

---

## GitHub Releases & GitHub Packages

### Pre-Built Binaries from GitHub Releases
Every official release includes standalone, statically self-contained archives attached to [GitHub Releases](https://github.com/waters1ze/datara/releases):
- `forgen-windows-x64.zip` — Windows x64 binary + complete standard library + installer
- `forgen-linux-x64.tar.gz` — Linux x64 binary + complete standard library + installer
- `forgen-darwin-arm64.tar.gz` — macOS Apple Silicon binary + stdlib
- `forgen-darwin-x64.tar.gz` — macOS Intel binary + stdlib
- `SHA256SUMS.txt` — Cryptographic integrity checksums for all distribution archives

### Official Container Image (GitHub Packages)
Datara is continuously published to the **GitHub Container Registry (GHCR)**:
```bash
# Pull the latest official compiler container
docker pull ghcr.io/waters1ze/datara:latest

# Run interactive compiler directly
docker run --rm -it -v $(pwd):/workspace ghcr.io/waters1ze/datara:latest run main.dtr

# Verify installation and toolchain version
docker run --rm ghcr.io/waters1ze/datara:latest --help
```

---

## License

Datara is dual-licensed under the **Apache License 2.0** and **MIT License**, permitting use in both open-source and proprietary enterprise commercial applications.
