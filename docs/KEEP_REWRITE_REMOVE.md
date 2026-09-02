# KEEP / REWRITE / REMOVE AUDIT — DATARA + FORGEN

**Audit date:** 2026-08-30  
**Purpose:** keep the canonical design while removing unsupported implementation claims.

| Artifact / feature | Decision | Current rule |
|---|---|---|
| `Философия.md` | KEEP | Design authority for minimal core, semantic classes, composition, safety, and library boundaries. |
| `Спецификация языка.md` | KEEP, RECONCILE | Normative source, but unresolved syntax/semantic decisions must be recorded before freeze. |
| `Архитектура Компилятора.md` | KEEP, STAGE | Architecture target; implementation status must be tracked separately. |
| `План.md` | KEEP, EXECUTE IN GATES | Use as roadmap, but every phase needs source, tests, diagnostics, and evidence. |
| AI/ML/database/HTTP/GUI in core | KEEP OUT OF CORE | Provide through libraries/contracts unless a future language decision says otherwise. |
| `model` keyword | REWRITE / DEPRECATE | Resolve historical syntax against library-level domain contracts before parser freeze. |
| Multiple concrete inheritance | REMOVE | Use one base plus explicit composition if confirmed by the specification decision. |
| Silent `Any` fallback | REMOVE / BAN | Safe mode must reject unknown types with stable diagnostics. |
| Exception-first runtime model | REWRITE | Define Result/Option propagation and the limited try/catch boundary explicitly. |
| `import` versus `use` | RECONCILE | Choose one canonical spelling and define compatibility behavior. |
| `with` versus `from Base + Component` | RECONCILE | Choose one composition grammar and reject ambiguous alternatives. |
| Ad-hoc optimization flags | REMOVE | Use named compiler modes and evidence-based pass contracts. |
| SIMD/parallel/async claims without lowering | REMOVE | Candidate or rejected only until real backend integration exists. |
| Report-only pipeline/BCE/layout passes | REWRITE | `Applied` requires an observable DMIR/backend delta. |
| Static profile counts as PGO measurements | REMOVE | Static profiles cannot mutate budgets or claim runtime hotness. |
| Performance parity/freeze claims | REMOVE | Reissue only after reproducible, output-checked, phase-separated benchmarks. |
| Bilingual stable diagnostics | KEEP, REFACTOR | Preserve stable codes; expand span and phase coverage. |
