# Enterprise Deployment & Systems Engineering Guide

This document outlines architectural patterns and operational standards for deploying Datara in mission-critical, enterprise, and cloud environments.

---

## 1. ABI Stability Guarantees

Datara v1.2 guarantees a stable, frozen C ABI for cross-language integration:

| Primitive Type | C Representation | Storage Alignment | Value Passing Convention |
|----------------|------------------|-------------------|--------------------------|
| `Int` / `i64`   | `int64_t`        | 8 bytes           | Direct register / stack  |
| `i32`          | `int32_t`        | 4 bytes           | Direct register / stack  |
| `Float` / `f64` | `double`         | 8 bytes           | Direct XMM/FPU register  |
| `f32`          | `float`          | 4 bytes           | Direct XMM/FPU register  |
| `Bool`         | `bool` (`uint8`) | 1 byte            | Direct register          |
| `Str`          | `const char*`    | 8 bytes           | Null-terminated UTF-8 ptr|
| Records/Structs| C `struct`       | Field-aligned     | Pass-by-pointer / stack  |

All exported symbols generated via `forgen build --embed` or `forgen export` carry the `DATARA_API` calling convention:
- **Windows**: `__declspec(dllexport)` / MSVC ABI.
- **Linux/POSIX**: `__attribute__((visibility("default")))` / System V AMD64 ABI.
- **macOS**: Mach-O exported symbol convention.

---

## 2. Containerization & Minimal Images

Because Datara binaries compiled with `--tiny` or standard release mode have zero external runtime dependencies beyond the system libc (`m`, `pthread`, `kernel32`), they are ideally suited for container distros like Alpine or distroless images:

```dockerfile
# Stage 1: Build binary using Forgen
FROM ubuntu:24.04 AS builder
RUN apt-get update && apt-get install -y clang curl
COPY . /app
WORKDIR /app
RUN forgen build --release --llvm

# Stage 2: Scratch or Distroless image
FROM gcr.io/distroless/cc-debian12
COPY --from=builder /app/target/release/server /server
ENTRYPOINT ["/server"]
```

### Static Linking
To build fully static binaries that run on bare metal or `FROM scratch` containers:
```bash
forgen build --release --target=x86_64-unknown-linux-musl
```

---

## 3. Determinism & Audit Compliance

In financial, aerospace, and regulatory audit environments, floating-point reproducibility and memory safety are paramount:

1. **IEEE-754 Bit-for-Bit Determinism**:
   By default, all floating-point math adheres strictly to IEEE-754 semantics. Compilations do not enable non-standard associativity or reciprocal approximations unless `@fast_math` or `--fast-math` is explicitly requested.
2. **Lockstep Verification**:
   Datara modules run through continuous lockstep verification (`tests/test_v120_phase*.rs`), verifying identical checksums across 20+ runs across diverse CPU architectures.
3. **Evidence Gate & Ledger**:
   Builds generated with `--ledger` produce an audit trail (`.ledger.json`) documenting every compiler optimization applied, including inlining decisions, dead code removals, and alias analyses.
