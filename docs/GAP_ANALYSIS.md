# GAP ANALYSIS — DATARA SPECIFICATION VS IMPLEMENTATION

**Audit date:** 2026-08-30  
**Status:** Evidence-based baseline; `✅` means directly verified in current tests, not merely parsed or documented.

## 1. Coverage matrix

| Area | Canonical feature | Status | Evidence / next gate |
|---|---|---:|---|
| Lexing | identifiers, literals, strings, comments | ✅ | lexer tests |
| Lexing | unknown-character diagnostics and BOM | ✅ | `test_lexer_unknown_characters` |
| Functions | `fn`/compatibility `function`, bindings, expression bodies | ✅ subset | frontend/native tests |
| Functions | lambdas | ✅ subset | `test_functions_lambdas_slice` |
| OOP | classes, behaviors, components, roles | ✅ tested subset | modern OOP tests |
| OOP | complete composition/replacement semantics | ⚠️ partial | expand resolver/type proofs |
| Control flow | if/else, loops, decide/match/select | ✅ tested subset | control-flow/native tests |
| Logical semantics | short-circuit `&&`/`||` | ✅ | logical operator tests |
| Modules | selected `use`/multi-file workflows | ⚠️ partial | define import/export and cycle semantics |
| Types | primitives and selected generics | ✅ tested subset | generic tests |
| Types | complete Option/Result semantics | ⚠️ partial | `?` propagation + signature/channel rules ✅ (`test_result_propagation`); remains: decide/exhaustiveness over Outcome/Maybe, convenience constructors |
| Safety | ownership/borrowing | ⚠️ tested slices | complete interprocedural proof coverage |
| Effects | effect inference | ⚠️ tested slices | complete effect lattice and diagnostics |
| Semantic graph | graph structures and scaling tests | ⚠️ partial | connect all frontend facts and query API |
| Optimizer | folding/DCE/inlining/local CSE/LICM | ✅ tested subset | structural IR/native tests |
| Optimizer | non-escaping SROA | ✅ tested path | DMIR `StructInit` removal proof |
| Optimizer | pipeline fusion | ❌ candidate only | add fused IR and backend lowering |
| Optimizer | bounds-check elimination | ❌ candidate only | add explicit access/check IR |
| Optimizer | loop unrolling | ❌ disabled | implement fresh SSA + CFG rewrite |
| Optimizer | SIMD/vectorization | ❌ unsupported | implement vector IR/lowering first |
| Optimizer | automatic parallel lowering | ❌ unsupported | define effect/determinism/runtime contract |
| Optimizer | async reactor lowering | ❌ unsupported | implement runtime and cancellation |
| Layout | AoS/SoA/AoSoA physical rewrite | ❌ candidate only | connect layout to type/DMIR/backend |
| PGO | runtime instrumentation | ❌ incomplete | collect real counters and provenance |
| Codegen | Cranelift native executable | ✅ | native backend tests |
| Tooling | diagnostics | ⚠️ partial | stabilize codes and spans across all phases |
| Tooling | formatter/docs/fuzz/differential gates | ⚠️ partial | implement first-stage tooling slice |

## 2. Decisions required before language freeze

The four authoritative concept documents still need explicit decisions for: `model` versus library-level domain contracts; `import` versus `use`; `with` versus `from Base + Component`; `fn` versus `function`; Result versus try/catch boundaries; Boolean coercion; integer overflow; numeric promotion; module cycles; role/component conflicts; async cancellation; parallel error semantics; ABI and packed layouts.

## 3. Rule for status changes

A feature moves to `✅` only after source implementation, negative diagnostics, structural IR/codegen evidence where relevant, native execution where relevant, and documentation agree. A cost model or passing unit test that only inspects a plan label is not sufficient.
