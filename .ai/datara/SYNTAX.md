# Datara Grammar & Syntax Reference

---

## 1. Variable Bindings & Declarations
```datara
let x: Int = 10      // Immutable, promoted to register
mut y: Int = 20      // Mutable, type-locked
val z = "inferred"   // Gradual / type-inferred binding

// Compile-time expression evaluation
let buffer_size: Int = comptime { 1024 * 64 }
```

---

## 2. Structural Metaprogramming (`@derive`)
```datara
@derive(Display, Json, Hash, Clone, Deserialize)
class UserAccount {
    id: Int
    username: Str
    active: Bool
}
```

---

## 3. FVRP Ranges & Units of Measure
```datara
// Interval range refinement
fn handle_port(port: Int<1024..65535>) -> Int {
    return port
}

// Unit of measure refinement
fn compute_velocity(speed: Float<m/s>) -> Float {
    return speed * 2.0
}
```

---

## 4. Functions & Control Flow
```datara
// Standard block body
fn add(a: Int, b: Int) -> Int {
    return a + b
}

// Expression body shorthand
fn double(x: Int) -> Int => x * 2

// Pure function (zero side effects)
pure fn square(x: Int) -> Int => x * x

// Range for loop (zero-cost abstraction)
for i in 0..100 {
    out i
}
```

---

## 5. Pipelines & Error Handling
```datara
// Pipe-forward & then chaining
let res = 15 |> double() then add(10)

// Postfix unpack and fallback
let port = parse_port(s)?
let fallback_port = parse_port(s) or 8080
```
