# Datara Module Pattern & Multi-File Architecture

Datara provides three progressive project tiers designed to scale seamlessly from single-file scripts to enterprise-scale systems.

---

## 1. Progressive Project Tiers

### Level 1: Single File Script
- **Invocation**: `forgen run script.dtr` or `forgen check script.dtr`
- **Manifest**: Zero manifest needed. Ideal for competitive programming, scripts, and micro-tools.

### Level 2: Folder Project
- **Structure**: Flat directory containing `main.dtr` and adjacent `.dtr` module files.
- **Invocation**: `forgen run` (automatically discovers `main.dtr` and all adjacent modules).
- **Import Syntax**:
  ```datara
  use config.Config
  use analyzer.run_analysis
  ```

### Level 3: Full Package Architecture
- **Structure**:
  ```
  my_project/
  ├── datara.toml       # Package manifest & optimization profiles
  ├── src/
  │   ├── main.dtr      # Application entry point
  │   ├── core.dtr      # Domain classes and components
  │   └── service.dtr   # Business behaviors and processes
  └── tests/
      └── test_main.dtr # Integration tests
  ```
- **Manifest (`datara.toml`)**:
  ```toml
  [package]
  name = "my_project"
  version = "0.1.0"
  entry = "src/main.dtr"
  edition = "2026"

  [profiles.release]
  opt_level = "3"
  lto = true
  ```

---

## 2. Idiomatic Multi-File Separation

Datara cleanly separates **Data Record Structure** from **Behaviors**:

### Data Definitions (`src/core.dtr`)
```datara
class User {
    name: Str
    email: Str
    age: Int
}
```

### Business Logic (`src/service.dtr`)
```datara
use core.User

behavior User {
    greet() -> Str => fmt"Hello, {name}!"
    is_adult() -> Bool => age >= 18
}
```

### Entry Point (`src/main.dtr`)
```datara
use core.User
use service.User

fn main() {
    let u = User { name: "Maria", email: "maria@example.com", age: 28 }
    out u.greet()
}
```

---

## 3. Compiler Rules for Modules
1. No circular module imports allowed (`E-RESOLVE-004`).
2. Module symbols must follow `PascalCase` for types and `snake_case` for functions.
3. Use `forgen tree` to visualize module dependency hierarchy and effect capability flow.
