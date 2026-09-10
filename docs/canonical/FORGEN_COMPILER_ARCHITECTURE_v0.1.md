# FORGEN COMPILER ARCHITECTURE v0.1

**Compiler:** Forgen  
**Source language:** Datara  
**Status:** architecture specification draft v0.1

---

# 0. MISSION

Forgen is the core competitive technology of Datara.

Its purpose is not simply:

```text
Datara source → LLVM IR
```

Its purpose is:

> **understand the whole semantic program, prove what can be proven, remove what is unnecessary, specialize what is known, choose a representation suited to the target, and generate a minimal efficient artifact without violating safety or observable semantics.**

---

# 1. PRIMARY DESIGN OBJECTIVES

Forgen MUST optimize for five dimensions simultaneously:

```text
1. runtime performance
2. compile-time UX
3. memory efficiency
4. safety/verification
5. explainability
```

The optimizer is not allowed to improve one dimension blindly while destroying the others.

---

# 2. PERFORMANCE TARGET

Project aspiration:

```text
Datara runtime ≈ Rust runtime
```

A 1–2% gap on comparable optimized workloads is considered acceptable.

A smaller gap is preferred.

Equal or better performance is ideal.

This is not a promise. Each release is validated through benchmark suites.

---

# 3. COMPILER PIPELINE

Canonical architecture:

```text
Source
  ↓
Lexer
  ↓
Parser
  ↓
DAST
  ↓
Resolver
  ↓
Type Inference / Checking
  ↓
Effect Analysis
  ↓
Ownership / Borrow Analysis
  ↓
Pattern / Control Flow Analysis
  ↓
Semantic Graph
  ↓
HIR
  ↓
DMIR
  ↓
Specialization Engine
  ↓
Optimization Engine
  ↓
Target Lowering
  ↓
Native / LLVM / Future Custom Backend
  ↓
Linker
  ↓
Runtime Minimization
  ↓
Artifact
```

The number of physical IR layers may evolve; the semantic boundaries MUST remain.

---

# 4. FRONTEND

Frontend packages:

```text
lexer
parser
source maps
syntax diagnostics
AST builder
formatter integration
```

Goals:

```text
very fast parsing
stable error recovery
excellent source locations
```

---

# 5. DAST

Datara AST preserves source constructs:

```text
class
behavior
component
role
flow
pipeline
match
decide
select
```

This allows tooling to preserve programmer intent before lowering.

---

# 6. NAME RESOLUTION

Resolver builds symbol identities.

It must resolve:

```text
modules
imports
classes
behaviors
roles
components
functions
generic parameters
locals
fields
```

Resolution errors are emitted before expensive optimization.

---

# 7. TYPE SYSTEM

The type checker validates:

```text
assignments
calls
generics
roles
operators
patterns
pipelines
ownership-sensitive APIs
units
```

Type inference MUST generate a complete semantic type environment.

---

# 8. WHY COMPACT SYNTAX DOES NOT HURT OPTIMIZATION

Input:

```dtr
name String
```

becomes internally:

```text
FieldSymbol(name, String, flags...)
```

The optimizer never works directly with source spelling.

It works with:

```text
types
layout facts
ownership
effects
usage
```

Therefore:

```text
name String
```

and:

```text
name: String
```

have identical optimization potential.

---

# 9. EFFECT ENGINE

Effects are inferred from the call graph.

Example:

```text
add → Pure
save → IO + Database
send → IO + Network
```

Effect information is attached to graph nodes and edges.

Uses:

```text
reordering safety
parallelism
memoization
constant evaluation
AI context
```

---

# 10. OWNERSHIP ENGINE

The ownership engine is the safety core.

It builds proofs for:

```text
owner existence
borrow lifetime
aliasing
mutation exclusivity
move validity
data race absence
escape behavior
```

The user-facing syntax is intentionally smaller than the internal proof system.

---

# 11. BORROW INFERENCE

Example:

```dtr
fn length(data view Str) -> Int { ... }

text := loadText()!
length(text)
```

Compiler determines that `text` must outlive the borrow.

No lifetime annotation is required.

If compiler cannot prove safety, compilation fails or requests explicit ownership information depending on profile.

---

# 12. EXPLICIT OWNERSHIP FALLBACK

```dtr
fn inspect(data view Data)
fn edit(data mut-view Data)
fn consume(data own Data)
fn share(data shared Data)
```

The explicit form is a fallback and an expert escape hatch, not the common case.

Compiler MAY emit warning when explicit annotation duplicates inferred semantics.

---

# 13. UNSAFE ENGINE

Unsafe regions are represented as explicit graph nodes.

Each unsafe region records:

```text
source span
reason
foreign calls
raw memory operations
invariants required
```

`forgen inspect safety` prints a complete unsafe inventory.

---

# 14. SEMANTIC GRAPH — HEART OF FORGEN

Graph nodes:

```text
Module
Symbol
Type
Function
Call
Class
Behavior
Role
Component
Flow
Task
Effect
Allocation
Memory region
Resource
ABI boundary
Target constraint
Profile fact
```

Edges:

```text
imports
calls
contains
implements
composes
reads
writes
borrows
owns
produces
consumes
may-parallelize-with
specializes-from
```

---

# 15. GRAPH GRANULARITY

The graph should support:

```text
package level
module level
symbol level
block level
operation level
```

This allows incremental compilation and AI context extraction without requiring a full repository dump.

---

# 16. FILES ARE NOT OPTIMIZATION BOUNDARIES

Suppose:

```text
model.dtr
kernel.dtr
tensor.dtr
main.dtr
```

Forgen may inline from `kernel.dtr` into `main.dtr` if allowed.

Filesystem organization is for humans and incremental invalidation, not runtime representation.

---

# 17. HIR

HIR represents semantically valid Datara constructs.

It should contain:

```text
resolved types
normalized ownership
normalized effects
control flow
canonical calls
canonical composition
```

At HIR, surface aliases such as `fn/function`, `field: Type/field Type` no longer matter.

---

# 18. FLOW IR

Pipeline and `flow` are lowered into explicit dataflow blocks:

```text
Input
 ↓
Op A
 ↓
Op B
 ↓
Branch
 ↓
Merge
```

Each op contains:

```text
input values
output values
effects
resource facts
```

---

# 19. DMIR

DMIR is machine-independent optimized IR.

Desired characteristics:

```text
SSA-like value model
explicit memory effects
explicit control flow
typed operations
ownership/lifetime metadata
vectorizable loops
parallel regions
```

DMIR is where Datara-specific optimizations live before target lowering.

---

# 20. SPECIALIZATION ENGINE

Specialization is a first-class compiler subsystem.

Inputs:

```text
actual types
actual generic instantiations
constant values
profile facts
hardware features
resource intents
reachable graph
```

Outputs:

```text
specialized functions
generic elimination
constant propagation
dead path removal
layout choices
```

---

# 21. WHOLE-PROGRAM REACHABILITY

Starting from entry points:

```text
main
exported runtime entry points
FFI-required symbols
reflection-required symbols
interrupt handlers
```

Forgen traces reachable graph.

Everything else becomes a removal candidate.

---

# 22. DEAD CODE ELIMINATION

Three levels:

```text
dead statement
dead function
dead module
```

Runtime modules can also become dead.

---

# 23. DEAD DATA ELIMINATION

Metadata, fields and tables can be removed if not observable.

Especially useful for:

```text
reflection-disabled builds
embedded
minimal CLI tools
AI inference-only applications
```

---

# 24. INLINING

Forgen uses cost model, not blanket inlining.

Inputs:

```text
call frequency
function size
code growth
branch structure
cache pressure
profiling
```

Hot small functions are likely inline candidates.

Large cold functions should stay out-of-line.

---

# 25. DEVIRTUALIZATION

If a role call resolves to a concrete type:

```text
role Renderer
 ↓
concrete VulkanRenderer
```

Forgen can replace indirect dispatch with direct call.

If runtime dynamic dispatch is genuinely required, retain indirect dispatch.

---

# 26. ALLOCATION ELIMINATION

Escape analysis checks:

```text
allocated?
returned?
stored?
shared?
passed to unknown FFI?
```

If safe, allocation may be:

```text
stack promoted
scalar replaced
arena allocated
eliminated
```

---

# 27. SCALAR REPLACEMENT

A small class:

```dtr
class Point {
    x Float
    y Float
}
```

may become:

```text
x SSA value
y SSA value
```

No object necessarily exists physically.

---

# 28. DATA LAYOUT OPTIMIZATION

For hot collections, Forgen may choose:

```text
AoS
SoA
AoSoA
packed representation
```

Only if semantics/ABI permit.

This is one of the mechanisms intended to make high-level class syntax compatible with high-performance data processing.

---

# 29. LOOP FUSION

Pipeline:

```dtr
values
    |> map(f)
    |> filter(g)
    |> reduce(sum)
```

can become one loop:

```text
for value:
  y = f(value)
  if g(y): sum += y
```

without intermediate arrays.

---

# 30. LOOP FISSION

Sometimes splitting one loop improves:

```text
cache locality
SIMD
parallelism
branch behavior
```

Forgen may perform fission when cost model says so.

---

# 31. VECTORIZATION

Prerequisites include:

```text
loop independence
alias safety
operation support
trip-count knowledge
alignment information
```

Compiler may generate SIMD code for supported target ISA.

---

# 32. MULTIVERSIONING

Possible versions:

```text
baseline
AVX2
AVX-512
NEON
RISC-V vector
```

Dispatch method depends on target profile.

For embedded targets, static target selection is preferred over runtime dispatch.

---

# 33. BOUNDS CHECK ELIMINATION

If analysis proves safe index range, checks may disappear.

If proof fails, check remains.

Unsafe mode can expose lower-level APIs, but safe mode never silently disables a required check.

---

# 34. BUFFER REUSE

For data/tensor pipelines Forgen computes:

```text
buffer lifetime
last use
alias sets
mutation state
shape
size
```

Then it can reuse storage.

---

# 35. MEMORY PLANNER

Domain data workloads may use a memory planning pass.

Goal:

```text
minimize peak live memory
minimize allocations
improve cache locality
```

Especially valuable for tensor workloads.

---

# 36. GENERIC OPTIMIZATION

Strategy selected by cost model:

```text
monomorphize
share body
specialize hot instantiation
specialize constant parameter
```

The user does not choose manually in normal source code.

---

# 37. CLOSURE OPTIMIZATION

For closure:

```dtr
factor := 2
values |> map(x => x * factor)
```

Forgen may:

```text
inline closure
turn capture into scalar
remove environment
fuse map into loop
```

Potentially the final code contains no closure object.

---

# 38. DECIDE OPTIMIZATION

`decide` supplies ordered guards.

Forgen may choose:

```text
branch chain
balanced decision tree
jump table
bit-test
branchless select
```

Only if evaluation semantics permit reorganization.

This is a language feature that exists specifically to give the compiler a clear decision graph rather than hiding intent in nested statements.

---

# 39. MATCH OPTIMIZATION

For finite enums/sums:

```text
jump table
switch
range checks
decision tree
```

can be selected automatically.

---

# 40. PARALLELIZATION ENGINE

Input:

```text
dependency graph
effects
ownership
work estimate
```

Possible outputs:

```text
sequential
parallel tasks
work stealing
thread pool
SIMD
GPU kernel
```

Compiler MUST consider overhead.

---

# 41. COST MODEL

Cost model inputs:

```text
CPU ISA
cache sizes
SIMD width
memory bandwidth
branch predictability
call frequency
estimated trip count
input size
allocation cost
thread overhead
GPU launch cost
transfer cost
binary size
startup latency
power budget
```

No blanket `all optimizations enabled` logic.

---

# 42. PROFILE-GUIDED OPTIMIZATION

`forgen profile` collects:

```text
hot functions
branch counts
call frequency
allocation hotspots
input distributions
```

`forgen domain` can consume these facts.

Profile facts are hints, not semantic authority.

---

# 43. PGO SAFETY

PGO cannot change semantic correctness.

A wrong profile can only choose a less-optimal strategy, not a different program meaning.

---

# 44. TARGET DETECTION

Forgen has target model:

```text
architecture
ISA extensions
cache hierarchy
OS
ABI
runtime
accelerators
```

Target feature information is immutable during a single codegen session.

---

# 45. DEVICE COST MODEL

For CPU/GPU selection, Forgen compares:

```text
compute cost
transfer cost
launch cost
memory cost
parallelism
```

GPU is not assumed faster simply because it exists.

---

# 46. RUNTIME MINIMIZATION

Runtime is modular.

Graph reachability includes runtime components.

If only `out` is used, networking runtime MUST NOT be linked.

If no dynamic reflection exists, reflection metadata MAY be omitted.

---

# 47. START MODE

```bash
forgen start
```

Optimization:

```text
local/moderate
```

Compiler behavior:

```text
incremental cache
parallel module compilation
fast codegen
readable diagnostics
```

Goal: edit → rebuild → run quickly.

---

# 48. DEBUG MODE

```bash
forgen debug
```

Priorities:

```text
diagnostics
source mapping
ownership visibility
bounds diagnostics
runtime checks
verifier information
```

Optimization is allowed but must remain debug-friendly.

---

# 49. RELEASE MODE

```bash
forgen release
```

High optimization without requiring the full cost of whole-program specialization.

---

# 50. DOMAIN MODE

```bash
forgen domain
```

Maximum safe specialization.

Steps:

```text
1. load all project semantics
2. construct whole-program graph
3. resolve reachable paths
4. specialize generics
5. propagate constants
6. derive effects
7. finalize ownership proofs
8. choose layouts
9. optimize flows
10. vectorize
11. analyze parallelism
12. eliminate allocations
13. strip runtime
14. LTO
15. target lower
16. verify artifact
```

---

# 51. DOMAIN IS NOT A BAG OF FLAGS

The user should not need:

```text
-O3
-flto
-funroll
-fsimd
-fwhatever
```

The compiler owns that strategy.

User gives intent/constraints.

Compiler chooses implementation.

---

# 52. INTENT ENGINE

Example:

```dtr
intent {
    performance maximum
    memory minimum
    latency <= 2ms
    deterministic true
}
```

The intent engine translates constraints into optimizer objective weights.

Example:

```text
latency high priority
memory high priority
code size medium
throughput medium
```

---

# 53. CONSTRAINT PROOF

For every strict intent, compiler tracks:

```text
proven
estimated
unknown
violated
```

Build result should say:

```text
latency <= 2ms
status: not proven
```

rather than pretending it was certified.

---

# 54. SEMANTIC VERIFIER

Every major optimization pass has:

```text
preconditions
transform
postconditions
```

IR verifier checks:

```text
types
SSA
control flow
memory invariants
ownership
effects
ABI
```

---

# 55. OPTIMIZATION PASS CONTRACT

Conceptual API:

```text
PassInput
PassAnalysis
Transform
Verifier
CostEstimate
Explanation
```

Each pass must be explainable.

---

# 56. PASS INVALIDATION

If a transformation invalidates cached analysis, Forgen must invalidate exactly the affected facts.

This enables efficient optimization pipelines and incremental builds.

---

# 57. INCREMENTAL COMPILATION

Cache key should include:

```text
source hash
dependency interface hash
compiler version
target
profile
relevant semantic facts
```

Changing a cold behavior file should not necessarily invalidate unrelated modules.

---

# 58. PARALLEL COMPILATION

Independent modules compile concurrently.

Optimizer can parallelize independent regions when pass ordering permits.

---

# 59. IR CACHE

Potential cached artifacts:

```text
AST
resolved names
typed HIR
semantic graph slices
DMIR
codegen artifacts
profile facts
```

Cache must be content-addressed and compiler-version aware.

---

# 60. DOMAIN CACHE

Domain builds are expensive.

Forgen should reuse:

```text
unchanged semantic graph slices
unchanged target analyses
previous profile data
precompiled library IR
```

when validity conditions are satisfied.

---

# 61. AI CONTEXT SERVICE

Command:

```bash
forgen context --symbol User.checkout
```

Output model:

```text
symbol
source span
signature
types
effects
ownership
dependencies
callers
callees
unsafe
performance hints
tests
```

This is a semantic API for IDEs and AI agents.

---

# 62. AI SEMANTIC PATCH

AI patch can be represented as:

```text
add behavior User.billing
add dependency Payments
no public API break
new effect Network
unsafe unchanged
```

Forgen validates it before build acceptance.

---

# 63. AI TEST SUPPORT

Because Forgen knows:

```text
types
roles
effects
states
error variants
```

tooling can generate targeted tests without parsing the entire repository as plain text.

---

# 64. AI OPTIMIZATION LOOP

Recommended workflow:

```text
AI proposes implementation
        ↓
Forgen checks
        ↓
Forgen benchmarks/profile
        ↓
Forgen explains costs
        ↓
AI receives semantic result
        ↓
AI improves source
```

The compiler is an evaluator, not merely a syntax gate.

---

# 65. DIAGNOSTIC ENGINE

Diagnostic object:

```text
code
severity
source range
human message
explanation
suggestion
related locations
machine payload
```

Localization is presentation-layer only.

---

# 66. RUSSIAN ERROR SUPPORT

Initial intended locales:

```text
en
ru
```

English internal error IDs remain stable.

Example:

```text
DTR-BORROW-004
Невозможно изменить `user.name`: значение сейчас заимствовано для чтения.
```

The machine payload remains language-neutral.

---

# 67. ERROR QUALITY

Good diagnostic should answer:

```text
what happened?
where?
why?
how to fix?
```

Compiler should avoid Rust-style lifetime diagnostics that are technically correct but inaccessible; ownership explanations must use semantic terms familiar to Datara.

---

# 68. CLI OUTPUT ENGINE

`out` and `err` map to optimized runtime sinks.

Forgen can specialize formatting based on static types.

For hot output it can use:

```text
buffered writes
pre-sized formatting
specialized numeric formatting
zero-copy string views
```

---

# 69. EMBEDDED BACKEND

Embedded profile needs:

```text
no mandatory allocator
no mandatory OS
linker script integration
startup code
interrupt model
MMIO
volatile semantics
peripheral drivers
stack limits
```

No heavy runtime by default.

---

# 70. INTERRUPT CONTEXT

Future source form:

```dtr
interrupt Timer0 {
    tick()
}
```

Compiler rules should enforce:

```text
no blocking
bounded work
controlled allocation
interrupt-safe APIs
```

Hard guarantees only if the backend can prove required properties.

---

# 71. INDUSTRIAL RESOURCE ANALYSIS

Forgen SHOULD support resource facts:

```text
stack usage
heap usage
RAM peak
CPU estimate
latency bound
power estimate
```

The compiler may report:

```text
RAM budget: proven
latency budget: estimated
```

---

# 72. DETERMINISTIC BUILD

`embedded` and deterministic profiles should support:

```text
reproducible ordering
stable binary metadata
controlled floating-point mode
controlled scheduler behavior
```

---

# 73. LINKER STAGE

Linker tasks:

```text
symbol resolution
section layout
dead stripping
runtime component inclusion
relocation
debug symbol management
```

Domain should provide linker with maximum reachability information.

---

# 74. SINGLE BINARY

Native CLI/app builds SHOULD default to a single executable where platform norms permit.

Exceptions:

```text
plugins
dynamic libraries
system frameworks
GPU driver dependencies
```

---

# 75. PACKAGE INTEROPERABILITY

Forgen must support package boundaries that preserve enough metadata for:

```text
type checking
public API
semantic contracts
target support
unsafe surface
```

---

# 76. C ABI

C ABI is the baseline FFI target because it is widely available and conceptually stable.

C interop creates:

```text
opaque boundary
potential aliasing uncertainty
potential external side effects
```

Forgen must treat unknown FFI calls conservatively.

---

# 77. RUST INTEROP

Long-term strategy may support Rust via:

```text
C ABI
custom bridge generator
native library wrappers
```

A direct Rust source dependency is not required for core language semantics.

---

# 78. PYTHON INTEROP

Python integration is ecosystem bridge, not language foundation.

Recommended hot-path shape:

```text
Python
 ↓
bulk transfer
 ↓
Datara native computation
 ↓
bulk result
 ↓
Python
```

Avoid millions of tiny interpreter boundary crossings.

---

# 79. LIBRARY SEMANTIC CONTRACTS

A Datara library may declare machine-readable properties:

```text
pure
no-alloc
read-only
parallel-safe
vectorizable
shape-preserving
```

Forgen can use them.

Safety-critical contracts must be independently validated.

---

# 80. STANDARD OPTIMIZATION PASSES

At minimum:

```text
constant folding
constant propagation
dead code elimination
dead data elimination
copy propagation
common subexpression elimination
inlining
devirtualization
generic specialization
closure elimination
escape analysis
allocation elimination
scalar replacement
bounds elimination
loop fusion
loop fission
loop invariant code motion
vectorization
SIMD lowering
parallel analysis
buffer reuse
data layout optimization
cross-module optimization
LTO
PGO
```

---

# 81. ADVANCED OPTIMIZATIONS ROADMAP

Future research:

```text
autonomous tiling
cache-aware scheduling
profile-guided layout
speculative specialization
adaptive runtime variants
compile-time query planning
auto batching
CPU/GPU co-scheduling
power-aware code generation
```

These are optional research tracks, not v0.1 guarantees.

---

# 82. DATAFLOW OPTIMIZATION

Native Forgen IR understands pipelines as graphs.

That enables database-like optimizations:

```text
projection pruning
predicate pushdown
operator fusion
stream scheduling
memory planning
```

The same graph model can support data libraries without making them core language features.

---

# 83. TENSOR OPTIMIZATION HOOKS

Tensor library can expose compiler intrinsics/contracts:

```text
shape
stride
layout
dtype
device
contiguity
```

Forgen can then optimize library calls without owning the full AI domain model.

---

# 84. MODEL/AI REMOVAL FROM CORE

There is intentionally no mandatory `model` language keyword in v0.1.

Model code is library code.

Compiler only understands generic semantic contracts exposed by the library.

This prevents AI features from inflating the language core.

---

# 85. DOMAIN + LIBRARIES

A library can be compiler-aware without becoming syntax.

Example concept:

```text
ai.tensor
  ↓ semantic contracts
Forgen
  ↓
fusion / memory / vector / device optimization
```

This is the preferred extensibility mechanism.

---

# 86. BUILD GRAPH

Forgen maintains:

```text
source dependency graph
semantic dependency graph
artifact dependency graph
runtime dependency graph
```

Each graph serves a separate purpose.

---

# 87. CACHE INVALIDATION

A module should be rebuilt when relevant semantic contracts change.

A comment-only change should ideally not invalidate typed IR.

An internal implementation change should not always invalidate dependents if public semantic interface remains unchanged.

---

# 88. ABI STABILITY

Public package ABI may be versioned.

Internal Domain artifacts are free to specialize aggressively.

Thus:

```text
stable library boundary
+
unstable internal optimization
```

can coexist.

---

# 89. DEBUGGER INTEGRATION

Forgen debug metadata should preserve:

```text
source locations
semantic variable identity
class/behavior identity
flow stages
ownership state
```

Even if class abstraction is optimized away, debugger should show the source-level abstraction where meaningful.

---

# 90. OPTIMIZED DEBUGGING

Goal:

> optimized program, useful debugger.

This requires variable location tracking and source mapping rather than simply disabling optimization.

---

# 91. INSPECT COMMANDS

```bash
forgen inspect types
forgen inspect effects
forgen inspect memory
forgen inspect safety
forgen inspect dependencies
forgen inspect graph
forgen inspect optimize
forgen inspect target
forgen inspect runtime
```

---

# 92. OPTIMIZATION REPORT

Example:

```text
Forgen Domain Report

Modules analyzed: 84
Reachable symbols: 1320
Removed symbols: 912
Inlined calls: 231
Allocations removed: 184
SIMD loops: 37
Parallel transforms: 4
Generic specializations: 19
Runtime modules: 7
Binary size: 2.1 MB
```

---

# 93. OPTIMIZATION EXPLANATION

For a rejected optimization:

```text
parallelization: rejected
reason: estimated work < scheduling overhead
```

For a successful one:

```text
allocation eliminated
proof: object does not escape function
```

This is a critical product feature.

---

# 94. COMPILER SELF-PROFILING

Forgen should benchmark its own passes and expose:

```text
front-end time
analysis time
optimization time
codegen time
link time
cache hit rate
```

---

# 95. COMPILER PERFORMANCE TARGETS

The compiler must also be pleasant.

Desired hierarchy:

```text
start → very fast
release → reasonable
domain → expensive but justified
```

No language adoption will survive if every small edit launches a full whole-program optimizer.

---

# 96. PARALLEL DOMAIN BUILD

Independent graph slices are optimized concurrently.

Pass scheduler tracks dependencies between analysis results.

Workers should avoid duplicated memory-heavy IR copies where possible.

---

# 97. MEMORY EFFICIENCY OF THE COMPILER

Forgen is itself a systems application.

Engineering targets:

```text
compact IR representation
arena allocation for compiler nodes
structural sharing
content-addressed caches
parallel-friendly immutable analysis facts
```

The compiler should remain practical on ordinary developer hardware.

---

# 98. VERIFIER ARCHITECTURE

Separate trust stages:

```text
frontend invariants
HIR verifier
DMIR verifier
target IR verifier
link artifact verifier
```

An optimizer pass that introduces invalid IR must fail fast.

---

# 99. TEST STRATEGY

Compiler testing layers:

```text
lexer snapshots
parser snapshots
semantic tests
type tests
borrow tests
effect tests
IR golden tests
optimization tests
backend tests
end-to-end runtime tests
ABI tests
fuzz tests
```

---

# 100. DIFFERENTIAL TESTING

For compatible subsets, Datara code can be compared against reference implementations.

Examples:

```text
interpreter model
slow reference implementation
C/Rust test oracle
```

The goal is semantic equivalence, not source equivalence.

---

# 101. FUZZING

High-value fuzz targets:

```text
parser
resolver
type checker
ownership engine
pattern matcher
IR verifier
optimizer
binary reader
```

---

# 102. BENCHMARK SUITE

Runtime benchmarks:

```text
CLI startup
text scan
JSON parse
CSV transform
hashing
sorting
matrix multiply
vector operations
async IO
parallel map
FFI calls
embedded kernels
```

---

# 103. COMPILER BENCHMARK SUITE

```text
parse MB/s
files/s
semantic analysis ms
incremental rebuild ms
Domain graph build time
optimizer time
codegen time
cache hit ratio
```

---

# 104. PERFORMANCE REGRESSION POLICY

CI SHOULD fail or at least flag when important benchmarks regress beyond configured thresholds.

Example:

```text
runtime regression > 2%
compile regression > 10%
``` 

Thresholds are workload-specific.

---

# 105. TARGET PROFILES

Default profiles:

```text
native
desktop
server
embedded
wasm
accelerated
```

Target profiles affect defaults, not source semantics.

---

# 106. DOMAIN DECISION ENGINE

Domain optimizer roughly performs:

```text
1. build fact set
2. identify objectives
3. estimate candidate transformations
4. simulate/score costs
5. apply profitable transformations
6. re-evaluate affected regions
7. verify
```

This makes optimization an iterative decision system rather than a static list of flags.

---

# 107. OPTIMIZATION SEARCH

For complex regions, Forgen may evaluate alternatives:

```text
version A: fused loop
version B: split loop
version C: parallel
version D: SIMD
```

Then cost model selects the best legal result.

This is a major future research area.

---

# 108. PROFILE + DOMAIN

Profile facts should be fed into candidate scoring.

Example:

```text
97% of inputs have N > 10000
```

May justify a parallel/vector strategy.

Compiler must keep a safe generic fallback for unexpected runtime conditions where assumptions are not guaranteed.

---

# 109. MULTI-VERSION CODE

Possible artifact:

```text
fast-hot
small-input
baseline
```

Dispatch logic should be minimal and target-specific.

---

# 110. START vs DOMAIN

Same semantic source.

Different compilation budget.

```text
start → low analysis cost
release → stronger optimization
domain → whole program + specialization
```

This is the core developer experience contract.

---

# 111. DOMAIN RUNTIME STRIPPING

Domain determines:

```text
reachable runtime modules
allocator requirements
error formatting requirements
reflection requirements
async runtime requirements
network requirements
```

Unused runtime pieces disappear.

---

# 112. PACKAGE FEATURE STRIPPING

Optional features MUST support fine-grained linking.

A package can contain:

```text
json
xml
yaml
csv
```

but an application using only JSON need not link XML/YAML code.

---

# 113. AI CONTEXT SLICING

Semantic graph enables context extraction at symbol level.

Example:

```bash
forgen context --symbol User.billing
```

AI receives:

```text
source slice
signature
related state
required capabilities
effects
errors
callee graph
```

This reduces context noise.

---

# 114. AI SAFETY BOUNDARY

AI can propose code.

Forgen decides:

```text
type-valid?
memory-safe?
effect-safe?
ABI-safe?
constraint-respecting?
```

AI cannot bypass compiler proofs in safe mode.

---

# 115. ERROR LOCALIZATION ARCHITECTURE

Messages are stored as localization keys plus structured arguments.

Example:

```text
DTR-BORROW-004
{symbol: user.name, expected: mutable, actual: shared-borrow}
```

Locale renders this into human language.

---

# 116. LANGUAGE LOCALIZATION ROADMAP

Phase 1:

```text
en
ru
```

Phase 2:

```text
more locales
```

Machine-readable diagnostic codes remain unchanged.

---

# 117. EMBEDDED DOMAIN

`forgen embedded` may imply:

```text
small runtime
static allocation preference
no dynamic loader
interrupt checks
MMIO semantics
linker script
binary size report
stack report
```

This is a target workflow, not a second language.

---

# 118. HARDWARE INTRINSICS

Backend API should expose verified intrinsics through standard libraries:

```text
simd
atomic
fence
mmio
volatile
interrupt
```

They should have target-specific implementations but stable semantic contracts.

---

# 119. ATOMIC / CONCURRENCY

The safety engine must model:

```text
atomic variables
immutable shared state
mutable exclusive state
message passing
locks
```

Forgen should prefer high-level safe primitives where possible and expose low-level atomics for experts.

---

# 120. DATA RACE ANALYSIS

Safe parallel code requires compiler proof that:

```text
conflicting writes do not overlap unsafely
```

or that access is synchronized through a recognized safe primitive.

---

# 121. ASYNC LOWERING

Possible representation:

```text
state machine
future object
continuation
runtime task
```

Forgen chooses according to target/runtime.

A simple async function need not allocate a general boxed future.

---

# 122. TAIL CALL / RECURSION

Optimizer may eliminate eligible tail calls and specialize recursive functions when useful.

Stack usage should be visible in embedded profile.

---

# 123. LINK-TIME OPTIMIZATION

Domain should support cross-module optimization equivalent in power to LTO but using Datara semantic graph before/alongside backend LTO.

---

# 124. LLVM ROLE

LLVM MAY be the initial codegen backend.

But:

```text
Datara semantics
Datara ownership
Datara effect model
Datara flow model
Datara specialization
```

must exist before LLVM lowering.

LLVM is backend infrastructure, not language semantics.

---

# 125. FUTURE CUSTOM BACKEND

If profiling or hardware targets justify it, Forgen may later add a custom codegen backend for:

```text
MCU
specialized accelerators
minimal binaries
```

No dependency on LLVM semantics should be assumed.

---

# 126. SOURCE MAPS

Every HIR/DMIR operation retains source origin where practical.

This is needed for:

```text
debugging
error reporting
AI explanations
optimization reports
```

---

# 127. ARTIFACT METADATA

Binary should optionally contain:

```text
compiler version
target
build profile
build hash
debug mapping
optimization summary
```

Release builds can strip nonessential metadata.

---

# 128. REPRODUCIBILITY

Domain builds SHOULD support deterministic mode.

If profile/PGo data is used, the profile itself becomes an input artifact and is hash-addressed.

---

# 129. SECURITY OF CACHE

Compiler cache artifacts should be content-addressed.

Future package ecosystem should include checksums/signatures.

The compiler MUST NOT trust executable cache artifacts from unverified locations in secure mode.

---

# 130. COMPILER PLUGIN POLICY

External optimizer plugins are not allowed to silently modify semantics.

Future plugin API should require:

```text
IR version
analysis dependencies
transform declaration
verification hook
```

---

# 131. DOMAIN EXPLAINABILITY

A Domain build should be able to answer:

```text
Why was this function removed?
Why wasn't this function inlined?
Why was this loop not parallelized?
Why wasn't GPU selected?
Why did heap allocation remain?
```

This is part of product design, not a debug luxury.

---

# 132. OPTIMIZER EXPLANATION FORMAT

Machine-readable form:

```json
{
  "pass": "escape-analysis",
  "symbol": "User.create",
  "decision": "allocation-eliminated",
  "proof": "non-escaping",
  "confidence": "proven"
}
```

The human renderer turns it into friendly text.

---

# 133. PROOF VS HEURISTIC

Forgen must distinguish:

```text
proven
profile-supported
heuristic
unknown
```

Safety claims require proof.

Performance choices may be heuristic.

---

# 134. NO UNSOUND OPTIMIZATION

The optimizer must never use a performance heuristic as a replacement for a semantic proof.

For example:

```text

# 194. FRONTEND PARSER ARCHITECTURE

Recommended internal pipeline:

```text
UTF-8 input
 → token stream
 → lossless syntax tree
 → AST
```

The lossless layer is useful for formatter/LSP preservation, while normalized AST is used by semantic analysis.

---

# 195. ERROR RECOVERY

Parser error recovery should:

```text
preserve later declarations
produce one useful primary diagnostic
avoid cascading nonsense errors
```

For AI-generated code, recovery quality is especially important because one syntax mistake should not hide 30 independent diagnostics.

---

# 196. TYPE INFERENCE ENGINE

Inference should be constraint-based.

Each expression contributes constraints:

```text
literal → numeric type set
call → parameter constraints
lambda → argument/result constraints
generic call → type parameter constraints
operator → role/capability constraints
```

A solver computes the most specific legal type.

---

# 197. NO `ANY` IN SAFE INFERENCE

If constraints cannot resolve to a safe type, Forgen should report ambiguity instead of silently selecting a dynamic type.

Explicit dynamic library types are allowed as semantic boundaries.

---

# 198. TYPE INFERENCE EXAMPLE

```dtr
xs := [1, 2, 3]
y := xs.map(x => x * 2)
```

Compiler derives approximately:

```text
xs: List<Int>
x: Int
y: List<Int>
```

No annotations are needed.

---

# 199. GENERIC SOLVER

Generic constraints are represented as proof obligations:

```text
T = concrete type
T satisfies Role
T convertible to U
```

The solver should avoid global search explosion through memoization and canonicalized constraints.

---

# 200. OVERLOAD RESOLUTION

Candidate ranking should be deterministic:

```text
exact match
→ safe widening
→ generic specialization
→ explicit conversion
```

Implicit narrowing should never win silently.

---

# 201. EFFECT INFERENCE ALGORITHM

For each function:

1. collect direct effects;
2. union callee effects;
3. incorporate mutation/resource operations;
4. propagate through the call graph;
5. invalidate/recompute only affected SCCs on incremental changes.

---

# 202. EFFECT SCC HANDLING

Recursive functions form strongly connected components.

The effect solver computes a fixed point across each SCC.

---

# 203. OWNERSHIP GRAPH

Forgen tracks:

```text
Value
Owner
Borrow
Borrow region
Mutation capability
Escape edge
```

The result is a proof graph, not just a set of warnings.

---

# 204. BORROW REGION INFERENCE

Borrow regions should be inferred from actual uses.

Example:

```dtr
view := user.name
use(view)
user.name = "Bob"
```

The borrow can end after the final use of `view` if no later use exists.

This enables fine-grained optimization and fewer false conflicts.

---

# 205. NLL-LIKE POLICY

Datara SHOULD use last-use/liveness information to end inferred borrow regions as early as possible.

The exact algorithm is implementation-defined but must remain sound.

---

# 206. DATA-RACE ANALYSIS

Parallel region safety is checked against ownership + effects + synchronization primitives.

Compiler should distinguish:

```text
independent values
shared immutable values
synchronized mutable state
unsafely aliased state
```

---

# 207. FLOW NORMALIZATION

Every `flow` becomes a graph:

```text
Input
 → Stage
 → Stage
 → Branch
 → Merge
 → Output
```

Each stage has effect and ownership metadata.

---

# 208. PIPELINE NORMALIZATION

Every `|>` chain is normalized before ordinary function optimization.

This allows:

```text
map/filter/reduce fusion
stream fusion
query planning
allocation elimination
```

---

# 209. QUERY-LIKE RECOGNITION

Compiler-aware data libraries can expose recognized operations.

Forgen MAY identify patterns such as:

```text
filter
project
aggregate
sort
join
```

without making `Table` a core language type.

---

# 210. LAMBDA CLOSURE CONVERSION

A lambda is lowered into:

```text
code pointer/target
capture environment
```

unless specialization can eliminate both environment and indirect invocation.

---

# 211. ESCAPE ANALYSIS

Escape states:

```text
NoEscape
ReturnEscape
StoreEscape
ThreadEscape
ForeignEscape
```

Only `NoEscape` provides the strongest allocation elimination opportunities.

---

# 212. SCALAR REPLACEMENT

Aggregates are recursively decomposed when:

```text
small
fixed layout not externally observable
field uses are analyzable
```

---

# 213. MEM2REG-LIKE TRANSFORMATION

Local mutable variables may become SSA values whenever address-taking is absent.

---

# 214. STACK PROMOTION

If allocation is required but lifetime is function-bounded and non-escaping, Forgen can promote to stack/region allocation.

---

# 215. REGION ALLOCATION

For workloads with many short-lived related objects, a region allocator can be selected if destruction/lifetime constraints permit.

This should be an optimizer decision, not a mandatory source-level concept.

---

# 216. BUFFER REUSE GRAPH

Tensor/data buffer planning uses:

```text
live intervals
shape compatibility
alignment
aliasing
mutation
producer/consumer relation
```

Two buffers can share physical storage only if all semantic constraints permit it.

---

# 217. LAYOUT SELECTION

Candidate layouts:

```text
AoS
SoA
AoSoA
packed
aligned
```

Forgen scores layouts against:

```text
access pattern
cache line utilization
SIMD
ABI constraints
memory footprint
```

---

# 218. FIELD REORDERING

Within internal/private classes, fields may be reordered for alignment/cache locality when reflection/ABI does not expose order.

Public ABI types cannot be silently reordered.

---

# 219. BIT PACKING

Small enums/bools may be packed in internal representations when semantic operations and atomicity rules permit.

---

# 220. BRANCH OPTIMIZATION

Forgen tracks branch probabilities from:

```text
static reasoning
PGO
input contracts
```

and can optimize layout without changing branch semantics.

---

# 221. DECIDE TREE OPTIMIZER

For pure numerical ranges:

```text
if x < 10
if x < 20
if x < 30
```

Forgen may create a balanced or range-based decision structure if it reduces expected cost.

---

# 222. MATCH LOWERING

Closed integer-like matches may use jump tables.

Sparse cases may use binary search/tree forms.

Small cases may stay branch chains.

---

# 223. LOOP VERSIONING

Compiler may emit fast path + fallback when runtime conditions can be cheaply checked:

```text
fast path: no alias / aligned / known bounds
fallback: general legal implementation
```

---

# 224. SPECULATIVE SPECIALIZATION

A future domain pass may generate:

```text
specialized hot version
fallback general version
```

only when a cheap runtime guard can preserve correctness.

---

# 225. AUTO-VECTORIZATION

The vectorizer consumes:

```text
loop dependence graph
alias facts
trip counts
alignment
operation legality
```

It must not use unsafe assumptions merely because a benchmark usually satisfies them.

---

# 226. SIMD LIBRARY CONTRACT

Library operations may declare vectorizable semantic patterns.

Forgen may inline them into target SIMD operations.

---

# 227. PARALLEL COST MODEL

For a candidate parallel region:

```text
parallel_cost = scheduling + synchronization + memory effects
serial_cost = estimated work
```

Parallelization occurs when expected benefit clears a safety and profitability threshold.

---

# 228. THREAD POOL SELECTION

Forgen should prefer reusable executors over creating one OS thread per parallel stage.

---

# 229. WORK STEALING

For irregular independent tasks, the runtime scheduler may use work stealing.

This is a runtime implementation detail hidden by `parallel` semantics.

---

# 230. ASYNC STATE MACHINE OPTIMIZATION

If an async function has no actual suspension after specialization, Forgen should be able to lower it to a synchronous implementation.

This avoids paying async overhead when it provides no benefit.

---

# 231. ASYNC FRAME ELIMINATION

If the compiler can prove a coroutine frame does not escape or does not need heap allocation, it may stack/promote/eliminate the frame.

---

# 232. TASK INLINE

Small tasks that never cross a scheduling boundary may be inlined as ordinary function calls.

The `task` abstraction therefore does not imply runtime overhead.

---

# 233. FFI CONSERVATISM

Unknown extern functions default to conservative assumptions:

```text
may read memory
may write memory
may perform IO
may block
may alias
```

Explicit trusted FFI metadata can reduce conservatism.

---

# 234. FFI CONTRACT

Potential declaration metadata:

```text
pure
readonly
no-return
noalias
nonblocking
```

The most safety-critical contracts must be verified or placed behind unsafe boundaries.

---

# 235. LINK-TIME GRAPH

After codegen candidate selection, linker graph includes:

```text
object files
runtime sections
libraries
FFI symbols
platform startup
```

Dead stripping is still performed.

---

# 236. RUNTIME MODULARITY

Runtime should be built as separately reachable units:

```text
core
memory
format
io
thread
async
network
reflection
panic
```

---

# 237. MINIMAL RUNTIME EXAMPLE

A CLI program using only:

```dtr
out "Hello"
```

should not pull in:

```text
HTTP
TLS
database
GPU
async scheduler
reflection
```

unless hidden platform startup requirements make a specific dependency unavoidable.

---

# 238. STARTUP SPECIALIZATION

Domain build may replace generic runtime initialization with a smaller specialized entry path.

---

# 239. ALLOCATOR SELECTION

Potential allocator strategies:

```text
system allocator
specialized small allocator
arena
bump allocator
static/no allocator
```

Selection depends on target and proven workload constraints.

---

# 240. EMBEDDED NO-ALLOC MODE

For suitable firmware, Forgen should prove or report that heap allocation is absent.

If heap allocation exists, report the reason.

---

# 241. STACK ANALYSIS

Embedded Domain builds should report estimated maximum stack usage.

Recursive or unbounded dynamic stack behavior should be flagged.

---

# 242. INTERRUPT SAFETY GRAPH

Interrupt context nodes record:

```text
allowed calls
allocation
blocking
locking
execution bound
```

Calling a forbidden operation from interrupt context becomes a compiler diagnostic.

---

# 243. REAL-TIME BUDGET ANALYSIS

For deterministic regions Forgen can maintain a budget expression:

```text
computation cost
memory access assumptions
external calls
scheduling effects
```

The result may be:

```text
proven
estimated
unknown
```

---

# 244. HARDWARE INTRINSIC SELECTION

A semantic operation such as vector add can lower to:

```text
scalar
SSE/AVX
NEON
RISC-V Vector
```

based on target features.

---

# 245. MULTIVERSION DISPATCH

Two strategies:

```text
compile-time target choice
runtime feature dispatch
```

The second is only used when deployment target is not fixed and code-size cost is acceptable.

---

# 246. PROFILE FORMAT

Profile artifact should be:

```text
versioned
hash-linked to binary/source
portable enough for same target family
explicit about collection method
```

---

# 247. PROFILE TRUST

Profiles influence profitability only.

They cannot prove safety.

---

# 248. PGO STABILITY

Forgen should not rebuild the program into semantically different code because of profile data.

Profile affects strategy selection inside the same legal semantics.

---

# 249. DOMAIN SEARCH BUDGET

Domain optimizer should have internal budget knobs:

```text
compile-time budget
memory budget
search depth
candidate limit
```

These may be auto-selected by the toolchain.

---

# 250. DOMAIN ADAPTIVE PASS ORDER

Forgen may change pass ordering when analysis facts show that a different order has higher expected payoff.

Example:

```text
specialize first
→ exposes constant branches
→ enables more DCE
```

versus:

```text
DCE first
→ reduces graph
→ cheaper specialization
```

---

# 251. PASS PROFILING

Every expensive pass should optionally self-profile:

```text
time
memory
nodes changed
benefit estimate
```

This lets compiler engineers optimize Forgen itself.

---

# 252. OPTIMIZER TRACE

`forgen inspect optimize --trace` MAY emit:

```text
Pass: Inliner
Candidate: User.greet
Decision: inline
Reason: hot + small + no size pressure
```

---

# 253. WHY-OPTIMIZATION API

The IDE/AI can request:

```text
why was this allocation kept?
why was this loop not vectorized?
why is this call indirect?
```

Forgen responds from recorded optimization decisions.

---

# 254. SEMANTIC CACHE DESIGN

Cache objects should use stable hashes of:

```text
source
public interface
semantic facts
compiler version
target
```

Private implementation changes need not invalidate all downstream caches when the affected artifact is internal.

---

# 255. CACHE SECURITY

Cached IR should be validated by verifier before reuse across trust boundaries.

---

# 256. COMPILER DAEMON

A future `forgend` daemon can keep:

```text
parsed modules
semantic graph
background index
incremental cache
```

alive for IDE/start mode.

---

# 257. ONE DAEMON, MULTIPLE CLIENTS

CLI, LSP and AI tooling should be able to share the same semantic server.

This prevents divergent interpretations of the project.

---

# 258. AI CONTEXT QUERY

Conceptual API:

```text
context(symbol)
context(type)
context(callsite)
context(error)
context(optimization)
```

Each returns only relevant graph slices.

---

# 259. AI PATCH VERIFICATION

An agent can submit:

```text
patch
expected effect delta
expected API delta
```

Forgen checks whether the semantic result matches the agent's claimed intent.

---

# 260. AI PERFORMANCE LOOP

Potential command workflow:

```bash
forgen bench target
forgen profile target
forgen domain
forgen inspect optimize
```

AI can then receive structured metrics and improve implementation.

---

# 261. LOCALIZATION ARCHITECTURE

Diagnostic IDs are language-neutral.

Example localization record:

```text
DTR-TYPE-001:
  en: "Type mismatch"
  ru: "Ошибка типов"
```

---

# 262. COMPILER BINARY COMPATIBILITY

Forgen should define compatibility of cached IR explicitly.

Changing internal IR layout should invalidate incompatible caches rather than attempting unsafe reuse.

---

# 263. BACKEND TESTING

Every backend needs:

```text
instruction selection tests
ABI tests
relocation tests
runtime behavior tests
optimization equivalence tests
```

---

# 264. LLVM BACKEND STRATEGY

Initial implementation can leverage LLVM for:

```text
instruction selection
register allocation
machine-specific lowering
object generation
```

Datara-specific semantic optimization remains before that boundary.

---

# 265. CUSTOM BACKEND CRITERION

A custom backend is justified only when:

```text
LLVM output is measurably insufficient

and

custom implementation gives a strategic advantage
```

Otherwise LLVM remains valuable infrastructure.

---

# 266. CODEGEN QUALITY METRICS

Measure:

```text
cycles
instructions
binary size
cache misses
branch misses
allocation count
startup time
```

Not all metrics are available on every target.

---

# 267. BENCHMARK NORMALIZATION

Benchmark harness records:

```text
hardware
OS
compiler version
commit
mode
input
result hash
```

Results without reproducibility metadata are considered incomplete.

---

# 268. REGRESSION POLICY

Suggested initial limits:

```text
critical runtime regression > 2% → investigate/fail gate
compile-time regression > 10% → investigate
binary size regression > configured threshold → investigate
```

Thresholds vary by benchmark class.

---

# 269. PERFORMANCE SCORECARD

Forgen release should publish an internal scorecard:

```text
Rust reference
Datara start
Datara release
Datara domain
JS/V8 reference
Python reference
```

The comparison should never mix algorithms or workloads unfairly.

---

# 270. REALISTIC PERFORMANCE CLAIM

The project should not claim “faster than Rust” as a universal property.

The defensible objective is:

```text
Rust-level native execution quality
with substantially lower source-level complexity
```

with approximately 1–2% acceptable gap on a carefully defined class of workloads.

---

# 271. COMPILER DEVELOPMENT ORDER

Recommended build order:

```text
1 lexer/parser
2 AST/diagnostics
3 resolver
4 type system
5 reference evaluator
6 basic native backend
7 classes/records
8 generics/roles
9 ownership
10 effects
11 flow/pipeline
12 optimizer
13 incremental cache
14 Domain
15 PGO
16 embedded
17 AI tooling
```

---

# 272. REFERENCE INTERPRETER

A slow but obviously correct interpreter is a powerful validation oracle.

It should prioritize semantic clarity over speed.

---

# 273. CONFORMANCE ORACLE

Every optimization test can compare:

```text
optimized binary
vs
reference evaluator
```

on randomized inputs where possible.

---

# 274. PROPERTY-BASED OPTIMIZER TEST

Examples:

```text
fusion(a |> f |> g) == sequential(a |> f |> g)
```

under generated inputs and defined effects.

---

# 275. FUZZING IR

Random valid IR graphs should be fed to:

```text
optimizer
verifier
backend serializer
```

The verifier must reject malformed IR gracefully.

---

# 276. COMPILER CRASH REDUCTION

A compiler crash should produce a minimized source reproduction whenever feasible.

This massively improves iteration speed during language design.

---

# 277. RELEASE GATES

A candidate Forgen release should pass:

```text
language conformance
optimizer soundness
backend correctness
benchmark stability
cache correctness
diagnostic tests
```

---

# 278. DOMAIN TRUST

A Domain artifact should include a manifest describing:

```text
source graph hash
dependency hashes
compiler version
target
profile
domain intent
```

---

# 279. REPRODUCIBLE DOMAIN

Same source + same compiler + same target + same profile inputs should aim for reproducible artifacts under deterministic settings.

---

# 280. FINAL FORGEN NORTH STAR

> **Forgen should be the place where the apparent simplicity of Datara is converted into machine-level sophistication.**

The compiler should perform the complicated work so that the programmer does not have to write complicated code merely to obtain safe, fast execution.
