# Datara + Forgen: Semantic Adaptation Engine (SAE) & Adaptive Execution Ladder

**Status**: **APPROVED & IMPLEMENTED ARCHITECTURE**  
**Core Philosophy**: **User declares WHAT ($\text{Semantic Intent}$) — Forgen decides HOW ($\text{Physical Representation & Strategy}$)**

---

## 1. Executive Vision & Core Philosophy

In traditional systems languages (C, C++, Rust), developers are forced to make premature low-level physical choices in their source code:
- Should this collection be Vec<Point> (AoS) or Points (SoA)?
- Should this calculation run on a thread pool or a sequential SIMD loop?
- Should this aggregate be allocated on the heap (Box<T>) or the stack?
- Should this pipeline use an intermediate buffer or a complex hand-fused iterator?

**Datara breaks this tradeoff.** 

The developer expresses clean, high-level **Semantic Intent**. The **Semantic Adaptation Engine (SAE)** in Forgen automatically deduces the optimal physical representation and machine execution strategy after multi-dimensional static and profile analysis:

\begin{matrix}
\boxed{\text{\textbf{Semantic Intent (WHAT)}}} \\
\text{\texttt{users \|> filter(.active) \|> map(.score) \|> reduce(sum)}}
\end{matrix}
\quad \xrightarrow[\text{Types, Effects, Ownership, Graph, PGO, Cost Model}]{\text{\textbf{Semantic Adaptation Engine (SAE)}}} \quad
\begin{matrix}
\boxed{\text{\textbf{Physical Execution (HOW)}}} \\
\begin{cases}
\text{Single Fused Loop} & (\text{Sequential stream, 0 allocations}) \\
\text{AVX2 SIMD Vectorization} & (10\text{K} \le N < 500\text{K}) \\
\text{Parallel ThreadPool} & (N \ge 500\text{K}, \text{multi-core pure})
\end{cases}
\end{matrix}

---

## 2. Four Pillars of Semantic Adaptation

### 2.1. Representation Adaptation (src/optimizer/adaptive/representation.rs)
- **Scalar SSA vs Aggregate**: Non-escaping structures (Point, Box<T>) are scalarized directly into virtual CPU registers (SROA), eliminating 100% of heap allocations in local scopes.
- **Stack Local vs Heap Managed**: Aggregates passed to leaf calls with bounded lifetimes are placed on the native stack frame rather than invoking malloc.
- **AoS vs SoA vs AoSoA**: Large collections with sparse column access (field selectivity $\le 35\%$) are transformed to Struct-of-Arrays (SoA) for maximum SIMD cache bandwidth.

### 2.2. Execution Strategy Adaptation (src/optimizer/adaptive/execution.rs)
- **Sequential Scalar**: Small iteration bounds ( < 10,000$) run on a zero-overhead sequential unrolled loop, avoiding thread synchronization penalties.
- **SIMD Vectorization**: Independent numerical loops (,000 \le N < 500,000$) emit 4-lane or 8-lane SIMD vector instructions.
- **Parallel Thread Pool**: Large, pure data sets ( \ge 500,000$) automatically split into chunked tasks across available CPU worker threads.
- **Async Task Reactor**: Loops containing blocking I/O or network operations adapt to non-blocking asynchronous event loops.

### 2.3. Data Layout & Packing Adaptation (src/optimizer/adaptive/layout.rs)
- **Alignment Hole Elimination**: Reorders struct fields descending by alignment requirement (8-byte pointers/integers $\to$ 4-byte floats/ints $\to$ 1-byte booleans) to eliminate internal struct padding holes, reducing memory footprint by up to 30%.

### 2.4. Strategy & Call Dispatch Adaptation (src/optimizer/adaptive/strategy.rs)
- **Pipeline Fusion**: Fuses multi-stage iterator pipelines (ilter -> map -> fold) into single-pass streaming loops with zero intermediate buffer allocations.
- **Devirtualization & Inline Caching**:
  - 1 concrete implementer: Direct inlined static call.
  - $\le 3$ implementers: Guarded polymorphic inline cache (PIC).
  - $> 3$ implementers: Dynamic vtable dispatch preserved.

---

## 3. Structured Decision Trace Specification

Every decision made by the SAE is logged with rigorous mathematical cost-benefit evidence:

`json
{
  category: Representation,
  candidate: compute:var_v14,
  decision: PromoteToScalarSSA,
  cost: 0.0,
  benefit: 30.0,
  reason: Aggregate does not escape lexical scope and has disjoint field access,
  evidence: Escape analysis proved non-escaping for Point with 2 fields
}
`

Developers and AI tooling can inspect these decisions via:
`ash
forgen sae main.dtr
forgen sae main.dtr --json
`

---

## 4. The 5-Tier Adaptive Compilation Ladder

To balance ultra-fast developer iteration against maximum production optimization, Forgen provides a 5-tier execution ladder:

`mermaid
graph TD
    A[Datara Source] --> B{Forgen Command}
    B -->|forgen check| C[1. Static Check: 0 Binaries, &le; 3ms]
    B -->|forgen quick / run| D[2. Quick Mode: Fast Compile + Incremental Cache]
    B -->|forgen debug| E[3. Debug Mode: Full Diagnostics + Debug Symbols]
    B -->|forgen release| F[4. Release Mode: SROA + Inlining + LICM]
    B -->|forgen domain| G[5. Domain Mode: Whole-Project SAE + PGO + LTO]
`

### 1. orgen check
- **Purpose**: Instant verification for IDEs, linter hooks, and pre-commit checks.
- **Pipeline**: Lexer $\to$ Parser $\to$ Minimal Resolver $\to$ TypeChecker $\to$ Effects $\to$ Ownership.
- **Output**: 0 binaries generated; diagnostics in $\le 3$ ms.

### 2. orgen quick / orgen run
- **Purpose**: Instant turnaround for rapid experimentation.
- **Pipeline**: Minimal pipeline $\to$ Cranelift $\to$ Native Executable.
- **Adaptive Caching**: If executable exists and source has not changed, launches the cached binary immediately (0 ms recompile).

### 3. orgen debug
- **Purpose**: Step-through debugging and panic diagnostic tracing.
- **Pipeline**: Unoptimized IR preservation with full variable maps and stack frames.

### 4. orgen release
- **Purpose**: High-performance production builds.
- **Pipeline**: SROA + Inlining + Constant Folding + Dead Code Elimination + LICM + Native PE binary.

### 5. orgen domain
- **Purpose**: Maximum whole-program optimization.
- **Pipeline**: Whole-project semantic graph $\to$ Reachability stripping $\to$ Deep SAE adaptation $\to$ PGO specialization $\to$ Minimal runtime stripping $\to$ Native PE binary.
