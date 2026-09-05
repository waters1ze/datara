# Datara Deterministic Error Handling

Exceptions and `try/catch` blocks are non-existent in Datara. All errors are represented as values using algebraic containers.

---

## 1. `Outcome<T>` (Result Type)
Imported from `stdlib.result.result.Outcome`:
```datara
class Outcome<T> {
    is_success: Bool
    value: T
    error_msg: Str
}
```

## 2. `Maybe<T>` (Option Type)
Imported from `stdlib.result.option.Maybe`:
```datara
class Maybe<T> {
    is_some: Bool
    value: T
}
```

---

## 3. Propagation with Postfix `?`

```datara
fn read_int(s: Str) -> Outcome<Int> {
    if s == "42" {
        return Outcome<Int> { is_success: true, value: 42, error_msg: "" }
    }
    return Outcome<Int> { is_success: false, value: 0, error_msg: "Not 42" }
}

fn process() -> Outcome<Int> {
    // Unpacks value if successful, or early-returns Outcome on error
    let val = read_int("42")?
    return Outcome<Int> { is_success: true, value: val * 2, error_msg: "" }
}
```

---

## 4. Fallbacks with `or`

```datara
let port = parse_port(input_str) or 8080
```
