# CURRENT ARCHITECTURE — FORGEN COMPILER TOOLCHAIN

**Language:** Datara (`.dtr`)  
**Compiler:** Forgen  
**Status:** Verified subset plus explicit implementation gaps

## 1. Actual native path

```text
Source
 -> lexer/parser
 -> AST/DAST subset
 -> resolver/type/effect/ownership checks (implemented slices)
 -> DMIR with BasicBlock/Terminator CFG
 -> scalar optimization passes with pass-specific evidence
 -> Cranelift lowering
 -> COFF/object
 -> MSVC linker
 -> native Windows executable
```

The canonical concept documents additionally require HIR, a complete semantic graph/dataflow boundary, full specialization, runtime PGO, and broader effect/ownership proofs. Those are architectural targets, not all verified current components.

## 2. Verified boundaries

### Frontend

The lexer rejects unsupported characters rather than dropping them. The parser and lowering support the tested Datara subset, including functions, classes, selected generics, control flow, modules, and short-circuit `&&`/`||`.

### DMIR and CFG

DMIR uses SSA-like `ValueId`s, basic blocks, and explicit terminators. Natural-loop and dominance analysis are available. Backend-visible control flow is represented by CFG terminators; legacy compound instructions are compatibility data and are not sufficient evidence of code generation.

### Optimizer

Verified scalar transformations include constant folding, dead-code elimination, inlining in tested cases, conservative local CSE, CFG LICM, and tested non-escaping aggregate scalarization. Candidate-only modules include pipeline strategy, representation/layout adaptation, analytical SIMD/parallel plans, and unsupported unrolling/BCE paths.

### Native codegen

Cranelift lowers the actual DMIR blocks and instructions. Unknown operators fail closed. The backend can emit and run native Windows executables for the tested subset.

## 3. Required pass contract

Each transformation intended for `Applied` status must document:

1. preconditions;
2. analysis and proof facts;
3. DMIR/CFG transformation;
4. postconditions and verifier;
5. effect/trap/ABI preservation;
6. cost estimate;
7. explanation and structural evidence.

## 4. Not yet complete

The following are planned boundaries rather than complete current guarantees:

- full HIR/DGraph lowering;
- complete Result/Option and try/catch semantics;
- complete iterator protocol;
- compiler-integrated `parallel`/`parallel for` lowering;
- async runtime and cancellation semantics;
- runtime PGO instrumentation;
- SIMD lowering;
- explicit bounds-check representation and elimination;
- physical AoS/SoA/AoSoA layout rewriting;
- formatter, documentation tooling, fuzzing, and differential test expansion.

This file describes the implementation that can be verified today, not the entire intended future architecture.
