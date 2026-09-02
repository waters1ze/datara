# Forgen Performance Status — Conservative Replacement Report

**Project:** Datara language / Forgen compiler  
**Target examined:** Windows native path, `x86_64-pc-windows-msvc`  
**Review date:** 31 August 2026  
**Status:** **Performance parity is not certified. Performance freeze is not authorized.**

## 1. Why the previous report was withdrawn

The previous version claimed “PERFORMANCE PARITY REACHED”, “READY FOR PERFORMANCE FREEZE”, emitted SIMD/vectorization, compiler-integrated parallel execution, pipeline fusion, and exact Datara-vs-Rust timing claims.

Those claims were stronger than the available evidence. The current backend is a scalar Cranelift path. Several older optimizer and SAE records described candidates as applied without a corresponding DMIR/backend transformation. Earlier measurements also mixed process startup, compile time, runtime, constant folding, dead-code elimination, and non-equivalent workloads.

This file deliberately replaces certification language with an evidence policy.

## 2. What is verified today

The active native path is:

```text
Datara source -> lexer/parser -> DMIR CFG -> optimizer -> Cranelift -> object -> MSVC linker -> native executable
```

The following categories have real implementation or test evidence, subject to the individual test cases:

- native executable generation;
- CFG-based `for`, `while`, and `loop` lowering;
- CFG-based short-circuit `&&` and `||`;
- LICM on actual natural loops;
- conservative local CSE;
- constant folding and dead-code/reachability transformations where the DMIR delta is inspected;
- proven non-escaping SROA paths where `StructInit` is physically removed;
- fail-closed handling for unknown operators and unknown lexer characters;
- honest SAE selection of sequential scalar execution;
- profile provenance distinction between static estimates and runtime profiles.

The proof must include a structural IR delta plus a native result. A trace line by itself is not proof.

## 3. Explicitly not certified

The current compiler does **not** have sufficient evidence to claim that it emits:

- SIMD or AVX2 vector loops;
- automatic parallel loop lowering or compiler-wired thread-pool execution;
- an async task reactor;
- fused `map`/`filter` pipelines;
- general bounds-check elimination;
- AoS-to-SoA/AoSoA physical layout conversion;
- sound loop unrolling;
- global CSE without dominance verification;
- PGO budget changes from static call-site counts;
- complete iterator lowering for every iterable value.

Target metadata, analytical plans, runtime helper modules, and enum variants do not establish that these features reach native codegen.

## 4. Current optimization decision vocabulary

| Decision | Meaning |
|---|---|
| `Applied` | A physical IR/backend transformation occurred and was verified. |
| `Rejected` | A candidate was found but no transformation was performed. |
| `Candidate` | Analytical possibility only; no emitted code is implied. |
| `Preserved` | Existing IR was intentionally kept for safety or unsupported lowering. |
| `Unknown` | Evidence is insufficient. |

## 5. Reproducible benchmark requirements

A benchmark may be promoted from exploratory to evidence only if it records:

1. equivalent algorithms and input data for every language;
2. runtime-derived input or an opaque input path that prevents constant folding;
3. an observable checksum/output and correctness assertion;
4. separate compile, process-startup, and kernel-runtime measurements;
5. repeated raw samples, not a single hand-selected value;
6. compiler mode, target triple, source revision, trip count, binary size, memory, and exit status;
7. the source of the harness and the exact command used;
8. structural DMIR/backend inspection for any claimed optimization.

The current benchmark artifacts are useful for investigation, but they do not authorize the earlier parity table as a certification result.

## 6. Required performance gates

Performance freeze can be considered only after:

- the DMIR verifier runs before and after every mutating optimization;
- every `Applied` decision has a matching structural/native test;
- the benchmark harness meets the requirements above;
- Datara and comparison implementations are algorithmically equivalent;
- results are reproduced on the stated target with raw measurements;
- the report names unsupported capabilities instead of treating them as emitted.

Until then, use language such as “exploratory measurement”, “candidate”, “scalar native baseline”, or “not measured”. Do not use “parity reached”, “faster than Rust”, “zero-cost”, or “ready for performance freeze” as project conclusions.

## 7. Evidence references

- `docs/AUDIT_OPTIMIZATION_FIXES.md` — optimization audit and source-level corrections.
- `docs/PROJECT_STATUS_AND_NEXT_STEPS.md` — current implementation map, inspection workflow, and Stage 1 acceptance criteria.
- `tests/test_optimizer_licm_proof.rs` — structural LICM and native correctness evidence.
- `tests/test_optimizer_golden.rs` — structural SROA and native output evidence.
- `tests/test_logical_operators.rs` — short-circuit and trap-preservation evidence.
- `tests/test_semantic_adaptation_engine.rs` — unsupported strategy rejection evidence.
- `tests/test_pgo.rs`, `tests/test_pgo_full_cycle.rs` — profile provenance and runtime-profile behavior.

## 8. Final status

**Forgen has a functioning native compiler baseline and several verified transformations. It does not yet have a defensible performance-parity certification.** The next phase is verified native core work: DMIR verification, honest reports, structural tests, reproducible measurements, and completion of the documented language/semantic gaps.
