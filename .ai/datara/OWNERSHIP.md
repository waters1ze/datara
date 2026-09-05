# Datara Affine Ownership & Zero-Copy Views

Datara provides memory safety and fearless concurrency through affine move semantics and scoped views, eliminating the need for a runtime garbage collector.

---

## 1. Affine Move Semantics

Every variable binding owns its value:
- When an owned variable is passed to a function or reassigned, ownership is **moved**.
- Accessing the variable after a move results in compile error `E-BORROW-001`.

```datara
let original = LargeData { id: 1 }
let target = original // 'original' moved to 'target'
// out original.id    // Error[E-BORROW-001]: use after move
```

---

## 2. Zero-Copy Views (`view`)

To borrow data without consuming ownership:
```datara
let original = LargeData { id: 1 }
let v = view original // Borrowed reference
out v.id              // Valid
out original.id       // Valid: original was not consumed
```

### Method-Call Syntax
```datara
mut count = 100
{
    mut v = count.view()
    out v
}
// Once 'v' goes out of scope, 'count' can be mutated again
count = 200
```
