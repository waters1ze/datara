# Datara Testing & Quality Assurance Pattern

Datara features built-in native test discovery and execution via `forgen test`.

---

## 1. Test Organization

Place all integration and system tests inside the `tests/` directory:
```
project/
├── src/
│   └── main.dtr
└── tests/
    ├── test_math.dtr
    └── test_service.dtr
```

---

## 2. Test Execution Contract

A Datara test executable is recognized as **PASS** when:
1. It exits with code `0`.
2. Neither `stdout` nor `stderr` contains the substring `FAIL:`.

```datara
// tests/test_math.dtr

fn test_addition() -> Int {
    return 20 + 30
}

fn test_multiplication() -> Int {
    return 10 * 5
}

fn main() {
    if test_addition() == 50 {
        out "PASS: test_addition"
    } else {
        err "FAIL: test_addition"
    }

    if test_multiplication() == 50 {
        out "PASS: test_multiplication"
    } else {
        err "FAIL: test_multiplication"
    }
}
```

---

## 3. CLI Test Commands

```bash
# Run all discovered tests in tests/
forgen test

# Run tests with ultra-optimized LLVM backend
forgen test --llvm

# Check static validity without building binaries
forgen check
```
