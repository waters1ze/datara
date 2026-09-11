# The Official Datara Programming Language Tutorial

Welcome to Datara! This tutorial walks you through the core language syntax, type system, modularity, security capability lattice, and compilation model in 10 progressive steps.

---

## Step 1: Hello World & Console Output
Source: `examples/tutorial/step01_hello/main.dtr`

```datara
fn main() {
    out "Hello, Datara!"
}
```

Compile and run:
```bash
forgen run examples/tutorial/step01_hello/main.dtr
```

---

## Step 2: Types, Variables & Explicit Mutability
Source: `examples/tutorial/step02_types_and_vars/main.dtr`

In Datara, variables are immutable by default with `let`. Use `mut` to declare mutable state.

```datara
fn main() {
    let name: String = "Datara"
    let version: Int = 1
    let pi_approx: Float = 3.14159
    let is_fast: Bool = true

    mut counter = 0
    counter = counter + 10

    out "Language: " + name
    out "Version: " + int_to_str(version)
    out "Counter: " + int_to_str(counter)
}
```

---

## Step 3: Control Flow & Loops
Source: `examples/tutorial/step03_control_flow/main.dtr`

Datara provides clean `if`/`else` conditionals and `while` loop iterations.

```datara
fn main() {
    mut sum = 0
    mut i = 1
    while i <= 10 {
        if i % 2 == 0 {
            sum = sum + i
        }
        i = i + 1
    }
    out "Sum of even numbers 1..10: " + int_to_str(sum)
}
```

---

## Step 4: Functions & Pure Computation
Source: `examples/tutorial/step04_functions/main.dtr`

Functions have typed signatures and explicit return types:

```datara
fn square(x: Int) -> Int {
    return x * x
}

fn fib(n: Int) -> Int {
    if n <= 1 {
        return n
    }
    return fib(n - 1) + fib(n - 2)
}

fn main() {
    let s = square(7)
    let f = fib(8)
    out "Square of 7: " + int_to_str(s)
    out "Fibonacci(8): " + int_to_str(f)
}
```

---

## Step 5: Records & Structural Types
Source: `examples/tutorial/step05_records/main.dtr`

Records group related data into typed aggregates with zero-cost memory layout:

```datara
record Point {
    x: Int,
    y: Int,
}

record Rectangle {
    top_left: Point,
    width: Int,
    height: Int,
}

fn area(r: Rectangle) -> Int {
    return r.width * r.height
}

fn main() {
    let origin = Point { x: 0, y: 0 }
    let rect = Rectangle {
        top_left: origin,
        width: 20,
        height: 10,
    }
    out "Rectangle area: " + int_to_str(area(rect))
}
```

---

## Step 6: Modules & Multi-File Architecture
Source: `examples/tutorial/step06_modules/`

Organize projects across multiple files using `import`:

`helper.dtr`:
```datara
fn greeting(name: String) -> String {
    return "Greetings from helper module, " + name + "!"
}
```

`main.dtr`:
```datara
use helper.greeting

fn main() {
    let msg = greeting("Developer")
    out msg
}
```

---

## Step 7: Error Handling & Outcome Pattern
Source: `examples/tutorial/step07_outcomes/main.dtr`

Datara replaces fragile exception handling with deterministic, typed outcome records:

```datara
record DivisionResult {
    success: Bool,
    quotient: Int,
}

fn safe_divide(numerator: Int, denominator: Int) -> DivisionResult {
    if denominator == 0 {
        return DivisionResult { success: false, quotient: 0 }
    }
    return DivisionResult { success: true, quotient: numerator / denominator }
}

fn main() {
    let res1 = safe_divide(100, 4)
    if res1.success {
        out "100 / 4 = " + int_to_str(res1.quotient)
    }
}
```

---

## Step 8: Security Capability Lattice
Source: `examples/tutorial/step08_capabilities/main.dtr`

Privileged operations (filesystem, network, process execution) require explicit capability tokens, preventing unauthorized side effects:

```datara
fn guarded_echo(text: String, caps: SystemCapabilities) {
    out "[Capability Guarded] " + text
}

fn main(caps: SystemCapabilities) {
    guarded_echo("Secure operation permitted under SystemCapabilities", caps)
}
```

---

## Step 9: System Runtime Interoperability
Source: `examples/tutorial/step09_interop/main.dtr`

Access built-in runtime conversions and string formatting natively:

```datara
fn main() {
    let val_str = int_to_str(42)
    let len = str_len(val_str)
    out "Value: " + val_str + " (length: " + int_to_str(len) + ")"
}
```

---

## Step 10: Production CLI Application
Source: `examples/tutorial/step10_production_cli/main.dtr`

Parse command-line arguments and build full standalone CLI binaries:

```datara
fn main() {
    let argc = args_count()
    out "Total CLI arguments: " + int_to_str(argc)
    mut i = 0
    while i < argc {
        let arg = args_get(i)
        out "  arg[" + int_to_str(i) + "] = " + arg
        i = i + 1
    }
    out "Step 10 Production CLI finished successfully."
}
```

Compile into an optimized native standalone `.exe`:
```bash
forgen build examples/tutorial/step10_production_cli
```

---

## Production Showcases

Beyond the tutorial steps, explore real-world production architectures in `examples/showcase/`:

### 1. HTTP Server Showcase (`examples/showcase/http_server`)
Demonstrates a pure-Datara HTTP/1.1 request router with deterministic response generation:
- **Echo Endpoint** (`POST /echo`): Validates incoming payload protocol.
- **JSON Endpoint** (`GET /api/status`, `GET /health`): Generates structured JSON responses.
- **Run Showcase**:
  ```bash
  forgen run examples/showcase/http_server
  ```

### 2. Capability-Guarded File I/O Showcase (`examples/showcase/file_io`)
Demonstrates Datara's capability-based security model:
- Privileged operations (`file_write`, `file_read`) require authorized tokens (`SystemCapabilities` / `Capability<FileRead>`).
- Unauthorized code paths are physically barred by the compiler (`E0940`).
- **Run Showcase**:
  ```bash
  forgen run examples/showcase/file_io
  ```
