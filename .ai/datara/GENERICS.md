# Datara Generics & Monomorphization

Datara features fully parameterized generic types and functions with compile-time monomorphization (0 vtables, 0 boxing overhead).

---

## 1. Generic Classes

```datara
class Pair<A, B> {
    first: A
    second: B
}

behavior Pair {
    get_first() -> A => this.first
    get_second() -> B => this.second
}
```

---

## 2. Generic Functions

```datara
fn identity<T>(val: T) -> T {
    return val
}

fn make_pair<T, U>(a: T, b: U) -> Pair<T, U> {
    return Pair<T, U> { first: a, second: b }
}
```

---

## 3. Solver Guarantees

- Types are checked statically before code generation.
- The optimizer emits specialized, optimal machine code for each concrete instantiation (e.g., `Pair<Int, Float>` is compiled to a specialized 16-byte record).
