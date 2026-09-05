# Datara Troubleshooting Guide

### 1. `error[E0941]: Proof-Carrying Code Violation: Unproven divisor`
- **Cause**: Arithmetic division by a variable or expression without a static guarantee of non-zero.
- **Solution**:
  1. Add `require divisor != 0, "Non-zero required"` contract to function header.
  2. Or wrap in `if divisor != 0 { ... }`.
  3. Or use refined type `NonZeroInt`.

### 2. `error[E0940]: Security Violation: Operation requires Capability<...>`
- **Cause**: Attempting disk, network, or subprocess I/O directly.
- **Solution**: Add `sys_caps: SystemCapabilities` parameter to `main()` and pass capability tokens down.

### 3. `error[E0943]: Concurrency Violation: Potential data race`
- **Cause**: Mutating a shared variable inside `parallel for`.
- **Solution**: Declare accumulators inside the parallel loop body as thread-local variables.

### 4. `error[E-BORROW-001]: Cannot mutate immutable variable`
- **Cause**: Reassigning a variable declared with `let` or `val`.
- **Solution**: Declare the variable with `mut`.
