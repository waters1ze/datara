# Datara Canonical Semantic Contract

## 1. Scope and Architectural Purpose

This document defines the **Canonical Semantic Contract** of the **Datara** programming language and the **Forgen** compiler.
It establishes the formal specification for type semantics, ownership, effects, runtime behavior, and permitted vs forbidden compiler transformations.

---

## 2. Core Language Constructs Specification

### 2.1 Bindings: `let`, `mut`, `const`
- **Syntax**: `x = expr`, `mut x = expr`, `const X = literal`
- **Type Semantics**: Strongly typed, statically inferred or explicitly annotated (`x Int = 10`).
- **Ownership Semantics**:
  - `x = expr` creates an immutable owner. Reassignment is a compile-time error (`E-BORROW-002`).
  - `mut x = expr` creates a mutable owner. Reassignment is permitted only when no active borrows exist.
  - `const X = literal` evaluates at compile-time with zero runtime storage footprint.
- **Permitted Transformations**: Dead binding elimination, scalar replacement of aggregates (SROA), constant propagation.
- **Forbidden Transformations**: Mutating an immutable binding under any optimization level.

### 2.2 Functions: `fn`
- **Syntax**: `fn name(params) -> ReturnType { body }` or `fn name(params) -> ReturnType => expr`
- **Type Semantics**: Static monomorphic signature or parametric generic `<T>`.
- **Effect Semantics**: Inferred through whole-body effect algebra. Purity (`Pure`) requires zero IO, zero mutation of external state, and deterministic execution.
- **Permitted Transformations**: Inlining pure leaf functions within cost-model budget, reordering pure calls.
- **Forbidden Transformations**: Eliminating functions with observable `IO`, `Network`, or `Database` side effects.

### 2.3 Object Model: `class` and Zero-Cost OOP
- **Syntax**: `class Name<T> { field Type }`
- **Semantics**: Value semantics by default. Objects represent semantic data layouts without default heap boxing.
- **Method Resolution**: Methods are statically dispatched free functions taking `this` as the first parameter (`Receiver`).
- **Permitted Transformations**: SROA (Scalar Replacement of Aggregates) flattening non-escaping local struct instances onto the stack, field dead code elimination.
- **Forbidden Transformations**: Introducing dynamic vtables or heap headers for statically resolvable class instances.

### 2.4 Modular Behaviors: `behavior for Class`
- **Syntax**: `behavior ModuleName for ClassName { fn method() { ... } }`
- **Semantics**: Split-behavior extensions. Methods declared in a behavior are logically attached to `ClassName` in the module namespace.
- **Domain Reachability Semantics**: If a behavior's methods are unreachable from the application entry point (`main`), the entire behavior, its methods, and its class interface projections **must be pruned** from the final binary.

### 2.5 Pattern Matching & Control: `decide`
- **Syntax**: `decide { condition => result, else => default }`
- **Semantics**: Deterministic multi-way branch evaluation with exhaustive fallback.
- **Permitted Transformations**: Branch elimination when conditions are statically known constants, conversion to jump tables or conditional select instructions.

### 2.6 Dataflow Pipelines: `|>`
- **Syntax**: `x |> f |> g`
- **Semantics**: Strict left-to-right evaluation equivalent to `g(f(x))`. Preserves intermediate ownership transitions and effect sequencing.

---

## 3. Effect System Rules for Optimizer Passes

| Effect Category | Reordering Allowed? | Inlining Allowed? | Dead Call Elimination Allowed? | Constant Folding Allowed? |
| :--- | :--- | :--- | :--- | :--- |
| **`Pure`** | **Yes** (Arbitrary) | **Yes** (Cost model gated) | **Yes** (If result unused) | **Yes** |
| **`Read`** | **Yes** (Across pure ops) | **Yes** | **Yes** (If state unobserved) | **No** (Runtime dynamic) |
| **`Write`** | **No** (Preserve sequence) | **Yes** | **No** | **No** |
| **`IO`** | **No** (Strict order) | **Yes** (Preserve side-effects) | **FORBIDDEN** | **FORBIDDEN** |
| **`Network`** | **No** (Strict boundary) | **Yes** | **FORBIDDEN** | **FORBIDDEN** |
| **`Database`** | **No** (Transaction order) | **Yes** | **FORBIDDEN** | **FORBIDDEN** |
| **`Unsafe`** | **Gated by proof** | **Gated** | **FORBIDDEN** | **FORBIDDEN** |
| **`Parallel`** | **Fork-join semantics** | **Yes** | **Yes** (If pure) | **Gated** |
| **`Nondeterministic`** | **No** | **Yes** | **FORBIDDEN** | **FORBIDDEN** |

---

## 4. Single Source of Semantic Truth (SSOT)

1. **AST is Syntactic Only**: The Parser constructs structural trees with zero semantic resolution.
2. **Resolver & TypeChecker Build Truth**: The Symbol Table, Type Environment, Effect Lattice, and Ownership Model constitute the canonical semantic truth.
3. **DMIR is a Pure Lowering**: DMIR reflects the typed semantic model in Static Single Assignment (SSA) form without inventing new semantic concepts.
4. **Optimizer Must Preserve Observability**: No optimization pass may alter program semantics, panic conditions, or observable IO.
