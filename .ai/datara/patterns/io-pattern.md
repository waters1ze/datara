# Datara Zero-Trust Capability I/O Pattern

In Datara, raw unmanaged OS interactions are strictly forbidden at compile-time by the **Evidence Gate Zero-Trust Capability Lattice** (`Error[E0940]`).

---

## 1. The Zero-Trust Security Principle

No function may execute file system, network, or process execution calls without being explicitly granted a `Capability` token witness.

---

## 2. Standard Capability Workflow

### Step 1: Request System Capabilities in `main`
```datara
fn main(sys_caps: SystemCapabilities) {
    // Explicitly grant read-only capability for specific target file
    let safe_token = sys_caps.files.grant_readonly("app.conf")
    let content = read_config("app.conf", safe_token)
    out content
}
```

### Step 2: Accept Capability in Worker Functions
```datara
fn read_config(path: Str, token: Capability<FileRead>) -> Str {
    let handle = token.open(path)
    return handle.read_all()
}
```

---

## 3. Capability Types & Operations

| Capability | Operations Allowed | Compiler Gate |
|---|---|---|
| `Capability<FileRead>` | `fs_open`, `token.open()`, `read_all()` | `E0940` |
| `Capability<FileWrite>` | `fs_write`, `token.create()`, `write_all()` | `E0940` |
| `Capability<NetworkConnect>` | `net_connect`, `token.connect(host, port)` | `E0940` |
| `Capability<ProcessExec>` | `proc_spawn`, `token.execute(cmd)` | `E0940` |

### Zero-Cost Witness Guarantee
Capability tokens exist solely in the static type lattice and are erased during native code generation. They incur **0 bytes heap allocation** and **0 CPU cycles overhead**.
