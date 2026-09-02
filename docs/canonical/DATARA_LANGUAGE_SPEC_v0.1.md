# DATARA LANGUAGE SPECIFICATION v0.1

**Язык:** Datara  
**Исходный файл:** `.dtr`  
**Спецификация:** draft v0.1  
**Статус:** normative design draft, до freeze grammar допускаются изменения.

---

# 0. STATUS OF THE SPEC

Этот документ формализует syntax и source-level semantics. Он не фиксирует конкретную реализацию backend.

Внутренне Forgen может менять representation, layout и execution strategy, если сохраняется observable semantics и constraints.

Обозначения:

```text
MUST     — обязательно
SHOULD   — предпочтительно
MAY      — разрешено
MUST NOT — запрещено
```

---

# 1. DESIGN TARGET

Datara должна быть:

```text
strictly typed
statically analyzed
memory safe by default
native compiled
AI-readable
pleasant for TypeScript/JavaScript/Python/Rust programmers
```

Главный принцип спецификации:

> source syntax описывает intent; compiler semantics определяет concrete representation.

---

# 2. CHARACTER SET

UTF-8 source.

Identifiers поддерживают Unicode, но standard style рекомендует ASCII identifiers для API и системных компонентов.

---

# 3. COMMENTS

Line comment:

```dtr
// comment
```

Block comment:

```dtr
/* comment */
```

Documentation comment в первой версии:

```dtr
/// Creates a user.
```

Forgen может экспортировать documentation graph.

---

# 4. IDENTIFIERS

Формат:

```text
identifier := letter { letter | digit | '_' }
```

Keywords не являются identifiers.

Case-sensitive.

Style:

```text
Types: PascalCase
values/functions: camelCase
constants: SCREAMING_SNAKE_CASE
```

Style не является semantic restriction.

---

# 5. KEYWORD SET

Минимальный core:

```text
let
mut
const
fn
function
class
record
component
role
behavior
from
with
replaces
export
import
as
if
else
for
while
loop
match
decide
select
return
break
continue
parallel
async
task
flow
unsafe
extern
true
false
None
```

`cli`, `table`, `stream`, `tensor`, `model`, `ai` не являются core keywords без отдельного language edition. Они должны по возможности жить в libraries.

---

# 6. LITERALS

## Integer

```dtr
0
42
1_000_000
```

## Float

```dtr
3.14
1.0e-3
```

## Boolean

```dtr
true
false
```

## String

```dtr
"hello"
```

Interpolated string:

```dtr
"Hello, {name}"
```

## Character

```dtr
'a'
```

---

# 7. TYPE NAMES

Built-ins:

```text
Int
UInt
Int8 Int16 Int32 Int64
UInt8 UInt16 UInt32 UInt64
Float16 Float32 Float64
Bool
Char
String
Str
Bytes
Unit
Never
```

Platform-sized aliases SHOULD exist:

```text
IntSize
UIntSize
```

Target-dependent size must be explicit in ABI-sensitive code.

---

# 8. VARIABLE DECLARATION

Canonical forms:

```dtr
let x = 10
mut x = 10
const X = 10
```

Compact inferred form:

```dtr
x := 10
mut x := 10
```

Rules:

1. `let` binding cannot be reassigned.
2. `mut` binding may be reassigned.
3. `:=` always creates a new local binding.
4. Type inference is static and strict.
5. Ambiguous inference MUST produce a diagnostic.

Explicit type:

```dtr
let x: Int = 10
```

Alternative field/parameter declaration syntax may omit colon:

```dtr
name String
```

Both representations normalize to the same semantic type.

---

# 9. SHADOWING

Shadowing is allowed for local bindings:

```dtr
let value = 10
let value = value + 1
```

But Forgen SHOULD warn in contexts where shadowing makes AI or human reasoning harder.

A stricter lint mode MAY ban implicit shadowing.

---

# 10. SCOPES

Lexical scopes are introduced by:

```text
functions
blocks
match branches
loops
parallel blocks
class/behavior declarations
modules
```

No dynamic scope.

---

# 11. FUNCTIONS

Long form:

```dtr
fn add(a Int, b Int) -> Int {
    a + b
}
```

Short form:

```dtr
fn add(a Int, b Int) -> Int => a + b
```

Legacy-friendly alias:

```dtr
function add(...) -> Int { ... }
```

---

# 12. IMPLICIT RETURN

Последнее expression блока возвращается автоматически:

```dtr
fn square(x Int) -> Int {
    x * x
}
```

`return` нужен для раннего выхода:

```dtr
fn abs(x Int) -> Int {
    if x < 0 {
        return -x
    }
    x
}
```

---

# 13. FUNCTION TYPES

```dtr
type BinaryOp = (Int, Int) -> Int
```

Closure:

```dtr
add := (a, b) => a + b
```

Тип выводится.

---

# 14. LAMBDA CAPTURE

Closure может захватывать внешние bindings.

```dtr
factor := 2
mul := x => x * factor
```

Compiler определяет:

```text
capture set
mutability
lifetime
escape
```

Если closure не escaping, environment SHOULD be eliminated.

---

# 15. CALL SYNTAX

```dtr
add(1, 2)
```

Named arguments:

```dtr
User.create(id: 10, name: "Alex")
```

Positional and named arguments MAY mix only under deterministic rule: positional arguments MUST precede named arguments.

---

# 16. CLASS

Canonical:

```dtr
class User {
    id UserId
    name String
    active Bool = true

    greet() -> String => "Hello {name}"
}
```

Fields are private to module by default unless `export` is used at member visibility point where supported by final profile.

Class identity is semantic. It does not force heap allocation.

---

# 17. CLASS INITIALIZER

Object creation:

```dtr
user := User {
    id: 10
    name: "Alex"
}
```

All required fields MUST be initialized.

Defaults are evaluated at construction time unless compiler proves constant.

---

# 18. CUSTOM CREATION

Preferred factory:

```dtr
class User {
    id UserId
    name String

    create(id UserId, name String) -> User!ValidationError {
        require name != ""
        User { id, name }
    }
}
```

`constructor` syntax is reserved for possible future explicit low-level construction semantics. It is NOT required in v0.1.

---

# 19. RECORD

```dtr
record Point {
    x Float
    y Float
}
```

Record has value semantics and no implicit identity behavior.

Compiler SHOULD use flat/inline representation where possible.

---

# 20. COMPONENT

```dtr
component Timestamped {
    createdAt Instant
    updatedAt Instant
}
```

Composition:

```dtr
class User with Timestamped {
    name String
}
```

Component introduces fields/behavior according to composition rules but does not create independent runtime identity.

---

# 21. ROLE

```dtr
role Serializable {
    serialize() -> Bytes
}
```

A role is a capability contract.

Implementation:

```dtr
class User with Serializable {
    ...
}
```

Role MAY contain default behavior in future editions, but v0.1 treats required behavior as primary model.

---

# 22. BEHAVIOR

```dtr
behavior User {
    isAdult() -> Bool => age >= 18

    rename(to newName String) {
        name = newName
    }
}
```

Semantics:

- behavior attaches to exactly one target type;
- no runtime extension object is created;
- compiler merges semantic members into target type;
- method conflict rules are checked at compile time.

---

# 23. SPLIT BEHAVIOR

Multiple behavior files are allowed:

```text
user/core.dtr
user/security.dtr
user/billing.dtr
```

Each can contain:

```dtr
behavior User { ... }
```

The compiler merges them by semantic identity.

Duplicate member definitions MUST be diagnosed unless one explicitly `replaces` another through an inheritance boundary.

---

# 24. INHERITANCE

Classic familiar form:

```dtr
class Admin extends User { ... }
```

Native form:

```dtr
class Admin from User {
    + Permissioned
    + Audited
}
```

Rules:

1. At most one class base in v0.1.
2. Multiple base classes are forbidden.
3. Components and roles are composable.
4. Diamond class inheritance is impossible.
5. `from` means inherited identity/behavior lineage.
6. `+` means explicit composition of named capability packages.

---

# 25. COMPOSITION OPERATOR `+`

Inside class body:

```dtr
class Admin from User {
    + Permissioned
    + Audited
}
```

A composition package can be:

```text
component
role
a named capability bundle
```

Resolution order is explicit and deterministic. Conflicts MUST be compile errors unless resolved using `replaces` or another future explicit rule.

---

# 26. REPLACES

```dtr
class Admin from User {
    replaces greet() {
        out "Admin"
    }
}
```

It means the inherited member is intentionally replaced.

No silent override.

`override` MAY exist as compatibility alias but SHOULD warn under native-style lint.

---

# 27. STATIC-STYLE MEMBERS

Datara prefers module functions over static methods.

Instead of:

```dtr
User.createGuest()
```

where no instance is required, either form MAY be provided by library style, but native recommendation is:

```dtr
module User {
    createGuest() -> User { ... }
}
```

This keeps instance identity and namespace utilities distinct.

---

# 28. MODULE

Every project consists of modules.

```dtr
module user
```

Import:

```dtr
import user
import user.billing
```

Alias:

```dtr
import user.billing as billing
```

---

# 29. EXPORT

Public API:

```dtr
export fn parse(...) -> Data
export class User { ... }
```

Default visibility is module-local.

Package-level exports are controlled by manifest/module root.

---

# 30. GENERICS

```dtr
class Box<T> {
    value T
}
```

Function:

```dtr
fn first<T>(items List<T>) -> T? {
    items[0]
}
```

Inference:

```dtr
value := first(users)
```

---

# 31. GENERIC CONSTRAINTS

```dtr
fn save<T: Serializable>(value T) -> Bytes {
    value.serialize()
}
```

Constraint means capability requirement.

Compiler MUST reject calls where the capability proof cannot be established.

---

# 32. TYPE INFERENCE

Inference MAY derive:

```text
literal types
function type arguments
return types in local contexts
closure types
generic arguments
collection element types
ownership categories
```

Inference MUST NOT silently degrade to a dynamic `Any`-like escape in safe mode.

Explicit dynamic values require explicit library/type boundary.

---

# 33. NULLABILITY

Optional:

```dtr
User?
```

No implicit null conversion.

`None` is the only absence variant.

---

# 34. RESULT

```dtr
User!DbError
```

This is a result channel with success type `User` and error type `DbError`.

Propagation:

```dtr
user := loadUser(id)!
```

The `!` postfix is only valid where error propagation can be typed.

---

# 35. PATTERN MATCH

```dtr
match state {
    Loading => showLoader()
    Ready(data) => show(data)
    Failed(error) => showError(error)
}
```

Compiler MUST check exhaustiveness for closed sum types.

---

# 36. DECIDE

`decide` evaluates guards in source order:

```dtr
decide {
    temperature > 100 => Alarm
    temperature > 80 => Warning
    else => Normal
}
```

Exactly one branch is selected.

Compiler MAY transform decision tree if observable ordering/side effects permit.

If guards have effects, reorder is forbidden unless proven equivalent.

---

# 37. SELECT

`select` is value-producing guard selection:

```dtr
label := select {
    score >= 0.9 => "A"
    score >= 0.8 => "B"
    else => "C"
}
```

All branches MUST unify to a compatible type.

---

# 38. IF

```dtr
if condition {
    foo()
} else {
    bar()
}
```

`if` is binary control flow and remains fundamental.

---

# 39. LOOPS

```dtr
for item in items { ... }
while condition { ... }
loop { ... }
```

Range:

```dtr
for i in 0..count { ... }
```

Range bounds have documented inclusive/exclusive semantics and must be stable across targets.

---

# 40. PARALLEL BLOCK

```dtr
parallel {
    a := loadA()
    b := loadB()
}
```

Semantics express independence, not thread count.

Compiler verifies dependency and effect constraints.

---

# 41. PARALLEL FOR

```dtr
parallel for item in items {
    process(item)
}
```

Compiler MAY lower to sequential execution if cost model or semantics prefer it.

---

# 42. ASYNC FUNCTIONS

```dtr
async fn fetch() -> Data!Error {
    ...
}
```

`await`:

```dtr
value := await fetch()!
```

Compiler may transform to state machine, task, completion-based execution or another runtime representation.

---

# 43. FLOW

```dtr
flow process(order Order) -> Receipt!OrderError {
    order
        |> validate()
        |> reserve()
        |> pay()
        |> ship()
}
```

Flow is a named data/control graph and may impose stronger static analysis than an arbitrary function body.

---

# 44. PIPELINE

```dtr
values
    |> map(x => x * 2)
    |> filter(. > 10)
    |> reduce(sum)
```

Compiler lowers pipeline as graph and may fuse operations.

---

# 45. METHOD REFERENCE / FIELD PROJECTION

```dtr
users |> map(.name)
```

`.name` in function position is a field projection lambda shorthand:

```dtr
x => x.name
```

This is static and typed.

---

# 46. OPERATOR OVERLOADING

Operators MAY be supplied through role contracts.

Example conceptual contract:

```dtr
role Addable<T> {
    add(T) -> T
}
```

`+` resolution is static.

Ambiguous operator resolution is a compile error.

Operators MUST NOT silently allocate or dispatch dynamically when a static implementation is available, unless the user explicitly enters a dynamic boundary.

---

# 47. UNITS

Library/core-integrated units may use:

```dtr
80 km/h
5 ms
24 V
```

Units participate in type checking.

Adding incompatible dimensions MUST be rejected.

Unit representation SHOULD be erased at runtime when it is compile-time provable.

---

# 48. ERROR MODEL

Canonical errors use `Result` semantics.

Exceptions are reserved for:

```text
host boundaries
legacy APIs
FFI adapters
runtime panic/debug aborts
```

Safe application code SHOULD NOT depend on uncaught exceptions as normal control flow.

---

# 49. PANIC / ABORT

A fatal condition may use:

```dtr
panic("unreachable")
```

Compiler treats it as `Never`.

In deterministic/embedded profiles, panic strategy must be explicit in target runtime policy.

---

# 50. OWNERSHIP SEMANTICS

Every value has an ownership relationship.

Common inferred states:

```text
Owned
Borrowed
Shared
Moved
```

Source-level syntax only exposes advanced forms when inference is insufficient.

---

# 51. VIEW

```dtr
fn parse(view text Str) -> Token!
```

`Str` is a borrowed textual view.

The compiler verifies owner lifetime.

---

# 52. MUT-VIEW

```dtr
fn normalize(data mut-view Buffer) {
    ...
}
```

At most one conflicting mutable access may exist in a safe region.

---

# 53. SHARED

```dtr
fn inspect(data shared Data) { ... }
```

Exact implementation may use reference counting, immutable aliasing, arena lifetime or another verified representation.

Semantics, not representation, are normative.

---

# 54. OWN

```dtr
fn consume(data own Data) -> Result { ... }
```

Function receives ownership.

Caller must relinquish access unless the compiler can prove a copy/specialized representation with identical semantics.

---

# 55. UNSAFE

```dtr
unsafe {
    raw.write(...)
}
```

Unsafe code can bypass selected safety proof obligations but remains visible to compiler tooling.

Unsafe cannot silently turn the entire program unsafe.

---

# 56. FFI

```dtr
extern "C" fn malloc_like(size UIntSize) -> *U8
```

Raw pointers only occur in explicit unsafe/FFI domains unless wrapped by verified safe abstractions.

---

# 57. EFFECTS

Compiler derives effect set from body and called functions.

Conceptual effect lattice:

```text
Pure
Read
Write
IO
Network
Database
Parallel
Nondeterministic
Unsafe
```

Effects are monotonic through call graph unless boundary annotations explicitly model containment.

---

# 58. CONST EVALUATION

`const` expressions SHOULD be evaluated at compile time when legal.

Pure functions may become compile-time evaluable in future editions.

Side effects MUST NOT occur during compile-time evaluation unless explicitly permitted by a future compiler-time capability.

---

# 59. TASK

```dtr
task compress(file File) -> Bytes!CompressionError {
    ...
}
```

Task does not imply thread or async.

The runtime strategy is chosen by Forgen/task scheduler.

---

# 60. CLI CORE OUTPUT

```dtr
out "Hello"
out value
err "Error"
```

Formatting is type-driven.

For high-volume output, Forgen may select buffered/native formatting paths.

---

# 61. MODULE FILE SLICING

Multiple files may define behavior for a type.

The module graph groups them into a shared semantic identity.

This is the basis for incremental compilation and AI context slicing.

---

# 62. LANGUAGE VS LIBRARY BOUNDARY

Core language MUST stay small.

Capabilities that can be implemented as libraries SHOULD be libraries.

Candidates:

```text
AI
Tensor
Database
HTTP
JSON
CSV
CLI schema
GUI
```

Candidates for compiler-aware libraries may provide semantic contracts to Forgen.

---

# 63. REFLECTION

Runtime reflection is not default.

If enabled, it creates a compiler knowledge boundary and may prevent some dead-data elimination.

Compile-time type information is always available to compiler.

---

# 64. MACROS / METAPROGRAMMING

v0.1 intentionally minimizes general-purpose macros.

Preferred mechanism:

```text
compiler-known schemas
roles
components
derive-like library contracts
```

General macros can be introduced later only if they preserve analyzability.

---

# 65. DOCUMENTATION

`///` comments are first-class source metadata and MAY be exported into documentation graph.

Forgen docs command:

```bash
forgen docs
```

---

# 66. FORMATTING

Official formatter command:

```bash
forgen fmt
```

Formatter MUST be deterministic.

The language should avoid syntax whose meaning depends on formatting.

---

# 67. DIAGNOSTICS

Every diagnostic has stable code.

Examples:

```text
DTR-TYPE-001
DTR-BORROW-001
DTR-EFFECT-001
DTR-MATCH-001
DTR-IMPORT-001
DTR-FFI-001
```

Terminal renderer is localized; machine schema remains stable.

---

# 68. LOCALIZATION

Canonical compiler vocabulary remains English in internal IDs.

User-facing terminal output supports locales:

```text
ru
 en
```

Russian is an intended early locale, primarily for novice accessibility.

---

# 69. AI TOOLING CONTRACT

Forgen exposes semantic data as machine-readable schemas:

```text
symbol
inputs
outputs
types
effects
ownership
dependencies
safety
performance
```

No AI model is required by the language runtime.

---

# 70. SOURCE COMPATIBILITY PHILOSOPHY

Datara uses familiar syntax but intentionally has different semantics.

No promise of TypeScript source compatibility.

No promise of Rust source compatibility.

The target is:

```text
recognizable to existing programmers
```

not:

```text
copy of another language
```

---

# 71. GRAMMAR EBNF — CORE OVERVIEW

```text
program          := { declaration }
declaration      := importDecl
                  | moduleDecl
                  | functionDecl
                  | classDecl
                  | recordDecl
                  | componentDecl
                  | roleDecl
                  | behaviorDecl
                  | constDecl
                  | taskDecl
                  | flowDecl
                  ;

statement        := letDecl
                  | mutDecl
                  | assignment
                  | expressionStmt
                  | ifStmt
                  | forStmt
                  | whileStmt
                  | loopStmt
                  | matchStmt
                  | decideStmt
                  | returnStmt
                  | breakStmt
                  | continueStmt
                  | parallelStmt
                  ;

expression       := literal
                  | identifier
                  | callExpr
                  | memberExpr
                  | binaryExpr
                  | unaryExpr
                  | lambdaExpr
                  | objectInit
                  | rangeExpr
                  | matchExpr
                  | decideExpr
                  | selectExpr
                  | pipelineExpr
                  ;
```

---

# 72. GRAMMAR — DECLARATIONS

```text
functionDecl     := [export] ("fn" | "function") identifier
                    [typeParams] "(" [params] ")"
                    ["->" type]
                    (block | "=>" expression)

classDecl        := [export] "class" identifier [typeParams]
                    [inheritance]
                    block

inheritance      := "extends" type
                  | "from" type

recordDecl       := [export] "record" identifier [typeParams] block

componentDecl    := [export] "component" identifier [typeParams] block

roleDecl         := [export] "role" identifier [typeParams]
                    [roleConstraints] block

behaviorDecl     := [export] "behavior" type block

taskDecl         := [export] "task" identifier [typeParams]
                    "(" [params] ")" ["->" type]
                    block

flowDecl         := [export] "flow" identifier [typeParams]
                    "(" [params] ")" ["->" type]
                    block
```

---

# 73. FIELD GRAMMAR

Field supports both canonical and explicit annotation spellings:

```text
field := identifier type ["=" expression]
      | identifier ":" type ["=" expression]
```

This is a syntax convenience, not two semantic models.

---

# 74. PARAMETER GRAMMAR

```text
param := identifier type
      | identifier ":" type
      | ownershipMode identifier type
```

Where:

```text
ownershipMode := "own"
               | "view"
               | "mut-view"
               | "shared"
```

---

# 75. LAMBDA GRAMMAR

```text
lambdaExpr := identifier "=>" expression
            | "(" [identifierList] ")" "=>" expression
            | identifier "=>" block
            | "(" [identifierList] ")" "=>" block
```

---

# 76. PIPELINE GRAMMAR

```text
pipelineExpr := expression { "|>" pipelineStage }

pipelineStage := expression
               | identifier
               | "." identifier
```

Compiler expands pipeline stages into a canonical flow representation before optimization.

---

# 77. CLASS COMPOSITION GRAMMAR

```text
classBodyItem := fieldDecl
               | methodDecl
               | compositionDecl
               | replaceDecl

compositionDecl := "+" identifier
                 | "+" qualifiedName

replaceDecl := "replaces" memberSignature block
```

---

# 78. MATCH GRAMMAR

```text
matchStmt := "match" expression "{" { matchArm } "}"
matchArm  := pattern ["when" expression] "=>" expressionOrBlock
```

Patterns MAY include:

```text
identifier
literal
constructor pattern
record pattern
wildcard _
```

---

# 79. DECIDE GRAMMAR

```text
decideExpr := "decide" "{" { guardArm } "}"

guardArm := expression "=>" expressionOrBlock
          | "else" "=>" expressionOrBlock
```

First matching guard wins.

---

# 80. SELECT GRAMMAR

```text
selectExpr := "select" "{" { guardArm } "}"
```

All result expressions must unify.

---

# 81. OPERATOR PRECEDENCE — DRAFT

От сильного к слабому:

```text
postfix call/member/index/!
unary ! - + ~
exponentiation
multiplicative * / %
additive + -
range ..
comparison < <= > >=
equality == !=
logical and
logical or
null/option coalescing
ternary-like future forms
pipeline |>
```

Точные precedence numbers будут frozen после parser implementation tests.

---

# 82. TYPE EXPRESSIONS

```text
type := simpleType
      | type "?"
      | type "!" type
      | type "<" [typeList] ">"
      | functionType
      | viewType
      ;
```

The `T!E` grammar is intentionally compact but must remain unambiguous with postfix `!` error propagation.

---

# 83. SUM TYPES

Conceptual syntax:

```dtr
type State = Loading | Ready(Data) | Failed(Error)
```

Closed unions MUST support exhaustive match analysis.

---

# 84. ENUMS

Enums are semantic sugar for tagged finite sums where appropriate.

Possible syntax:

```dtr
enum Status {
    Idle
    Running
    Failed
}
```

Final spelling will be frozen during grammar stage.

---

# 85. ARRAY / COLLECTION TYPES

```dtr
List<Int>
Array<Float32, 128>
Map<String, User>
Set<UserId>
```

Fixed-size array metadata MAY participate in compile-time shape analysis.

---

# 86. TYPE LAYOUT

Source code MUST NOT assume physical layout unless an ABI/packed annotation says so.

Default layout is compiler-selected.

At FFI boundary layout becomes explicit.

---

# 87. ABI

ABI-stable declarations MUST specify:

```text
calling convention
field order
alignment
primitive widths
ownership contract
error boundary
```

Internal Datara values may use different layout.

---

# 88. DYNAMIC VALUES

A dynamic/opaque value MAY exist through a standard library type, but safe core does not silently insert dynamic types.

Examples might include:

```text
Any
Dyn
Opaque
```

Their presence is explicit and creates optimization boundaries.

---

# 89. REFLECTION BOUNDARY

Reflection metadata is optional.

Without reflection requirement, Forgen may strip type metadata from final binary.

---

# 90. OPTIMIZATION-RELEVANT SEMANTICS

The language semantics should expose enough information for:

```text
noalias inference
purity inference
constant folding
escape analysis
specialization
allocation elimination
branch elimination
loop fusion
```

The source syntax does not dictate these transformations.

---

# 91. OBSERVABLE BEHAVIOR

Optimizer MUST preserve:

```text
program result
specified side effects
required evaluation ordering
error behavior
FFI ABI
explicit determinism constraints
```

It MAY change:

```text
layout
allocation strategy
call structure
loop structure
execution strategy
```

when not observable under the language contract.

---

# 92. FLOATING POINT

Floating-point operations require a defined strictness policy.

Default mode:

```text
preserve language-specified ordering where observable
```

Relaxed numeric intent MAY permit additional reassociation if explicitly requested.

Example future profile:

```dtr
intent {
    numeric relaxed
}
```

---

# 93. DETERMINISM

`intent { deterministic true }` requests reproducible observable behavior where the runtime/target supports it.

Compiler MUST report unproven deterministic constraints.

---

# 94. RESOURCE INTENTS

Supported conceptually:

```dtr
intent {
    performance maximum
    memory minimum
    latency <= 2ms
    deterministic true
}
```

Exact grammar for project/module/function scopes will be frozen with manifest semantics.

---

# 95. TESTS

Language test syntax is not required in core v0.1.

Preferred project command:

```bash
forgen test
```

Tests may live in `.dtr` modules and use standard library assertions.

Compiler can access semantic graph to generate targeted diagnostics.

---

# 96. BENCHMARKS

```bash
forgen bench
```

Bench functions MAY be marked by package metadata rather than a new core keyword.

Benchmark results should include:

```text
compiler mode
CPU target
input size
iterations
runtime
memory
binary
```

---

# 97. SINGLE-FILE MODE

```bash
forgen run script.dtr
```

No manifest required.

Imports may use standard library and configured local paths.

---

# 98. PROJECT MODE

Project structure example:

```text
app/
    datara.toml
    src/
        main.dtr
        user/
            core.dtr
            billing.dtr
```

Exact manifest format will be specified separately.

---

# 99. RUST/TS/PYTHON MIGRATION

The language itself is not a compatibility shell, but source-to-source tools may later assist migration:

```bash
forgen migrate typescript
forgen migrate rust
forgen migrate python
```

These are tooling projects, not grammar commitments.

---

# 100. STYLE LINT

Compiler warnings MAY recommend native style without rejecting valid code.

Examples:

```text
DTR-STYLE-001: `function` is valid; `fn` is the preferred compact form.
DTR-STYLE-002: explicit type annotation is unnecessary here.
DTR-STYLE-003: inherited method can be expressed through `behavior`.
```

A future `strict-style` mode MAY turn selected style warnings into errors.

---

# 101. AI-READABILITY RULES

Source SHOULD have:

```text
stable constructs
few aliases
explicit error types
explicit effects in semantic graph
low ambiguity
small modules
predictable naming
```

AI tooling can consume semantic graph instead of raw text alone.

---

# 102. LIBRARY SEMANTIC CONTRACT

Libraries MAY ship machine-readable metadata describing:

```text
pure
allocates
parallel-safe
blocking
vectorizable
noalias
layout requirements
```

Forgen can use these facts in optimization, but only trusted/verified facts may influence safety proofs.

---

# 103. COMPILER DIAGNOSTIC CONTRACT

Compiler error structure:

```text
code
severity
locale
location
primary message
related spans
cause chain
suggestions
machine payload
```

---

# 104. FINAL LANGUAGE PRINCIPLES

1. Familiar enough to learn quickly.
2. New enough to have a reason to exist.
3. Strict enough to support compiler proofs.
4. High-level enough to be pleasant.
5. Low-level enough to build real systems.
6. Modular enough for large projects.
7. Dense enough for scripting and CLI.
8. Explicit enough for embedded/industrial work.
9. Semantic enough for aggressive whole-program compilation.
10. Structured enough for AI-assisted development.

---

# 105. LANGUAGE SUCCESS CRITERION

A programmer should be able to write:

```dtr
users := loadUsers()!

users
    |> filter(.active)
    |> map(.name)
    |> each(out)
```

without learning ownership syntax, allocator design, thread pools or compiler flags.

At the same time an advanced engineer must be able to write:

```dtr
fn process(data view Buffer<Float32>) -> Float32 {
    ...
}

parallel for item in data {
    ...
}

intent {
    latency <= 1ms
    memory <= 64MB
}
```

and expect Forgen to use that information without sacrificing safety.

---

# 106. OPEN ITEMS BEFORE LANGUAGE FREEZE

```text
exact bool/null coercion rules
integer overflow policy
exact numeric promotion rules
exhaustiveness algorithm
generic coherence
role conflict rules
component ordering rules
module cycle policy
compile-time evaluation limits
reflection API
macro policy
pattern binding ownership
async cancellation semantics
parallel exception/result semantics
ABI attributes
packed layout syntax
volatile/MMIO syntax
interrupt context syntax
```

These items are deliberately open until compiler prototype tests exist.

# 107. NORMATIVE EXAMPLE — SMALL CLI

```dtr
fn main() -> Int {
    args := args()

    if args.length == 0 {
        out "usage: app <file>"
        return 1
    }

    text := fs.readText(args[0])!
    out "bytes: {text.byteLength}"
    0
}
```

Interpretation:

- `args()` returns a typed argument collection supplied by the standard runtime;
- `!` propagates the declared error channel;
- `out` writes through the compiler-known output sink;
- no `console` object exists in the core language;
- the final integer is the function result.

---

# 108. NORMATIVE EXAMPLE — DATA PIPELINE

```dtr
records := csv.read("sales.csv")!

result := records
    |> where(.amount > 100)
    |> map(r => r.amount)
    |> reduce(sum)

out "total: {result}"
```

The semantics are equivalent to a dataflow graph. Intermediate collections are not observable unless materialized.

---

# 109. NORMATIVE EXAMPLE — CLASS + BEHAVIOR

```dtr
class Account {
    id AccountId
    balance Money
}

behavior Account {
    deposit(amount Money) {
        balance += amount
    }

    withdraw(amount Money) -> Bool {
        if amount > balance {
            return false
        }
        balance -= amount
        true
    }
}
```

A separate file may add:

```dtr
behavior Account {
    toJson() -> Json {
        ...
    }
}
```

There is one semantic `Account`; there need not be one physical class layout per source file.

---

# 110. CLASS CREATION RULES

Given:

```dtr
class User {
    name String
    age Int = 0
}
```

The following is valid:

```dtr
u := User { name: "Alex" }
```

The compiler supplies `age = 0`.

This is invalid:

```dtr
u := User {}
```

because `name` has no default and no value was provided.

Diagnostic MUST point to the missing field.

---

# 111. FIELD ACCESS

```dtr
user.name
user.age
```

For a mutable class instance in a legal mutable access context:

```dtr
user.age = 21
```

If a value is viewed immutably, mutation is rejected.

---

# 112. METHOD CALLS

Method receiver is implicit:

```dtr
user.rename("Bob")
```

The semantic form is equivalent to a function call with an instance receiver, but the exact calling convention is compiler-defined.

---

# 113. BEHAVIOR TARGET UNIQUENESS

A `behavior X` block must resolve `X` to exactly one type in the current package namespace.

Ambiguous target resolution is a compile error.

---

# 114. ROLE SATISFACTION

Given:

```dtr
role Hashable {
    hash() -> UInt64
}
```

and:

```dtr
class User with Hashable {
    hash() -> UInt64 => ...
}
```

The role obligation is satisfied.

If method signature does not match, compiler emits a role conformance diagnostic.

---

# 115. ROLE USAGE IN GENERICS

```dtr
fn makeHash<T: Hashable>(value T) -> UInt64 {
    value.hash()
}
```

A caller must supply a type satisfying `Hashable`.

This is conceptually capability-based generic programming.

---

# 116. COMPONENT COLLISIONS

Given:

```dtr
component A { id Int }
component B { id String }
```

this is invalid:

```dtr
class X {
    + A
    + B
}
```

unless the compiler can disambiguate field names through an explicit future namespace rule. v0.1 SHOULD reject the ambiguous composition rather than inventing an implicit precedence.

---

# 117. COMPOSITION ORDER

Composition order is semantic but does not imply dynamic dispatch precedence.

The compiler constructs a merged declaration graph.

Conflicts MUST be diagnosed explicitly.

---

# 118. `REPLACES` SAFETY

A replacement is valid only if:

```text
member exists on inherited path
signature is compatible
visibility does not weaken illegally
role obligations remain satisfied
```

Otherwise compilation fails.

---

# 119. INHERITANCE LIMIT

A class has at most one class base in v0.1.

This deliberately removes multiple-inheritance ambiguity.

Multiple capabilities are represented by composition/roles.

---

# 120. FINAL METHOD DISCUSSION

A future `final` modifier may exist if library authors need to forbid replacement.

It is not required for v0.1; the language first relies on explicit inheritance semantics and package API rules.

---

# 121. FUNCTION OVERLOADING

Overloading is allowed only when argument types make the call statically unambiguous.

Example:

```dtr
fn parse(value String) -> Int
fn parse(value Bytes) -> Int
```

An ambiguous generic overload must fail rather than use runtime dispatch.

---

# 122. DEFAULT ARGUMENTS

Preferred approach:

```dtr
fn connect(host String, port Int = 80) -> Connection!Error
```

The default is compile-time known and may be propagated.

Too many defaults should not be used to simulate configuration objects; records are preferred for large option sets.

---

# 123. NAMED ARGUMENTS

```dtr
connect(host: "localhost", port: 8080)
```

Names are resolved at compile time.

Renaming a parameter can therefore be a source-compatible consideration and should appear in API change analysis.

---

# 124. RECORD UPDATE

Proposed syntax:

```dtr
updated := user with {
    age: 21
}
```

Semantics: create a value equivalent to `user` with selected fields changed.

Compiler may apply structural update in place when ownership permits and no observation of the original is required.

---

# 125. DESTRUCTURING

```dtr
(name, age) := user
```

for tuple-like values, or:

```dtr
User { name, age } := user
```

for matching record/class fields where supported by the finalized pattern rules.

---

# 126. ENUM / SUM TYPE PATTERNS

```dtr
type ResultState = Pending | Ready(Data) | Failed(Error)
```

The compiler must know all variants for exhaustive matching.

A wildcard `_` explicitly accepts remaining variants.

---

# 127. EXHAUSTIVENESS

For:

```dtr
match state {
    Pending => ...
    Ready(data) => ...
}
```

with `Failed` also present, Forgen emits a non-exhaustive match error unless a wildcard or `Failed` branch exists.

---

# 128. DECIDE SEMANTICS

`decide` evaluates branches in source order.

Given:

```dtr
decide {
    checkA() => A
    checkB() => B
    else => C
}
```

`checkB()` is never evaluated if `checkA()` succeeds.

Therefore optimizer may not reorder effectful guards.

---

# 129. DECIDE WITH PURE GUARDS

If Forgen proves:

```text
all guards pure
no observable evaluation order
```

it may build a more efficient decision tree.

---

# 130. SELECT

`select` is expression-oriented and must produce a value.

Example:

```dtr
category := select {
    x < 0 => Negative
    x == 0 => Zero
    else => Positive
}
```

---

# 131. BOOLEAN OPERATORS

Short-circuit semantics are required for `and` and `or`.

Optimizer may transform them only when preserving effects and observable timing constraints.

---

# 132. NULL / OPTION OPERATORS

The language should provide a concise safe navigation/coalescing form, but exact tokens must be frozen after parser conflict testing.

Candidate:

```dtr
user?.profile?.name ?? "unknown"
```

This is a proposed v0.1 extension and remains open until grammar freeze.

---

# 133. ERROR PROPAGATION IN FLOW

```dtr
flow loadAndSave(path Path) -> Receipt!Error {
    file := fs.open(path)!
    data := parse(file)!
    save(data)!
}
```

A failed stage exits the flow along the error channel.

---

# 134. ERROR TRANSFORMATION

Errors can be mapped explicitly:

```dtr
value := parse(text) !> DomainError.Parse
```

The `!>` syntax is a candidate and remains subject to grammar review; libraries may initially use ordinary mapping functions instead.

---

# 135. TASK AND OWNERSHIP

A task that captures owned mutable state must satisfy task safety rules.

Example invalid concept:

```dtr
mut buffer := Buffer()
parallel {
    taskA(buffer)
    taskB(buffer)
}
```

If both tasks require conflicting mutable access, compiler rejects or requires explicit synchronization.

---

# 136. TASK RESULTS

Preferred pattern:

```dtr
parallel {
    a := taskA()
    b := taskB()
}

use(a, b)
```

The compiler models the synchronization point at the end of the parallel scope.

---

# 137. ASYNC CANCELLATION

The first release should define cancellation at task boundary rather than allowing arbitrary implicit cancellation from language syntax.

A cancellation token/standard capability can be introduced by the library/runtime.

---

# 138. IO BOUNDARIES

Any operation that performs external IO is effectful.

The compiler cannot treat it as pure even if its function body is opaque.

---

# 139. IMPURE FUNCTION OPTIMIZATION

Forgen may still inline impure functions if ordering and effects remain equivalent.

Purity is not an absolute requirement for inlining.

---

# 140. CONST FUNCTION EVALUATION

A future compile-time evaluator may run a pure function when:

```text
inputs are compile-time known
function uses allowed operations
no external side effects
bounded evaluation
```

---

# 141. NUMERIC PROMOTION

Numeric conversion rules must be explicit.

Unsafe implicit narrowing is forbidden.

Potential widening example:

```dtr
let x: Float64 = 10
```

may be accepted through a defined widening rule.

But:

```dtr
let x: Int8 = 1000
```

must be rejected unless explicit checked/unchecked conversion is requested.

---

# 142. OVERFLOW POLICY

Safe integer arithmetic should have a specified overflow behavior.

The preferred initial model is checked overflow in safety-oriented profiles and a defined wrapping primitive for low-level code.

Compiler may remove checks when range proof exists.

---

# 143. BOUNDS AND INDEXING

Normal indexing is safe:

```dtr
value := items[i]
```

If `i` cannot be proven valid, runtime bounds checks remain.

A separate `unsafe` raw indexing API can exist for systems code.

---

# 144. ITERATORS

Iterators are library/semantic graph objects, not necessarily heap objects.

Forgen should lower common iterator pipelines directly to loops.

---

# 145. `collect()` AS MATERIALIZATION BOUNDARY

`collect()` explicitly requests materialization.

This is important to the memory planner.

---

# 146. STREAM OWNERSHIP

A stream may borrow its source or own its source.

This must be represented in its type/semantic contract so a stream cannot outlive the resource it reads from.

---

# 147. FFI POINTER TYPES

Conceptual syntax:

```dtr
*U8
```

means raw pointer in unsafe/FFI contexts.

Safe code should prefer:

```dtr
view Bytes
```

or equivalent safe abstractions.

---

# 148. VOLATILE MEMORY

Embedded syntax remains a finalization item.

Candidate library-facing primitive:

```dtr
mmio.read<T>(address)
mmio.write<T>(address, value)
```

with compiler-known volatile semantics.

---

# 149. INLINE ASSEMBLY

Not part of safe core v0.1.

Future low-level support should be isolated behind an explicitly unsafe target-specific facility.

---

# 150. MODULE CYCLES

Module import cycles are allowed only if the semantic dependency graph remains well-founded. Cycles that make initialization order ambiguous should be rejected.

The implementation should prefer acyclic compile-time symbol dependency graphs even where runtime references form cycles.

---

# 151. INITIALIZATION ORDER

Global/module initialization must be deterministic.

The preferred v0.1 rule is to minimize implicit executable global initialization and prefer explicit initialization functions.

---

# 152. GLOBAL MUTABLE STATE

Allowed only when explicitly declared and subject to concurrency rules.

Example:

```dtr
mut globalCounter: Atomic<Int>
```

This area requires careful specification before language freeze.

---

# 153. FUNCTION VISIBILITY

Default function visibility is module-local.

```dtr
export fn parse(...) -> Data
```

exports the symbol.

---

# 154. TYPE VISIBILITY

Default class/record visibility is module-local.

Public types MUST expose all information needed to instantiate or use them without leaking private implementation details.

---

# 155. SEMVER COMPATIBILITY

Packages should define compatibility through public semantic interfaces, not private implementation layout.

---

# 156. SOURCE MAP REQUIREMENTS

Every diagnostic should preserve:

```text
file
line
column
span
symbol identity
```

IR transformations should retain source provenance where practical.

---

# 157. ERROR EXAMPLE — TYPE

```text
DTR-TYPE-001
Type mismatch

expected: Int
found: String

main.dtr:18:12
18 │ total += count
              ^^^^^

suggestion: convert `count` to Int explicitly.
```

---

# 158. ERROR EXAMPLE — OWNERSHIP

```text
DTR-BORROW-004
Cannot mutate `user.name` while `user` is borrowed for reading.

read borrow begins here:
12 │ inspect(user)

mutation occurs here:
15 │ user.name = "Bob"

suggestion: finish the read before mutation, or move the mutation into a separate scope.
```

---

# 159. ERROR EXAMPLE — ROLE

```text
DTR-ROLE-002
`FileCache` does not satisfy role `Serializable`.

missing operation:
    serialize() -> Bytes
```

---

# 160. AI MACHINE DIAGNOSTIC

Every error also supports machine payload:

```json
{
  "code": "DTR-BORROW-004",
  "kind": "ownership-conflict",
  "symbol": "User.name",
  "owner": "user"
}
```

---

# 161. LANGUAGE CONFORMANCE SUITE

A language release is conforming only if it passes:

```text
lexer suite
parser suite
semantic suite
type suite
ownership suite
effect suite
control-flow suite
FFI suite
runtime behavior suite
```

---

# 162. GOLDEN EXAMPLES

The specification should maintain canonical examples for:

```text
CLI
class
behavior
role
component
flow
parallel
async
Result
Option
generic
lambda
pattern matching
embedded
FFI
```

---

# 163. MIGRATION STYLE

Forgen MAY suggest:

```text
function → fn
explicit local type → inferred let/:=
override → replaces
manual utility class → module function
inheritance tree → composition with +
```

Migration suggestions never alter semantics automatically without user approval.

---

# 164. DEPRECATION POLICY

A language feature can move through:

```text
experimental
stable
deprecated
removed
```

Warnings must provide replacement syntax.

---

# 165. LANGUAGE CORE SIZE TARGET

The exact number of keywords is not the goal. The design target is a small core where every keyword corresponds to meaningful semantics.

---

# 166. FINAL LANGUAGE SURFACE

The canonical Datara surface is expected to revolve around:

```text
let / mut / const / :=
fn / function
class / record
behavior / role / component
from / with / + / replaces
if / match / decide / select
for / while / loop
flow / task
parallel / async
import / export
unsafe / extern
```

Everything else should first be considered a library or compiler service.

---

# 167. IMPLEMENTATION PRINCIPLE

Before freezing any syntax, implement at least one parser, resolver and semantic test for it.

Syntax that looks elegant but causes ambiguous parsing or poor diagnostics must be redesigned.

---

# 168. FINAL LANGUAGE NORTH STAR

> **Datara should make correct code feel simple, fast code feel normal, and advanced control feel available without becoming mandatory.**
