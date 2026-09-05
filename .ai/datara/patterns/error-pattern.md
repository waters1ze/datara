# Datara Error Handling & Propagation Pattern

Datara eliminates traditional exception overhead (`try/catch` was completely removed from the language) in favor of deterministic, zero-allocation algebraic types and operators.

---

## 1. Core Error Types

### `Outcome<T>` (Result Type)
Imported from `stdlib.result.result.Outcome`:
```datara
class Outcome<T> {
    is_success: Bool
    value: T
    error_msg: Str
}
```

### `Maybe<T>` (Option Type)
Imported from `stdlib.result.option.Maybe`:
```datara
class Maybe<T> {
    is_some: Bool
    value: T
}
```

---

## 2. The Unpack Operator `?`

The postfix operator `?` checks whether the returned container succeeded:
- On **Success**: Unpacks the inner `value` into the variable.
- On **Failure**: Performs an **immediate early return** of the failure to the caller.

```datara
use stdlib.result.result.Outcome

fn parse_port(s: Str) -> Outcome<Int> {
    if s == "8080" {
        return Outcome<Int> { is_success: true, value: 8080, error_msg: "" }
    }
    return Outcome<Int> { is_success: false, value: 0, error_msg: "Invalid port" }
}

fn start_server(port_str: Str) -> Outcome<Int> {
    // ? automatically unpacks Int or does early return with Outcome
    let port = parse_port(port_str)?
    println(fmt"Server listening on {port}")
    return Outcome<Int> { is_success: true, value: port, error_msg: "" }
}
```

---

## 3. The Fallback Operator `or`

When a default fallback is available, use `or` to eliminate boilerplate matching:

```datara
fn main() {
    // If parse_port fails, fallback to 3000
    let port = parse_port("invalid") or 3000
    out fmt"Selected port: {port}"
}
```

---

## 4. Pattern Matching on Outcomes

```datara
match res {
    Outcome { is_success: true, value: val, error_msg: _ } => {
        out fmt"Success: {val}"
    },
    Outcome { is_success: false, value: _, error_msg: err } => {
        out fmt"Failed: {err}"
    }
}
```
