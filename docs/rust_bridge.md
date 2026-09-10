# Rust Bridge Architecture: High-Performance Crate Interop

The Datara toolchain provides seamless bidirectional interop with the Rust ecosystem. Developers can leverage crates from crates.io directly from Datara with zero-copy buffer views and automatic panic barriers.

---

## 1. CLI Usage: `dpm rust-bridge`

The `dpm` package manager includes a built-in generator for Rust bridge crates:

```bash
dpm rust-bridge <crate_name> --api manifest.toml [--out-dir <dir>]
```

### Pipeline Overview
1. **Manifest Ingestion**: Reads `manifest.toml` defining exposed functions, parameters, return types, and optional custom Rust glue code.
2. **Shim Crate Generation**: Scaffolds a dedicated wrapper crate with `crate-type = ["cdylib", "staticlib"]`.
3. **Panic Safety Barrier**: Encloses every exported `extern "C" fn` in `std::panic::catch_unwind(AssertUnwindSafe(|| ...))`. Panics in Rust dependencies never unwind across the C ABI boundary, preventing undefined behavior.
4. **Cargo Build Execution**: Discovers the host Rust toolchain via `find_cargo()` and compiles the bridge in release mode.
5. **Datara Binding Generation**: Emits a `<crate>_bridge.dtr` module with `extern fn` declarations, C header prototypes (`<crate>_bridge.h`), and zero-copy buffer view constructors.

---

## 2. API Manifest Format (`manifest.toml`)

```toml
[crate]
name = "regex"
version = "1.13"

[[functions]]
name = "regex_is_match"
params = [
    { name = "pattern", type = "String" },
    { name = "text", type = "String" }
]
return_type = "Bool"
code = """
let Ok(re) = regex::Regex::new(pattern) else { return false; };
re.is_match(text)
"""

[[functions]]
name = "regex_find"
params = [
    { name = "pattern", type = "String" },
    { name = "text", type = "String" }
]
return_type = "String"
code = """
let Ok(re) = regex::Regex::new(pattern) else { return String::new(); };
re.find(text).map(|m| m.as_str().to_string()).unwrap_or_default()
"""
```

---

## 3. Zero-Copy Buffer Views

To pass large memory buffers (such as image pixel arrays, network packets, or audio samples) without heap allocation or copying, the bridge pairs a `Pointer` (`*const u8`) and an `Int` (`len`):

```toml
[[functions]]
name = "encode_rgb_png"
params = [
    { name = "buf_ptr", type = "Pointer" },
    { name = "buf_len", type = "Int" },
    { name = "width", type = "Int" },
    { name = "height", type = "Int" }
]
return_type = "String"
code = """
// Zero-copy access directly into Datara arena memory
let raw_bytes = unsafe { std::slice::from_raw_parts(buf_ptr, buf_len as usize) };
// Process raw bytes without allocation
format!("PROCESSED_BYTES:{}", raw_bytes.len())
"""
```

In Datara:
```datara
let buffer = datara_rt_arena_alloc(1024)
let res = encode_rgb_png(buffer, 1024, 32, 32)
```

---

## 4. Showcases

Pre-configured showcases are available under `examples/showcase/rust_bridge/`:

| Showcase | Crate | Purpose | Key Features |
|---|---|---|---|
| `regex` | `regex` | Regular expression search & replace | Substring extraction, validation, sanitization |
| `serde_json` | `serde_json` | Structured JSON processing | Zero-copy string lookup, JSON object creation |
| `image` | `image` / `png` | Image encoding & decoding | Zero-copy buffer views into arena memory |

---

## 5. Architectural Limitations & Boundaries

1. **Synchronous Execution Only (v1 Scope)**:
   - Async Rust runtimes (`tokio`, `async-std`) cannot be polled directly across the Datara C ABI boundary without an internal blocking runtime runner (`tokio::runtime::Runtime::block_on`).
   - Pure synchronous functions and compute tasks are recommended.
2. **Lifetime Safety**:
   - Pointers passed to Rust functions must remain valid for the duration of the call.
   - Returning Rust references (`&'a T`) across the ABI is prohibited; returned data must be converted to scalar types or owned C-compatible handles.

---

## 6. Reverse Bridge: Embedding `forgen` in Rust

Datara programs can also be compiled and executed from Rust applications:

### In-Process Rust API
```rust
use forgen::driver::ForgenCompiler;
use std::path::Path;

let compiler = ForgenCompiler::new("release");
let result = compiler.compile_file(Path::new("app.dtr"), Some(Path::new("app.exe")));
assert!(result.success);
```

### C ABI Embedding (`libforgen`)
See `docs/SPEC_V1.md` and `include/forgen.h` for embedding `forgen` into C, C++, or Rust via `forgen_init`, `forgen_load_module`, `forgen_call_fn`, and `forgen_shutdown`.
