# Datara Effect Lattice & Zero-Trust Capabilities

Datara tracks side-effects using a compile-time **Effect Lattice**, preventing unchecked I/O, security bypasses, and data races.

---

## 1. Purity Tracking (`pure`)

Functions marked or inferred as `pure` are guaranteed to have zero side effects:
- Cannot access global mutable state.
- Cannot perform file, network, or process I/O.
- Evidence Gate optimizes pure functions with aggressive constant folding, dead-code elimination, and loop vectorization.

```datara
pure fn add(a: Int, b: Int) -> Int => a + b
```

---

## 2. Zero-Trust Security Capability Tokens (`E0940`)

Privileged OS operations require an explicit capability token:
- `Capability<FileRead>`: File read operations (`token.open()`).
- `Capability<FileWrite>`: File write operations (`token.create()`).
- `Capability<NetworkConnect>`: Socket operations (`token.connect()`).
- `Capability<ProcessExec>`: Subprocess execution (`token.execute()`).

### Delegating Capabilities
```datara
fn read_data(path: Str, token: Capability<FileRead>) -> Str {
    let handle = token.open(path)
    return handle.read_all()
}

fn main(sys_caps: SystemCapabilities) {
    let safe_token = sys_caps.files.grant_readonly("data.csv")
    let contents = read_data("data.csv", safe_token)
    out contents
}
```

Tokens are zero-cost static witnesses erased at compile time (0 heap allocations).
