# Datara + Forgen Semantic Invariants

This document establishes the mandatory **compiler invariants** enforced throughout all stages of the Forgen toolchain.

---

## 1. Type Invariants

1. **Total Type Resolution**: Every expression, binding, and literal in the AST must resolve to a concrete `DataraType` before lowering to DMIR.
2. **No Hidden Dynamic `Any`**: In safe mode, implicit downcasts or unchecked dynamic dispatch are strictly prohibited.
3. **Monomorphic Soundness**: Generic types `Class<T>` must be fully specialized at compile-time before native code generation.
4. **Strict Assignment Compatibility**: A value of type `T` can only be assigned to a binding of type `U` if `T == U`.

---

## 2. Ownership & Memory Safety Invariants

1. **Single Active Owner**: Every resource has exactly one owning binding at any point in its lifetime.
2. **Move Invariant (`Use-After-Move Prevention`)**:
   - When an owner is consumed (e.g. by `destroy(x)` or move-by-value), its state transitions to `ValueState::Moved`.
   - Any subsequent read, write, borrow, or move of `x` produces `E-BORROW-001`.
3. **Borrow Invariant (Aliasing XOR Mutability)**:
   - For any resource `R`:
     $$\text{ActiveMutableBorrows}(R) \le 1 \land (\text{ActiveMutableBorrows}(R) > 0 \implies \text{ActiveImmutableBorrows}(R) == 0)$$
   - Simultaneous mutable borrows produce `E-BORROW-004`.
   - Mutation or reassignment during active view produces `E-BORROW-003`.
4. **Non-Escaping Local Views**:
   - A borrow of a local stack binding `x` cannot outlive the lexical scope of `x`.
   - Returning a view of a local variable produces `E-BORROW-005`.

---

## 3. Effect Invariants

1. **Purity Isolation**: A function declared or inferred as `Pure` can only call functions with effect `Pure`. Calling an `IO`, `Network`, or `Database` function automatically upgrades the function's effect set.
2. **Lattice Monotonicity**: Effect propagation across the call graph is monotonically non-decreasing.
3. **Observable Preservation**: Instructions with `IO`, `Network`, or `Database` effects cannot be eliminated by Dead Code Elimination (DCE).

---

## 4. Intermediate Representation (DMIR) Invariants

1. **Static Single Assignment (SSA)**: Every `ValueId` is defined exactly once in a basic block.
2. **Dominance Frontier**: Every use of a `ValueId` must be strictly dominated by its definition.
3. **Well-Formed Control Flow Graph**: Basic blocks have explicit single entries and terminate with unambiguous control transfers or returns.
4. **Type-Consistent Instructions**: Operands to binary operations `Inst::BinOp` must have matching operand types (`Int + Int`, `Float + Float`).
