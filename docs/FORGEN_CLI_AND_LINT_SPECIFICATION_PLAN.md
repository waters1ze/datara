# Datara Official Code Style, Linting & Forgen CLI Expansion Specification

## 1. Official Datara Code Style Guide & Naming Conventions

### 1.1 Naming Rules
- **Local Variables & Function Parameters**: snake_case (e.g. user_count, item_id, 	otal_sum).
- **Mutable Variables**: mut snake_case (e.g. mut buffer_size, mut retry_count).
- **Constants & Values**: SCREAMING_SNAKE_CASE (e.g. al MAX_BUFFER_SIZE = 4096).
- **Classes, Structs, Enums**: PascalCase (e.g. Point3D, HttpClient, UserSession).
- **Components & Roles**: PascalCase (e.g. TransformComponent, AuthRole).
- **Functions & Methods**: snake_case (e.g. calculate_distance, etch_order).
- **Packets / Data Transfer Contracts**: PascalCase (e.g. UserCreatedEvent, OrderPayload).
- **Modules & Namespaces**: snake_case (e.g. 	ime::clock, math::geometry).

### 1.2 Variable Triad Idioms (let, mut, val)
1. **Unnecessary mut**:
   If a variable is declared mut x = 10, but its value is never reassigned in its scope:
   warning[perf::unnecessary_mut]: variable 'x' does not need to be mutable -> help: use 'let x = 10'.
2. **Unused Variables**:
   Any binding that is defined but never read:
   warning[style::unused_variable]: unused variable 'idx' -> help: prefix with underscore: '_idx'.

### 1.3 Loop & Control Flow Idioms
1. **Countable Range Loops (prefer_for_loop)**:
   Countable while-loops with index increments:
   mut i = 0; while i < 100 { ... i = i + 1 }
   warning[style::prefer_for_loop]: manual while loop index increment detected -> help: use 'for i in 0..100'.
2. **Redundant Condition Parentheses**:
   if (x > 10) -> help: remove redundant parentheses: if x > 10.
3. **Redundant Boolean Comparison**:
   if is_ready == true -> help: simplify condition to: if is_ready.
   if is_ready == false -> help: simplify condition to: if !is_ready.

## 2. The forgen lint Static Analysis Engine
- **Command**: orgen lint [target] [--fix]
- **Rich Diagnostics**: ANSI color-highlighted source snippets, caret pointers, error codes, and suggestions.
- **Auto-Fix**: Automatically re-writes unmutated mut to let, strips redundant parentheses, and simplifies booleans.

## 3. The New Forgen CLI Commands
- orgen clean: Removes 	arget/ and temporary compilation outputs (.ll, .pdb, .obj).
- orgen watch: Fast file watcher triggering instant compilation (~40ms) on file save.
- orgen explain <code>: Displays detailed terminal documentation and good/bad code examples for error and lint codes.
- orgen tree: Displays dependency hierarchy with effect lattice capability audits ([pure], [io], [net], [mut]).
- orgen doc: Generates single-file interactive documentation.
- orgen repl: Interactive JIT terminal.
