# Datara: Инвентаризация ядра и стандартной библиотеки

**Принцип архитектуры:** *«Минимальное доказуемое ядро компилятора — всё остальное выразимо в коде стандартной библиотеки»*.

---

## 1. Граница ответственности

```text
+--------------------------------------------------------------------+
|                           Forgen Core                              |
|  - Лексер, парсер, резолвер, строгий TypeChecker (SSA, эффекты)    |
|  - DMIR (CFG, Basic Blocks, Block Parameters, SSA Verifier)       |
|  - Оптимизатор (Evidence Gate, Mem2Reg, Dominance CSE, LoopFold)   |
|  - Cranelift Codegen (x86_64, Native MSVC/SysV Linker)            |
+--------------------------------------------------------------------+
                                  |
                                  v
+--------------------------------------------------------------------+
|                       Datara Standard Library                      |
|  - result / option: Outcome<T>, Maybe<T>, unwrap, map, and_then    |
|  - collections: List<T>, Map<K, V>, итераторы, фильтры             |
|  - text: String, Format, StringBuilder, StringView                 |
|  - io: FS, Args, Console, Path helpers                             |
|  - time: Clock, Duration, Timestamp                                |
+--------------------------------------------------------------------+
```

---

## 2. Что входит в ядро (Compiler Core)

1. **Скалярные примитивы:** `Int` (i64), `Float` (f64), `Bool` (i64/flag), `String` (fat ptr), `Unit`.
2. **Базовый Control Flow:** `if`/`else`, `while`, `for ... in ...`, `decide`, `match`, `return`.
3. **Объектная модель и композиция:**
   - `struct` / `class` (поля с естественным C ABI выравниванием).
   - `behavior` (методы над типами).
   - `role` / `component` / `with` (статическая плоская композиция без vtable).
4. **Семантика безопасности:**
   - Affine ownership и borrow checking (`View<T>`, `mut View<T>`).
   - Effect lattice (`Pure`, `IO`, `Network`, `Database`, `Unsafe`, `Nondeterministic`).
5. **Оператор распространения ошибок:**
   - Синтаксический сахар `?` и `!`, строго привязанный к структуре `Outcome<T>` / `Maybe<T>`.

---

## 3. Что вынесено в библиотеки (`stdlib/`)

1. **`stdlib/result/result.dtr`:**
   - Тип `Outcome<T> { is_success: Bool, value: T, error_msg: String }`.
   - Методы `is_ok()`, `is_err()`, `unwrap()`, `unwrap_or(default)`.
2. **`stdlib/result/option.dtr`:**
   - Тип `Maybe<T> { is_some: Bool, value: T }`.
   - Методы `has_value()`, `unwrap_or(default)`.
3. **`stdlib/collections/list.dtr` & `map.dtr`:**
   - Динамические массивы и хэш-таблицы поверх нативного рантайма.
4. **`stdlib/text/string.dtr` & `format.dtr`:**
   - Утилиты конкатенации, срезов, форматирования, экранирования.
5. **`stdlib/io/fs.dtr` & `args.dtr`:**
   - Файловые операции (чтение/запись строк) и аргументы командной строки.
6. **`stdlib/time/clock.dtr`:**
   - Монотонные часы и замеры времени.
