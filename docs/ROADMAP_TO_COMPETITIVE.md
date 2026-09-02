# Roadmap — Making Datara Genuinely Competitive

**Date:** 2026-08-31
**Supersedes:** the stage ordering in `PROJECT_STATUS_AND_NEXT_STEPS.md` §12
**Companion to:** `AUDIT_2026-08-31_COMPLETION_REPORT.md`

---

## 0. Strategic position — read this first

The measured facts from this session:

- On work **both** compilers execute, Datara's scalar loop codegen is within ~1x of
  LLVM (`float_loop`: 307 ms vs 346 ms, Datara 0.89x).
- Datara **compiles ~2.5x faster** than rustc (254–428 ms vs 736–759 ms) and produces
  **~17x smaller binaries** (9.7 KB vs 163 KB).
- On reducible integer kernels the gap is **unbounded**: rustc replaces a
  500-million-iteration loop with a closed-form expression (0 ms); Forgen runs every
  iteration (205 ms).

That last point reframes the whole roadmap. Datara will not win a raw-performance
contest against a 15-year-old SSA optimizer by adding SIMD, and chasing SIMD first
would be repeating the mistake that produced the fabricated reports.

**The honest competitive position is not "faster than Rust". It is:**

1. **Fast compile + small output** — already true, and directly valuable for
   scripting, tooling, embedded and WASM.
2. **Lower cognitive cost than Rust** for the target audience (TS/JS/Python
   developers) while staying native and memory-safe.
3. **A machine-readable semantic graph** — the design documents call for an
   AI-readable IR. Nothing else in this space ships that. It is the one genuinely
   differentiated feature, and it is currently half-built.

Performance work should target the *unbounded* gaps (where Forgen is infinitely
worse), not the 10% gaps.

---

## 1. The five structural debts

Everything below is downstream of these. They are ordered by how much else they block.

### D1 — DMIR is not real SSA

`src/dmir/verifier.rs` currently *cannot* check single assignment:

> "The current lowering uses repeated ValueIds for mutable loop-carried variables.
> Until those variables are represented by explicit block parameters/phis, duplicate
> IDs cannot be rejected globally."

This is the deepest debt. Because loop-carried variables reuse `ValueId`s, constant
folding, CSE and LICM all operate on a partially-invalid SSA assumption. The existing
passes are conservative enough not to break today, but every future optimization —
global CSE, unrolling, vectorization, better LICM — needs a real dominance-based
proof that is impossible while ids are reused.

**Fix:** represent loop-carried variables as block parameters and phi nodes. Cranelift
already exposes the block-param mechanism the backend uses for the entry block.

**This is the highest-value item in the roadmap. Nothing advanced is sound without it.**

### D2 — The `Applied` contract is not enforced mechanically

The project adopted the right vocabulary (`Applied` / `Rejected` / `Candidate` /
`Preserved`) but nothing enforces it. This session found a pass still reporting a
detection count as a transformation. A pass can record `Applied` after changing
nothing, and only human review catches it.

**Fix:** make `Applied` unreachable without evidence. Concretely, have each mutating
pass return the new IR and have the driver diff before/after, downgrading any
`Applied` record with an empty diff to `Rejected` automatically.

### D3 — The build is machine-specific

`backend.rs:1336` hardcodes `d:\DATARA\datara + forgen\src\runtime\datara_runtime.obj`.
The project cannot be built from a checkout anywhere else. The runtime object is also
a checked-in binary that silently goes stale whenever the C source changes — which is
exactly why the float bug could exist at all.

**Fix:** a `build.rs` using the `cc` crate. Compiles the runtime on every build,
resolves paths portably, and deletes the checked-in `.obj`.

### D4 — No CI

Nothing runs `fmt`, `clippy` or the tests automatically. With ~190 clippy warnings and
a fabricated-benchmark history, the project needs a mechanical gate rather than
discipline.

**Fix:** GitHub Actions running `fmt --check`, `clippy -D warnings`, and
`test --release` on every push.

### D5 — The specification is not frozen

`GAP_ANALYSIS.md` lists 13 unresolved questions (`import` vs `use`, `fn` vs
`function`, `Result` vs `try/catch`, boolean coercion, overflow, numeric promotion,
module cycles, …). You cannot build a competitive language on a spec that is still
moving, and you cannot write a stable stdlib against one.

**Fix:** decide all 13, write the decisions into the spec, and freeze v1.0. Prefer
boring answers.

---

## 2. Stages

Effort is in engineer-weeks and assumes familiarity with the codebase.

### Stage 0 — Trust and portability (1–2 weeks)

Goal: anyone can clone, build and verify; the build cannot silently rot.

| # | Task | Why |
|---|---|---|
| 0.1 | `build.rs` + `cc` for the runtime; delete checked-in `.obj` | D3 |
| 0.2 | Resolve the runtime path relative to `CARGO_MANIFEST_DIR` | D3 |
| 0.3 | CI: `fmt --check`, `clippy -D warnings`, `test --release` | D4 |
| 0.4 | Drive clippy from ~190 warnings to 0 | D4 |
| 0.5 | Linux + macOS targets (ELF/Mach-O, linker detection) | Windows-only today |
| 0.6 | Clean the repo root (286 stray `.exe`/`.obj`/`.dtr`) | signal vs noise |

**Exit criteria:** clean clone builds and passes tests on Windows and Linux; CI green;
zero clippy warnings.

### Stage 1 — Verification gate (2–3 weeks)

Goal: no optimization can be claimed without mechanical proof.

| # | Task | Why |
|---|---|---|
| 1.1 | Real SSA: block params + phis for loop-carried variables | D1 |
| 1.2 | Strengthen the verifier: single assignment, dominance, use-before-def | D1 |
| 1.3 | Run the verifier before and after every mutating pass | catches 2.1-class bugs |
| 1.4 | Driver-enforced `Applied` evidence (diff-or-downgrade) | D2 |
| 1.5 | Structural tests per pass (IR shape), not trace-string assertions | replaces theater tests |
| 1.6 | Differential testing: optimized vs unoptimized output must match | cheap, high yield |
| 1.7 | Remove or isolate legacy `Inst::WhileLoop` / `Inst::TryCatch` | backend ignores both |

**Exit criteria:** every pass has a structural test; the verifier rejects hand-written
broken IR; optimization reports are generated, not asserted.

### Stage 2 — Close the unbounded performance gaps (4–6 weeks)

Goal: stop being infinitely worse. This stage requires D1 complete.

Ordered by leverage, **not** by glamour:

| # | Task | Impact |
|---|---|---|
| 2.1 | Scalar evolution + induction-variable simplification | the 500M→0 ms gap; biggest single win |
| 2.2 | Loop-idiom recognition (recognise closed-form accumulations) | same gap, generalised |
| 2.3 | Strength reduction (multiply→add in induction chains) | complements 2.1 |
| 2.4 | Global CSE with dominance | currently block-local only |
| 2.5 | Sound loop unrolling (fresh SSA, CFG duplication, remainder) | enables everything below |
| 2.6 | Bounds-check elimination with an explicit access/check IR pair | needs IR work first |

Note what is deliberately **not** here: SIMD, auto-parallelization, async, fusion.
They are Stage 5 and must not be started before Stage 2 lands. Adding them early is
what produced the fabricated benchmark table.

**Exit criteria:** `int_loop` is within ~1.5x of rustc rather than unboundedly behind;
every new pass has a structural test and a benchmark entry.

### Stage 3 — Language completeness (4–6 weeks)

Goal: the language is usable for real programs, not just benchmarks.

| # | Task |
|---|---|
| 3.1 | Freeze the spec (all 13 open questions) — **prerequisite for 3.2–3.7** |
| 3.2 | `Result` and `Option` with a propagation operator and exhaustiveness |
| 3.3 | A real iterator protocol for all iterable values |
| 3.4 | Modules: import/export resolution, cycle diagnostics, multi-file ABI |
| 3.5 | Ownership/borrow completeness including escape and mutation |
| 3.6 | Effect inference for IO, network, async, parallel |
| 3.7 | Structured diagnostics: stable codes, spans, fixes |
| 3.8 | stdlib grown against the frozen spec (currently 9 `.dtr` files) |

**Exit criteria:** a non-trivial program (say a JSON parser or an HTTP client) written
in Datara end-to-end, without workarounds.

### Stage 4 — Ecosystem and differentiation (ongoing)

This is where Datara can actually win, and it is currently the least-invested area.

| # | Task | Note |
|---|---|---|
| 4.1 | Formatter + doc generator | table stakes |
| 4.2 | LSP server (hover, goto-definition, diagnostics) | table stakes |
| 4.3 | Package manager + registry on the project model already in `src/project/` | table stakes |
| 4.4 | **Semantic-graph export as a first-class artifact** | the differentiator |
| 4.5 | AI-facing tooling built on 4.4 | the differentiator |
| 4.6 | WASM target | plays to fast compile + small binaries |
| 4.7 | Public docs site with a real tutorial | adoption |

4.4 deserves emphasis: the design documents specify an AI-readable semantic graph, and
`src/semantic_graph/` already exists. Published as a stable artifact it is something no
mainstream language offers, and it is far more defensible than a 10% benchmark win.

### Stage 5 — Advanced optimization (only after Stage 2)

Each item requires a real lowering plus a structural test, or it does not ship:

- SIMD with target-feature detection and actual vector instructions
- Auto-parallelization with formal effect and error semantics
- Async runtime with cancellation
- Pipeline fusion through a real iterator IR
- Runtime PGO instrumentation with provenance and versioning
- Physical layout transformation (AoS/SoA) wired to type + DMIR + backend

---

## 3. What to do in the next session

In order:

1. **D3** — `build.rs` with `cc`. Small, removes a real trap, unblocks collaboration.
2. **D4** — CI. Mechanical enforcement instead of discipline.
3. **D1** — real SSA for loop-carried variables. The hard one, and the one everything
   else waits on.
4. **Spec freeze** — decide the 13 open questions. Blocks real stdlib and ecosystem work.
5. **2.1/2.2** — scalar evolution and loop-idiom recognition. Closes the only
   unbounded performance gap.

## 4. What not to do

- Do not add SIMD, parallel, async or fusion until Stage 2 is done. Their absence is
  not what makes Datara slow; they are how the project ended up with numbers it had
  to fake.
- Do not write another performance report until `harness.py` runs in CI and the numbers
  are reproducible from a clean checkout.
- Do not add language features before the spec is frozen.
- Do not trust a pass because its tests pass. Ask what IR it changed.

## 5. Honest outlook

Datara has a real compiler: a working frontend, a CFG-based IR, a native Cranelift
backend, and scalar codegen already within ~1x of LLVM on work both compilers run.
That is a better starting point than most language projects ever reach.

It is not yet a competitive language, and the reason is not missing features — it is
that the IR cannot support the analyses a modern optimizer needs (D1), the build only
works on one machine (D3), nothing enforces optimization honesty mechanically (D2),
and the specification is still moving (D5).

Fix those five and the performance work becomes tractable. Skip them and any further
optimization work will be built on the same sand as the last round.
