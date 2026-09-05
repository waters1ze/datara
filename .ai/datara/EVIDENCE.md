# Datara Evidence Gate & Proof-Carrying Code

The **Evidence Gate** is the formal verification and optimization engine inside Forgen.

---

## Key Verification Passes

1. **Formal Value Range Propagation (FVRP)**:
   Refinements such as `val: Int<0..255>` emit `@llvm.assume` and LLVM `!range` metadata nodes (`!{i64 0, i64 255}`). This allows hardware-level register packing and dead-code branch pruning.

2. **Bounds-Check Elimination (BCE)**:
   When an array index is guarded by refinement or loop ranges (e.g. `idx: Int in 0..<arr.len()`), the compiler mathematically proves safety and eliminates runtime boundary checks.

3. **Proof-Carrying Division (`E0941`)**:
   Prohibits division-by-zero crashes at compile time.

4. **Capability Lattice (`E0940`)**:
   Audits all external side effects, guaranteeing zero unhandled security leaks.

5. **Explainability Trace**:
   Run `forgen why <symbol>` to inspect exactly which passes were applied (e.g., `EvidenceGate:BCE`, `EvidenceGate:Mem2Reg`).
