# FORENSIC AUDIT — DATARA + FORGEN COMPILER
**Auditor role:** Independent Senior Compiler Engineer / Systems Engineer / Security Reviewer / Performance Engineer  
**Audit method:** Cold-start, zero prior briefings. Direct code reading + test execution.  
**Date:** 2026-08-30

---

## 1. WHAT EXISTS — REAL INVENTORY

### Source Modules

| Module | File(s) | LOC (est.) | Role |
|---|---|---|---|
| `lexer` | `mod.rs`, `tokens.rs` | ~420 | Handwritten lexer |
| `parser` | `mod.rs` | ~1 253 | Recursive-descent parser |
| `ast` | `mod.rs` | 404 | AST node definitions |
| `resolver` | `mod.rs` | 679 | Name resolution, scope tracking |
| `types` | `mod.rs` | 587 | Type inference, generic instantiation |
| `effects` | `mod.rs` | 317 | Effect lattice analyzer |
| `ownership` | `mod.rs` | 483 | Borrow / move / view checker |
| `semantic_graph` | `mod.rs` | 436 | Rich semantic graph builder |
| `dmir` | `mod.rs` | 660 | Custom IR (DMIR) + lowering |
| `optimizer/mod.rs` | + 4 sub-files | ~1 600 | Optimizer pipeline |
| `codegen/cranelift/clif.rs` | + `mod.rs` | ~400 | CLIF IR emitter (text) |
| `codegen/legacy_bootstrap.rs` | | 562 | C# / csc.exe bootstrap backend |
| `codegen/target.rs` | | 185 | Target triple / ABI metadata |
| `diagnostics` | `engine.rs`, `codes.rs`, `span.rs` | ~300 | Error reporting |
| `pgo.rs` | | 80 | Profile-guided optimization |
| `incremental.rs` | | 80 | Incremental cache (FNV-1a hash) |
| `driver.rs` | | 425 | Compilation pipeline orchestrator |
| `cli.rs` | | ~600 | CLI entry point |

**Tests:** 32 integration test files, covering compilation, optimization, ownership, generics, PGO, incremental, Cranelift IR, effects, semantic graph, and differential backend execution.

---

## 2. WHAT REALLY WORKS — HONEST VERDICT

### ✅ Genuinely working pipeline

The **full compilation pipeline** runs end-to-end and all 66 integration tests pass (0 failures):

```
Lexer → Parser → Resolver → TypeChecker → EffectAnalyzer →
OwnershipTracker → SemanticGraph → DMIR Lowering → Optimizer → 
(CLIF text emitter) + (C# bootstrap lowering → csc.exe → .exe) → execution
```

Programs compiled from `.dtr` source execute correctly with verified stdout output.

### ✅ What is semantically correct

- **Lexer** — Correct. Handles all language tokens including domain-specific ones (`decide`, `view`, `mut-view`, `|>`, `:=`, `=>`). Multi-char and Unicode-safe (char-indexed Vec).
- **Parser** — Correct and complete. Recursive-descent, 1 253 lines covering classes, behaviors, components, roles, generics, lambdas, pipelines, try/catch, parallel, `with`, `decide`, `match`, `select`.
- **Resolver** — Correct. Proper scope stack, inheritance chain resolution, composition merging, role verification, export tracking.
- **Type checker** — Correct for the type system implemented. Handles inheritance chain field lookup, composition merging, generic template instantiation tracking, `TypeParam` unification via `is_compatible`.
- **Effects system** — Correct. Lattice: `Pure < Read < Write < IO < Network < Database < Unsafe < Nondeterministic`. Union semantics. `allows_parallel()` correctly blocks on `Write/IO/Network`.
- **Ownership / borrow checker** — Correct for the model implemented. Catches: use-after-move, mutation during active view, multiple mutable views, immutable binding mutation, escaping view, simultaneous alias in call arguments.
- **DMIR** — Sound. SSA-like value ID system, single flat basic block per function (see §5 for limitation). `WhileLoop`, `TryCatch`, `Decide` are nested instruction compound nodes.
- **Optimizer passes** — Correct and all producing real transformations:
  - **Constant folding** — propagates `i64` and `String` constants across `LoadVar`/`BinOp`.
  - **CSE** — correctly identifies and deduplicates pure BinOp subexpressions.
  - **SROA** — escape analysis + scalarization of non-escaping struct inits. StructInit nodes eliminated, GetField replaced by direct value copies.
  - **LICM** — invariant code hoisted from loop body to pre-header based on preheader-available value set.
  - **Inlining** — pure, non-recursive, single-block functions inlined at call sites with fresh value ID renaming.
  - **DCE** — unreferenced pure value defs removed.
  - **Dead Symbol Elimination** — call-graph reachability from `main`, strips unreachable functions.
- **PGO** — Correct data-layer: serialize/load profiles, `is_hot()` threshold, `apply_pgo_boost` doubles inlining/unroll budgets. Integrated with optimizer.
- **Incremental cache** — Correct FNV-1a content hashing, JSON serialization, freshness checking, invalidation.
- **Cranelift CLIF emitter** — Produces syntactically correct CLIF text for all DMIR instructions. Type mapping is complete (i64, f64, i8, calling convention).
- **Semantic graph** — Correctly built, queryable, integrates with optimizer report for `attach_optimization_report`.
- **Generic monomorphization** — Tracked via `generic_specializations` in TypeChecker, C# specializations emitted with mangled names (`Box_Int`, `Box_Float`).

---

## 3. HOW IT IS IMPLEMENTED — ARCHITECTURE

### Compilation pipeline (driver.rs)

```
compile_source()
  │
  ├─ Lexer::tokenize()
  ├─ Parser::parse_program()
  ├─ Resolver::resolve_program()
  ├─ TypeChecker::check_program()
  ├─ EffectAnalyzer::analyze_program()
  ├─ OwnershipTracker::check_program()
  ├─ SemanticGraph::build()
  ├─ Lowering::lower_program()  → DMIR Module
  ├─ Optimizer::optimize_module()
  │     ├─ inline_pure_functions()
  │     ├─ optimize_function() [per fn, up to 10 iterations domain / 3 release]:
  │     │     ├─ ScalarOptimizer::eliminate_common_subexpressions()
  │     │     ├─ LoopOptimizer::optimize_loops()
  │     │     ├─ scalarize_structures()
  │     │     ├─ constant_fold()
  │     │     └─ dead_code_elimination()
  │     └─ dead_symbol_elimination()
  ├─ CraneliftBackend::emit_clif()  → CLIF text (written to .clif file)
  └─ LegacyBootstrapCodegen::emit() → C# source → csc.exe → .exe
```

### Backend truth

> [!IMPORTANT]
> The **actual execution backend** is `LegacyBootstrapCodegen` — it generates C# and compiles via `csc.exe` (Windows .NET Framework 4.x). The `CraneliftBackend` generates CLIF text and **writes it to disk**, then delegates execution to the bootstrap backend. There is **no Cranelift library linkage** (no `cranelift-codegen` crate dependency). The CLIF text is for inspection/verification purposes only.

```toml
[dependencies]
serde = { version = "1.0.229", features = ["derive"] }
serde_json = "1.0.151"
```

No `cranelift-*` crates are in `Cargo.toml`. The Cranelift "backend" is a textual CLIF **emitter** — a human-readable IR printer.

---

## 4. ARCHITECTURE QUALITY ASSESSMENT

### Strengths

| Area | Grade | Notes |
|---|---|---|
| Pipeline separation | **A** | Each stage is cleanly separated with well-defined input/output types |
| Error propagation | **A-** | `DiagnosticEngine` with error codes, spans, source context |
| Ownership model design | **A** | Sound for the implemented subset; negative tests verify all core safety rules |
| Optimizer architecture | **B+** | Clean pass structure, cost model, decision trace, PGO integration |
| Generics | **B** | Monomorphization tracked; template specialization in C# works |
| Test coverage | **B+** | 32 test files, 66 tests, good positive/negative/golden/differential coverage |
| DMIR design | **B** | SSA value IDs are sound; compound instruction nodes are pragmatic |
| Effect system | **B+** | Lattice-based, correct `allows_parallel`, pluggable |
| Incremental compilation | **B** | Content-hash based, correct semantics; no dependency-graph propagation yet |
| CLIF emitter | **B** | Complete and correct text format; not connected to machine code generation |

### Weaknesses

| Area | Grade | Notes |
|---|---|---|
| Real native codegen | **C** | CLIF is text-only; actual execution is C# → .NET; no LLVM or real Cranelift linkage |
| CFG / basic blocks | **C+** | All functions have exactly 1 basic block (`entry`). Branches are embedded compound instructions (`WhileLoop`, `Decide`). No real SSA φ-nodes. |
| Type inference | **C+** | Bidirectional inference not implemented. `TypeParam` unification is trivially `true` — any type param accepts any type. |
| Optimizer soundness | **B-** | CSE does not account for store/call aliasing (safe because only pure BinOps are CSE'd); LICM correctness depends on preheader set being exact |
| Parallel semantics | **D** | `parallel { }` and `parallel for` are syntactically parsed and effects-analyzed, but in C# lowering they execute **sequentially** — no threading |
| Module system | **C** | `use` declarations parsed and resolved; but multi-module compilation just concatenates AST declarations into a single flat program — no separate compilation |
| Error propagation (`?`) | **C** | `ErrorPropagate` AST node exists; in bootstrap lowering it is not specially handled — equivalent to `try/catch` semantics only |
| Vectorization | **D** | `vectorization_width` exists in cost model and evaluator; no actual SIMD code emitted |

---

## 5. HIDDEN SIMPLIFICATIONS — WHAT IS NOT WHAT IT APPEARS TO BE

### 5.1 "Cranelift backend" — CLIF text emitter, not native codegen

```rust
// cranelift/mod.rs:49-52
fn compile_to_executable(&self, source: &str, output_path: &Path) -> Result<PathBuf, String> {
    let clif_path = output_path.with_extension("clif");
    let _ = fs::write(&clif_path, source);
    self.fallback.compile_to_executable(source, output_path) // ← delegates to C# bootstrap
}
```

The CLIF text is written to disk as a side-effect but never passed to a Cranelift compiler. The `fallback` (C# bootstrap) actually compiles using `csc.exe`. This is architecturally honest — the module is labeled "legacy bootstrap" and the code comment says "NOT part of the canonical production compiler architecture" — but the test name `test_cranelift_backend_multi_target_clif` may mislead.

### 5.2 Single basic block — no real CFG

```rust
// dmir/mod.rs:117-132
let mut block = BasicBlock { label: "entry".into(), instructions: Vec::new() };
let ret_val = self.lower_stmt(&f.body, &mut block);
// ...
Function { ..., blocks: vec![block] }
```

Every function gets exactly one basic block. Control flow (if/while/loops) is encoded as compound `Inst` variants (`WhileLoop`, `Decide`, `TryCatch`) containing nested `Vec<Inst>`. This works for bootstrap execution (C# handles control flow natively), but it means:
- No SSA φ-nodes
- LICM only hoists from the single top-level block
- DCE doesn't cross compound instruction boundaries

### 5.3 Parallel blocks are sequential

```rust
// ownership/mod.rs:205-206
Stmt::Parallel(body, _) => { self.check_stmt(body, diag, false); }
```

```rust
// dmir/mod.rs: lower_stmt for Stmt::Parallel
// → lowers to the same block.instructions, no threading
```

The effects system correctly marks `parallel {}` as requiring `allows_parallel()`, but the DMIR lowering and C# codegen emit no threading primitives. Parallel for-loops execute sequentially.

### 5.4 Type `is_compatible` is over-permissive

```rust
// types/mod.rs:36-41
if let (DataraType::TypeParam(_), _) = (self, other) { return true; }
if let (_, DataraType::TypeParam(_)) = (self, other) { return true; }
```

Any `TypeParam` is compatible with any type. This means the type checker accepts programs with unsound generic instantiations that would be rejected by a proper constraint-solving system.

### 5.5 String constants are `iconst.i64 0` in CLIF

```rust
// codegen/cranelift/clif.rs:195-196
Inst::ConstStr { dest, value } => {
    format!("    ; const string {:?}\n    v{} = iconst.i64 0\n", value, dest.0)
}
```

All string constants are emitted as CLIF `iconst.i64 0`. The same applies to variable loads (`LoadVar`). The CLIF text is not semantically correct for strings — it would fail actual Cranelift compilation. This is acceptable because CLIF is used for inspection only, not real codegen.

### 5.6 MethodCall in CLIF is wrong

```rust
// clif.rs:247-251
Inst::MethodCall { dest, object, method, args, .. } => {
    // ...
    format!("    v{} = call fn${}({})\\n", dest.0, method, args_str)
}
```

Method calls in CLIF use only the method name, not the `object.method` qualified name. Multiple methods with the same name on different types would collide.

### 5.7 `NativeCodegen` type alias points to C#

```rust
// codegen/mod.rs:23
pub type NativeCodegen = BootstrapBackend;  // = LegacyBootstrapCodegen
```

The type publicly advertised as "native codegen" is the C# bootstrap.

### 5.8 PGO profile data is never collected at runtime

`ProfileData` can record call counts, but the system never instruments produced executables to feed back actual runtime counts. PGO is only tested by manually constructing `ProfileData` in unit tests.

---

## 6. OWNERSHIP / SECURITY ANALYSIS

### Positive
- The ownership model is **sound for the implemented invariants**. All four ownership errors (use-after-move, mutation during view, multiple mutable views, immutable binding mutation) are correctly detected and all 8 ownership tests pass.
- The model correctly handles nested borrows, escaping views, and simultaneous alias detection in function call arguments.
- `unsafe` keyword is parsed and tracked in effects (`Effect::Unsafe`).

### Gaps
- **No lifetime analysis** — borrows do not have lifetimes; active_borrows are tracked per-scope but borrows are never released at scope exit. This means a view created in an inner scope and used in an outer scope would not be flagged.
- **Borrow records never cleared** — once a borrow is recorded in `active_borrows`, it stays for the entire function scope. There is no scope-exit cleanup. This means false "still borrowed" errors would occur in practice if a view variable goes out of scope before the source.
- **No aliasing in field access** — `obj.field` field reads are not tracked as borrows of `obj`, only as `Effect::Read`.
- **No cross-function ownership tracking** — function parameters are simply inserted as `Active` without verifying calling-side ownership; no inter-procedural borrow checking.

---

## 7. PERFORMANCE CLAIMS VS. REALITY

### Claimed / Tested
- **Zero-cost OOP** (`test_zero_cost_oop_point_length`): `total_heap_allocations == 0` ✅ — verified via DMIR inspection after SROA.
- **Zero-cost generics** (`test_zero_cost_generic_box_suite`): Monomorphized correctly ✅.
- **Constant folding**: `10 * 20 → 200`, verified in IR ✅.
- **Inlining**: pure leaf function `add` eliminated from caller IR ✅.
- **SROA**: `Point { x, y }` StructInit eliminated, direct field forwarding ✅.

### Reality check
All "zero-cost" claims are verified against the **DMIR** (an IR in Rust memory), not against the actual executed binary. The actual executed binary is compiled C# running on .NET. The CLR JIT does its own optimizations and heap management. The `heap_allocations == 0` count is a count of `StructInit` nodes in DMIR that survive escape analysis — not actual malloc calls in the produced .exe.

The correct interpretation: **the DMIR optimizer achieves zero-allocation IR**, which is a meaningful semantic claim and a sound foundation for a future native backend.

---

## 8. TEST SUITE QUALITY

**Result:** 66 tests, **0 failures**, across all 32 test files.

### Strong tests
- `test_ownership_safety.rs` / `test_ownership_soundness.rs` — negative tests with exact error code assertions
- `test_optimizer_golden.rs` — IR-level golden tests (BinOp presence, StructInit absence)
- `test_zero_cost_proof.rs` — end-to-end: DMIR inspection + CLIF assertions + execution output
- `test_differential_backends.rs` — compares outputs across optimization modes
- `test_modern_oop_slice.rs` — negative tests for unsatisfied roles and ambiguous override

### Test gaps
- **No fuzz testing** of the lexer/parser
- **No multi-level inheritance depth tests** (only 1–2 levels tested)
- **No test for parallel code actually running in parallel** (because it doesn't)
- **No test for the `?` error propagation operator behavior** in complex scenarios
- **No test verifying borrow scope exit** (the gap identified in §6)
- **No test for negative type errors** (type mismatch, wrong generic arg types)

---

## 9. WHAT IS MISSING / NEXT GAPS

| Feature | Status | Comment |
|---|---|---|
| Real native codegen | ❌ Not present | CLIF text is inspection-only; no Cranelift linkage |
| Real SSA + CFG | ❌ Not present | Single basic block per function; compound node control flow |
| Parallel execution | ❌ Not implemented | Syntax + effects present; lowering is sequential |
| Lifetime analysis | ❌ Not present | Borrow records not scoped; no scope-exit cleanup |
| Type inference (bidirectional) | ❌ Not present | Type params accepted trivially |
| Error propagation (`?`) in lowering | ⚠️ Partial | AST node exists; lowering does not implement early-return semantics |
| Vectorization | ❌ Declared, not emitted | Cost model has it; no SIMD instructions generated |
| Cross-function borrow checking | ❌ Not present | Intra-function only |
| Real incremental compilation | ⚠️ Partial | Cache freshness works; dependency invalidation propagation not implemented |
| Linker / object format output | ❌ Not present | Output is a .NET exe via csc.exe |
| Debug info / DWARF | ❌ Not present | |
| Macro / metaprogramming | ❌ Not present | |

---

## 10. EXECUTIVE SUMMARY

**What this is:** A semantically coherent, architecturally well-designed **compiler frontend and IR optimizer**, with a C# bootstrap backend for execution, and a textual Cranelift IR emitter for inspection.

**What it is not:** A native code compiler. There is no machine code generation, no object file emission, no real Cranelift linkage, no vectorization, no true parallelism.

**Quality of what exists:**
- The frontend (lexer → parser → resolver → type checker → effects → ownership) is production-quality and complete for the language subset.
- The DMIR and optimizer pipeline is solid, correct, and well-tested at the IR level.
- The ownership/borrow model is sound for the implemented invariants, with clearly documented scope.
- The architecture is designed for a native backend to be plugged in — CLIF emission is the correct first step.

**Primary honest gap:** The system is described and tested as if CLIF → native compilation exists. It does not. The `.exe` files are .NET assemblies from `csc.exe`. The "zero-cost" guarantees are IR-level proofs, not machine-code proofs.

**Architectural verdict:** This is the right architecture. The pipeline is clean, the IR is sound, the optimizer passes are real, the test suite is strong. The primary work remaining is wiring real Cranelift compilation (`cranelift-codegen` crate), implementing true CFG with φ-nodes, and adding lifetime-scoped borrow tracking.

---

## 11. CRITICAL DEFECTS REQUIRING ATTENTION

| # | Severity | Location | Description |
|---|---|---|---|
| 1 | **HIGH** | `codegen/mod.rs:23` | `NativeCodegen = BootstrapBackend` — misleading alias for C# backend |
| 2 | **HIGH** | `codegen/cranelift/mod.rs:49-52` | CLIF compilation delegates to C# without executing CLIF |
| 3 | **MEDIUM** | `ownership/mod.rs` | Active borrows not released at scope exit — false positives in complex programs |
| 4 | **MEDIUM** | `types/mod.rs:36-41` | `TypeParam` is_compatible accepts anything — unsound for constrained generics |
| 5 | **MEDIUM** | `dmir/mod.rs` | Single-block CFG — LICM/DCE cannot see across compound instruction boundaries |
| 6 | **LOW** | `codegen/cranelift/clif.rs:195-199` | Strings and LoadVar emit `iconst.i64 0` — CLIF text semantically incorrect for strings |
| 7 | **LOW** | `clif.rs:247-251` | MethodCall CLIF uses only method name — would collide on multi-type dispatch |
| 8 | **INFO** | `pgo.rs` | PGO profile never automatically collected from executed binaries |
