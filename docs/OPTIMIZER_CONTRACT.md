# Forgen Optimizer Pass Contract

Every optimization pass in the Forgen compiler core must adhere to this formal engineering contract.

---

## 1. Pass Specifications

### Pass 1: Pure Function Inlining (`inline_pure_functions`)
- **Preconditions**: Call graph built; callee is pure (`Effect::Pure`), single-block, non-recursive.
- **Input Invariants**: Valid DMIR Module with well-typed call sites.
- **Cost Model**: Callee instruction count $\le \text{budget}$ (default 20 in domain mode).
- **Transformation**: Caller `Inst::Call` is replaced by callee instructions with fresh SSA `ValueId` mappings. Callee return value is mapped to caller call destination.
- **Preserved Invariants**: SSA validity, type consistency, effect correctness.
- **Observable Delta**: Zero runtime function call prologue/epilogue overhead.

### Pass 2: SROA Stack Scalarization (`scalarize_structures`)
- **Preconditions**: Local `Inst::StructInit` does not escape to calls, returns, format strings, or method dispatches.
- **Input Invariants**: Struct fields statically known.
- **Transformation**: `Inst::StructInit` deleted; all `Inst::GetField` operations replaced with direct scalar field value bindings.
- **Preserved Invariants**: Field value identities, variable scoping.
- **Observable Delta**: Heap allocation count reduced to zero for local struct instances.

### Pass 3: Constant Folding & Propagation (`constant_fold`)
- **Preconditions**: Operands of `Inst::BinOp`, `Inst::UnOp`, or `Inst::FormatStr` are statically known integer/string constants.
- **Input Invariants**: Pure deterministic operations.
- **Transformation**: Runtime computation instruction replaced with `Inst::ConstInt`, `Inst::ConstStr`, or `Inst::ConstBool`.
- **Preserved Invariants**: Deterministic numeric and string values.

### Pass 4: Dead Code Elimination (`dead_code_elimination`)
- **Preconditions**: Pure instruction whose destination `ValueId` is never referenced in subsequent basic block instructions.
- **Input Invariants**: Destination value has 0 uses.
- **Transformation**: Instruction removed from basic block.
- **Preserved Invariants**: Program output and side-effects preserved.

### Pass 5: Whole-Program Reachability & Dead Symbol Stripping (`dead_symbol_elimination`)
- **Preconditions**: Domain or Release compilation mode; entry point `@main` identified.
- **Transformation**: Transitive call graph computed from `main`. All unreachable functions, split behaviors, and unused class method forwarders are pruned from the DMIR module and emitted binary.
- **Preserved Invariants**: All reachable execution paths intact.
