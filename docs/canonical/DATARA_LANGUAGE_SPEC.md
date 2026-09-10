# DATARA LANGUAGE SPECIFICATION v1.0 (CANONICAL FREEZE)

**Language:** Datara  
**Source Extension:** .dtr  
**Specification Version:** 1.0.0-canonical  
**Status:** Normative Standard — Frozen Specification for 1.0 Release.

---

## 1. FORMAL GRAMMAR (EBNF)

Below is the complete, closed Extended Backus-Naur Form (EBNF) specification of the Datara language syntax.

`ebnf
(* Lexical Grammar *)
Letter              = "A" .. "Z" | "a" .. "z" | "_" | UnicodeLetter ;
Digit               = "0" .. "9" ;
HexDigit            = Digit | "A" .. "F" | "a" .. "f" ;
OctDigit            = "0" .. "7" ;
BinDigit            = "0" | "1" ;

Identifier          = ( Letter ) { Letter | Digit } ;
IntLiteral          = [ "-" ] ( "0x" { HexDigit }+ | "0b" { BinDigit }+ | "0o" { OctDigit }+ | { Digit }+ ) [ IntSuffix ] ;
IntSuffix           = "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "usize" | "isize" ;
FloatLiteral        = [ "-" ] { Digit }+ "." { Digit }+ [ ( "e" | "E" ) [ "+" | "-" ] { Digit }+ ] [ FloatSuffix ] ;
FloatSuffix         = "f32" | "f64" ;
BoolLiteral         = "true" | "false" ;
StringLiteral       = '"' { StringChar | EscapeSeq } '"' ;
CharLiteral         = "'" ( CharContent | EscapeSeq ) "'" ;
EscapeSeq           = "\" ( "n" | "t" | "r" | "\\" | '"' | "'" | "0" | "x" HexDigit HexDigit | "u{" { HexDigit }+ "}" ) ;

(* Compilation Unit *)
CompilationUnit     = { ImportDecl | TopLevelDecl } ;

ImportDecl          = "use" ImportPath [ "as" Identifier ] [ ";" ] ;
ImportPath          = Identifier { "::" Identifier } ;

TopLevelDecl        = [ "pub" ] ( FunctionDecl
                                | ClassDecl
                                | StructDecl
                                | EnumDecl
                                | TraitDecl
                                | ImplDecl
                                | RoleDecl
                                | ComponentDecl
                                | EntityDecl
                                | PacketDecl
                                | ExternDecl
                                | GlobalVarDecl ) ;

(* Declarations *)
FunctionDecl        = [ AttributeList ] ( "fn" | "function" ) Identifier [ GenericParamList ] "(" [ ParamList ] ")" [ "->" TypeExpr ] [ EffectSpec ] Block ;
GenericParamList    = "<" GenericParam { "," GenericParam } ">" ;
GenericParam        = Identifier [ ":" TypeBoundList ] ;
TypeBoundList       = TypeBound { "+" TypeBound } ;
TypeBound           = Identifier [ GenericArgList ] ;

ParamList           = Param { "," Param } ;
Param               = [ "mut" ] Identifier ":" TypeExpr ;

EffectSpec          = "/" Identifier { "+" Identifier } ;

ClassDecl           = "class" Identifier [ GenericParamList ] [ "with" TraitList ] "{" { ClassItem } "}" ;
StructDecl          = "struct" Identifier [ GenericParamList ] "{" { StructField } "}" ;
EnumDecl            = "enum" Identifier [ GenericParamList ] "{" { EnumVariant } "}" ;
TraitDecl           = "trait" Identifier [ GenericParamList ] "{" { TraitItem } "}" ;
ImplDecl            = "impl" [ GenericParamList ] Identifier [ "for" TypeExpr ] "{" { ImplItem } "}" ;

RoleDecl            = "role" Identifier [ GenericParamList ] "{" { ClassItem } "}" ;
ComponentDecl       = "component" Identifier [ GenericParamList ] "{" { ClassItem } "}" ;
EntityDecl          = "entity" Identifier [ GenericParamList ] "{" { ClassItem } "}" ;
PacketDecl          = "packet" Identifier "{" { StructField } "}" ;

TraitList           = TypeExpr { "," TypeExpr } ;
ClassItem           = FieldDecl | MethodDecl ;
StructField         = [ "pub" ] Identifier ":" TypeExpr [ ";" ] ;
FieldDecl           = [ "pub" ] [ "mut" ] Identifier ":" TypeExpr [ "=" Expr ] [ ";" ] ;
MethodDecl          = [ "pub" ] ( "fn" | "function" ) Identifier [ GenericParamList ] "(" [ ParamList ] ")" [ "->" TypeExpr ] [ EffectSpec ] ( Block | ";" ) ;
EnumVariant         = Identifier [ "(" [ TypeList ] ")" | "{" { StructField } "}" ] [ "=" IntLiteral ] [ "," ] ;
TraitItem           = ( "fn" | "function" ) Identifier [ GenericParamList ] "(" [ ParamList ] ")" [ "->" TypeExpr ] [ EffectSpec ] ( Block | ";" ) ;
ImplItem            = MethodDecl ;

ExternDecl          = "extern" StringLiteral ( "fn" | "function" ) Identifier "(" [ ParamList ] ")" [ "->" TypeExpr ] [ ";" ] ;
GlobalVarDecl       = ( "let" | "mut" | "const" ) Identifier [ ":" TypeExpr ] "=" Expr [ ";" ] ;

AttributeList       = { Attribute }+ ;
Attribute           = "@" Identifier [ "(" [ AttrArgList ] ")" ] ;
AttrArgList         = AttrArg { "," AttrArg } ;
AttrArg             = Expr | Identifier "=" Expr ;

(* Types *)
TypeExpr            = SimpleType | GenericType | ReferenceType | ArrayType | TupleType | FunctionType ;
SimpleType          = Identifier ;
GenericType         = Identifier "<" TypeList ">" ;
ReferenceType       = "&" [ "mut" ] TypeExpr ;
ArrayType           = "[" TypeExpr ";" Expr "]" | "[" TypeExpr "]" ;
TupleType           = "(" TypeList ")" ;
FunctionType        = "fn" "(" [ TypeList ] ")" [ "->" TypeExpr ] ;
TypeList            = TypeExpr { "," TypeExpr } ;

(* Statements *)
Block               = "{" { Stmt } "}" ;
Stmt                = VarDeclStmt
                    | AssignStmt
                    | ExprStmt
                    | IfStmt
                    | WhileStmt
                    | ForStmt
                    | MatchStmt
                    | ReturnStmt
                    | OutStmt
                    | RequireStmt
                    | EnsureStmt
                    | UnsafeBlockStmt
                    | WithResourceStmt ;

VarDeclStmt         = ( "let" | "mut" | "val" ) Identifier [ ":" TypeExpr ] "=" Expr [ ";" ] ;
AssignStmt          = LValue AssignOp Expr [ ";" ] ;
AssignOp            = "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=" ;
ExprStmt            = Expr [ ";" ] ;

IfStmt              = "if" Expr Block [ "else" ( IfStmt | Block ) ] ;
WhileStmt           = "while" Expr Block ;
ForStmt             = "for" Identifier "in" Expr Block ;
MatchStmt           = "match" Expr "{" { MatchArm } "}" ;
MatchArm            = Pattern [ "if" Expr ] "=>" ( Expr [ "," ] | Block ) ;

ReturnStmt          = "return" [ Expr ] [ ";" ] ;
OutStmt             = "out" Expr [ ";" ] ;
RequireStmt         = "requires" Expr [ ";" ] ;
EnsureStmt          = "ensures" Expr [ ";" ] ;
UnsafeBlockStmt     = "unsafe" Block ;
WithResourceStmt    = "with" Expr [ "as" Identifier ] Block ;

(* Patterns *)
Pattern             = LiteralPattern
                    | IdentPattern
                    | WildcardPattern
                    | EnumPattern
                    | TuplePattern
                    | StructPattern ;
LiteralPattern      = IntLiteral | FloatLiteral | BoolLiteral | StringLiteral ;
IdentPattern        = [ "mut" ] Identifier ;
WildcardPattern     = "_" ;
EnumPattern         = Identifier [ "::" Identifier ] [ "(" [ PatternList ] ")" ] ;
TuplePattern        = "(" PatternList ")" ;
StructPattern       = Identifier "{" { FieldPattern } "}" ;
FieldPattern        = Identifier [ ":" Pattern ] [ "," ] ;
PatternList         = Pattern { "," Pattern } ;

(* Expressions *)
Expr                = DecideExpr | BinaryExpr ;
DecideExpr          = "decide" "{" { DecideArm } [ "else" "=>" Expr ] "}" ;
DecideArm           = Expr "=>" Expr [ "," ] ;

BinaryExpr          = UnaryExpr { BinaryOp UnaryExpr } ;
BinaryOp            = "||" | "&&"
                    | "==" | "!=" | "<" | "<=" | ">" | ">="
                    | "|" | "^" | "&" | "<<" | ">>"
                    | "+" | "-" | "*" | "/" | "%" ;

UnaryExpr           = [ UnaryOp ] PrimaryExpr ;
UnaryOp             = "-" | "!" | "~" | "*" | "&" | "&mut" ;

PrimaryExpr         = Atom { PostfixOp } ;
PostfixOp           = CallOp | IndexOp | FieldAccessOp | MethodCallOp | QuestionOp ;
CallOp              = "(" [ ArgList ] ")" ;
IndexOp             = "[" Expr [ ".." [ Expr ] ] "]" ;
FieldAccessOp       = "." Identifier ;
MethodCallOp        = "." Identifier [ GenericArgList ] "(" [ ArgList ] ")" ;
QuestionOp          = "?" ;

GenericArgList      = "<" TypeList ">" ;
ArgList             = Arg { "," Arg } ;
Arg                 = [ Identifier ":" ] Expr ;

Atom                = Identifier
                    | IntLiteral
                    | FloatLiteral
                    | BoolLiteral
                    | StringLiteral
                    | CharLiteral
                    | TupleExpr
                    | ArrayExpr
                    | StructInitExpr
                    | WrappingExpr
                    | SaturatingExpr
                    | "(" Expr ")" ;

TupleExpr           = "(" Expr "," [ ArgList ] ")" ;
ArrayExpr           = "[" [ ArgList ] "]" ;
StructInitExpr      = Identifier [ GenericArgList ] "{" [ FieldInitList ] "}" ;
FieldInitList       = FieldInit { "," FieldInit } [ "," ] ;
FieldInit           = Identifier [ ":" Expr ] ;

WrappingExpr        = "wrapping" "(" Expr ")" ;
SaturatingExpr      = "saturating" "(" Expr ")" ;

LValue              = Identifier { PostfixOp } ;
`

---

## 2. MEMORY MODEL & OWNERSHIP

Datara enforces memory safety at compile time without requiring a global tracing garbage collector.

### 2.1 Ownership & Affine Types
1. Every value has exactly one owner binding at any point during program execution.
2. Assignment of a non-Copy value moves ownership from the source to the destination.
3. Once moved, reading the source binding triggers diagnostic [E-BORROW-001] (use-after-move).

### 2.2 Borrowing & Views
1. **Immutable Views (&T)**: Multiple concurrent read-only views may co-exist over the same storage region.
2. **Mutable Views (&mut T)**: A mutable view has strictly exclusive access to the target region. No other views (mutable or immutable) may exist concurrently.
3. Violation of view exclusivity triggers diagnostic [E-BORROW-006] or [E-BORROW-004].
4. Views cannot outlive their referenced root binding ([E-BORROW-005]).

### 2.3 Zero-Cost Slices & Reference Counting Fallback
1. Slices and sub-views over contiguous memory (View<T>, [T], &str) are zero-overhead fat pointers consisting of { pointer: u64, length: u64 }.
2. Where static ownership cannot be proven (such as complex cyclical graph nodes), Datara provides standard library reference-counted containers (Arc<T>) that decrement their counter atomically upon out-of-scope drop.

---

## 3. TYPE SYSTEM SEMANTICS

### 3.1 Primitive Types
- **Signed Integers:** Int (alias for i64), Int32 (i32), Int16 (i16), Int8 (i8), isize.
- **Unsigned Integers:** UInt (u64), UInt32 (u32), UInt16 (u16), UInt8 (u8), Byte (u8), usize.
- **Floating Point:** Float (alias for 64), Float32 (32).
- **Boolean:** Bool (distinct 1-byte logical type, non-coercible to/from integer values).
- **Unit:** Unit (zero-sized type ()).

### 3.2 Strings & Characters
- String: Dynamically allocated, growable UTF-8 buffer { ptr: *u8, len: usize, cap: usize }.
- Str / &str: Non-owning view into a valid UTF-8 byte slice { ptr: *u8, len: usize }.
- Char: 32-bit Unicode scalar point (values $ to $, excluding surrogate pairs).

### 3.3 Sum Types & Algebraic Data Types
1. Outcome<T, E> is represented as a tagged sum type containing an active discriminant tag (u8) followed by an aligned union of payloads $ and $.
2. Pattern matching (match) over sum types is checked for exhaustiveness at compile time ([E0310]). Unreachable match arms are reported as errors ([E0311]).

### 3.4 Traits & Polymorphism
1. **Monomorphization**: Generic functions and classes parameterized by concrete types are specialized at compile time without runtime dispatch overhead.
2. **Dynamic Trait Objects (dyn Trait)**: When runtime polymorphism is required, trait objects are represented as two-word fat pointers { instance_ptr: *void, vtable_ptr: *const VTable }.

---

## 4. OVERFLOW & ARITHMETIC SEMANTICS

Datara guarantees deterministic arithmetic behavior across all operating systems and architectures (Windows x86_64, Linux, macOS, WebAssembly):

1. **Default Arithmetic:**
   - Standard binary operators (+, -, *) perform checked arithmetic.
   - Overflow on critical paths traps unconditionally with exit code != 0 in both Debug and Release modes, avoiding silent data corruption.
2. **Explicit Wrapping:**
   - Evaluated via wrapping(a + b) or intrinsic wrapping_*(a, b).
   - Guaranteed two's-complement modular arithmetic modulo 2^N.
3. **Explicit Saturating:**
   - Evaluated via saturating(a + b) or intrinsic saturating_*(a, b).
   - Clamps values to [T_min, T_max] on overflow/underflow.
4. **Division by Zero:**
   - Integer division or remainder with divisor 0 traps immediately with standard machine trap code (INTEGER_DIVISION_BY_ZERO).
   - i64::MIN / -1 and i64::MIN % -1 trap with INTEGER_OVERFLOW.

---

## 5. STRING & UNICODE GUARANTEES

1. **Encoding:** All string contents in Datara are verified UTF-8 sequences. Construction from non-UTF-8 bytes fails with diagnostic or runtime error.
2. **Length Semantics:**
   - str_len(s) / s.byte_len(): Returns length in bytes in O(1) time.
   - s.char_len(): Traverses the UTF-8 sequence, returning the count of Unicode scalar values in O(N) time.
3. **Slicing:**
   - Substring indexing s[start..end] operates in O(1) time over byte indices.
   - Indexing into the middle of a multi-byte UTF-8 character boundary traps deterministically.
4. **Grapheme Clusters:**
   - Complex Unicode cluster boundaries (e.g. skin-tone emojis, combining diacritics) are handled via std::unicode::graphemes.

---

## 6. CONCURRENCY & WAVEFRONT SCHEDULER

1. **Actor Model & Tasks:**
   - Independent concurrent routines are spawned as lightweight tasks (	ask).
   - Communication between tasks occurs exclusively via typed channels (Channel<T>).
2. **Proof-Carrying Wavefront Scheduling:**
   - Pure, independent task DAGs are compiled into deterministic wavefront schedules.
   - Topological dependency ordering guarantees lock-free execution without data races ([E0943]).
3. **Thread Boundaries (@shared):**
   - Types sent across threads must satisfy thread-safety invariants (deeply immutable or uniquely moved ownership).

---

## 7. EFFECTS SYSTEM & COMPOSITION RULES

Functions and methods may declare their observable side-effects:

`dtr
fn compute_hash(data: String) -> String / pure
fn fetch_record(id: Int) -> Record / io
fn async_fetch(url: String) -> Response / io + async
`

### 7.1 Effect Classes
- pure: Pure calculation. Deterministic, zero I/O, no heap allocation if annotated @no_alloc.
- io: Interacts with operating system, disk, environment, or network.
- sync: Asynchronous cooperative execution context.
- unsafe: Interacts with raw memory pointers or external C ABI boundaries.

### 7.2 Composition Rules
1. A function marked / pure MUST NOT invoke functions with / io or / unsafe effects ([E-EFFECT-001]).
2. Calling foreign C code or performing raw pointer dereferencing requires an explicit unsafe { ... } block ([E-EFFECT-002], [E0942]).
3. Side effects in @no_alloc contexts cannot invoke dynamic memory allocation ([E0950]).
4. @no_panic contexts must be statically verified to have zero panic paths ([E0951]).

---

## 8. SEMVER GUARANTEES

Datara 1.0.0 normative standard establishes strict semantic versioning guarantees across all 1.x releases:

### 8.1 Syntax & Language Core Stability
1. **Grammar Invariance:** The formal EBNF grammar defined in Section 1 is frozen. No valid Datara 1.0 program will fail to parse or compile on any future 1.x compiler release.
2. **Deterministic Arithmetic:** Integer overflow trap guarantees (Section 4), wrapping/saturating intrinsic semantics, and division-by-zero behaviors are permanent and immutable across all backends.
3. **Type System:** Primitive numeric types (`i8..i64`, `u8..u64`, `f32`, `f64`, `usize`, `isize`, `byte`, `char`), sum-types (`Outcome<T, E>`), and reference lifetimes will maintain backward-compatible typechecking rules.

### 8.2 Polyglot Foreign Function Interface (C ABI)
1. **Layout Stability:** Struct memory representations conforming to standard C ABI (`#[repr(C)]`) maintain byte-identical alignment and sizing.
2. **Export Symbols:** The exported C API functions in `datara.h` (`forgen_create_runtime`, `forgen_eval_source`, `forgen_call_fn`, `forgen_free_runtime`) preserve their exact signatures, calling conventions, and ABI compatibility across 1.x.
3. **Linker Compatibility:** Native object artifacts emitted by Forgen remain linkable with MSVC `link.exe`, GNU `ld`, and LLVM `lld`.

### 8.3 Package Ecosystem & DPM / Sparks Guarantees
1. **Lockfile Stability:** The `datara.lock` schema (v1) remains forward- and backward-compatible. DPM 1.x will always accept and deterministically reproduce dependency graphs from version 1 lockfiles.
2. **Manifest Schema:** `datara.toml` format and metadata requirements remain non-breaking.
3. **Sparks Sparse Protocol:** Sparse index layout (`packages/<name>/<version>.json`), SHA-256 Merkle digests, and ed25519 signature formats are standardized and frozen under Schema Version 1.

### 8.4 Compatibility & Deprecation Policy (0.1.x -> 1.0.0)
- The legacy untyped `Outcome` runtime string field `error_msg` is formally deprecated in favor of typed `Outcome<T, E>` sum-type layout.
- Non-deterministic global effect escapes without explicit capability tokens are rejected at compile time ([E0940]).
- Breaking changes or semantic modifications will only be considered under Datara 2.0 with a minimum 12-month deprecation cycle.

---

**End of Datara Language Specification v1.0**
