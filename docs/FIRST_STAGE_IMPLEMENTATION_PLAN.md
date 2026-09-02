# First-Stage Implementation Plan — Verified Datara Core

**Date:** 2026-08-30  
**Purpose:** define the first implementable stage after the optimization honesty audit. This is an engineering gate, not a promise that the full concept documents are already implemented.

## 1. Stage objective

Turn the current native vertical slice into a small, explicitly specified and mechanically verified Datara core. The stage prioritizes semantic correctness and honest evidence over breadth or benchmark claims.

The stage ends with a compiler that can reliably process a bounded core program, reject invalid programs with stable diagnostics, lower it to canonical DMIR CFG, run only verified scalar optimizations, and produce a native executable whose output and exit code are checked.

## 2. Scope

### In scope

1. **Core language contract**
   - primitives: `Int`, `Float`, `Bool`, `String`;
   - immutable/mutable bindings and explicit assignment rules;
   - functions, parameters, return types, final-expression returns;
   - `if`, `while`, counted `for`, `loop`, `return`;
   - arithmetic, comparisons, normalized Boolean operators, short-circuit `&&`/`||`;
   - one canonical module spelling for the current supported subset;
   - explicit native entry point and deterministic stdout/exit behavior.

2. **Compiler correctness boundaries**
   - lexer/parser diagnostics with source spans;
   - name resolution and type checking for the core subset;
   - effect classification sufficient to reject unsafe optimizer assumptions;
   - CFG construction, dominance, natural-loop discovery, and terminator invariants;
   - DMIR verifier for value definitions, block targets, terminators, and return types.

3. **Verified optimizer subset**
   - constant folding where traps are preserved;
   - dead-code elimination with value-use checks;
   - local CSE only within a block;
   - LICM on natural loops with alias/effect/trap guards;
   - non-escaping SROA only when the structural DMIR delta is proven.

4. **Native evidence**
   - Cranelift backend lowering of the canonical CFG;
   - native output/exit-code tests;
   - structural DMIR tests before and after each applied pass;
   - differential tests comparing optimized and optimization-disabled execution.

5. **Honest tooling**
   - optimization decision states: `Applied`, `Candidate`, `Rejected`, `Preserved`;
   - no performance parity or freeze claim;
   - benchmark harness specification with separated compile/startup/kernel timings.

## 3. Explicit non-goals

The following are deferred and must not be simulated by report text:

- SIMD/vector lowering;
- compiler-generated parallel loops or automatic thread-pool dispatch;
- async runtime/reactor and cancellation;
- general iterator protocol;
- complete Result/Option/try-catch semantics;
- physical AoS/SoA/AoSoA rewriting;
- loop unrolling;
- global CSE across CFG joins;
- runtime PGO instrumentation;
- full IDE, LSP, formatter, documentation generator, and library ecosystem;
- performance parity with Rust or any other language.

## 4. Architecture for this stage

```text
Lexer
 -> Parser / AST
 -> Resolver + core type checks
 -> Effect and mutation facts
 -> DMIR CFG
 -> DMIR verifier
 -> verified scalar optimizer pipeline
 -> DMIR verifier again
 -> Cranelift lowering
 -> native executable
 -> output/exit-code oracle
```

The semantic graph and HIR remain integration targets. They must not be used as a reason to bypass the DMIR verifier.

## 5. Implementation phases

### Phase A — Invariants and verification

- define the DMIR verifier API;
- validate unique value definitions within the supported SSA model;
- validate all branch targets and return terminators;
- validate that instruction operands are available on the current block/path under the conservative rules used by each pass;
- run verification before optimization, after each applied pass in debug/test mode, and before codegen.

**Gate:** malformed DMIR unit tests fail with actionable diagnostics; valid existing fixtures pass.

### Phase B — Core semantics and diagnostics

- freeze the core grammar decisions for bindings, booleans, numeric promotion, overflow, modules, and returns;
- map parser/type/ownership failures to stable diagnostic codes and spans;
- add negative tests for unknown names, wrong types, invalid mutation, invalid returns, unreachable/invalid control flow, and unsupported operators.

**Gate:** every negative test fails for the intended reason and no unsupported syntax is silently accepted.

### Phase C — Verified scalar optimizer pipeline

- keep local CSE, constant folding, DCE, LICM, and proven SROA;
- require a structural before/after assertion for every `Applied` record;
- keep pipeline fusion, BCE, SIMD, parallel, layout, and unrolling candidate-only/rejected;
- preserve evaluation order, side effects, traps, ABI, and deterministic output.

**Gate:** optimized and unoptimized native executions agree on a corpus containing zero-trip loops, early returns, side effects, division guards, branches, and escaping aggregates.

### Phase D — Native and benchmark evidence

- make the native harness record compiler mode, target, source hash, compile time, startup time, kernel time, output digest, exit code, binary size, and memory method;
- prevent constant-fold-only workloads by using runtime-derived inputs and observable outputs;
- publish raw repetitions and uncertainty;
- keep results exploratory until the certification gate is satisfied.

**Gate:** a benchmark can be independently rerun without relying on report prose.

### Phase E — Documentation synchronization

- update implementation audit, current architecture, gap matrix, keep/rewrite/remove decisions, and performance report from test results;
- document unresolved concept decisions before expanding syntax;
- record every future optimization as `REUSE`, `EXTEND`, `FIX`, or `REPLACE`.

**Gate:** no document claims a feature is complete when the source/test evidence says partial or unsupported.

## 6. Dependencies

1. DMIR verifier precedes new optimizer passes.
2. Core semantic decisions precede parser grammar freeze.
3. Effect/mutation facts precede aggressive optimization.
4. Structural optimizer tests precede `Applied` report status.
5. Native differential harness precedes any performance claim.
6. Module and ownership expansions must not weaken the core verifier.

## 7. Acceptance criteria

The first stage is complete only when:

- `cargo test --release -j 2` passes;
- all core negative tests have stable diagnostic codes;
- DMIR verification runs at the optimizer/codegen boundary;
- every `Applied` optimization in the core pipeline has a structural test;
- unsupported capabilities are reported as candidates/rejected rather than applied;
- optimized and unoptimized native outputs match on the differential corpus;
- benchmark output includes the required metadata and does not claim parity;
- the five status documents agree with the code and tests.

## 8. Immediate implementation order

1. Add the DMIR verifier and call it at optimizer/codegen boundaries.
2. Add verifier-focused unit tests and preserve the existing LICM/SROA/CSE tests.
3. Add an optimized-vs-unoptimized differential helper for the core corpus.
4. Run the complete release suite.
5. Only then begin the next language slice: explicit module semantics and complete Result/Option behavior.
