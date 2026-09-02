# Optimization Audit and Correctness Fixes

**Project:** Datara language / Forgen compiler  
**Audit date:** 2026-08-30  
**Scope:** optimizer decisions, DMIR transformations, Cranelift reachability, PGO provenance, and performance claims

## 1. Audit rule

An optimization is reported as **Applied** only when the compiler changes the canonical DMIR/CFG (or a later lowering stage) and the generated backend consumes that change. A cost-model result, an eligibility check, a target capability, or a planned representation is not proof of an emitted optimization.

The audit uses direct source inspection, structural DMIR checks, native execution checks, and regression tests. The repository has no useful tracked baseline: the current Git status reports the project files as untracked. Therefore, file contents and reproducible commands are the evidence baseline rather than a Git diff.

## 2. Verified pipeline boundary

The active native path is:

```text
Datara source -> lexer/parser -> DMIR CFG -> optimizer -> Cranelift -> COFF/object -> MSVC linker -> native executable
```

The native backend is `src/codegen/cranelift/backend.rs`. Its lowering is driven by actual `BasicBlock` instructions and `Terminator` values. Legacy compound instructions such as `Inst::WhileLoop`, `Inst::TryCatch`, and instruction-level `Inst::Return` are ignored by the backend (the compatibility nodes are not a proof of generated control flow). Real loop transformations therefore have to modify CFG blocks and terminators.

## 3. Change classification

| Area | Classification | Decision |
|---|---|---|
| LICM on natural loops | **FIX / REUSE** | Keep and verify the real CFG implementation in `src/optimizer/loops.rs`; do not revive the old compound-node path. |
| Naive loop unrolling | **REMOVE** | Keep disabled until fresh SSA IDs, CFG duplication, partial-trip handling, and verification exist. |
| Short-circuit `&&` / `||` | **FIX / EXTEND** | Keep CFG-based lowering and regression tests; never lower these as arithmetic `BinOp`s. |
| Pipeline fusion | **FIX** | Candidate detection remains, but report is now `Rejected` and the pass returns zero until it rewrites DMIR. |
| Bounds-check elimination | **FIX** | Comparisons are not bounds checks; report `Rejected` and preserve IR until an explicit access/check pair exists. |
| CSE | **FIX / REUSE** | Restrict reuse to one basic block, which provides local dominance. Cross-block reuse remains out of scope. |
| SROA reporting | **FIX** | Escape analysis is completed before reporting; the helper reports `Preserved` rather than claiming complete allocation removal. The outer scalarization pass is the structural transformation. |
| SIMD / parallel / async strategy selection | **FIX** | Report sequential scalar emission and rejected unsupported strategies. No SIMD/thread-pool/async claim is emitted. |
| Representation/layout adapters | **FIX** | Decisions are labeled as candidates; they do not claim DMIR/backend layout changes. |
| PGO | **FIX** | Only profiles with `source = "runtime"` may mutate optimization budgets. Static call-site estimates are rejected for budget changes. |
| Official parity report | **REWRITE** | Remove unsupported parity, SIMD, fusion, thread, and freeze claims. |
| Stale audit documents | **REWRITE** | Distinguish intended architecture from verified implementation and mark gaps explicitly. |

## 4. Source-level fixes

### 4.1 Pipeline fusion — `src/optimizer/pipeline_fusion.rs`

Previous behavior counted `filter`/`map` calls and chained `BinOp`s, then recorded `Applied` without changing any instruction. The current pass:

- scans for pipeline-shaped candidates;
- records `Rejected` with the missing IR/backend requirement;
- leaves calls and arithmetic instructions unchanged;
- returns `0`, so the optimizer cannot enter a false fixed-point iteration.

A future implementation must introduce a real fused iterator/stream representation or a verified CFG rewrite before using `Applied`.

### 4.2 Bounds-check elimination — `src/optimizer/memory.rs`

The previous pass treated every `<` or `<=` comparison as a removable bounds check. DMIR currently has no explicit array-access plus bounds-check instruction pair. The pass now records a rejected candidate and returns `0`. It does not delete comparisons.

Required future preconditions:

1. an explicit array access and check are represented in DMIR;
2. the index/range proof dominates the access;
3. aliasing and mutation are excluded;
4. the check is physically removed;
5. the native output and trap behavior are verified.

### 4.3 SROA / structure scalarization — `src/optimizer/memory.rs`, `src/optimizer/mod.rs`

The outer optimizer already has a structural non-escaping scalarization path: it removes `StructInit` and forwards `GetField` values for proven non-escaping aggregates. The helper in `memory.rs` was reporting `Applied` too early and was forwarding fields without an escape guard. It now:

- waits until escape analysis is complete;
- preserves escaping aggregates;
- reports the helper result as `Preserved` when it only forwards fields;
- leaves the complete allocation-removal claim to the outer pass, where `StructInit` removal is observable in DMIR.

The structural regression in `tests/test_optimizer_golden.rs` checks that `StructInit` is absent and that the native program still prints `40`.

### 4.4 CSE — `src/optimizer/scalar.rs`

The old single expression map was shared across all basic blocks without a dominance proof. It could reuse a value defined on a path that does not dominate the current block. The pass now uses a fresh expression map per basic block. This is conservative local CSE; a future global CSE pass must use `ControlFlowGraph::dominates` and handle joins explicitly.

### 4.5 Cost model — `src/optimizer/cost_model.rs`

`evaluate_loop_unroll`, `evaluate_parallelization`, and `evaluate_vectorization` now return rejected/candidate-only explanations. They remain useful for analytical cost exploration but cannot authorize code generation. The cost model no longer says that unrolling, worker-thread speedup, or SIMD lanes were emitted.

### 4.6 Representation and layout decisions — `src/optimizer/adaptive/representation.rs`, `layout.rs`, `strategy.rs`, `cost.rs`

The SAE can still describe a candidate such as `Candidate:PromoteToScalarSSA`, `Candidate:TransformToStructOfArrays`, or an analytical AVX2/parallel plan. These records explicitly state that no standalone DMIR/backend rewrite is emitted. The execution adapter continues to select `SequentialScalar`, which is the only strategy connected to the current native lowering path.

### 4.7 PGO — `src/pgo.rs`

`ProfileData.source` distinguishes:

- `static`: compiler call-graph/call-site estimate, not an execution count;
- `runtime`: reserved for actual instrumentation and execution.

Only `runtime` profiles now call `apply_pgo_boost(true)` and receive an `Applied` PGO budget record. Static profiles receive `Rejected`; they cannot widen inlining or unrolling budgets. Branch bias is also reported as observed-but-not-reordered because the CFG layout is not changed by the current pass.

## 5. Structural and runtime evidence

The following evidence is considered valid:

- `tests/test_optimizer_licm_proof.rs`: checks that LICM physically removes the invariant from the loop, does not grow/duplicate the loop body, and preserves native results for zero, one, seven, and twenty-one iterations.
- `tests/test_optimizer_golden.rs`: checks non-escaping `StructInit` removal and native result `40`.
- `tests/test_logical_operators.rs`: checks truth tables, skipped side effects, and division-by-zero short-circuit guards.
- `tests/test_semantic_adaptation_engine.rs`: checks that unsupported SIMD/parallel/async strategies are not selected and that the explanation mentions missing lowering.
- `tests/test_pgo.rs` and `tests/test_pgo_full_cycle.rs`: runtime-marked profiles may apply PGO; static profiles must not be treated as measurements.
- Release regression command: `cargo test --release -j 2`.

A trace line alone is not evidence. Native evidence requires an emitted executable, a verified output/exit code, and a structural inspection of the optimized DMIR or backend IR.

## 6. Explicitly unsupported capabilities

The current compiler must not claim these as emitted optimizations:

- SIMD/vector instructions or AVX2 loop lowering;
- automatic parallel loop lowering or a compiler-wired thread pool;
- async task reactor lowering;
- pipeline fusion into one streaming loop;
- general bounds-check elimination;
- AoS -> SoA/AoSoA physical layout conversion;
- PGO from static call-site counts;
- loop unrolling;
- global CSE without dominance proof;
- complete iterator protocol for all iterable values.

Runtime helper code and target metadata may exist, but existence is not equivalent to integration into source-to-native code generation.

## 7. Performance-claim gate

No Datara-vs-Rust parity statement is accepted until a benchmark:

1. uses equivalent algorithms and inputs;
2. prevents constant folding and benchmark-specific dead-code elimination;
3. validates outputs;
4. separates compile, process-startup, and kernel runtime;
5. records compiler mode, target, input/trip count, runtime method, binary size, memory, and correctness;
6. publishes raw repeated measurements and the harness source.

Until that gate is satisfied, performance results are exploratory measurements, not certification.
