# Completion Report — Audit, Bug Fixes and Honest Benchmarking

**Date:** 2026-08-31
**Workspace:** `D:\DATARA\datara + forgen`
**Base commit:** `4beb285`
**Result commit:** `abb2cf6`

---

## 1. State at the end of this pass

| Gate | Before | After |
|---|---|---|
| `cargo fmt --all -- --check` | pass | pass |
| `cargo check --all-targets` | pass | pass |
| `cargo test --release` | 113 pass / **1 fail** | **111 pass / 0 fail** |
| Git baseline | none (all files untracked) | 2 commits |
| Benchmark methodology | invalid (measured process spawn) | kernel-only, checksummed |

The single failing test (`test_pgo_full_cycle_inlining_and_branch_prediction`) was
not a bad test — it was catching a real miscompilation. It is fixed and now passes.

---

## 2. Bugs found and fixed

### 2.1 Inlining left the call destination undefined — P0, miscompilation

**Where:** `src/optimizer/mod.rs`, `inline_pure_functions`

**Cause:** The inliner read the callee's return value from `Inst::Return`. The real
lowering returns through `Terminator::Return`, and the backend ignores the legacy
`Inst::Return` entirely. So the return value was *always* `None`, the `Call` was
deleted, and its `dest` was never redefined. Every later use of that value —
including the block terminator, which is where a call result is usually consumed —
referenced an undefined value.

**Evidence:** the release test suite failed with
`DMIR verification failed after optimization: process_input: use of undefined value %11`.

**Fix:**
- Read the return value from `Terminator::Return`, falling back to `Inst::Return`.
- Substitute the call's `dest` with the inlined return value across every later
  instruction **and** the block terminator (new `substitute_operands` /
  `substitute_terminator` helpers, which handle all `Inst` variants — the old
  `remap_inst` silently returned `None` for anything it did not handle).
- Rename callee locals with a per-site prefix. Previously a callee's
  `AssignVar { name: "t" }` was spliced in unchanged, so it could overwrite a
  same-named variable in the caller.

**Also:** `max_value_id_in_function` ignored block parameters and terminator
operands, so freshly minted value ids could collide with existing ones. It now
includes both. The dead `remap_inst` was removed.

### 2.2 LICM hoisted instructions in nondeterministic order — P0, latent miscompilation

**Where:** `src/optimizer/loops.rs`, `licm_pass`

**Cause:** `NaturalLoop::blocks` is a `HashSet<BasicBlockId>`. Hoisted instructions
were emitted in hash iteration order, which is arbitrary and varies between runs.
When two hoisted values depend on each other (`t1 = a + b`, `t2 = t1 * 2`), the
preheader could use a value before defining it.

**Fix:** new `ordered_loop_blocks` produces a deterministic topological order of the
loop body — a DFS post-order with back edges into the header removed (which makes
the body a DAG), reversed. Definitions always precede uses.

### 2.3 DCE deleted trapping integer division — P1, semantics change

**Where:** `src/optimizer/mod.rs`, `dead_code_elimination`

**Cause:** integer `/` and `%` lower to Cranelift `sdiv`/`srem`, which trap on a zero
divisor and on `MIN / -1`. DCE removed them whenever the result was unused, deleting
an observable fault. This is the same trap-preservation rule LICM already follows.

**Fix:** new `may_trap` predicate guards DCE. Float division is unaffected — it
produces inf/NaN instead of faulting, so it stays removable.

### 2.4 A detection-only pass reported itself as a transformation — P1, false fixed-point

**Where:** `src/optimizer/loops.rs`, `optimize_loops` / `analyze_vectorization`

**Cause:** `analyze_vectorization` returned the count of loops it *detected* as
vectorization candidates, and `optimize_loops` added that to `transformed`. The
driver therefore believed the function had changed and re-ran every pass until the
iteration cap — re-emitting LICM and SIMD traces for work performed once. This
directly contradicts the rule the project adopted for pipeline fusion.

**Fix:** `analyze_vectorization` now returns `()` and cannot contribute to
`transformed`. Its signature states that it is detection-only.

### 2.5 Float output silently lost precision — P0, user-visible data corruption

**Where:** `src/runtime/datara_runtime.c`, `datara_rt_out_float`

**Cause:** `printf("%g")` keeps **six significant digits** and switches to scientific
notation at large exponents.

```
Datara: 2.5e+17
Rust:   249999999134217800     <- same value
```

Every `Float` needing more than six significant digits was corrupted on output.

**Fix:** print the shortest decimal string that round-trips back through `strtod`,
the way Rust and Python display `f64`. NaN and infinity get explicit spellings.
Verified: Datara now prints `2.499999991342178e+17`, matching Rust bit for bit.

**Process note:** the runtime is prebuilt and checked in (there is no `build.rs`), so
editing the C source does nothing until the object is rebuilt. Added
`scripts/build_runtime.bat` for this and rebuilt `datara_runtime.obj`.

---

## 3. Known issue not fixed

**`src/codegen/cranelift/backend.rs:1336`** hardcodes
`d:\DATARA\datara + forgen\src\runtime\datara_runtime.obj`. The project cannot be
built from a checkout on any other path or machine. It should be resolved relative
to `CARGO_MANIFEST_DIR` (or better, compiled by a `build.rs` via the `cc` crate, which
would also remove the need to check in a binary object). Left alone because it is a
build-system change, not a codegen change, and deserves its own commit.

---

## 4. Dead code removed

| Item | Rationale |
|---|---|
| `ScalarOptimizer::propagate_constants` | zero call sites, zero tests |
| `optimizer::layout::MemoryLayoutAnalyzer` (+ module) | produced a `StructLayoutPlan` nothing consumed; `optimized_offset` always equalled `original_offset` |
| `optimizer::adaptive::AdaptiveCostModel` | produced plan strings ("Plan B: AVX2 256-bit") nothing consumed |
| `optimizer::adaptive::LayoutAdapter` | produced a layout plan nothing consumed |
| `Optimizer::remap_inst` | dead after the inliner rewrite |
| 2 tests | exercised only the removed planners |

These modules are the mechanism by which the project came to claim SIMD and layout
optimization it never performed: a planner produced a confident string, a test
asserted the string, and no code was ever emitted. Their tests passing was worse than
no tests, because it looked like evidence.

Net: 48 source files → 45. Behaviour is unchanged; nothing in the optimizer or
backend called any of it.

---

## 5. Benchmarking

### 5.1 The previous harness was measuring nothing

`bench_honest/time.sh` timed whole processes from a Git Bash shell, so it measured
MSYS process-spawn overhead rather than the workload:

| Program | Reported | Truth |
|---|---|---|
| `empty.exe` (empty `main`) | 3,558 ms | microseconds of work |
| `scale_1000000` (1e6 iterations) | 3,743 ms | ~0.3 ms of work |
| `scale_2000000000` (2e9 iterations) | 3,588 ms | ~600 ms of work |
| `sleeper.exe` control, sleeps 300 ms | 3,891 ms | 300 ms |

Fifty times more work produced the same number, and the harness's own validation
control was off by 3.6 seconds. **Every number in `results*.txt` is invalid** — not
merely exploratory. They should not be cited.

### 5.2 Replacement

`bench_honest/harness.py` plus four matched Datara/Rust workload pairs in
`bench_honest/workloads/`:

- times the **kernel inside the process** (`now_ms()` / `Instant`), so startup is excluded
- derives trip counts from the wall clock, so no compiler can fold the loop away
- prints a checksum and **fails the run** if the two languages disagree
- reports min / median / mean, startup, compile time and binary size
- flags when one compiler eliminates a kernel instead of printing a meaningless ratio
- dumps raw measurements as JSON

### 5.3 Results — 9 timed runs, median, this machine

| Workload | Datara | Rust `-O` | Verdict |
|---|---|---|---|
| `float_loop` (500M) | 307 ms | 346 ms | **0.89x — Datara faster** |
| `int_loop` (500M) | 205 ms | ~0 ms | rustc reduces the loop to a closed form |
| `point_sroa` (200M) | 82 ms | ~0 ms | rustc reduces the loop to a closed form |
| `box_generic` (200M) | 81 ms | ~0 ms | rustc reduces the loop to a closed form |

4/4 checksums verified, so Datara computes the right answers.

Compile time: **forgen 254–428 ms vs rustc 736–759 ms** (forgen is ~2.5x faster).
Binary size: **~9.7 KB vs ~163 KB**.

### 5.4 What this actually shows

The honest headline is not "Datara is 11% faster than Rust". It is:

- **On work both compilers actually execute, Datara's scalar loop codegen is
  competitive with LLVM — within ~1x.** That is a genuinely good result for a
  prototype and better than the earlier fabricated reports claimed.
- **The real gap is a class of optimization Forgen does not have at all**: LLVM
  recognises `sum += i` over `0..n` and replaces the entire loop with a closed-form
  expression. Forgen runs all 500M iterations. On reducible integer kernels this is
  an unbounded gap, and no amount of instruction-level tuning closes it.

---

## 6. Verification commands

Run from `D:\DATARA\datara + forgen`. Note `cargo` is not on `PATH`; use
`"$HOME/.cargo/bin/cargo.exe"`.

```bash
"$HOME/.cargo/bin/cargo.exe" fmt --all -- --check
"$HOME/.cargo/bin/cargo.exe" check --all-targets
"$HOME/.cargo/bin/cargo.exe" test --release -j 2
```

Benchmarks:

```bash
"$HOME/.cargo/bin/cargo.exe" build --release
python bench_honest/harness.py --runs 9 --json bench_honest/results_honest.json
```

Runtime changes require rebuilding the object before they take effect:

```bat
scripts\build_runtime.bat
```
