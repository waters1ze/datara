# Datara Style Guide & Linter Conventions

Datara enforces a clean, readable syntax aligned with native systems languages.

---

## 1. Naming Conventions

- **`snake_case`**: Variables, parameters, function names, method names.
  - Example: `let item_count = 10`, `fn compute_total()`.
  - Audit Rule: `style::non_snake_case`.
- **`PascalCase`**: Classes, entities, components, roles, packets, enums.
  - Example: `class UserAccount`, `enum HttpResponse`.
  - Audit Rule: `style::non_camel_case_types`.

---

## 2. Mutability & Register Promotion

- Prefer `let` over `mut` whenever a variable is not reassigned.
- Unnecessary `mut` flags prevent register allocation and trigger `perf::unnecessary_mut`.
- Auto-repair via `forgen lint --fix`.

---

## 3. Formatting Rules

- Use 4 spaces for indentation (no tabs).
- Run `forgen format` to format files automatically according to official compiler rules.
