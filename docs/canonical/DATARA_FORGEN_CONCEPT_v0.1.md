# DATARA + FORGEN — ПОЛНАЯ КОНЦЕПЦИЯ v0.1 (CONSOLIDATED)

**Язык:** Datara  
**Компилятор / toolchain:** Forgen  
**Предлагаемое расширение:** `.dtr`  
**Статус:** архитектурный концепт v0.1 — consolidated baseline

> **Datara — это строготипизированный, memory-safe, нативный язык, в котором человек пишет простую и выразительную программу, а Forgen понимает её семантику целиком и превращает её в специализированное исполнение под конкретную задачу и железо.**

---

## 0. МАНИФЕСТ

Datara не должна быть «ещё одним Python», «TypeScript на Rust» или «упрощённым Rust». Она должна объединять сильные стороны существующих подходов и отбрасывать их исторический багаж.

Из Python берём выразительность, плотность кода, удобную работу с коллекциями и data workflows.

Из JavaScript/TypeScript берём знакомый синтаксический стиль, быстрый вход, modules, generics, inference и удобное описание структур. TypeScript уже показывает практическую ценность вывода типов из контекста; Datara сохраняет это удобство, но с нативной строгой типовой моделью. [TS Type Inference]

Из Rust берём инженерную планку: ownership, borrowing, отсутствие обязательного GC, compile-time контроль памяти и data races. В Rust ownership/borrowing используются для memory safety без garbage collector; Datara должна стремиться к той же силе гарантий, но скрывать большую часть когнитивной нагрузки. [Rust Book]

Из C/C++ берём полноценный доступ к системному уровню, ABI, hardware и минимальный runtime там, где он необходим.

Из LLVM и современных компиляторов берём идею множества анализов и transformations: optimizer должен не просто «включить O3», а понимать граф программы, её зависимости, значения, эффекты и target. LLVM прямо строит optimizer вокруг analysis/transform passes; Forgen будет использовать этот принцип, но главным источником информации для него будет собственная semantic model Datara. [LLVM Passes]

### Главный закон

> **Не заставлять человека выражать вручную то, что компилятор способен надёжно вывести или доказать. И не брать с runtime плату за информацию, которую compiler уже знает.**

### Второй закон

> **Абстракция считается хорошей, если её можно сохранить в исходном коде и убрать из машинного кода.**

### Третий закон

> **Маленький surface, глубокий compiler.**

---


# КОНСОЛИДИРОВАННЫЙ СЛОЙ РЕШЕНИЙ v0.1

> Этот документ является **единым базовым концептом Datara + Forgen v0.1**.
> Весь исходный первый концепт сохранён ниже без удаления его разделов.
> Этот слой фиксирует решения, которые были приняты после первой версии, и устраняет противоречия, возникшие в ходе дальнейшего проектирования.

## Правило интеграции

Первый концепт остаётся историческим фундаментом и источником всех исходных идей. Новые решения не должны незаметно уничтожать уже сформированные идеи: они либо уточняют их, либо переводят конкретную возможность из **ядра языка** в **стандартную библиотеку / extension / target module**, если эта возможность не нужна каждому Datara-приложению.

## Главное новое уточнение: минимальное ядро

Datara должна быть **обычным универсальным языком в ядре**. Прикладные области не должны превращать язык в огромный DSL. Поэтому:

```text
DATARA CORE
    ↓
stdlib / modules
    ↓
extensions
    ↓
target profiles
    ↓
application
```

В ядре остаются только механизмы, которые нужны для самой модели языка: типы, значения, функции, классы, composition, roles, behavior, generics, control flow, pattern/decision, ownership/safety, effects, concurrency primitives, module semantics и низкоуровневый FFI contract.

AI/ML, базы данных, HTTP, CSV, tensor operations, LLM/модели, industrial APIs, GPIO и специализированные CLI-frameworks не являются обязательными частями языка. Они предоставляются через стандартные или официальные библиотеки и extensions, сохраняя возможность для Forgen видеть их semantics там, где библиотека предоставляет compiler contracts.

## AI: убрать `model` из ядра

Предыдущая идея отдельной языковой конструкции `model` считается **необязательной и вынесенной из core**. Модель, нейросеть, tokenizer, tensor runtime и т.п. реализуются библиотеками Datara. Это важно, потому что AI является областью применения языка, а не самой сутью языка.

При этом библиотека может предоставлять высокоуровневый API, а Forgen может узнавать специальные compiler contracts через extensions/attributes/IR interfaces. Поэтому получается:

```text
Datara core
    ↓
AI library
    ↓
compiler-aware library contracts
    ↓
Forgen optimization
```

а не:

```text
Datara core = AI language
```

## `task` сохраняется как универсальная конструкция

`task` не считается AI-конструкцией. Это общая единица работы, которую можно применять к автоматизации, параллельному вычислению, IO, data processing и другим задачам. AI может использовать `task`, но `task` не знает об AI.

## CLI: язык даёт быстрый фундамент, CLI-экосистема — библиотека

Нужно сохранить идею удобного:

```dtr
out "Hello, {name}"
err "Invalid argument"
```

но сложная декларативная CLI-модель (`app`, `command`, parser, completion и т.п.) должна быть реализуема стандартной библиотекой/официальным extension. Таким образом быстрый output является частью базового execution/runtime contract, а полноценный CLI framework — не обязательным ядром языка.

## OOP v0.1 — не старый `class`, а современная модель

Класс остаётся главным знакомым входом для OOP-разработчика, но его semantics строятся вокруг:

```text
identity
+ state
+ behavior
+ roles/capabilities
+ components
+ composition
```

При этом `behavior` можно выносить в отдельные файлы; `component` является композиционной частью состояния/структуры; `role` описывает capability/contract; наследование допускает только один базовый class и дополняется композицией через `+`.

Пример нативного стиля:

```dtr
class Admin from User + Permissioned + Audited {
    permissions Permissions
}
```

Идея `+` считается частью нового OOP surface: она визуально показывает, что дополнительные возможности **собираются**, а не образуют ещё один лес наследования.

## Компактный синтаксис не является runtime-моделью

Запись:

```dtr
name String
age Int
```

не имеет никакого отношения к тому, как поле будет храниться в памяти. После parsing и semantic analysis Forgen получает полную типовую информацию. Поэтому сокращение `:` является только surface-level ergonomic feature и не ухудшает optimization quality.

## Двойной путь для новичка и эксперта

Datara допускает более явную запись там, где человек хочет контроля, и более короткую запись там, где компилятор может вывести semantics безопасно. При этом язык может предупреждать, если явный стиль противоречит рекомендованному idiomatic style. На ранних версиях это остаётся предупреждением, а не запретом.

Принцип:

```text
beginner → compiler infers
expert   → compiler can accept explicit intent
```

## Переменные

Базой остаются `let`, `mut`, `const`, но добавляется короткая форма `:=` как отдельная ergonomic construction, **не аналог Go declaration semantics**, а shorthand для новой локальной immutable binding при однозначном inference:

```dtr
count := 10
name := "Alex"
```

Если type inference неоднозначен, компилятор обязан сообщить, что тип нужно уточнить. В safe mode скрытого динамического `any` нет.

## Функции

Основная форма:

```dtr
fn add(a Int, b Int) -> Int => a + b
```

и полноразмерная:

```dtr
fn add(a Int, b Int) -> Int {
    a + b
}
```

`function` допускается как читабельный синоним, чтобы старым разработчикам было легче войти. Стилевой анализатор может рекомендовать `fn`, но ранняя версия не запрещает `function`.

## Новая модель условий

Обычный `if/else` сохраняется, но для многоветочной логики основной современный инструмент — `decide`, а для data/state matching — `match`.

```dtr
decide {
    score >= 90 => grade = 'A'
    score >= 75 => grade = 'B'
    score >= 60 => grade = 'C'
    else => grade = 'F'
}
```

`decide` выражает **условия как набор независимых правил**, поэтому Forgen может анализировать взаимную исключительность, range coverage, branch frequency и перестраивать control flow.

`match` остаётся структурным сопоставлением значений/типов:

```dtr
match state {
    Ready(data) => use(data)
    Failed(error) => err error
    _ => wait()
}
```

Это различие должно сохранять семантическую ясность и давать optimizer больше информации, чем произвольная цепочка `if`.

## Ownership и явность

Автоматическое выведение остаётся основным путём. Если оно недостаточно, разрешаются явные формы:

```dtr
own Data
view Data
mut-view Data
shared Data
```

Эти конструкции являются human-readable surface для ownership/aliasing contracts. Компилятор обязан сохранять тот же уровень функциональности и безопасности, что и в inferred path.

## Ошибки

`Result` остаётся основным error channel, но допускается структурированный `try/catch` как boundary-friendly sugar поверх Result semantics. Это позволяет новичку писать привычно, не возвращая исключения в качестве основной скрытой модели контроля потока.

## Concurrency

Основной принцип:

```dtr
parallel {
    a = loadA()
    b = loadB()
}
```

означает «операции независимы и могут выполняться совместно», а не «создать N потоков». `async` сохраняется как явный advanced mechanism, но compiler может сам строить asynchronous execution plan там, где semantics позволяют.

Для циклов:

```dtr
parallel for item in data {
    process(item)
}
```

явно сообщает Forgen о независимой итерации.

## Основная performance-цель

Цель проекта фиксируется как engineering target, а не маркетинговая гарантия:

> Для сопоставимых алгоритмов и одинакового target hardware Datara/Forgen должны стремиться к native performance максимально близкой к Rust; допустимое отставание порядка 1–2% считается приемлемым, а идеальный результат — равная или лучшая производительность на отдельных workload-классах. При этом Datara должна стабильно превосходить интерпретируемые/виртуализированные сценарии Python и быть конкурентной либо быстрее JS/TS на соответствующих CPU workloads.

Никакой claim о том, что язык «всегда быстрее Rust», в концепт не вносится.

## Главный смысл `domain`

`domain` — не набор флагов и не просто максимальный `-O`. Это сборка, где Forgen понимает весь проект и принимает решения на основе:

```text
what is used
what is reachable
what is hot
what is pure
what effects exist
what data shapes are real
what target hardware exists
what runtime features are needed
what memory/layout choices are profitable
```

После этого compiler может делать specialization, cross-module optimization, devirtualization, layout transforms, allocation elimination, loop fusion/fission, vectorization, parallelization, multi-versioning, PGO и runtime stripping.

## Главное правило модульности

Файл не является optimization barrier. Организация кода для человека и физическая организация бинарника — разные уровни.

```text
source files
    ↓
semantic graph
    ↓
reachable graph
    ↓
specialized IR
    ↓
optimized artifact
```

Именно поэтому splitting class/behavior по файлам не должно ухудшать runtime.

## AI-friendly по умолчанию, но не AI-язык

AI-инструменты используют compiler semantic API:

```text
types
effects
ownership
dependencies
call graph
flow graph
contracts
optimization report
tests
```

Однако AI не является частью core semantics. Это **особый consumer semantic graph**, так же как IDE, static analyzer или documentation generator.

## Русские диагностики

Forgen должен иметь многоязычную diagnostic system. Русский язык может быть официальным localization target. Сначала поддержка English + Russian, затем дополнительные языки. Внутренние diagnostic IDs остаются стабильными независимо от языка интерфейса.

```text
DG1234 = canonical diagnostic ID
ru-RU  = русское описание
en-US  = английское описание
```

Это позволяет не ломать IDE, CI и AI tooling при смене языка терминала.

## Компилятор как система доказательств

Каждая серьёзная оптимизация должна сохранять invariants:

```text
type safety
memory safety
ownership invariants
effect invariants
control-flow correctness
observable semantics
```

Forgen не может считать оптимизацию успешной только потому, что она быстрее. Она должна быть **доказуемо корректной в рамках доступной semantic information**.

## Новая формула языка

```text
FAMILIAR SURFACE
        +
NEW SEMANTIC MODEL
        +
AUTOMATIC SAFETY
        +
WHOLE-PROGRAM KNOWLEDGE
        +
SPECIALIZATION
        +
MINIMAL CORE
        =
DATARA
```

## Новая формула Forgen

```text
SOURCE
  ↓
MEANING
  ↓
PROOF
  ↓
PROGRAM GRAPH
  ↓
SPECIALIZATION
  ↓
COST MODEL
  ↓
TARGET CODE
```


# 1. ЧТО ИМЕННО МЫ СТРОИМ

Datara — язык общего назначения, рассчитанный на четыре больших слоя одновременно:

```text
APP / CLI
DATA / AUTOMATION
AI / COMPUTE
SYSTEMS / EMBEDDED
```

Но это не четыре режима языка. Это один язык с разными target profiles и разными lowering strategies.

Один и тот же исходник должен потенциально использоваться:

```text
в CLI
в desktop app
на сервере
в data pipeline
в локальном AI inference
в embedded controller
в промышленной автоматике
в WASM
```

Различается не surface syntax, а то, во что Forgen преобразует semantic graph.

---

# 2. ГЛАВНЫЙ ПРОТИВНИК — НЕ ДРУГИЕ ЯЗЫКИ

Datara не нужно «победить Python, Rust и TypeScript». У неё должна быть собственная инженерная ниша:

**простота + строгая безопасность + нативная производительность + semantic-aware compilation.**

Вместо соревнования «у кого больше keywords» конкурентным преимуществом должен стать следующий принцип:

```text
человек пишет WHAT
          ↓
Datara фиксирует semantics
          ↓
Forgen решает HOW
          ↓
hardware выполняет минимально необходимое
```

---

# 3. БАЗОВАЯ МОДЕЛЬ ПРОГРАММЫ

Datara не будет чисто ООП-языком, не будет чисто функциональным языком и не будет чисто dataflow DSL.

Её базовая семантика строится вокруг:

```text
DATA
BEHAVIOR
ROLE
COMPONENT
FLOW
```

### DATA
Что существует и какие данные образуют значение/сущность.

### BEHAVIOR
Какие операции допустимы над данными.

### ROLE
Какую способность или контракт объект предоставляет.

### COMPONENT
Переиспользуемый фрагмент структуры/состояния.

### FLOW
Как данные и операции проходят через систему.

Эти понятия могут смешиваться на source level, но в compiler graph должны оставаться различимыми.

---

# 4. ПОЧЕМУ МЫ НЕ УБИРАЕМ ООП

Потому что программируют люди, а не идеологические комитеты.

Большое количество разработчиков понимает мир через сущности:

```text
User
Order
Drone
Machine
Database
Model
Window
```

Поэтому классы должны остаться.

Но класс Datara — **не обещание старого object runtime**.

```text
class в исходнике
≠
обязательно heap object
≠
обязательно virtual dispatch
≠
обязательно object header
```

Это semantic abstraction.

Forgen имеет право превратить её в:

```text
struct
inline value
stack object
scalar SSA values
packed storage
registers
```

если observable semantics сохраняется.

---

# 5. НОВЫЙ DATARA-CLASS

Рабочая форма:

```dtr
class User {
    name String
    age Int
    active Bool = true

    isAdult() -> Bool {
        age >= 18
    }

    rename(to newName String) {
        name = newName
    }
}
```

Что здесь намеренно изменено относительно привычного TypeScript/Java/C# style:

- field type можно писать без `:`;
- `this` не нужен в обычном member behavior;
- constructor не обязателен;
- getter/setter boilerplate не нужен;
- initialization expression является нормальным способом создания;
- compiler не считает class обязательным heap allocation;
- поведение можно вынести в другой файл без создания subclass.

Создание:

```dtr
user = User {
    name: "Alex"
    age: 20
}
```

`new` не нужен по умолчанию.

---

# 6. ПОЧЕМУ `name String` НЕ ВРЕДИТ ОПТИМИЗАЦИИ

Не вредит.

Синтаксис исходника не является IR.

Все варианты:

```dtr
name: String
name String
name = "Alex"
```

после semantic analysis могут стать одной сущностью:

```text
FieldSymbol
  name = name
  type = String
  mutability = ...
  layout = compiler selected
```

Следовательно, отсутствие `:` не делает язык динамическим и не уменьшает возможности optimizer.

Более того, Datara может разделить два уровня:

```text
surface declaration
semantic type
```

Если тип выводится, compiler всё равно знает его абсолютно точно.

### Правило

```dtr
let x = 42
```

означает строго типизированный `Int`, а не `Any`.

Если компилятор не может однозначно вывести тип:

```text
compile error → add type annotation
```

Никакого скрытого `any` в safe/default mode.

---

# 7. ПЕРЕМЕННЫЕ

### Значение по умолчанию — immutable binding

```dtr
let count = 10
```

### Изменяемость — явно

```dtr
mut count = 10
count += 1
```

### Compile-time constant

```dtr
const BUFFER_SIZE = 4096
```

Compiler должен использовать это знание для specialization.

Например bounds, размеры массивов, switch branches и buffer allocation могут стать compile-time facts.

---

# 8. TYPE SYSTEM

Основные группы:

```text
primitives
records
classes
sum types
generics
roles
views
optionals
results
```

Типовая система должна быть:

- статической;
- строгой;
- выводимой;
- предсказуемой;
- пригодной для compiler proofs;
- пригодной для AI analysis.

---

# 9. OPTIONAL И RESULT

Предлагаемая поверхность:

```text
T?      optional
T!E     result/error
```

Пример:

```dtr
function findUser(id UserId) -> User? {
    ...
}

function loadUser(id UserId) -> User!DbError {
    ...
}
```

Ошибка распространяется:

```dtr
user = loadUser(id)!
```

Это должно быть одной из центральных конструкций control-flow analysis.

---

# 10. НЕТ КЛАССИЧЕСКОГО EXCEPTION-FIRST DESIGN

Exceptions могут использоваться на boundaries, но основной путь Datara — explicit result flow.

Причины:

```text
предсказуемый control flow
лучший static analysis
проще AI reasoning
проще optimizer
лучше latency predictability
```

---

# 11. RECORD

Для чистых значений:

```dtr
record Point {
    x Float
    y Float
}
```

Record должен стремиться к zero-overhead value semantics.

---

# 12. COMPONENT

```dtr
component Timestamped {
    createdAt Instant
    updatedAt Instant
}
```

Использование:

```dtr
class User with Timestamped {
    name String
}
```

`component` — не отдельный runtime object. Это compositional declaration.

---

# 13. ROLE

Вместо нескольких почти одинаковых конструкций interface/trait/mixin/protocol предлагается одна пользовательская идея:

```dtr
role Serializable {
    serialize() -> Bytes
}
```

И:

```dtr
class User with Serializable {
    ...
}
```

`role` отвечает на вопрос «что эта сущность гарантированно умеет», а не «от какого класса она наследуется».

---

# 14. BEHAVIOR

Поведение может жить отдельно:

```dtr
behavior User {
    validate() -> Bool {
        name != "" && age >= 0
    }
}
```

Compiler семантически объединяет behavior с User.

Runtime wrapper не создаётся.

---

# 15. SPLIT CLASS — ОДНА ИЗ КЛЮЧЕВЫХ ФИШЕК

```text
user/
    core.dtr
    billing.dtr
    security.dtr
    serialization.dtr
```

`core.dtr`:

```dtr
class User {
    id UserId
    name String
}
```

`billing.dtr`:

```dtr
behavior User {
    invoice() -> Invoice {
        ...
    }
}
```

Для человека это один User.

Для compiler это отдельные incremental units.

Для AI это логические slices контекста.

Для Domain это один semantic graph.

---

# 16. НАСЛЕДОВАНИЕ

Классическое:

```dtr
class Admin extends User {
    permissions Permissions
}
```

может существовать ради совместимости мышления.

Но native Datara style:

```dtr
class Admin from User with Permissioned, Audited {
    permissions Permissions
}
```

Смысл:

```text
one base identity
+
capabilities
+
composed behavior
```

Multiple class inheritance не является целью.

---

# 17. КОМПОЗИЦИЯ ВМЕСТО ЛЕСА НАСЛЕДОВАНИЯ

```dtr
component Permissioned {
    permissions Permissions
}

component Audited {
    createdAt Instant
    updatedAt Instant
}

class Admin with Permissioned, Audited {
    name String
}
```

Compiler может inline'ить component state.

---

# 18. STATIC

Мы не хотим заставлять пользователя писать `static` там, где семантика очевидна.

Если логика не зависит от instance state, можно использовать модульную функцию:

```dtr
module User {
    createGuest() -> User {
        User { name: "Guest", age: 0 }
    }
}
```

Вызов:

```dtr
User.createGuest()
```

Это снижает boilerplate.

Для специальных ABI/reflection cases `static` может остаться как advanced keyword.

---

# 19. GENERICS

Простой случай:

```dtr
class Box<T> {
    value T
}

box = Box { value: 10 }
```

Forgen выводит `T = Int`.

Ограничение:

```dtr
function save<T: Serializable>(value T) -> Bytes {
    value.serialize()
}
```

Generic system должен позволять compiler выбирать между:

```text
monomorphization
shared implementation
specialized hot path
```

по cost model.

---

# 20. ЛЯМБДЫ

Короткая форма:

```dtr
users.map(x => x.name)
```

Блок:

```dtr
users.map(user => {
    normalized = normalize(user)
    return score(normalized)
})
```

Forgen должен по возможности:

```text
inline closure
remove environment allocation
stack-promote capture
specialize generic call
```

---

# 21. PIPELINE

```dtr
result = data
    |> normalize()
    |> filter(x => x.score > 0.8)
    |> map(.value)
    |> reduce(sum)
```

Pipeline — semantic graph, а не просто syntactic sugar.

Compiler может:

```text
fuse
inline
vectorize
parallelize
eliminate intermediate containers
```

---

# 22. FLOW

```dtr
flow processOrder(order Order) -> Receipt!OrderError {
    order
        |> validate()
        |> calculate()
        |> reserve()
        |> pay()
        |> ship()
}
```

`flow` явно говорит compiler, что перед ним named execution graph.

Это позволяет tooling показывать graph программы без угадывания.

---

# 23. PARALLEL

```dtr
parallel {
    users = loadUsers()
    orders = loadOrders()
    products = loadProducts()
}
```

Это не должно буквально означать «создай три thread».

Semantic meaning:

> эти операции не зависят друг от друга и могут выполняться независимо.

Forgen может выбрать:

```text
sequential
thread pool
work stealing
async task
SIMD
GPU
```

смотря что выгоднее.

---

# 24. EFFECT SYSTEM

Compiler должен уметь выводить effects:

```text
Pure
Read
Write
IO
Network
Database
Unsafe
Parallel
Nondeterministic
```

Например:

```dtr
function add(a Int, b Int) -> Int {
    a + b
}
```

является pure без обязательной annotation.

А:

```dtr
function save(user User) -> Result {
    database.write(user)
}
```

получает `DatabaseWrite + IO`.

Это помогает и оптимизации, и AI.

---

# 25. PURE FUNCTIONS

Если Forgen доказал pure semantics, он может:

```text
constant-fold
memoize locally
remove duplicate call
reorder
parallelize
execute at compile-time
```

Это одна из главных причин встроить effects в semantic model.

---

# 26. MEMORY MODEL

Цель:

> Rust-level safety goals without forcing everyday Rust-level syntax.

Основная memory architecture:

```text
ownership
borrowing
alias analysis
escape analysis
lifetime inference
```

Но подавляющее большинство пользователя пишет:

```dtr
let data = load()
process(data)
```

а не lifetime annotations.

---

# 27. OWNERSHIP

Каждое значение имеет однозначный ownership model.

Forgen способен доказать:

```text
who owns value
who borrows value
whether value escapes
whether aliasing is legal
```

Если proof успешен, дополнительные runtime checks могут исчезнуть.

---

# 28. BORROWING

Обычная ситуация:

```dtr
read(user => {
    out user.name
})
```

Мутация:

```dtr
edit(user => {
    user.name = "Bob"
})
```

Такой API может позволить выразить exclusive/shared access, не заставляя пользователя вручную управлять lifetimes.

---

# 29. ADVANCED OWNERSHIP SYNTAX

Только если compiler не может вывести relationship:

```dtr
Borrow<T>
Owned<T>
View<T>
Shared<T>
```

Эти формы должны быть доступны, но не навязываться beginners.

---

# 30. UNSAFE

```dtr
unsafe {
    memory.write(address, value)
}
```

`unsafe` локализует ручной контроль.

Compiler должен не только отмечать unsafe region, но и отслеживать его boundary.

---

# 31. DATA RACE SAFETY

Safe code должен стремиться к гарантиям:

```text
no use-after-free
no invalid aliasing
no data race
no unchecked memory access
```

Это долгосрочный критерий безопасности, а не маркетинговое обещание до появления proof suite.

---

# 32. STRING MODEL

Предлагается:

```text
String      owned text
Str         borrowed text view
Bytes       binary data
```

Это важно для memory layout и FFI.

---

# 33. COLLECTIONS

Core должен иметь немного основных контейнеров:

```text
List<T>
Array<T, N>
Map<K,V>
Set<T>
Queue<T>
Deque<T>
```

Не делать сотню похожих containers в первой версии.

---

# 34. TABLE / STREAM / TENSOR

Datara должна различать:

```text
Table
Stream
Tensor
```

потому что их compiler semantics разные.

Table — column/data analysis.

Stream — lazy/online processing.

Tensor — numeric/AI graph.

---

# 35. TABLE EXAMPLE

```dtr
users = table.read("users.csv")!

result = users
    |> where(age >= 18)
    |> select(name, age)
    |> groupBy(country)
    |> aggregate(avg(age), count())
```

Forgen может видеть не четыре collection operations, а один graph и выполнить:

```text
column pruning
fusion
vectorization
parallel reduction
без промежуточных таблиц
```

---

# 36. TENSOR

```dtr
x Tensor<Float32>[B, N, H]
```

Compiler может проверять shape compatibility.

Дальше:

```dtr
x |> normalize() |> matmul(weights) |> softmax()
```

может стать fused tensor graph.

---

# 37. MODEL

AI должен быть частью языка, но не магией:

```dtr
model TinyClassifier {
    weights Tensor<Float32>

    forward(x Tensor<Float32>) -> Tensor<Float32> {
        x
            |> normalize()
            |> matmul(weights)
            |> softmax()
    }
}
```

`model` полезен потому, что compiler видит graph целиком.

---

# 38. LLM / AI SYSTEMS

Большая цель:

```text
Tokenizer
Embedding
Transformer
Attention
KV cache
Sampling
Decoder
```

могут быть реализованы на Datara без обязательной Python runtime layer.

Это не означает, что Python нельзя импортировать. Это означает, что Datara имеет собственный путь до native AI execution.

---

# 39. MODEL OPTIMIZATION

Domain compiler может применять:

```text
operator fusion
buffer reuse
memory planning
layout selection
SIMD kernels
GPU kernels
constant folding
shape specialization
batch specialization
```

Если модель только для inference, training-only state может быть вырезан.

---

# 40. DEVICE ABSTRACTION

Рабочая идея:

```dtr
device auto
```

и advanced:

```dtr
device cpu
device gpu
```

Compiler рассматривает transfer cost и availability.

GPU не должен автоматически считаться быстрее CPU.

---

# 41. KERNEL

Будущий surface:

```dtr
kernel scale(x Buffer<Float32>, factor Float32) {
    i = index()
    x[i] *= factor
}
```

Kernel lowering — отдельный backend layer.

---

# 42. CLI — ПЕРВАЯ «ДОМЕННАЯ ФИШКА»

Никакого обязательного `console.log`.

```dtr
out "Hello, {name}"
err "Invalid argument"
```

Compiler знает эти sinks и может использовать:

```text
buffered IO
fast formatting
specialized formatter
minimal runtime
```

---

# 43. DECLARATIVE CLI

Потенциально:

```dtr
cli app "grepfast" {
    command search {
        pattern String
        path Path
        ignoreCase Bool = false

        run {
            searchFile(path, pattern, ignoreCase)!
                |> each(out)
        }
    }
}
```

Из semantic declaration Forgen может автоматически получить:

```text
argument parser
help
validation
completion metadata
usage
error handling
```

Это пример semantic compression.

---

# 44. FAST FORMATTER

```dtr
out "{user.name}: {user.age}"
```

Для простых типов compiler знает format shape заранее.

---

# 45. UNITS OF MEASURE

Для системного/industrial кода полезно:

```dtr
speed = 80 km/h
period = 5 ms
voltage = 24 V
```

И type system не должен позволять передать `Time` туда, где требуется `Length`.

Это потенциально одна из самых полезных domain-independent features.

---

# 46. STATE MACHINES

Для embedded/industrial:

```dtr
machine Door {
    Closed
    Opening
    Open
    Closing

    Closed -> Opening when openCommand
    Opening -> Open when position == 100%
    Open -> Closing when closeCommand
    Closing -> Closed when position == 0%
}
```

Compiler способен сделать compact deterministic representation.

---

# 47. REAL-TIME CONSTRAINTS

```dtr
intent {
    latency <= 2ms
    deterministic = true
}
```

Важно: Forgen не должен притворяться, что доказал то, чего доказать не может.

Если guarantee не доказана:

```text
constraint not proven
```

---

# 48. INTENT — ОДНА ИЗ ГЛАВНЫХ НОВЫХ ИДЕЙ

Вместо множества микрографиков:

```text
@inline
@fast
@whatever
```

предлагается semantic constraint:

```dtr
intent {
    performance = maximum
    memory = low
    deterministic = true
}
```

Разработчик сообщает **цель и ограничения**, а не диктует optimizer конкретный метод.

---

# 49. COST MODEL

Forgen должен выбирать оптимизацию на основе:

```text
CPU
cache
vector width
memory bandwidth
branch behavior
input size
allocations
call frequency
parallel overhead
binary size
startup latency
power budget
GPU transfer cost
```

Поэтому `domain` не должен быть просто «больше флагов оптимизации». Он должен быть **decision engine**.

---

# 50. SEMANTIC GRAPH

Это сердце всего проекта.

Compiler должен строить граф:

```text
symbols
modules
types
calls
data flows
effects
ownership
roles
behaviors
resources
models
hardware constraints
```

Из него строится optimized program.

---

# 51. ПОЧЕМУ FILES НЕ ЯВЛЯЮТСЯ OPTIMIZATION BOUNDARY

Разработчик пишет:

```text
model.dtr
tokenizer.dtr
tensor.dtr
main.dtr
```

Domain видит:

```text
all source
→ one semantic graph
→ reachable program
→ specialization
```

Файлы остаются полезными для человека, но compiler смотрит сквозь них.

---

# 52. INCREMENTAL COMPILATION

`start` не должен пересобирать весь проект.

Cache key примерно:

```text
source hash
+ dependency interface hash
+ compiler version
+ target
+ profile
```

Изменил `billing.dtr` → не нужно пересобирать tokenizer и hardware backend.

---

# 53. ПАРАЛЛЕЛЬНАЯ КОМПИЛЯЦИЯ

Если:

```text
A ─┐
B ─┼→ Core
C ─┘
```

то A/B/C компилируются параллельно.

Forgen сам должен использовать parallel compiler architecture.

---

# 54. КЭШИРОВАНИЕ IR

Хранить не только binary:

```text
AST
name resolution
inferred types
semantic graph slices
DMIR
backend artifacts
profile data
```

Это ускорит и `start`, и `domain`.

---

# 55. АРХИТЕКТУРА FORGEN

```text
Source
  ↓
Lexer
  ↓
Parser
  ↓
AST
  ↓
Resolver
  ↓
Type Checker / Inference
  ↓
Effect Analysis
  ↓
Ownership / Borrow Analysis
  ↓
Semantic Graph
  ↓
HIR
  ↓
DMIR / SSA-like IR
  ↓
Specialization Engine
  ↓
Optimization Engine
  ↓
Target Lowering
  ↓
LLVM / native backend / future custom backend
  ↓
Linker / Packager
  ↓
Native artifact
```

---

# 56. LLVM НЕ ЯВЛЯЕТСЯ МОЗГОМ

LLVM можно использовать как backend infrastructure.

Но Datara нельзя просто перевести в LLVM IR и надеяться на чудо.

До LLVM должны жить:

```text
semantic graph
ownership facts
effects
flow graph
intent constraints
AI/tensor semantics
```

Именно там лежит уникальность языка.

---

# 57. DATARA IR

Предлагаются уровни:

```text
DAST — syntax
HIR — semantic program
DGraph — cross-module semantic graph
DMIR — optimized machine-independent IR
Target IR — CPU/GPU/WASM/MCU
```

Не обязательно физически делать пять отдельных форматов в первой версии; важно сохранить концептуальные границы.

---

# 58. OPTIMIZER

Минимальный набор:

```text
constant folding
constant propagation
dead code elimination
dead data elimination
inlining
devirtualization
generic specialization
closure elimination
escape analysis
allocation elimination
buffer reuse
loop optimization
vectorization
SIMD lowering
parallelization analysis
data layout optimization
cross-module optimization
LTO
PGO
```

LLVM уже предоставляет большое количество analysis/transform passes; Forgen должен добавлять собственные passes, которые используют более богатую semantic information Datara. [LLVM Passes]

---

# 59. ZERO-COST OOP

Пример:

```dtr
class Point {
    x Float
    y Float

    length() -> Float {
        sqrt(x*x + y*y)
    }
}
```

Если `Point` локальный и concrete type известен, compiler может превратить весь class abstraction в операции над двумя floats.

Не обязательно:

```text
heap
vtable
header
virtual call
```

---

# 60. DE-VIRTUALIZATION

```dtr
role Renderer {
    draw()
}
```

Если Domain видит только `SvgRenderer`, он может сделать:

```text
dynamic call → direct call
```

Это позволяет иметь высокоуровневую polymorphism model без автоматического runtime penalty.

---

# 61. ESCAPE ANALYSIS

Если объект:

```text
создан внутри функции
не возвращён
не сохранён
не передан в unknown FFI
```

то heap allocation может исчезнуть.

---

# 62. DATA LAYOUT OPTIMIZATION

Compiler может выбирать:

```text
AoS
SoA
AoSoA
```

для hot collections, если ABI boundary не фиксирует layout.

Это позволит писать удобные objects, но исполнять их в cache-friendly representation.

---

# 63. BOUNDS CHECK ELIMINATION

```dtr
for i in 0..values.length {
    sum += values[i]
}
```

Если compiler доказывает valid range:

```text
runtime bounds check → remove
```

Если не может доказать:

```text
minimal check remains
```

---

# 64. SPECIALIZATION ПО ФАКТИЧЕСКОМУ ИСПОЛЬЗОВАНИЮ

Если библиотека содержит:

```text
1000 functions
```

а приложение использует 17 reachable functions, Domain binary не должен тащить остальные.

Если generic вызывается только как `List<Float32>`, compiler может не генерировать лишние specializations.

---

# 65. WHOLE-PROGRAM DOMAIN BUILD

Главная цепочка:

```text
all modules
↓
reachability
↓
semantic graph
↓
usage analysis
↓
specialization
↓
optimization
↓
target tuning
↓
LTO
↓
minimal runtime
```

Это и есть смысл `domain`.

---

# 66. `START`

```bash
forgen start
```

Цель — мгновенная разработка.

```text
incremental
moderate optimization
fast codegen
good diagnostics
debug metadata
```

Это не benchmark build.

---

# 67. `DEBUG`

```bash
forgen debug
```

Максимум информации:

```text
source maps
ownership diagnostics
bounds diagnostics
runtime checks where useful
sanitizer-compatible hooks
```

---

# 68. `RELEASE`

```bash
forgen release
```

Серьёзная production optimization без полной стоимости Domain.

---

# 69. `DOMAIN`

```bash
forgen domain
```

Это **максимальный safe whole-program compilation mode**.

```text
deep inference
whole-project graph
specialization
cross-module inlining
layout optimization
vectorization
parallel analysis
LTO
optional PGO
runtime stripping
```

---

# 70. ДОПОЛНИТЕЛЬНЫЕ РЕЖИМЫ

```bash
forgen profile
forgen inspect
forgen verify
forgen embedded
```

`profile` — собрать performance data.

`inspect` — объяснить semantic/optimization model.

`verify` — максимально строгая проверка.

`embedded` — deterministic/no-heavy-runtime target profile.

---

# 71. `DOMAIN` И ПАМЯТЬ

Domain должен уметь определить:

```text
which runtime features are reachable
which allocators are needed
which collection types are used
which error paths exist
which reflection metadata exists
```

Если dynamic reflection не включён, огромное количество metadata можно не включать.

---

# 72. MINIMAL RUNTIME

Runtime состоит из модулей:

```text
memory
core IO
formatting
tasks
sync
platform bindings
```

Если сеть не используется — networking runtime не линкуется.

Если GUI не используется — GUI runtime не нужен.

---

# 73. DYNAMIC FEATURES И OPTIMIZATION

Dynamic loading, reflection и plugins не запрещать.

Но они должны быть явными boundaries.

```text
dynamic boundary
→ less compiler knowledge
→ less specialization
```

То есть developer понимает цену динамичности.

---

# 74. AI-FRIENDLY LANGUAGE

AI должен видеть не просто текст.

Forgen tooling может выдавать:

```text
AST
symbol table
types
effects
ownership
module graph
call graph
optimization report
test links
```

Это превращает compiler в semantic API для AI.

---

# 75. AI CONTEXT

Пример:

```bash
forgen context --symbol User.checkout
```

результат:

```text
symbol: User.checkout
affects: DatabaseRead, Network
returns: Receipt!CheckoutError
dependencies: Cart, Payment, Shipping
ownership: borrowed input, owned receipt
unsafe: false
tests: checkout_success, checkout_declined
```

AI получает именно нужный кусок knowledge graph, а не весь repository.

---

# 76. AI SEMANTIC DIFF

После AI refactor Forgen должен показывать:

```text
public API changed: no
new unsafe region: no
new IO effect: yes
memory behavior changed: no
new dependency: billing
```

Это гораздо полезнее простого text diff.

---

# 77. COMPILER EXPLAINABILITY

```bash
forgen inspect optimize
```

Пример:

```text
analyze():
  inlined: yes
  allocation removed: 12
  SIMD: enabled
  parallel: rejected
  reason: input size below threshold
  specialization: Float32
```

Compiler объясняет не только ошибку, но и оптимизацию.

---

# 78. ПЕРВАЯ ПОТЕНЦИАЛЬНО УНИКАЛЬНАЯ ФИШКА — SEMANTIC COMPRESSION

Одна конструкция должна задавать несколько согласованных вещей.

Например:

```dtr
cli app "convert" {
    command json {
        input Path
        run => convert(input)
    }
}
```

Из этого compiler выводит:

```text
argument schema
parser
help
validation
command entry point
```

Человеку — мало текста.

Compiler — получает много информации.

---

# 79. ВТОРАЯ ФИШКА — EXECUTION SPECIALIZATION

Один source способен получить разные binaries:

```text
start
release
domain
embedded
domain + profile
```

Причём semantic program одинаковая.

---

# 80. ТРЕТЬЯ ФИШКА — SPLIT BEHAVIOR

Класс можно разбивать по аспектам без наследования и без wrappers.

Это одновременно:

```text
архитектура
incremental compilation
AI context slicing
```

---

# 81. ЧЕТВЁРТАЯ ФИШКА — COMPILER-SEEN FLOW

`flow` позволяет compiler видеть graph напрямую.

Это даёт:

```text
optimization
visualization
testing
AI reasoning
performance analysis
```

---

# 82. ПЯТАЯ ФИШКА — INTENT/CONSTRAINTS

Пользователь сообщает не конкретную optimization pass, а цель:

```dtr
intent {
    latency <= 5ms
    memory <= 64MB
}
```

Forgen ищет реализацию, но обязан сообщить, что доказал, а что нет.

---

# 83. ШЕСТАЯ ФИШКА — ОДНА СХЕМА, МНОГО ГЕНЕРАЦИЙ

Будущая конструкция:

```dtr
schema User {
    id UserId
    name String
}
```

может породить:

```text
validation
serialization
deserialization
CLI argument model
API docs
AI tool schema
```

Compiler делает это из одной semantic description.

---

# 84. AI: ЛОКАЛЬНАЯ МОДЕЛЬ КАК ОБЫЧНАЯ ПРОГРАММА

Datara должна позволять реализовать модель напрямую:

```dtr
model TinyLLM {
    embedding Tensor<Float16>[V,H]
    layers List<TransformerLayer>

    forward(tokens Tensor<Int>) -> Tensor<Float16> {
        x = embedding[tokens]
        for layer in layers {
            x = layer(x)
        }
        x
    }
}
```

Forgen должен понимать не только вызовы функций, но и tensor graph.

Это позволяет Domain compiler планировать:

```text
memory
kernels
layout
fusion
parallelism
device
```

---

# 85. AI НЕ ДОЛЖЕН БЫТЬ ВМОНТИРОВАН «МАГИЕЙ»

Не нужно делать:

```dtr
ai.solveEverything(...)
```

AI capability должна быть инженерной:

```text
model
Tensor
Stream
Kernel
Device
Inference
Training
```

и использовать общий compiler pipeline.

---

# 86. LOCAL-FIRST AI

AI-модели должны работать:

```text
local CPU
local GPU
local NPU/accelerator
```

Облачные API — внешние integrations.

Язык не должен требовать интернет для базовой работы compiler/toolchain.

---

# 87. PYTHON INTEROP

Python interop полезен, особенно для существующей AI/data ecosystem.

Но архитектурно:

```text
Datara core
↕
FFI / Python boundary
↕
Python ecosystem
```

а не:

```text
Datara = Python runtime wrapper
```

Hot path желательно компилировать в Datara native code.

---

# 88. JS/TYPESCRIPT INTEROP

Для web/WASM можно иметь:

```dtr
js.import("package")
```

Но Datara-side code продолжает иметь native semantics.

WASM/WASI рассматривается как дополнительный target; актуальная экосистема WebAssembly поддерживает модульность и системный интерфейс WASI. [WebAssembly Specs]

---

# 89. RUST/C/C++ FFI

Системный слой должен иметь FFI.

```dtr
extern "C" function native_call(ptr *U8, size Int) -> Int
```

Все foreign boundary должны быть видимы для semantic graph.

---

# 90. FFI COST AWARENESS

Forgen может предупреждать:

```text
FFI calls: 18,000,000
estimated boundary overhead: high
recommendation: batch calls
```

Это часть compiler diagnostics.

---

# 91. EMBEDDED

Datara не должна иметь отдельный язык для микроконтроллеров.

Нужен target profile:

```bash
forgen embedded
```

Он выбирает:

```text
no GC
minimal runtime
deterministic allocations
hardware ABI
interrupt/task support
small code size
```

---

# 92. HARDWARE

Потенциальные стандартные abstractions:

```dtr
hardware GPIO
hardware UART
hardware SPI
hardware I2C
hardware Timer
hardware PWM
hardware ADC
```

Пример:

```dtr
led = gpio(13)

loop {
    led.high()
    sleep(500ms)
    led.low()
    sleep(500ms)
}
```

Compiler должен по возможности превращать это в прямой peripheral access без unnecessary abstraction.

---

# 93. INDUSTRIAL

Datara подходит для:

```text
controllers
robots
machines
sensors
automation
industrial gateways
```

потому что имеет:

```text
strict types
units
state machines
real-time intent
hardware access
predictable memory
```

---

# 94. UNITS OF MEASURE

```dtr
speed = 80 km/h
voltage = 24 V
delay = 5 ms
```

Compiler должен ловить очевидные ошибки единиц.

Для production industrial code это потенциально важнее части syntactic sugar.

---

# 95. STATE MACHINE

```dtr
machine Door {
    Closed
    Opening
    Open
    Closing
    Fault

    Closed -> Opening when openCommand
    Opening -> Open when position >= 100%
    Open -> Closing when closeCommand
    Closing -> Closed when position <= 0%
    * -> Fault when emergencyStop
}
```

Compiler может построить deterministic state machine.

---

# 96. REAL-TIME INTENT

```dtr
intent {
    latency <= 2ms
    jitter <= 200us
    deterministic = true
}
```

Важно: compiler может гарантировать только доказуемые constraints.

---

# 97. CONTRACTS

В будущем:

```dtr
function divide(a Float, b Float)
    requires b != 0
    -> Float
{
    a / b
}
```

Contracts могут помогать:

```text
optimizer
bounds analysis
AI
verification
```

Но full formal verification не входит в MVP.

---

# 98. CLI DSL НЕ ДОЛЖЕН РАЗДУВАТЬ ЯЗЫК

CLI — хорошая domain feature, потому что она решает реальную boilerplate problem.

GUI/SQL/Web DSL в первую версию нельзя превращать в keywords.

Главная идея — маленькое ядро, богатые compiler-aware libraries.

---

# 99. ERROR MODEL

Ошибка должна быть локализованной и actionable.

Пример:

```text
D2031 Mutable access conflicts with shared read.

read:   user.dtr:40
write:  user.dtr:42

Possible fixes:
  finish the read
  create a copy
  use exclusive edit block
```

Это одновременно UX и AI feature.

---

# 100. AI-COMPATIBLE DIAGNOSTICS

Forgen должен иметь machine-readable output:

```bash
forgen check --json
```

Каждая ошибка имеет:

```text
code
severity
source span
semantic cause
related spans
safe fixes
machine-readable metadata
```

AI сможет исправлять код структурированно.

---

# 101. FORMATTER

```bash
forgen fmt
```

Обязательный инструмент.

Один canonical style снижает:

```text
noise
diff size
AI ambiguity
review cost
parser weirdness
```

---

# 102. LSP

Forgen LSP должен знать semantic graph.

Поддержка:

```text
autocomplete
rename
go to definition
type hints
effect hints
ownership information
optimization information
AI context extraction
```

---

# 103. DOCUMENTATION GENERATOR

```bash
forgen docs
```

Генерирует из semantic graph:

```text
API
roles
classes
flows
module graph
unsafe surface
performance notes
```

---

# 104. AI CONTEXT EXTRACTION

```bash
forgen context --symbol User.checkout
```

возвращает:

```text
signature
inputs
outputs
effects
ownership
roles
dependencies
tests
```

AI получает не весь repository, а минимально достаточный semantic context.

---

# 105. SEMANTIC DIFF

После изменения:

```text
API changed: yes/no
effects changed: yes/no
unsafe surface changed: yes/no
ownership changed: yes/no
dependencies changed: yes/no
```

Это должно быть доступно и людям, и AI.

---

# 106. TESTS

```dtr
test "adult user" {
    user = User { name: "Alex", age: 20 }
    assert user.isAdult()
}
```

Tests компилируются отдельным target.

---

# 107. PROPERTY TESTS

Позже:

```dtr
property "sort preserves elements" {
    forAll values: List<Int> {
        assert sameElements(sort(values), values)
    }
}
```

Это хороший second-stage feature.

---

# 108. BENCHMARKS

```dtr
benchmark "matrix multiply" {
    matrixMultiply(a, b)
}
```

Forgen должен показывать:

```text
median
p95
allocations
memory
binary size
```

---

# 109. BENCHMARK DISCIPLINE

Нельзя сравнивать:

```text
Debug Datara
```

с:

```text
Release Rust
```

Correct comparison:

```text
Datara start
Datara release
Datara domain
Rust release
C++ optimized
```

на одинаковом hardware и workload.

---

# 110. METRICS

Обязательные:

```text
wall time
throughput
latency p50/p95/p99
RSS memory
allocations
startup
binary size
compile time
embedded code size
```

---

# 111. PERFORMANCE TARGETS

Нельзя сейчас заявить «быстрее Rust всегда».

Правильная инженерная цель:

```text
Datara Domain should be within a small gap of optimized Rust
on representative native workloads,
without requiring equivalent source complexity.
```

А в data/flow/model workloads, где Datara compiler располагает дополнительной semantic information, целью может быть превосходить наивные ручные baseline implementations.

Это цель, которую доказывают benchmark suite, а не маркетинговый лозунг.

---

# 112. WHY SPEED CAN BE CLOSE TO RUST

Если Datara code lowered в:

```text
native code
no mandatory GC
no mandatory object allocation
no mandatory virtual dispatch
specialized generics
bounds-proof elimination
SIMD
LTO
PGO
```

нет принципиального закона, который заставлял бы высокоуровневый source автоматически быть медленным.

Основная задача — сохранить optimizer visibility.

---

# 113. WHY SOURCE SIMPLICITY DOES NOT IMPLY RUNTIME COST

```dtr
class Point {
    x Float
    y Float
}
```

может быть семантически полноценным class, но после lowering стать:

```text
two scalar values
```

То есть хороший compiler отделяет:

```text
logical abstraction
```

от:

```text
physical representation
```

---

# 114. COMPILER OPTIMIZATION HIERARCHY

```text
level 0: syntax cleanup
level 1: local optimization
level 2: function optimization
level 3: module optimization
level 4: project graph optimization
level 5: profile-guided optimization
level 6: target-specific optimization
level 7: specialized domain optimization
```

`domain` стремится пройти максимально глубоко.

---

# 115. WHOLE-PROGRAM REACHABILITY

Forgen стартует с entry points и определяет:

```text
reachable functions
reachable types
reachable modules
reachable runtime features
reachable metadata
```

Всё остальное удаляется.

---

# 116. DEAD CODE

Библиотека может содержать 1000 функций.

Программа использует 20.

Domain binary должен стремиться содержать 20 нужных путей плюс необходимый runtime.

---

# 117. INTERMEDIATE DATA ELIMINATION

Для:

```dtr
x |> map(f) |> filter(g) |> reduce(sum)
```

не обязательно создавать три arrays.

Forgen может превратить это в один loop с accumulator.

Это одна из главных причин иметь native Flow IR.

---

# 118. BUFFER REUSE

Для AI/data compiler должен отслеживать:

```text
last use
aliasing
lifetimes
mutation
```

и переиспользовать buffers там, где это безопасно.

---

# 119. DATA LAYOUT

Compiler может оптимизировать:

```text
AoS
SoA
AoSoA
```

если исходный API не требует конкретного ABI.

---

# 120. SPECIALIZED GENERICS

Если используются:

```text
List<Int>
List<Float32>
```

Forgen может строить разные specialized paths.

Если generic code cold, может применить shared body.

Cost model решает.

---

# 121. MULTI-VERSIONING

В Domain compiler потенциально создаёт:

```text
CPU generic version
AVX2 version
AVX-512 version
ARM NEON version
```

и выбирает при запуске или compile-time, если target known.

Это future optimization.

---

# 122. PROFILE-GUIDED OPTIMIZATION

```bash
forgen profile
```

собирает:

```text
hot functions
branch frequencies
allocation hot spots
input distributions
```

Потом:

```bash
forgen domain --profile-guided
```

использует profile.

---

# 123. DOMAIN BUILD AS SPECIALIZATION COMPILER

Это основная идея Forgen:

```text
generic source
      ↓
actual project usage
      ↓
actual target
      ↓
actual profile
      ↓
specialized program
```

---

# 124. COST MODEL MUST BE SMARTER THAN «ALL OPTIMIZATIONS ON»

Parallelization иногда медленнее.

Inlining иногда увеличивает code size и ухудшает cache.

GPU иногда медленнее CPU из-за transfer.

SoA иногда хуже AoS.

Поэтому Forgen нужен cost model.

---

# 125. COST MODEL INPUTS

```text
CPU target
cache sizes
vector width
memory bandwidth
estimated branch predictability
input size
call frequency
allocation count
parallel overhead
GPU transfer cost
latency requirement
binary size budget
power budget
```

---

# 126. DOMAIN EXPLAIN REPORT

После сборки:

```text
Modules analyzed: 84
Reachable symbols: 1320
Removed symbols: 912
Inlined functions: 231
Allocations removed: 184
SIMD loops: 37
Parallel transforms: 4
Generic specializations: 19
Runtime modules linked: 7
```

Это превращает optimizer из black box в понятную систему.

---

# 127. `forgen inspect`

Полезные команды:

```bash
forgen inspect types
forgen inspect effects
forgen inspect memory
forgen inspect dependencies
forgen inspect optimize
forgen inspect graph
```

---

# 128. `forgen inspect optimize`

Пример:

```text
calculate():
  inline: yes
  allocations removed: 3
  SIMD: yes
  parallel: no
  specialization: Float32
  reason for no parallelism:
      estimated input too small
```

---

# 129. COMPILER INTERNAL VERIFIER

Каждый optimization pass должен иметь:

```text
preconditions
transformation
verification
```

После transformations IR verifier проверяет:

```text
types
SSA
control flow
memory invariants
ownership invariants
effect invariants
```

---

# 130. OPTIMIZER MUST NOT CHANGE SEMANTICS

Особенно опасны:

```text
floating-point reordering
IO reordering
parallel mutation
alias-sensitive transformations
unsafe FFI
```

Для некоторых optimization domains нужны explicit intent/relaxed numeric profiles.

---

# 131. RUNTIME PROFILES

Предлагаемые профили:

```text
auto
native
deterministic
compact
managed
```

`auto` — default.

`deterministic` — embedded/industrial/reproducibility-sensitive workloads.

`managed` — optional prototype/tooling environment, если это оправдает стоимость разработки.

---

# 132. TARGET PROFILES

```text
desktop
server
embedded
wasm
accelerated
```

Профиль определяет defaults, но язык остаётся одним.

---

# 133. FORGEN COMMANDS

```bash
forgen new app
forgen run
forgen start
forgen debug
forgen release
forgen domain
forgen test
forgen bench
forgen profile
forgen inspect
forgen check
forgen fmt
forgen docs
forgen embedded
```

---

# 134. `start`

Задача:

> минимальная задержка между изменением кода и запуском.

Использует:

```text
incremental compilation
cache
parallel module compilation
moderate optimization
```

---

# 135. `debug`

Задача:

> максимальная наблюдаемость.

```text
debug info
source mapping
ownership diagnostics
bounds diagnostics
runtime checks when helpful
```

---

# 136. `release`

Задача:

> production-ready balance.

Основные optimizer passes включены.

---

# 137. `domain`

Задача:

> максимальная безопасная специализация конкретного проекта.

```text
whole-program analysis
specialization
cross-module optimization
layout tuning
inlining
devirtualization
allocation elimination
vectorization
parallel analysis
LTO
optional PGO
runtime stripping
```

---

# 138. `verify`

Задача:

> максимально строгий static verification layer.

Можно включать в CI.

---

# 139. `profile`

Задача:

> собирать фактическую информацию об исполнении.

---

# 140. `embedded`

Задача:

> target without heavyweight runtime.

---

# 141. ОБЪЕКТНАЯ МОДЕЛЬ: FINAL DRAFT

Главный OOP surface v1:

```text
class
record
component
role
behavior
```

`entity` остаётся внутренним архитектурным понятием или будущим sugar, чтобы не раздувать keyword set.

Это важное сокращение: мы хотим новую модель, но не хотим пять почти одинаковых объявлений объектов.

## 141.1 Class

```dtr
class User {
    id UserId
    name String
    age Int

    greet() -> String => "Hello {name}"
}
```

## 141.2 Behavior

```dtr
behavior User {
    isAdult() -> Bool => age >= 18
}
```

## 141.3 Component

```dtr
component Timestamped {
    createdAt Instant
    updatedAt Instant
}
```

## 141.4 Role

```dtr
role Serializable {
    serialize() -> Bytes
}
```

## 141.5 Composition

```dtr
class User with Timestamped, Serializable {
    ...
}
```

Обычный OOP programmer может почти сразу писать классы. Более продвинутый developer получает compositional model.

---

# 142. CLASS НЕ РАВЕН HEAP ALLOCATION

Это должно быть прямо записано в спецификации.

`class` задаёт semantic identity/behavior model, но не требует конкретного memory layout.

Compiler может выбрать:

```text
stack
inline
register-like scalarization
heap
arena
shared representation
```

в зависимости от escape analysis, ownership, target и ABI constraints.

---

# 143. CLASS INITIALIZATION

Основной путь:

```dtr
user = User {
    id: 10
    name: "Alex"
    age: 20
}
```

Если нет special invariant, compiler автоматически создаёт initialization path.

---

# 144. CONSTRUCTOR REPLACEMENT

Не заставлять писать:

```dtr
constructor(...)
```

Если создание простое — literal/init block.

Если нужна логика — explicit factory/create function:

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

Цель: constructor ceremony disappears from most code.

---

# 145. PRIVACY

Вместо множества:

```text
public/private/protected/internal/friend
```

предлагается:

```text
default = module-private
export = public API
```

Для более точных случаев можно вводить `internal` на module/package boundary, но не делать пять уровней обязательными.

---

# 146. METHODS VS FUNCTIONS

Метод, которому нужен instance state:

```dtr
user.rename("Bob")
```

Обычная pure/domain operation:

```dtr
normalize(user)
```

Forgen не должен заставлять разработчика помещать каждую операцию внутрь класса.

---

# 147. EXTENSIONS

```dtr
behavior User {
    toJson() -> Json { ... }
}
```

Никаких extension-wrapper objects.

Это language-level extension, который compiler может полностью flatten.

---

# 148. INHERITANCE MODEL

Обычное:

```dtr
class Admin extends User { ... }
```

может быть поддержано как familiar surface.

Native approach:

```dtr
class Admin from User with Permissioned, Audited { ... }
```

Правила:

- одна class base максимум в первой версии;
- capabilities/roles/composition для дополнительного поведения;
- multiple class inheritance не нужна;
- diamond inheritance не допускается;
- compiler старается eliminate dispatch, если concrete type known.

---

# 149. POLYMORPHISM

Главная форма:

```dtr
role Renderer {
    draw(scene Scene)
}
```

Implementations:

```dtr
class OpenGLRenderer with Renderer { ... }
class VulkanRenderer with Renderer { ... }
```

Call:

```dtr
render(renderer, scene)
```

Если target known — devirtualize.

Если неизвестен — stable indirect dispatch.

---

# 150. ROLE КАК CAPABILITY

Например:

```dtr
role Sendable { ... }
role Serializable { ... }
role Comparable<T> { ... }
```

Role не обязана хранить state.

Это решает значительную часть проблем, для которых старые языки раздували inheritance tree.

---

# 151. COMPONENT КАК DATA MIXIN БЕЗ MIXIN HELL

```dtr
component AuditInfo {
    createdAt Instant
    updatedAt Instant
}
```

И:

```dtr
class User with AuditInfo { ... }
```

Component не является отдельным identity.

Compiler может inline его fields.

---

# 152. MODERN OOP PRINCIPLE

Datara OOP:

```text
identity
+ state
+ behavior
+ capabilities
+ composition
```

а не:

```text
identity
+ inheritance tree
+ hidden mutable state
+ mandatory heap object
```

---

# 153. FUNCTION SYSTEM

Функции first-class:

```dtr
function add(a Int, b Int) -> Int {
    a + b
}
```

Short form:

```dtr
function add(a Int, b Int) -> Int => a + b
```

Lambda:

```dtr
x => x * 2
```

---

# 154. GENERICS AND INFERENCE

```dtr
function first<T>(items List<T>) -> T? {
    items[0]?
}
```

Вызов:

```dtr
user = first(users)
```

T выводится.

Compiler не должен заставлять пользователя писать generic arguments, если inference однозначен.

---

# 155. GENERIC CONSTRAINTS

```dtr
function save<T: Serializable>(value T) -> Bytes {
    value.serialize()
}
```

Смысл constraint — требовать capability, а не конкретный class.

---

# 156. GENERIC SPECIALIZATION

Forgen смотрит на фактическое использование.

Если:

```text
Box<Int>
```

используется в hot path, может создать специализированную реализацию.

Если generic cold and broad — может использовать shared implementation.

---

# 157. SUM TYPES

```dtr
type State = Loading | Ready(Data) | Failed(Error)
```

```dtr
match state {
    Loading => out "Loading"
    Ready(data) => use(data)
    Failed(error) => err error
}
```

Compiler проверяет exhaustiveness.

---

# 158. OPTIONAL

```dtr
User?
```

может быть `User` или `None`.

Никаких произвольных null-подстановок в safe mode.

---

# 159. ERROR TYPE

```dtr
User!DbError
```

это result channel.

`!` после expression распространяет ошибку.

---

# 160. MATCH И NARROWING

```dtr
match user {
    User { age } when age >= 18 => adult(user)
    _ => minor(user)
}
```

Compiler получает branch facts для оптимизации последующих операций.

---

# 161. CONTROL FLOW

Основные конструкции:

```text
if
else
for
while
loop
match
return
break
continue
```

Ничего экзотического ради экзотики.

---

# 162. ITERATION

```dtr
for user in users {
    process(user)
}
```

или:

```dtr
users |> each(process)
```

Compiler должен видеть оба как возможные реализации одной iteration semantics.

---

# 163. PIPELINE IS SEMANTIC

```dtr
users
    |> filter(.active)
    |> map(.score)
    |> reduce(sum)
```

Pipeline graph доступен optimizer и AI tooling.

---

# 164. CLOSURES

Closure capture должен быть:

```text
explicit in semantic model
optimized in runtime
```

Если closure не escaping — environment allocation может исчезнуть.

---

# 165. MEMORY MODEL DRAFT

Datara должна использовать модель:

```text
ownership + borrow checking + escape analysis + lifetime inference
```

но не повторять Rust syntax mechanically.

Главное правило:

> lifetime information обязана существовать; lifetime syntax не обязана существовать в каждом source file.

---

# 166. BORROW INFERENCE

Если compiler может доказать:

```text
borrow starts
borrow ends
owner remains alive
no conflicting mutation
```

annotation не нужна.

---

# 167. ADVANCED BORROW

В системном коде можно писать явнее:

```dtr
function view(data Borrow<Array>) -> View {
    ...
}
```

Точная syntax будет уточняться.

---

# 168. OWNERSHIP TRANSFER

По умолчанию compiler выводит move/borrow strategy.

Advanced user может сделать явный boundary:

```dtr
move buffer into process(buffer)
```

если это действительно нужно для ясности.

---

# 169. SHARED STATE

Для shared mutable state требуется явный synchronization model.

Никаких «два потока одновременно пишут в одно поле и надеемся».

---

# 170. CONCURRENCY MODEL

Основные конструкции:

```dtr
parallel { ... }
async function ...
await ...
actor ...   // future
```

Compiler видит execution dependencies.

---

# 171. PARALLEL SEMANTICS

```dtr
parallel {
    a = loadA()
    b = loadB()
}
```

Semantics:

> a и b независимы.

Implementation:

```text
serial
thread pool
async
SIMD
GPU
```

выбирает Forgen.

---

# 172. ASYNC LOWERING

```dtr
async function fetchUser(id UserId) -> User!Error {
    data = await http.get(...)
    parse(data)
}
```

Compiler может строить state machine без лишних heap allocations.

---

# 173. ACTOR AS FUTURE

Actor полезен для isolated state:

```dtr
actor Counter {
    value Int

    on Increment {
        value += 1
    }
}
```

Но actor не должен стать mandatory concurrency model.

---

# 174. EFFECT SYSTEM

Compiler infer:

```text
pure
read
write
io
network
database
unsafe
parallel
nondeterministic
```

Source annotations optional unless API needs a guarantee.

---

# 175. EFFECTS + OPTIMIZATION

Pure function можно:

```text
fold
cache locally
duplicate safely
reorder
parallelize
```

IO function нельзя переставлять без доказательства semantic equivalence.

---

# 176. EFFECTS + AI

AI tooling получает:

```text
what function returns
what it changes
what it reads
whether unsafe
```

Это уменьшает hallucinated assumptions при генерации кода.

---

# 177. MODULE MODEL

Module = architectural boundary.

Но module не обязан быть runtime boundary.

```dtr
use users.User
use users.validation
```

---

# 178. SPLIT BEHAVIOR

```text
User.core.dtr
User.billing.dtr
User.security.dtr
```

Допустимы несколько behavior declarations для одной class.

---

# 179. PARTIAL CLASS SEMANTICS

Compiler объединяет части в один symbol graph, но incremental compiler сохраняет отдельные file artifacts.

Таким образом:

```text
human modularity
+
compiler global visibility
```

---

# 180. MODULE API

```dtr
export class User { ... }
```

Всё остальное module-private по умолчанию.

---

# 181. IMPORTS

Основной syntax:

```dtr
use users.User
```

и:

```dtr
use users::{User, validate}
```

---

# 182. PROJECT GRAPH

Forgen создаёт:

```text
module graph
symbol graph
call graph
dataflow graph
effect graph
ownership graph
```

---

# 183. DOMAIN COMPILATION

В `domain` границы файлов почти исчезают для optimizer.

Compiler строит единый program graph.

---

# 184. INCREMENTAL COMPILATION

Start build использует:

```text
source hash
API/interface hash
dependency hash
compiler version
target/profile
```

изменённые regions пересобираются.

---

# 185. PARALLEL COMPILATION

Independent modules компилируются одновременно.

Это обязательная часть архитектуры Forgen, а не поздняя оптимизация.

---

# 186. CACHE

Cache artifacts:

```text
AST
resolved symbols
types
semantic slices
DMIR
backend objects
profile data
```

---

# 187. PROJECT MANIFEST

Рабочая идея:

```toml
[project]
name = "analyzer"
version = "0.1.0"

[target]
platform = "native"
profile = "domain"

[performance]
throughput = "high"
memory = "low"
```

Но manifest syntax не должен мешать language spec.

---

# 188. STANDARD TOOLCHAIN

Один `forgen` должен координировать:

```text
compiler
runtime
package manager
formatter
linter
runner
test runner
benchmark runner
profiler
LSP
```

---

# 189. CLI TOOLCHAIN

```bash
forgen new app
forgen run
forgen start
forgen debug
forgen release
forgen domain
forgen test
forgen bench
forgen profile
forgen inspect
forgen check
forgen fmt
forgen docs
```

---

# 190. OFFLINE-FIRST

Поддержка:

```bash
forgen build --offline
```

После того как dependencies закэшированы, базовая сборка не должна требовать internet.

---

# 191. REPRODUCIBLE BUILDS

```bash
forgen domain --reproducible
```

цель — одинаковый artifact при одинаковых inputs/toolchain.

---

# 192. DOMAIN + PROFILE

```bash
forgen profile
forgen domain --profile-guided
```

PGO используется как дополнительный источник информации, а не как semantic truth.

---

# 193. DOMAIN + TARGET

```bash
forgen domain --target server
forgen domain --target embedded
forgen domain --target accelerated
```

Один source → разные lowering decisions.

---

# 194. DOMAIN + INTENT

```dtr
intent {
    performance = maximum
    memory = low
}
```

Cost model получает constraints.

---

# 195. INTENT MUST NOT BE A MAGIC OPTIMIZER SWITCH

Нельзя писать:

```text
@fast
```

и считать проблему решённой.

`intent` — это goal, который optimizer пытается удовлетворить.

---

# 196. IMPOSSIBLE INTENTS

Если compiler не может доказать:

```text
latency <= 1ms
```

то build не должен молча обещать это.

Например:

```text
constraint not proven
```

с возможностью explicit override только для user, который понимает risk.

---

# 197. PERFORMANCE CONTRACTS

В project config:

```toml
[performance]
max_memory_mb = 64
max_binary_mb = 8
```

Forgen может fail CI, если artifact нарушает budget.

---

# 198. SAFETY CONTRACTS

```toml
[safety]
allow_unsafe = false
allow_network = false
```

Это полезно для secure/embedded deployments.

---

# 199. RUNTIME MINIMALISM

Большая ecosystem не должна означать большой executable.

```text
library surface ≠ linked runtime surface
```

---

# 200. TREE SHAKING

Reachability analysis должен работать не только по функциям, но и по:

```text
types
methods
metadata
runtime modules
serialization support
reflection
```

---

# 201. REFLECTION

Полная runtime reflection по умолчанию вредна для whole-program knowledge.

Поэтому reflection — opt-in:

```dtr
compile reflection User
```

Compiler генерирует только запрошенную metadata.

---

# 202. DYNAMIC LOADING

Plugins/dynamic libraries возможны, но являются явным boundary:

```text
dynamic boundary
→ compiler knows less
→ specialization becomes weaker
```

Forgen должен показывать эту цену в `inspect`.

---

# 203. FFI

Datara должна иметь простой low-level bridge:

```dtr
extern "C" function native_call(ptr *U8, len Int) -> Int
```

Foreign boundaries помечаются compiler-ом и учитываются optimizer.

---

# 204. ABI

Внутри Datara compiler может свободно менять layout.

На ABI boundary layout фиксируется.

Это позволяет одновременно иметь:

```text
aggressive optimization
+
stable interoperability
```

---

# 205. C/RUST/CPP INTEROP

Цель — использовать existing ecosystem без необходимости переводить её вручную.

Особенно важны:

```text
C ABI
Rust libraries
C++ wrappers
system APIs
GPU APIs
```

---

# 206. PYTHON INTEROP

Для AI/data ecosystem нужен bridge.

Но hot path должен по возможности выглядеть так:

```text
Python boundary
   ↓
batch input
   ↓
Datara native kernel
   ↓
batch result
   ↓
Python
```

а не миллион маленьких cross-runtime calls.

---

# 207. WEB/WASM

WASM — дополнительный target.

Datara может использовать WASM/WASI там, где portable sandbox execution полезен. Современная WebAssembly спецификация и WASI позволяют рассматривать такой target как отдельный execution environment, не делая его runtime foundation Datara. [WebAssembly Specs]

---

# 208. DESKTOP/SERVER

Native target по умолчанию:

```text
x86-64
ARM64
```

Затем другие architectures.

---

# 209. EMBEDDED TARGETS

Стартовая цель:

```text
ARM Cortex-M
```

Следом:

```text
RISC-V MCU
```

Понадобятся:

```text
linker scripts
startup code
interrupt model
MMIO
peripheral abstractions
no-OS runtime
```

---

# 210. HARDWARE API

```dtr
led = gpio(13)
button = gpio(4)
```

Low-level access должен быть возможен без ухода в другой язык.

---

# 211. INTERRUPTS

Будущая syntax:

```dtr
interrupt Timer0 {
    tick()
}
```

Compiler должен анализировать ограничения interrupt context:

```text
no blocking
limited allocation
bounded work
```

---

# 212. REAL-TIME

Для deterministic systems нужно различать:

```text
best effort
soft real-time
hard real-time
```

Но hard real-time guarantee допускается только когда compiler/toolchain действительно способен доказать соответствующие constraints.

---

# 213. INDUSTRIAL RESOURCE MODEL

В будущем можно иметь constraints для:

```text
CPU budget
RAM budget
stack budget
power budget
latency budget
```

Это можно встроить в `intent`/manifest.

---

# 214. POWER-AWARE OPTIMIZATION

Для embedded compiler может учитывать:

```text
CPU frequency
sleep opportunities
memory traffic
instruction count
```

Но power model должен быть target-specific.

---

# 215. CLI / AUTOMATION

Datara должна быть естественным инструментом для:

```text
file automation
log processing
data conversion
system scripting
build tooling
ETL
```

Native executable и static types устраняют часть проблем Python shell tools.

---

# 216. SINGLE-FILE MODE

Для маленькой утилиты:

```dtr
out "Hello"
```

можно позволить запускать:

```bash
forgen run hello.dtr
```

Без обязательного project scaffolding.

---

# 217. PROJECT MODE

Для серьёзного проекта:

```text
src/
  main.dtr
  users/
  data/
  ai/
```

с manifest и dependencies.

---

# 218. REPL

Нужен, но должен быть lightweight:

```bash
forgen repl
```

Типы и semantic model остаются строгими.

---

# 219. SCRIPT MODE

Можно запускать:

```bash
forgen run script.dtr
```

Compiler быстро компилирует single-file application с incremental cache.

---

# 220. SCRIPT → APP PATH

Одна из сильных UX-идей:

```text
script.dtr
   ↓
project grows
   ↓
same source
   ↓
full project
```

Никакой обязательной миграции в другой язык/формат при росте программы.

---

# 221. SCRIPT → DOMAIN

Тот же script:

```bash
forgen domain script.dtr
```

может получить aggressive AOT binary.

---

# 222. LANGUAGE SURFACE MINIMALISM

Нельзя добавлять десять способов создать объект.

Основные формы должны быть:

```text
record
class
component
role
behavior
```

Остальные архитектурные идеи должны опираться на них.

---

# 223. KEYWORD REVIEW RULE

Перед добавлением keyword задаём четыре вопроса:

```text
Это новая семантическая категория?
Получает ли compiler новую информацию?
Решает ли это старую сложность?
Будет ли человек использовать это регулярно?
```

Если нет — DSL/library вместо keyword.

---

# 224. НЕ ДОБАВЛЯТЬ СИНТАКСИС РАДИ БРЕНДА

Datara должна отличаться не потому, что `function` заменили на `fnx`, а потому что язык действительно делает новое.

---

# 225. SOURCE COMPATIBILITY PHILOSOPHY

Не нужен 100% TypeScript compatibility.

Нужна:

```text
recognizable syntax
```

при собственной semantics.

То же для Rust/Python.

---

# 226. AI CODE GENERATION

Для AI полезно иметь:

```text
stable grammar
low ambiguity
strong types
explicit effects
clear modules
machine diagnostics
```

Это должно проектироваться с самого начала.

---

# 227. AI CONTEXT GRAPH

Forgen может экспортировать:

```json
{
  "symbol": "User.checkout",
  "inputs": ["Cart", "Payment"],
  "output": "Receipt!CheckoutError",
  "effects": ["Database", "Network"],
  "unsafe": false
}
```

Это не runtime API, а tooling contract.

---

# 228. AI SEMANTIC PATCH

AI patch должен описываться не только diff:

```text
add behavior User.billing
add dependency Payments
no public API break
no unsafe
adds Network effect
```

Forgen может автоматически проверять этот semantic patch.

---

# 229. AI TEST GENERATION

Forgen знает:

```text
types
contracts
effects
states
```

и может предоставлять эту структуру AI для генерации тестов.

---

# 230. AI OPTIMIZATION LOOP

В идеале:

```text
AI writes code
↓
Forgen checks
↓
Forgen reports constraints
↓
Forgen benchmarks
↓
AI receives structured result
↓
AI improves implementation
```

Это сильнее обычного «попросили модель написать код».

---

# 231. COMPILER AS VERIFIER FOR AI

Нейросеть может ошибаться.

Compiler должен быть последней линией:

```text
type proof
memory proof
control-flow check
effect check
```

AI не получает права обойти compiler safety просто потому, что оно «уверено».

---

# 232. WHY DATAFLOW HELPS AI

У `flow` есть explicit graph:

```text
A → B → C → D
```

AI легче анализирует dependency chain, чем большую функцию с десятками side effects.

---

# 233. WHY SPLIT BEHAVIOR HELPS AI

Большой class:

```text
2000 lines
```

разбивается на:

```text
core
security
billing
serialization
```

AI может работать на нужном semantic slice.

---

# 234. WHY STRONG TYPES HELP AI

Если parameter:

```dtr
amount Money
```

AI видит semantic restriction, а не просто `number`.

Это снижает количество логических ошибок.

---

# 235. AI-FACING API RULE

Forgen tooling должен использовать стабильные machine-readable schemas.

Это позволит создавать IDE/agents независимо от конкретной AI модели.

---

# 236. PERFORMANCE OBSERVABILITY

Каждая серьезная Domain сборка должна уметь показать:

```text
compile duration
reachable symbols
removed symbols
allocations removed
inlined functions
vectorized loops
parallel transforms
generic specializations
binary size
```

---

# 237. PROFILE

Пример:

```text
Hot:
  matrixMultiply 48%
  tokenizer      19%
  parser         9%
```

И:

```text
matrixMultiply
  vectorized: yes
  parallel: yes
  buffer reuse: yes
```

---

# 238. REGRESSION CONTROL

Если commit ухудшил benchmark на 5%, CI может fail.

Это нужно для compiler project.

---

# 239. COMPILER BENCHMARKS

Сам Forgen должен иметь benchmark suite:

```text
parse speed
semantic analysis
memory checker
IR generation
optimization time
codegen time
incremental rebuild
```

Compiler performance тоже является продуктовой характеристикой.

---

# 240. BUILD TIME PHILOSOPHY

Нельзя сделать только быстрый runtime.

Нужно:

```text
fast start
reasonable release
expensive but powerful domain
```

Так developer не будет ненавидеть compiler.

---

# 241. DOMAIN CACHE

Если исходник и target не изменились, часть дорогих analyses можно переиспользовать.

Domain не обязательно должен каждый раз начинать с полного чистого graph build.

---

# 242. DOMAIN PARTIAL REBUILD

Изменение cold module не должно обязательно приводить к полной перестройке hot optimizer state.

Это отдельная compiler research area.

---

# 243. PARALLEL OPTIMIZER

Independent optimization regions могут обрабатываться параллельно.

При этом pass dependencies должны быть graph-aware.

---

# 244. COMPILER RESOURCE MANAGEMENT

Domain может использовать много CPU/RAM.

Нужен простой control:

```bash
forgen domain --jobs 8
```

Но default должен auto-detect разумное число workers.

---

# 245. DEBUG OPTIMIZATION LEVEL

Debug не обязан быть полностью без оптимизации.

Лучше:

```text
низкая оптимизация
+
debug-friendly transformations
```

чтобы stack traces оставались полезными.

---

# 246. RELEASE

Release должен иметь:

```text
high optimization
reasonable compile time
complete symbols for production diagnostics
```

---

# 247. DOMAIN

Domain:

```text
highest compiler cost
highest semantic visibility
most specialization
```

Это не просто `release + 2`.

---

# 248. DOMAIN + HARDWARE PROFILE

Если compiler знает:

```text
AVX2
32KB L1
large L3
```

он может выбрать другой generated code.

---

# 249. DOMAIN + INPUT PROFILE

Если profile показывает:

```text
input size usually 1M–5M
```

optimizer может выбрать другую strategy, чем для 100-element arrays.

---

# 250. DOMAIN + MODEL PROFILE

Если AI model всегда:

```text
batch=1
seq<=512
Float16
```

compiler может специализировать kernels.

---

# 251. MULTI-VERSION OUTPUT

Можно генерировать несколько versions:

```text
fast-hot
small-input
fallback
```

и выбирать по runtime facts.

---

# 252. NO MAGIC PREDICTIONS

Профиль может подсказать compiler, но не может менять semantic meaning программы.

---

# 253. MEMORY LAYOUT OPTIMIZATION

Hot fields могут быть физически организованы иначе, если language semantics и ABI это позволяют.

---

# 254. CACHE LOCALITY

Forgen должен учитывать:

```text
field access order
loop nesting
data reuse
working set
```

---

# 255. LOOP FUSION

```dtr
values
 |> map(f)
 |> filter(g)
 |> reduce(sum)
```

может быть превращено в один loop.

---

# 256. LOOP FISSION

Иногда наоборот выгоднее разделить loop для locality/vectorization.

Optimizer выбирает.

---

# 257. BUFFER PLANNING

Для tensor/data pipeline строить lifetime graph buffers и минимизировать peak memory.

---

# 258. MODEL GRAPH FUSION

Например:

```text
normalize → linear → activation
```

может стать одним optimized kernel если target supports it.

---

# 259. CPU/GPU CROSSOVER

Cost model должен оценивать:

```text
compute
transfer
launch overhead
memory bandwidth
```

а не просто «GPU быстрее».

---

# 260. EMBEDDED CODE SIZE

Отдельный optimizer target:

```text
remove unused
inline selectively
compress tables
avoid heavy runtime
```

---

# 261. LINKER STRIPPING

Unused runtime modules должны быть убраны link stage.

---

# 262. SINGLE-BINARY GOAL

Для простых native applications желательно получать один executable без огромной install dependency chain.

Для plugins/dynamic libraries exceptions остаются.

---

# 263. BINARY STARTUP

CLI application должна запускаться быстро.

Не нужен обязательный VM startup.

---

# 264. PACKAGE ECOSYSTEM

Пакет должен иметь:

```text
name
version
source
public API
target support
native deps
unsafe surface
features
```

---

# 265. FEATURE FLAGS

Optional feature code должен быть tree-shakable.

---

# 266. DEPENDENCY GRAPH

Forgen должен строить dependency graph до symbol level, когда package metadata позволяет.

---

# 267. SECURITY OF DEPENDENCIES

В будущем:

```text
lockfile
checksums
signed metadata
reproducible build support
```

---

# 268. STANDARD LIBRARY PHILOSOPHY

Core маленький.

Modules rich.

Everything optional where possible.

---

# 269. CORE

В core желательно оставить:

```text
primitive types
control flow
memory primitives
basic collections support
Result/Option
basic formatting contracts
```

---

# 270. HIGHER LIBS

Отдельно:

```text
http
json
csv
database
tensor
ai
hardware
cli
crypto
```

---

# 271. JSON

Нативный parser/serializer полезен как benchmark и everyday capability.

Он должен иметь zero-copy/borrowed path там, где возможно.

---

# 272. CSV

Data workflows должны быть эффективны без Python dependency.

---

# 273. STREAM

```dtr
stream = file.lines(path)!

stream
    |> filter(.nonEmpty)
    |> map(parseLog)
    |> each(analyze)
```

Compiler может строить streaming loop без materializing entire file.

---

# 274. LAZY VS EAGER

Pipeline не должен автоматически создавать intermediate collections.

Forgen выбирает lazy/fused representation.

---

# 275. EXPLICIT MATERIALIZATION

Если нужна коллекция:

```dtr
result = stream |> map(f) |> collect()
```

`collect()` является явной semantic boundary.

Это поможет compiler понимать memory behavior.

---

# 276. ASYNC STREAMS

Будущий вариант:

```dtr
stream = http.stream(url)

async for chunk in stream {
    process(chunk)
}
```

---

# 277. ERROR PROPAGATION IN PIPELINES

```dtr
data
    |> parse!
    |> validate!
    |> save!
```

Можно исследовать shorthand для propagation, но не добавлять syntax until semantics ясна.

---

# 278. LAMBDA PERFORMANCE

Lambda должна быть обычной compile-time object when needed, but ideally disappear:

```text
lambda
→ closure IR
→ inline
```

---

# 279. FUNCTION VALUES

First-class functions остаются возможными.

Compiler знает, когда function value является concrete target.

---

# 280. CLOSURE ESCAPE

Если closure escaping:

```text
heap/state object может быть нужен
```

Если нет:

```text
stack/register representation
```

---

# 281. PATTERN MATCH OPTIMIZATION

Compiler может преобразовывать exhaustive match в:

```text
jump table
branch tree
bit test
```

по target.

---

# 282. ENUM LAYOUT

Sum types должны иметь compact representations.

---

# 283. NULL SAFETY

`null` не является универсальным значением любого типа.

Используются `T?` / Optional.

---

# 284. INTEGER SAFETY

Нужны чёткие правила conversion.

Implicit lossy conversion не должно проходить silently в safe mode.

---

# 285. OVERFLOW

Нужно выбрать модель:

```text
checked
wrapping
saturating
```

и сделать её предсказуемой.

Оператор/метод для альтернативных режимов может быть явным.

---

# 286. FLOATING POINT

Нужны профили:

```text
strict
fast
```

`fast` может позволять дополнительные reassociation, если user explicitly permits.

---

# 287. DETERMINISTIC FLOAT

Для reproducible numerical workloads compiler должен иметь strict mode.

---

# 288. RESOURCE MANAGEMENT

Файлы/socket/locks должны закрываться детерминированно.

Предпочтительна RAII-like semantic model, но syntax может быть проще классического C++.

---

# 289. DEFER

Возможно иметь:

```dtr
defer close(file)
```

Но это нужно реализовать так, чтобы compiler lowering был очевиден и предсказуем.

---

# 290. RESOURCE BOUNDARIES

`defer` должен быть known control-flow operation, чтобы optimizer учитывал cleanup paths.

---

# 291. RESOURCE SAFETY

Для ресурсов нужно различать:

```text
owned
borrowed
shared
closed
```

Compiler может исключать двойное закрытие и use-after-close там, где semantics достаточно строгая.

---

# 292. NETWORK

Network API должен иметь typed request/response:

```dtr
response = http.get(url)!
```

Future schema layer сможет автоматически проверять API payloads.

---

# 293. PROCESS

CLI/system tooling:

```dtr
result = process.run("git", ["status"])!
out result.stdout
```

Никаких untyped string-only APIs везде, где это можно избежать.

---

# 294. ENVIRONMENT

```dtr
home = env["HOME"]?
```

Optional semantics сохраняется.

---

# 295. TIME

Нужны:

```text
Instant
Duration
Date
Time
```

`Duration` не должен смешиваться с `Instant`.

---

# 296. UNITS

```dtr
sleep(500ms)
```

Compiler знает, что `500ms` — Duration.

---

# 297. SCHEMA

Будущая native schema:

```dtr
schema User {
    id UserId
    name String
    email Email
}
```

Может использоваться для:

```text
validation
serialization
API
CLI
AI tools
```

---

# 298. SCHEMA GENERATION

Одна schema должна по возможности порождать:

```text
parser
serializer
validator
docs
AI function schema
```

---

# 299. DOMAIN-SPECIFIC COMPILATION

Не делать отдельные языки внутри Datara.

Вместо этого:

```text
common semantic core
+
optional domain IR
+
backend
```

---

# 300. DATA DOMAIN

Compiler получает Table/Stream semantics.

---

# 301. AI DOMAIN

Compiler получает Tensor/Model semantics.

---

# 302. EMBEDDED DOMAIN

Compiler получает Hardware/Deterministic semantics.

---

# 303. CLI DOMAIN

Compiler получает Command/Argument semantics.

---

# 304. INDUSTRIAL DOMAIN

Compiler получает Machine/State/Units/Constraint semantics.

---

# 305. DOMAIN IR НЕ ДОЛЖЕН БЫТЬ БОГАТОЙ КУЧЕЙ DSL

Например:

```text
Table IR
Tensor IR
Hardware IR
```

это backend-level representations, а не десятки новых keywords.

---

# 306. DOMAIN LOWERING

```text
Datara source
↓
semantic graph
├── normal code
├── table graph
├── tensor graph
└── hardware graph
↓
combined optimizer
```

---

# 307. WHY THIS IS IMPORTANT

Одна функция может одновременно использовать:

```text
file
→ parse
→ table
→ tensor
→ output
```

Compiler должен видеть всё как одну систему, а не как пять несвязанных frameworks.

---

# 308. AI MODEL AS GRAPH

```text
input
 ↓
embed
 ↓
attention
 ↓
mlp
 ↓
logits
```

Domain optimizer может делать graph-level passes.

---

# 309. MODEL MEMORY PLANNER

Для inference compiler может построить lifetime graph каждого tensor buffer и уменьшить peak memory.

---

# 310. KV CACHE

Если LLM implementation использует KV cache, это должен быть обычный typed structure/buffer с compiler-visible lifetime.

---

# 311. MODEL QUANTIZATION

Будущая compiler/domain feature:

```text
FP32
FP16
BF16
INT8
INT4
```

При explicit model constraints compiler может использовать подходящий representation/kernel.

---

# 312. QUANTIZATION SHOULD NOT CHANGE SOURCE API

Source может оставаться:

```dtr
model(tokens)
```

а Domain backend выбирать optimized representation, когда semantic contract позволяет.

---

# 313. AUTO BATCHING

Для server inference future feature:

compiler/runtime может объединять совместимые requests.

Но это runtime scheduling policy, а не обязательная часть language semantics.

---

# 314. SERVER CONCURRENCY

Datara server может получать:

```text
request flow
```

и compiler сможет анализировать per-request state isolation.

---

# 315. ASYNC WITHOUT CALLBACK HELL

```dtr
async function load() {
    a = await getA()
    b = await getB()
    return merge(a, b)
}
```

---

# 316. PARALLEL AS DATAFLOW

Если `a` и `b` независимы:

```dtr
parallel {
    a = await getA()
    b = await getB()
}

merge(a,b)
```

Compiler может выбирать task scheduling.

---

# 317. ACTOR/STATE ISOLATION

В будущем actor может быть лишь одной implementation strategy для isolated mutable state.

---

# 318. MEMORY + CONCURRENCY CONNECTION

Ownership graph и execution graph должны анализироваться вместе.

Это важнее отдельных «thread primitives».

---

# 319. DATA RACE PROOF

Если compiler может доказать, что два parallel branches не разделяют mutable alias — разрешить parallel execution.

Если не может — либо serial, либо compile error для strict domain.

---

# 320. STRICT PARALLEL MODE

```dtr
intent { parallelism = strict }
```

Если безопасную parallel implementation доказать нельзя, compiler сообщает причину, а не молча заменяет поведение.

---

# 321. AI + PARALLELISM

Для tensor workloads compiler может находить parallel dimensions без explicit thread loops.

---

# 322. CLI + PARALLELISM

Для batch file processing:

```dtr
files
    |> parallelMap(processFile)
```

может стать work-stealing implementation.

---

# 323. AUTOMATION

Datara должна быть удобна для маленьких automation scripts:

```dtr
for file in files("logs/*.log") {
    process(file)
}
```

и не заставлять пользователя разворачивать тяжёлый project skeleton.

---

# 324. SCRIPT EXECUTION

```bash
forgen run cleanup.dtr
```

---

# 325. SCRIPT -> PROJECT

Когда программа выросла, её можно перенести в project mode без переписывания core logic.

---

# 326. PROJECT -> DOMAIN

```bash
forgen domain
```

тот же semantic source превращается в максимально специализированный artifact.

---

# 327. PACKAGE MANAGER PHILOSOPHY

Package manager не должен диктовать language architecture.

Он управляет:

```text
sources
binary artifacts
version locks
native dependencies
features
```

---

# 328. BINARY CACHING

В будущем package manager может скачивать prebuilt artifacts, но Domain build должен уметь их переоптимизировать при наличии source/IR.

---

# 329. SECURITY

Dependencies должны иметь checksums/locks.

---

# 330. SUPPLY CHAIN

Позже возможны signed package metadata и reproducible build verification.

---

# 331. COMPILER TRUST MODEL

Forgen должен быть разделён:

```text
front-end trust
IR verifier
backend
linker
```

Каждая стадия должна иметь tests.

---

# 332. FFI TRUST MODEL

`unsafe`/extern boundary должна быть видимой semantic graph.

---

# 333. SAFE API DESIGN

Unsafe low-level implementation может иметь safe high-level wrapper, если wrapper действительно доказывает safety invariants.

---

# 334. DOCUMENTED UNSAFE SURFACE

`forgen inspect safety` должен показывать:

```text
unsafe blocks
unsafe FFI
unsafe dependencies
```

---

# 335. PERFORMANCE SURFACE

`forgen inspect performance` должен показывать:

```text
hot functions
allocations
vectorization
parallelism
FFI boundaries
cache-sensitive loops
```

---

# 336. COMPILER PROFILE

Каждая сборка должна хранить compiler version + target + profile в artifact metadata.

---

# 337. BUILD PROFILES

Рекомендуемая матрица:

| Profile | Цель | Optimizer | Debug | Runtime |
|---|---|---|---|---|
| `start` | быстрая разработка | средний | да | минимальный |
| `debug` | диагностика | низкий/безопасный | максимум | проверки |
| `release` | production | высокий | ограниченный | минимальный |
| `domain` | максимум specialization | максимально глубокий | metadata | минимальный |
| `embedded` | embedded | target-specific | optional | deterministic/minimal |
| `verify` | статический контроль | вторичен | максимум | optional |

---

# 338. WHY `start` SHOULD NOT EQUAL `release`

Если start будет полностью оптимизировать whole-program, каждый edit станет дорогим.

Нужен отдельный fast path.

---

# 339. WHY `domain` SHOULD EXIST

Потому что иногда пользователь готов потратить минуты/десятки минут на compiler, чтобы получить лучший binary.

Для production build это может быть оправдано.

---

# 340. DOMAIN SHOULD BE DETERMINISTIC IN DECISIONS WHERE POSSIBLE

Для одинакового source/target/profile compiler должен стремиться делать повторяемые optimization decisions, за исключением специально разрешённых profile-dependent strategies.

---

# 341. PGO

PGO data — дополнительная evidence.

Она не заменяет static semantics.

---

# 342. MULTI-VERSIONING

Compiler может генерировать несколько implementations и выбирать по known CPU/conditions.

---

# 343. HOT/COLD SPLIT

Domain может отделять cold error paths от hot execution path.

---

# 344. ERROR PATH OPTIMIZATION

Ошибочные ветви не должны ухудшать hot path, если compiler может вынести их отдельно.

---

# 345. BOUNDS CHECK HOISTING

Compiler может доказать range один раз вместо каждой итерации.

---

# 346. LOOP PEELING / UNROLLING

Применяется только по cost model.

---

# 347. BRANCH SPECIALIZATION

PGO может показать, что branch A почти всегда true.

Forgen может расположить hot path выгоднее.

---

# 348. CONSTANT PROPAGATION

Compile-time configuration должна удалять code paths.

---

# 349. DEAD FEATURE REMOVAL

Feature flag, который равен false, должен приводить к удалению недостижимой ветки, если semantics позволяет.

---

# 350. ABI PRESERVATION MODE

Для libraries нужен profile, сохраняющий public ABI и ограничивающий layout transformations на boundary.

---

# 351. STATIC LIBRARY MODE

```bash
forgen build --lib
```

для использования Datara libraries из других languages.

---

# 352. C ABI GENERATION

Forgen может генерировать C-compatible wrappers для export functions.

---

# 353. WASM

WASM target полезен для portable plugins/web workloads; WebAssembly modules designed for embedding и WASI дают отдельную system interface layer. [Wasm Specs]

---

# 354. NO RUNTIME DEPENDENCY WHERE POSSIBLE

Для simple CLI binary:

```text
Datara code
+
minimal runtime
```

а не полный platform environment.

---

# 355. BINARY SIZE

Domain `small-binary` future profile должен оптимизировать code size отдельно от maximum throughput.

---

# 356. POWER PROFILE

Future:

```bash
forgen domain --profile power
```

для embedded/mobile.

---

# 357. LATENCY PROFILE

Future:

```bash
forgen domain --profile latency
```

может предпочитать low-jitter paths.

---

# 358. THROUGHPUT PROFILE

Future:

```bash
forgen domain --profile throughput
```

может предпочитать batch/vector/parallel strategies.

---

# 359. SIMPLE USER INTERFACE TO COMPLEX OPTIMIZER

Основные commands остаются маленькими. Advanced configuration живёт в project intent/profile.

---

# 360. DATARA'S IDEAL RELATION TO OOP

Datara не говорит:

> «ООП устарело».

Она говорит:

> «Класс — удобная модель сущности. Но сущность не обязана быть запечатана в одном файле, а behavior не обязан жить в inheritance tree». 

Это намного практичнее.

---

# 361. DATARA'S IDEAL RELATION TO FUNCTIONAL PROGRAMMING

Datara не говорит:

> «всё должно быть immutable functions».

Она использует functional ideas там, где они дают:

```text
composability
purity
parallelism
optimization
```

---

# 362. DATARA'S IDEAL RELATION TO PROCEDURAL PROGRAMMING

Процедурный стиль остаётся:

```dtr
for i in 0..n {
    process(i)
}
```

Если он самый понятный — используем его.

---

# 363. PARADIGM AGNOSTIC, SEMANTICALLY STRONG

Datara не обязана выбрать одну школьную парадигму.

Она должна иметь одну **compiler-friendly semantic model**.

---

# 364. НЕОБХОДИМАЯ ЯЗЫКОВАЯ КУЛЬТУРА

Код Datara должен поощрять:

```text
small modules
clear flows
explicit effects
strong types
composable behavior
```

а не:

```text
massive classes
hidden globals
magic reflection
runtime metaprogramming everywhere
```

---

# 365. GLOBAL STATE

Не запрещать, но считать effectful state.

Compiler tooling должен его показывать.

---

# 366. DEPENDENCY INJECTION

Не нужен обязательный DI framework.

Обычные function parameters и components должны покрывать базовые случаи.

---

# 367. SERVICE OBJECTS

Если нужен service:

```dtr
class Mailer { ... }
```

Если state не нужен:

```dtr
module Mail {
    send(...) -> Result
}
```

Не заставлять объектно-ориентировать всё.

---

# 368. SINGLETON

Не делать language-level singleton keyword.

Module state/controlled instance должны быть достаточны.

---

# 369. BUILDER

В большинстве случаев named initialization решает проблему.

Специализированные builder patterns остаются library constructs.

---

# 370. GETTERS/SETTERS

Обычный computed property syntax должен покрывать большинство случаев.

---

# 371. OBSERVABLE PROPERTY

Future reactive domains могут добавить compiler-aware observation, но не в core v1.

---

# 372. EVENTS

`event/on` возможны позже, если compiler получит полезную semantic visibility для них.

---

# 373. MACROS

Macro system после core stability.

Предпочтение — compile-time functions/semantic generation.

---

# 374. REFLECTION

Opt-in.

---

# 375. CODE GENERATION

Если Datara получила schema/contract, compiler может генерировать code artifacts без runtime reflection.

---

# 376. NO MAGIC CODE GENERATION

Generated code должен быть inspectable:

```bash
forgen inspect generated
```

---

# 377. AI-GENERATED CODE

AI output должен проходить:

```text
formatter
compiler
test
benchmark
safety checks
```

---

# 378. AI SHOULD BE ABLE TO ASK COMPILER QUESTIONS

Например:

```text
What type is X?
What does this function mutate?
Why can this not be parallelized?
What allocations remain?
```

`forgen inspect` должен отвечать структурировано.

---

# 379. AI PERFORMANCE FEEDBACK

AI получает:

```text
This function accounts for 61% runtime.
Allocation count = 4.1M.
SIMD unavailable because of aliasing.
```

AI может предложить изменения, но compiler решает, корректны ли они.

---

# 380. HUMAN + AI SHARED SEMANTIC LANGUAGE

Datara становится не «языком для AI», а языком, у которого:

```text
человек читает semantics
AI читает semantics
compiler читает semantics
```

---

# 381. CODE REVIEW

Reviewer может понимать:

```text
what changed
what effects changed
what APIs changed
what memory behavior changed
```

---

# 382. SEMANTIC PULL REQUEST

Future tooling может строить PR summary:

```text
Public API: unchanged
Unsafe: unchanged
Dependencies: +1
Peak memory: -12%
Hot path: changed
```

---

# 383. DEPLOYMENT

Simple native binary should be easy to deploy.

---

# 384. CONTAINER

Future:

```bash
forgen package --container
```

с минимальным runtime base image.

---

# 385. SERVERLESS

Native startup может быть advantage, но не является отдельной language feature.

---

# 386. EMBEDDED FIRMWARE

```bash
forgen embedded --target cortex-m4
```

должен производить:

```text
ELF/bin/hex
map
symbols
```

---

# 387. MAP FILE

Memory map должен быть частью embedded report.

---

# 388. STATIC RESOURCE CHECKS

Forgen должен уметь показывать:

```text
RAM usage
flash usage
stack estimate
```

если target toolchain способен это вычислять.

---

# 389. STACK ANALYSIS

Future static stack analysis поможет embedded.

---

# 390. INTERRUPT SAFETY

Interrupt handler API должен иметь строгий execution contract.

---

# 391. PERIPHERAL OWNERSHIP

Hardware resource должен иметь ownership:

```text
GPIO pin owned by module
```

и compiler может предотвращать конфликтующие claims.

---

# 392. HARDWARE RESOURCE GRAPH

Это extension semantic graph:

```text
CPU
├── UART0
├── SPI1
└── GPIO13
```

---

# 393. MACHINE CONFIGURATION

Configuration может быть частью project manifest, чтобы compiler мог specialize hardware mappings.

---

# 394. INDUSTRIAL MACHINE GRAPH

```text
Sensor → Controller → Actuator
```

может быть представлен как flow.

---

# 395. FAIL-SAFE STATE

State machine может требовать explicit safe state.

---

# 396. SAFETY DOMAIN

В будущем отдельный safety profile может запрещать часть динамики:

```text
no unsafe
no dynamic loading
no unbounded allocation
```

---

# 397. FORMAL VERIFICATION FUTURE

После stable semantics можно исследовать:

```text
contracts
invariants
model checking
SMT-backed proofs
```

Но не делать это фундаментом MVP.

---

# 398. WHY NOT PUT EVERYTHING IN THE LANGUAGE

Потому что это убьёт главную философию:

```text
small surface
```

Поэтому много возможностей должны жить как:

```text
stdlib modules
compiler plugins/backends
project declarations
```

---

# 399. CORE VS LIBRARY DECISION RULE

Если feature нужна почти каждому проекту и compiler может получить из неё уникальную semantic information — candidate for core.

Если feature специфична и не обязательна — library/domain module.

---

# 400. FINAL CORE SET

Предварительно ядро Datara v1:

```text
let
mut
const
function
return
if/else
for/while/loop
match
record
class
component
role
behavior
from/with
use/export
flow
parallel
async/await
Result/Optional
lambda
out/err
unsafe
extern
intent
```

Это и без того серьёзный, но ещё контролируемый surface.

---

# 401. V1 BACKEND

Первый серьёзный backend:

```text
x86-64
ARM64
```

через LLVM infrastructure как pragmatic starting point.

---

# 402. V1 RUNTIME

Минимальный native runtime:

```text
memory
IO
formatting
basic scheduler
platform
```

---

# 403. V1 OPTIMIZER

Обязательно:

```text
reachability
constant folding
DCE
inlining
monomorphization/specialization
escape analysis
allocation elimination
basic vectorization
cross-module optimization
LTO
```

---

# 404. V1 TOOLING

```text
forgen start
forgen debug
forgen release
forgen domain
forgen test
forgen bench
forgen fmt
forgen check
forgen inspect
```

---

# 405. V1 AI TOOLING

Минимум:

```text
machine-readable diagnostics
semantic symbol query
dependency graph export
optimization report
```

---

# 406. V1 EMBEDDED

Не обязательно full universal MCU support.

Один хорошо поддержанный target лучше десяти полуготовых.

---

# 407. V1 DATA

CSV/JSON/Table/Stream должны быть usable.

---

# 408. V1 AI

Tensor core можно начать с CPU backend.

GPU later.

---

# 409. V1 OOP

Самая важная проверка:

```text
class code
```

должен быть таким же удобным, как привычный high-level OOP, но не иметь обязательного runtime penalty.

---

# 410. ENGINEERING SUCCESS CRITERIA

Проект нельзя считать успешным только после запуска compiler.

Успех — когда выполнены одновременно:

```text
удобный source
строгая type safety
safe memory model
native execution
real optimization gain
module architecture
AI-friendly tooling
```

---

# 411. RED FLAGS

Нужно остановить feature growth, если появляются:

```text
50+ keywords
mandatory annotations everywhere
multiple overlapping type systems
multiple inheritance systems
runtime magic everywhere
huge mandatory runtime
```

---

# 412. RED FLAG #2

Если чтобы сделать быстрый код пользователь вынужден переписывать программу в другой стиль:

```text
принцип сломан
```

---

# 413. RED FLAG #3

Если compiler optimisation невозможно объяснить:

```text
trust the compiler
```

недостаточно.

Нужен inspect/report.

---

# 414. RED FLAG #4

Если AI постоянно получает ошибки из-за неоднозначности языка:

```text
нужно менять syntax/semantic rules
```

а не просто писать больше prompt templates.

---

# 415. RED FLAG #5

Если embedded требует полностью отдельную syntax ветку:

```text
архитектура слишком связана с desktop runtime
```

---

# 416. RED FLAG #6

Если Domain compiler стал настолько медленным, что release developer flow непригоден:

```text
улучшать incremental cache/parallelism
```

а не убирать optimization.

---

# 417. DEVELOPMENT LOOP

```text
write
↓
forgen start
↓
run
↓
forgen check
↓
forgen bench
↓
forgen domain
↓
inspect
```

---

# 418. AI DEVELOPMENT LOOP

```text
human intent
↓
AI generates Datara
↓
Forgen compile
↓
Forgen diagnostics
↓
Forgen semantic report
↓
AI patch
↓
benchmark
```

---

# 419. PERFORMANCE DEVELOPMENT LOOP

```text
baseline
↓
profile
↓
inspect hot path
↓
Domain
↓
benchmark
↓
regression check
```

---

# 420. FINAL ARCHITECTURE

```text
                           DATARA
                              │
        ┌─────────────────────┼─────────────────────┐
        │                     │                     │
       OOP                 DATA/FLOW               AI
        │                     │                     │
 class/component        table/stream            model/tensor
 behavior/role          pipeline                 kernel
        │                     │                     │
        └─────────────────────┼─────────────────────┘
                              ↓
                       SEMANTIC GRAPH
                              │
       ┌──────────────────────┼──────────────────────┐
       │                      │                      │
    TYPE/MEMORY             EFFECTS               EXECUTION
       │                      │                      │
 ownership/borrow       IO/unsafe/pure       parallel/async/flow
       └──────────────────────┼──────────────────────┘
                              ↓
                     SPECIALIZATION ENGINE
                              │
                usage + target + profile + intent
                              ↓
                          DMIR / SSA
                              ↓
                   OPTIMIZATION ENGINE
                              │
        ┌─────────────────────┼──────────────────────┐
        ↓                     ↓                      ↓
      scalar                SIMD                  parallel
        ↓                     ↓                      ↓
        └─────────────────────┼──────────────────────┘
                              ↓
                     TARGET BACKENDS
                              ↓
             CPU / GPU / WASM / MCU / future
                              ↓
                        minimal runtime
```

---

# 421. ФИНАЛЬНАЯ ФОРМУЛА КЛАССА

```dtr
class User {
    id UserId
    name String
    age Int

    isAdult() -> Bool => age >= 18
}
```

Создание:

```dtr
user = User {
    id: 10
    name: "Alex"
    age: 20
}
```

Дополнительное поведение:

```dtr
behavior User {
    validate() -> Bool => name != "" && age >= 0
}
```

Роль:

```dtr
role Serializable {
    serialize() -> Bytes
}
```

Композиция:

```dtr
component Audited {
    createdAt Instant
    updatedAt Instant
}

class Admin from User with Audited, Serializable {
    permissions Permissions
}
```

Это должен быть узнаваемый, современный Datara OOP.

---

# 422. ФИНАЛЬНАЯ ФОРМУЛА FLOW

```dtr
flow analyze(data Table) -> Report!Error {
    data
        |> normalize()
        |> filter(.valid)
        |> aggregate()
        |> report()
}
```

Compiler видит graph.

---

# 423. ФИНАЛЬНАЯ ФОРМУЛА PARALLEL

```dtr
parallel {
    users = loadUsers()
    products = loadProducts()
    orders = loadOrders()
}
```

Compiler выбирает execution strategy.

---

# 424. ФИНАЛЬНАЯ ФОРМУЛА CLI

```dtr
out "Processed {count} files"
err "Cannot open {path}"
```

---

# 425. ФИНАЛЬНАЯ ФОРМУЛА AI

```dtr
model Classifier {
    forward(x Tensor<Float32>) -> Tensor<Float32> {
        x |> normalize |> infer
    }
}
```

---

# 426. ФИНАЛЬНАЯ ФОРМУЛА DOMAIN

```bash
forgen domain
```

означает:

```text
понимать весь проект
понимать target
понимать actual usage
понимать profile
понимать intent
найти reachable graph
специализировать
оптимизировать
проверить IR
собрать native artifact
```

---

# 427. ГЛАВНАЯ ПРОВЕРКА ИДЕОЛОГИИ

Любую будущую feature проверять четырьмя вопросами:

```text
1. Удобнее ли человеку?
2. Умнее ли становится semantic model?
3. Может ли compiler сделать больше с этой информацией?
4. Не увеличиваем ли мы language surface без необходимости?
```

---

# 428. ИТОГОВАЯ ИДЕОЛОГИЯ

Datara не выбирает между:

```text
OOP / functions
Python / Rust
high-level / low-level
human / AI
simple / powerful
safe / fast
```

Она меняет уровень, на котором эти противопоставления вообще возникают.

```text
HIGH-LEVEL SOURCE
        ↓
RICH SEMANTIC MODEL
        ↓
STRICT VERIFICATION
        ↓
WHOLE-PROGRAM UNDERSTANDING
        ↓
SPECIALIZATION
        ↓
NATIVE OPTIMIZATION
        ↓
MINIMAL EXECUTION
```

---

# 429. ЧТО НУЖНО СЧИТАТЬ «СЕРДЦЕМ» ПРОЕКТА

Не красивый syntax.

Не AI.

Не benchmark.

Не embedded.

**Сердце Datara — семантический граф + Forgen, который умеет превращать этот граф в специализированную машину.**

Всё остальное является способом наполнить граф полезной информацией.

---

# 430. ЧТО НУЖНО СЧИТАТЬ «ЛИЦОМ» ПРОЕКТА

**Лицо Datara — современный, знакомый и очень компактный исходник.**

Разработчик должен открыть файл и быстро понять программу.

Forgen должен открыть этот же файл и увидеть гораздо больше:

```text
types
lifetimes
ownership
effects
dataflow
call graph
resource usage
AI graph
optimization opportunities
```

---

# 431. ЧТО НУЖНО СЧИТАТЬ «МЫШЦАМИ» ПРОЕКТА

**Forgen optimization engine.**

Он должен делать тяжёлую работу:

```text
analyze
prove
specialize
fuse
inline
vectorize
parallelize
layout-optimize
remove
link
```

---

# 432. ЧТО НУЖНО СЧИТАТЬ «НЕРВАМИ» ПРОЕКТА

**Tooling и diagnostics.**

Они связывают:

```text
human
compiler
AI
runtime
```

---

# 433. ЧТО НУЖНО СЧИТАТЬ «СКЕЛЕТОМ» ПРОЕКТА

**Type system + memory model + module system.**

Если эти три вещи слабые, ни один optimizer не спасёт язык.

---

# 434. ЧТО НУЖНО СЧИТАТЬ «СИСТЕМОЙ КРОВООБРАЩЕНИЯ»

**IR и dependency graph.**

Они доставляют информацию между front-end и back-end.

---

# 435. ЧТО НУЖНО СЧИТАТЬ «ОРГАНИЗМОМ»

```text
Datara source
+
Forgen compiler
+
stdlib
+
LSP/tooling
+
package ecosystem
+
bench/test infrastructure
```

Всё это один продукт.

---

# 436. ПЕРВЫЙ БОЛЬШОЙ РЕЛИЗ

Datara 0.1 должна доказывать не ширину, а качество.

Цель:

```text
small language
strong compiler
beautiful UX
native output
safe memory model
real optimizer
```

---

# 437. DATARA 0.2

Расширить:

```text
advanced behavior/extensions
cross-module specialization
PGO
benchmark/report tooling
AI context API
```

---

# 438. DATARA 0.3

Расширить:

```text
Tensor
Model
better data pipelines
embedded target
state machines
```

---

# 439. DATARA 1.0

Стабилизировать:

```text
language spec
ABI
stdlib core
package manager
LSP
compiler formats
memory model
```

---

# 440. LONG-TERM

```text
GPU
NPU
industrial
advanced formal verification
distributed compilation
adaptive specialization
```

---

# 441. ОКОНЧАТЕЛЬНЫЙ МАНИФЕСТ

> **Datara не должна заставлять человека быть компилятором.**
>
> **Datara не должна заставлять человека быть memory manager.**
>
> **Datara не должна заставлять человека быть scheduler.**
>
> **Datara не должна заставлять человека быть optimizer.**
>
> **Datara не должна заставлять человека писать boilerplate ради архитектуры.**
>
> Но Datara и не должна скрывать опасность: если операция небезопасна, nondeterministic или имеет серьёзный side effect, это должно быть известно compiler и tooling.

---

# 442. ОКОНЧАТЕЛЬНАЯ ФОРМУЛА ПРОСТОТЫ

```text
простота source
≠
простота compiler
```

Мы специально делаем compiler сложным, чтобы source мог быть простым.

---

# 443. ОКОНЧАТЕЛЬНАЯ ФОРМУЛА БЫСТРОТЫ

```text
fast code
≠
low-level source
```

Мы хотим доказать обратное:

```text
high-level source
+
semantic visibility
+
strong compiler
=
near-zero abstraction overhead
```

---

# 444. ОКОНЧАТЕЛЬНАЯ ФОРМУЛА БЕЗОПАСНОСТИ

```text
safe source
+
ownership
+
effects
+
verified boundaries
=
сильная compile-time safety model
```

Цель — сравнимый класс гарантий с Rust в safe subset, но с меньшей когнитивной стоимостью; утверждение о полном превосходстве возможно только после формальных доказательств и огромного test suite.

---

# 445. ОКОНЧАТЕЛЬНАЯ ФОРМУЛА AI

```text
AI
+
semantic source
+
compiler feedback
+
structured diagnostics
+
benchmark evidence
=
AI-assisted engineering rather than text generation
```

---

# 446. ОКОНЧАТЕЛЬНАЯ ФОРМУЛА УНИВЕРСАЛЬНОСТИ

```text
one language
+
multiple lowering strategies
+
target profiles
+
minimal runtime
=
multiple domains without multiple languages
```

---

# 447. ОКОНЧАТЕЛЬНЫЙ ОБРАЗ

Человек пишет:

```dtr
class User {
    name String
    age Int
}

flow adults(users List<User>) -> List<User> {
    users |> filter(.age >= 18)
}

out "done"
```

Forgen видит:

```text
User
├── identity/value semantics
├── layout constraints
└── methods/behaviors

adults
├── List<User>
├── predicate
└── dataflow graph

output
└── IO effect
```

А затем:

```text
reachability
→ specialization
→ memory analysis
→ bounds proof
→ pipeline fusion
→ allocation elimination
→ SIMD analysis
→ target lowering
→ link
```

И конечная машина никогда не обязана знать, насколько красивым и высокоуровневым был исходник.

---

# 448. THE DATARA PRINCIPLE

> **Write clearly. Define intent. Let Forgen build the machine.**

Русская версия:

> **Пиши понятно. Описывай намерение. Пусть Forgen строит машину.**

---

# 449. ОТДЕЛЬНАЯ ЗАМЕТКА О ТЕХНИЧЕСКОЙ ЧЕСТНОСТИ

Некоторые цели этого документа — **design goals**, а не доказанные свойства готовой реализации.

Особенно это касается:

```text
«не отставать от Rust»
«быть быстрее Rust»
«быть быстрее C++»
«гарантировать real-time»
«оптимизировать абсолютно всегда»
```

Их нужно превращать в измеримые benchmark/verification criteria по мере реализации.

Это не ослабляет идею. Наоборот: это превращает её из маркетинговой фантазии в инженерную программу.

---

# 450. ФИНАЛЬНЫЙ CHECKLIST ПЕРЕД РЕАЛИЗАЦИЕЙ

## Language

```text
[ ] syntax grammar
[ ] lexical rules
[ ] variable rules
[ ] type inference
[ ] strict typing
[ ] record/class/component/role/behavior
[ ] generics
[ ] closures
[ ] match
[ ] Result/Optional
[ ] modules
[ ] visibility
[ ] effects
[ ] ownership
[ ] concurrency
[ ] async
[ ] intent
```

## Compiler

```text
[ ] parser
[ ] resolver
[ ] type checker
[ ] ownership analyzer
[ ] effect analyzer
[ ] semantic graph
[ ] HIR
[ ] DMIR
[ ] optimizer
[ ] backend
[ ] linker
[ ] incremental cache
[ ] Domain build
```

## Tooling

```text
[ ] fmt
[ ] check
[ ] test
[ ] bench
[ ] profile
[ ] inspect
[ ] LSP
[ ] AI context
[ ] semantic diff
```

## Domains

```text
[ ] CLI
[ ] data
[ ] native server
[ ] tensor/AI
[ ] embedded
[ ] industrial
[ ] WASM
```

---

# 451. ФИНАЛЬНАЯ КОНЦЕПЦИЯ В ОДНОМ ДЕРЕВЕ

```text
DATARA
│
├── Human-friendly syntax
│   ├── TS-like familiarity
│   ├── Python-like density
│   └── Datara-specific semantics
│
├── Type system
│   ├── strict
│   ├── inference
│   ├── generics
│   ├── sum types
│   └── contracts/future
│
├── Object model
│   ├── class
│   ├── record
│   ├── component
│   ├── role
│   └── behavior
│
├── Program composition
│   ├── functions
│   ├── lambdas
│   ├── flow
│   ├── pipeline
│   ├── parallel
│   └── async
│
├── Safety
│   ├── ownership
│   ├── borrowing
│   ├── effects
│   ├── Result/Optional
│   └── unsafe boundary
│
├── Domains
│   ├── CLI
│   ├── Data
│   ├── AI/Tensor
│   ├── Embedded
│   ├── Industrial
│   └── WASM
│
└── FORGEN
    ├── semantic graph
    ├── reachability
    ├── specialization
    ├── escape analysis
    ├── allocation elimination
    ├── devirtualization
    ├── vectorization
    ├── parallelization
    ├── data layout
    ├── cross-module optimization
    ├── LTO
    ├── PGO
    ├── target tuning
    └── native codegen
```

---

# 452. ФИНАЛЬНЫЙ ВЫВОД

**Datara не должна стать новым языком только потому, что у неё новый syntax.**

Она должна стать новым языком потому, что у неё другая инженерная единица мысли:

```text
не class
не function
не file
не module
не thread
не tensor
не library

а

SEMANTIC PROGRAM GRAPH
```

Class — один способ описать сущность.

Behavior — способ разделить поведение.

Flow — способ описать выполнение.

Role — способ описать способность.

Component — способ переиспользовать структуру.

Table/Stream/Tensor — специализированные data semantics.

Intent — способ сообщить цель.

А Forgen объединяет всё это в один граф и получает возможность строить конкретную машину.

**Именно это является фундаментом всего проекта.**

---

# 453. NEXT SPECIFICATION

Следующий документ проекта должен быть уже не концепцией, а строгой спецификацией:

**`DATARA_LANGUAGE_SPEC_v0.1.md`**

В нём нужно без философских рассуждений зафиксировать:

```text
grammar
keywords
operator precedence
literals
variables
scopes
functions
classes
records
components
roles
behaviors
modules
imports
visibility
generics
lambda
match
Optional
Result
ownership
borrowing
effects
async
parallel
unsafe
FFI
CLI
compiler semantics
```

После этого отдельно:

**`FORGEN_COMPILER_ARCHITECTURE_v0.1.md`**

с точной схемой:

```text
AST
→ Resolver
→ Type System
→ Ownership
→ Effects
→ Semantic Graph
→ HIR
→ DMIR
→ Optimizer
→ LLVM/backend
→ Linker
→ Runtime
```

И только затем имеет смысл писать implementation backlog.

---

# 454. Sources / Engineering References

1. **The Rust Programming Language — Understanding Ownership / Concurrency.** Официальная документация Rust. Используется как reference для ownership/borrowing и compile-time safety goals.
2. **TypeScript Handbook — Type Inference / Classes / Modules.** Официальная документация TypeScript. Используется как reference для ergonomic inference и familiar syntax.
3. **LLVM Documentation — Analysis and Transform Passes.** Reference для pass-based compiler architecture и разделения analyses/transformations.
4. **WebAssembly Specifications / WASI.** Reference для дополнительного portable target и системной interface model.

Эти технологии являются инженерными ориентирами, а не шаблонами для копирования. Datara должна иметь собственную semantic model, собственный surface syntax и собственную optimization strategy.

---

# 455. ONE-LINE IDENTITY

> **Datara — язык для человека; Forgen — компилятор для машины; semantic graph — место, где они встречаются.**


# ПРИЛОЖЕНИЕ A — ПОЛНЫЙ СЛОЙ УТОЧНЁННОЙ КОНЦЕПЦИИ

Ниже сохранён полный материал второй редакции концепта как трассируемый слой проектирования. Он не заменяет первый фундамент: его задача — сохранить все решения, появившиеся во второй редакции, чтобы ничего не потерялось при переходе к формальной спецификации.

# 1. ЦЕЛЬ ПРОЕКТА

Datara — строго типизированный, memory-safe по умолчанию, нативно компилируемый язык общего назначения.

Он должен быть естественен для:

```text
CLI и системных утилит
автоматизации
data processing
backend / server
desktop tools
high-performance applications
AI/data libraries
simulation
embedded systems
real-time control
industrial automation
hardware-oriented software
```

При этом это **один язык**, а не четыре DSL внутри одного продукта.

Разница между задачами появляется главным образом в:

```text
libraries
runtime profile
resource constraints
hardware target
compiler strategy
```

а не в смене синтаксиса.

---

# 2. ЧЕГО DATARA НЕ ДЕЛАЕТ

Datara не пытается:

1. заменить весь существующий ecosystem с первого релиза;
2. включить AI как магическую часть grammar;
3. сделать обязательным ООП;
4. заставить всех писать в functional style;
5. реализовать десятки почти одинаковых языковых конструкций;
6. переносить Rust syntax без изменений;
7. обещать абсолютную победу над Rust для каждого benchmark.

Фактическая performance-цель проекта гораздо конкретнее:

> **для сопоставимых алгоритмов Datara должна стремиться к native performance уровня Rust, желательно в пределах примерно 1–2% от хорошо оптимизированной Rust-реализации на релевантных workloads, при этом предоставляя более простой source-level model.**

Это инженерная цель, а не гарантия.

Для JavaScript/TypeScript и особенно Python целевой benchmark profile может показывать существенный выигрыш там, где Datara имеет прямой native path и где JS/Python несут соответствующий runtime overhead. Но это также проверяется benchmark suite, а не предполагается заранее.

---

# 3. ГЛАВНАЯ МОДЕЛЬ ПРОГРАММЫ

Datara использует композиционную семантическую модель:

```text
DATA
BEHAVIOR
ROLE
COMPONENT
FLOW
TASK
```

### DATA
Описание значений и их структуры.

### BEHAVIOR
Набор операций, относящихся к конкретному типу/классу.

### ROLE
Capability/contract: что сущность обязана уметь.

### COMPONENT
Переиспользуемая часть состояния или структуры без отдельной identity.

### FLOW
Явный граф прохождения данных и операций.

### TASK
Переиспользуемая типизированная вычислительная единица. Она не связана с AI и предназначена для любой долгой, параллельной, асинхронной, распределённой или просто логически выделенной работы.

Эти конструкции могут встречаться в одном проекте и быть свободно смешаны.

---

# 4. ООП DATARA: НЕ ОТКАЗ ОТ КЛАССОВ, А ПЕРЕСБОРКА КЛАССОВ

Программисты уже умеют мыслить через:

```text
User
Order
Machine
Device
Model
Renderer
```

Поэтому `class` остаётся частью языка.

Но Datara-class — это **семантическая сущность**, а не обязательный heap-object.

```text
class source abstraction
        ↓
semantic class
        ↓
physical representation chosen by Forgen
```

Физически класс может стать:

```text
scalar values
struct-like layout
stack object
inline aggregate
arena object
heap allocation
packed storage
register values
```

если observable semantics сохраняется.

---

# 5. НОВЫЙ SYNTAX КЛАССА

Базовая форма:

```dtr
class User {
    id UserId
    name String
    age Int
    active Bool = true

    isAdult() -> Bool => age >= 18

    rename(to newName String) {
        name = newName
    }
}
```

Основные решения:

- имя поля и тип можно писать как `name String`;
- двоеточие не является обязательным;
- `this` не требуется для обычного доступа к полям;
- краткие выражения допускают `=>`;
- конструктор не является обязательным;
- объект создаётся через typed initializer;
- behavior можно вынести в отдельный файл;
- composition выполняется через `with` и `+` blocks;
- inheritance не является единственной формой повторного использования.

---

# 6. КОМПАКТНАЯ ФОРМА VS ОПТИМИЗАЦИЯ

Запись:

```dtr
name String
```

никоим образом не означает динамический тип.

После semantic analysis Forgen имеет эквивалент:

```text
FieldSymbol
name: name
semantic_type: String
mutability: known
layout: selected later
ownership: inferred
```

То есть:

```text
surface syntax ≠ semantic representation ≠ machine representation
```

Это фундаментальный принцип Datara.

Можно писать компактнее для человека, не платя за это оптимизацией.

---

# 7. ПЕРЕМЕННЫЕ

Предлагаемый базовый синтаксис:

```dtr
let count = 10
mut count = 10
const BUFFER_SIZE = 4096
```

`let` — immutable binding по умолчанию.  
`mut` — явно изменяемое binding.  
`const` — compile-time constant, если выражение действительно может быть вычислено на compile time.

### Дополнительная короткая форма

Для локальных переменных с полностью выводимым типом предлагается оператор `:=`:

```dtr
name := "Alex"
count := 0
buffer := Array<Float32>(1024)
```

`:=` означает:

> «создай новое локальное immutable binding с type inference».

Он **не копирует Go semantics**: в Datara `mut` остаётся явным.

То есть:

```dtr
x := 10
```

— immutable.

Для mutation:

```dtr
mut x := 10
```

Таким образом `:=` — это удобная запись declaration + inference, а не отдельная модель переменных.

Рекомендованный стиль для публичных API — явный тип там, где он улучшает документацию:

```dtr
export fn parse(input String) -> Result<Data, ParseError>
```

---

# 8. ДВЕ СТУПЕНИ СИНТАКСИЧЕСКОЙ ЯВНОСТИ

Datara сознательно допускает:

### Beginner-friendly

```dtr
user := User {
    name: "Alex"
    age: 20
}
```

### Explicit / advanced

```dtr
let user: User = User {
    name: "Alex"
    age: 20
}
```

Обе формы имеют одну семантику.

Forgen должен уметь сообщать style warning:

```text
hint: Datara style prefers inferred local bindings here
```

Но не запрещать альтернативный способ только ради догмы.

Позднее language editions могут ужесточать style policy, сохраняя semantic compatibility.

---

# 9. FUNCTIONS

Основная короткая форма:

```dtr
fn add(a Int, b Int) -> Int => a + b
```

Полная форма:

```dtr
fn add(a Int, b Int) -> Int {
    a + b
}
```

`function` остаётся допустимым alias для читабельности в явных API declarations и migration scenarios:

```dtr
function loadConfig(path Path) -> Config!ConfigError {
    ...
}
```

Рекомендация: `fn` для нового native style, `function` — когда форма читается лучше в длинном публичном API.

---

# 10. FUNCTION PIPELINE

Функция в Datara имеет три важных слоя:

```text
signature
semantic effects
execution strategy
```

Например:

```dtr
fn normalize(data Table) -> Table {
    ...
}
```

Forgen отдельно выводит:

```text
type effect ownership cost
```

и уже после этого решает:

```text
inline?
vectorize?
parallelize?
materialize?
```

---

# 11. TASK

`task` — не AI keyword.

Это универсальная единица вычисления:

```dtr
task parseLargeFile(path Path) -> Table!ParseError {
    ...
}
```

Она может использоваться для:

```text
parallel work
background work
async IO orchestration
batch jobs
worker pools
distributed adapters
hardware jobs
```

AI-библиотека может определить свои собственные `task`, но язык ничего не знает про нейросеть.

---

# 12. LAMBDA

Короткая форма:

```dtr
x => x * 2
```

Несколько параметров:

```dtr
(a, b) => a + b
```

Блок:

```dtr
user => {
    score := normalize(user)
    score * 2
}
```

Closure semantics строго типизированы.

Forgen обязан пытаться:

```text
inline
stack-promote captures
eliminate environment allocation
specialize generic closure
```

если это безопасно.

---

# 13. УНИКАЛЬНЫЙ PIPELINE

```dtr
result = data
    |> normalize()
    |> filter(x => x.score > 0.8)
    |> map(.value)
    |> reduce(sum)
```

`|>` — не просто sugar.

Pipeline превращается в semantic data-flow graph.

Это позволяет Forgen анализировать всю цепочку как единое вычисление.

Он может получить:

```text
loop fusion
intermediate elimination
vectorization
parallelization
buffer reuse
layout optimization
```

В результате красивый pipeline может скомпилироваться в один низкоуровневый loop.

---

# 14. НОВАЯ СИСТЕМА УСЛОВИЙ: MATCH + DECIDE

Обычный `if` остаётся:

```dtr
if age >= 18 {
    adult()
} else {
    minor()
}
```

Но для многовариантной логики Datara предлагает две формы.

## `match`

Для структурного pattern matching:

```dtr
match state {
    Loading => showLoader()
    Ready(data) => show(data)
    Failed(error) => showError(error)
}
```

## `decide`

Новая конструкция для **условий по порядку приоритетов**, когда программист хочет описывать не структуру типа, а набор логических guard-веток:

```dtr
decide {
    score >= 0.9 => Excellent
    score >= 0.7 => Good
    score >= 0.4 => Normal
    else => Poor
}
```

Это читается как:

> «выбери первый подходящий guard».

Forgen может строить decision tree, сортировать независимые проверки, использовать jump-table/branchless form или другие реализации, если семантика допускает.

## `select`

Для небольшого значения-результата:

```dtr
label := select {
    score >= 0.9 => "A"
    score >= 0.8 => "B"
    else => "C"
}
```

`decide` и `select` не заменяют `if`; они уменьшают необходимость строить длинные лестницы `else if`.

---

# 15. MATCH, DECIDE И AI READABILITY

AI-помощнику проще анализировать:

```text
match → finite state alternatives
decide → ordered guards
select → value-producing guards
if → binary control flow
```

чем пытаться угадывать назначение длинной цепочки вложенных условий.

Это не AI syntax. Это **чёткая semantics**, которая одновременно удобна человеку и хорошо представима в semantic graph.

---

# 16. ООП: SPLIT BEHAVIOR

Основной класс:

```dtr
class User {
    id UserId
    name String
}
```

Отдельный behavior:

```dtr
behavior User {
    isAdult() -> Bool => age >= 18

    rename(to newName String) {
        name = newName
    }
}
```

Для пользователя это всё ещё `User`.

Для Forgen это отдельные incremental units, объединённые в один semantic identity.

Для AI это отдельные context slices.

---

# 17. SPLIT CLASS

Файл/пакет:

```text
user/
    core.dtr
    billing.dtr
    security.dtr
    serialization.dtr
```

`core.dtr`:

```dtr
class User {
    id UserId
    name String
}
```

`billing.dtr`:

```dtr
behavior User {
    invoice() -> Invoice!BillingError {
        ...
    }
}
```

Это даёт одновременно:

```text
small files
incremental compilation
better ownership of concerns
AI context slicing
cross-module optimization
```

---

# 18. COMPONENT

```dtr
component Timestamped {
    createdAt Instant
    updatedAt Instant
}
```

Использование:

```dtr
class User with Timestamped {
    name String
}
```

`component` не имеет собственной identity.

Forgen может физически inline'ить его поля.

---

# 19. ROLE

```dtr
role Serializable {
    serialize() -> Bytes
}
```

Implementation:

```dtr
class User with Serializable {
    ...
}
```

Role задаёт capability/contract, а не parent identity.

---

# 20. НОВОЕ НАСЛЕДОВАНИЕ

Обычный знакомый путь остаётся возможным:

```dtr
class Admin extends User {
    permissions Permissions
}
```

Но native Datara-style:

```dtr
class Admin from User {
    + Permissioned
    + Audited

    permissions Permissions
}
```

Здесь:

```text
from = базовая identity
+ = добавление capability/component/role package
```

Несколько class bases не разрешаются в v1.

`+` не создаёт runtime wrapper.

---

# 21. REPLACING BEHAVIOR

Вместо обязательного `override` предлагается:

```dtr
class Admin from User {
    replaces greet() {
        out "Admin hello"
    }
}
```

Смысл явный:

> `Admin.greet` заменяет inherited `User.greet`.

Для migration можно поддержать `override` как alias с warning в strict-style mode.

---

# 22. CONSTRUCTOR MODEL

В большинстве случаев constructor не нужен:

```dtr
user := User {
    id: 10
    name: "Alex"
}
```

Если нужны инварианты, factory-style creation:

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

Создание:

```dtr
user := User.create(10, "Alex")!
```

В языке не запрещаются advanced custom constructors, но они не являются обязательной церемонией.

---

# 23. PRIVACY

Базовая модель:

```text
default = module-private
export = public
```

Публичный API явно помечается:

```dtr
export class User {
    id UserId
}
```

Локальные детали не нужно помечать `private` по каждой строке.

---

# 24. GENERICS

```dtr
class Box<T> {
    value T
}

box := Box { value: 10 }
```

`T` выводится как `Int`.

Constraint:

```dtr
fn save<T: Serializable>(value T) -> Bytes {
    value.serialize()
}
```

Datara предпочитает capability constraints над наследованием от base class.

---

# 25. GENERIC STRATEGY

Forgen не обязан всегда выбирать одну реализацию generic.

Cost model может выбрать:

```text
monomorphization
shared generic body
partial specialization
hot-path specialization
```

Если generic становится hotspot, Forgen может сформировать специализированный вариант.

---

# 26. TYPES

Основные категории:

```text
Int / UInt / Float
Bool
Char
String / Str
Bytes
Array
List
Map
Set
record
class
union/sum type
T?
T!E
generics
function types
views
units
```

---

# 27. OPTIONAL

```dtr
user: User?
```

Отсутствие значения представляется `None`-подобным вариантом, а не произвольным nullable state.

Pattern:

```dtr
match user {
    Some(value) => use(value)
    None => fallback()
}
```

Синтаксический sugar для безопасного unwrap допускает `?`/optional chaining, но должен быть однозначно типизирован.

---

# 28. RESULT

```dtr
fn load(path Path) -> Config!ConfigError
```

Распространение:

```dtr
config := load(path)!
```

Datara предпочитает explicit error flow в обычном коде.

Exceptions могут существовать на внешних boundaries, но не являются основной control-flow моделью языка.

---

# 29. OWNERSHIP — ОСНОВНОЙ ПРИНЦИП

Datara стремится к Rust-level safety guarantees без копирования Rust surface syntax.

Внутри Forgen присутствуют:

```text
ownership
borrow inference
alias analysis
escape analysis
lifetime inference
```

Но обычный разработчик не должен писать lifetime annotations.

---

# 30. BORROW KEYWORDS

Когда inference недостаточно, доступны явные формы:

```dtr
fn inspect(data view Data) -> Int
fn edit(data mut-view Data)
fn consume(data own Data)
fn share(data shared Data)
```

Также допустим более Rust-like advanced syntax как compatibility/low-level mode, но compiler style guide рекомендует human-readable form.

Forgen может давать hint:

```text
hint: inferred borrow; explicit `view` is unnecessary here
```

---

# 31. UNSAFE

```dtr
unsafe {
    memory.write(address, value)
}
```

Unsafe boundaries должны быть локализованы.

`forgen inspect safety` показывает все unsafe regions, FFI boundaries и unsafe dependencies.

---

# 32. EFFECT SYSTEM

Forgen выводит эффекты:

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

Например:

```dtr
fn add(a Int, b Int) -> Int => a + b
```

имеет `Pure`.

Эффекты помогают:

```text
optimization
parallelization
AI reasoning
testing
verification
```

---

# 33. PARALLEL

```dtr
parallel {
    users := loadUsers()
    orders := loadOrders()
    products := loadProducts()
}
```

Смысл не «создать три потока», а:

> эти операции независимы и могут быть выполнены независимо.

Forgen выбирает:

```text
sequential execution
task scheduling
thread pool
work stealing
SIMD
GPU
```

на основании cost model.

---

# 34. PARALLEL FOR

```dtr
parallel for item in items {
    process(item)
}
```

Compiler проверяет:

```text
aliasing
mutation
reduction semantics
effects
work granularity
```

и при необходимости отказывается от parallelization.

---

# 35. ASYNC

`async/await` остаётся доступным, но не обязателен.

```dtr
async fn load() -> Data!LoadError {
    ...
}
```

В user-facing style можно позволить Forgen выводить async lowering для подходящих APIs, если это безопасно и однозначно.

Принцип:

> advanced developer может управлять concurrency явно; beginner может писать естественный последовательный код, а compiler подсказывает возможности оптимизации.

---

# 36. FLOW

```dtr
flow processOrder(order Order) -> Receipt!OrderError {
    order
        |> validate()
        |> calculate()
        |> reserve()
        |> pay()
        |> ship()
}
```

`flow` — named execution graph.

Он нужен не ради красивого ключевого слова, а для:

```text
optimization
visualization
testing
profiling
AI context
```

---

# 37. STREAM

```dtr
stream := file.lines(path)!

stream
    |> filter(.nonEmpty)
    |> map(parseLog)
    |> each(analyze)
```

Forgen старается сохранить streaming semantics и не materialize весь input.

Явная materialization:

```dtr
result := stream |> map(f) |> collect()
```

---

# 38. TABLE

```dtr
users := table.read("users.csv")!

result := users
    |> where(age >= 18)
    |> select(name, age)
    |> groupBy(country)
    |> aggregate(avg(age), count())
```

`Table` остаётся library/type-level capability, а не частью language core.

---

# 39. TENSOR И AI

**AI не является специальным ядром Datara.**

Язык предоставляет:

```text
numeric types
arrays
views
memory model
generic functions
FFI
parallelism
native kernels
```

А AI/data stack поставляется библиотеками:

```text
Datara tensor library
Datara linear algebra library
Datara model/inference library
Datara tokenizer library
Datara dataset/dataframe library
```

То есть `model` не является обязательной language keyword.

Любая модель — обычная библиотечная структура:

```dtr
import ai.tensor
import ai.inference
```

Это сохраняет маленькое ядро и не превращает язык в AI-only DSL.

---

# 40. AI-FIRST TOOLING, А НЕ AI-FIRST LANGUAGE

Datara должна быть удобной для нейросетей прежде всего через Forgen:

```text
stable grammar
semantic graph
strong types
effects
ownership
machine-readable diagnostics
context slicing
semantic diff
optimization report
```

AI не получает отдельные «магические» права.

Компилятор остаётся конечным verifier.

---

# 41. CLI

Вместо:

```dtr
console.log("Hello")
```

Datara предлагает:

```dtr
out "Hello"
err "Invalid argument"
```

Для структурированного CLI:

```dtr
cli app "grepfast" {
    command search {
        pattern String
        path Path
        ignoreCase Bool = false

        run {
            searchFile(path, pattern, ignoreCase)!
                |> each(out)
        }
    }
}
```

Внутри это semantics, из которой Forgen может вывести:

```text
argument parser
validation
help
completion metadata
entry point
fast output path
```

CLI syntax остаётся библиотечной/domain capability, а core printer/output primitives остаются минимальными.

---

# 42. ЕДИНИЦЫ ИЗМЕРЕНИЯ

```dtr
speed := 80 km/h
period := 5 ms
voltage := 24 V
```

Units являются типовой семантикой, а не только runtime values.

Это особенно важно для:

```text
embedded
industrial
robotics
physics
simulation
```

---

# 43. STATE MACHINE

Для реальных систем:

```dtr
machine Door {
    Closed
    Opening
    Open
    Closing

    Closed -> Opening when openCommand
    Opening -> Open when position == 100%
    Open -> Closing when closeCommand
    Closing -> Closed when position == 0%
}
```

`machine` — кандидат на стандартную библиотечную/compiler capability, а не обязательная universal language primitive до тех пор, пока semantics и verifier не будут полностью формализованы.

---

# 44. TARGETS

Основные цели:

```text
x86-64
ARM64
WASM/WASI
ARM Cortex-M
RISC-V MCU
```

Позже:

```text
GPU targets
DSP / accelerator targets
industrial SoCs
```

Datara source language остаётся одинаковым.

---

# 45. FFI

```dtr
extern "C" fn native_call(ptr *U8, len Int) -> Int
```

ABI boundary фиксирует layout.

Внутри Datara Forgen может свободно менять representation.

Это принципиальная граница:

```text
inside semantic graph → aggressive freedom
ABI boundary → stable contract
```

---

# 46. МОДУЛИ

Минимальный module syntax:

```dtr
import user
import user.billing

export fn checkout(...)
```

Поведение можно экспортировать из отдельных файлов без wrappers.

Файлы — организационная единица, но не optimization boundary.

---

# 47. ФИЛОСОФИЯ ФАЙЛОВ

Один большой класс может быть разделён:

```text
core.dtr
security.dtr
billing.dtr
network.dtr
serialization.dtr
```

Но в Domain build Forgen объединяет всё в semantic graph.

Поэтому:

```text
developer modularity
       ≠
performance loss
```

---

# 48. STANDARD LIBRARY

Core должен быть маленьким.

Богатый functionality должна жить в standard modules:

```text
std.io
std.fs
std.net
std.json
std.csv
std.time
std.text
std.cli
std.math
std.crypto
std.sync
std.async
std.data
std.tensor
std.hardware
```

Некоторые из них могут быть официальными, некоторые community-provided.

---

# 49. RUNTIME

Runtime modular:

```text
memory
io
formatting
async
sync
networking
threading
platform
```

Если networking не используется, его код не должен попадать в Domain binary.

---

# 50. FORGEN — ГЛАВНАЯ ИНЖЕНЕРНАЯ ИДЕЯ

Forgen не должен быть «парсером + LLVM».

Он должен быть semantic compiler platform:

```text
source
 ↓
syntax tree
 ↓
name resolution
 ↓
type/effect/ownership analysis
 ↓
semantic graph
 ↓
usage analysis
 ↓
specialization
 ↓
optimization
 ↓
target lowering
 ↓
link/runtime minimization
 ↓
native artifact
```

LLVM, если используется, является backend infrastructure, а не мозг Datara.

---

# 51. SEMANTIC GRAPH

Это центральный объект Forgen.

Он содержит:

```text
modules
symbols
types
calls
data flows
effects
ownership
roles
behaviors
resource usage
ABI constraints
runtime features
profile facts
hardware capabilities
```

Внутри graph сохраняются связи, которые обычный текстовый компилятор может потерять.

---

# 52. ФАЙЛЫ НЕ ОГРАНИЧИВАЮТ ОПТИМИЗАТОР

Проект:

```text
main.dtr
tokenizer.dtr
model.dtr
tensor.dtr
utils.dtr
```

Forgen строит:

```text
all source
 ↓
semantic graph
 ↓
reachable graph
 ↓
specialized graph
 ↓
optimized artifact
```

Поэтому функция, находящаяся в другом модуле, всё ещё может быть inline'лена.

---

# 53. OPTIMIZATION PHILOSOPHY

Не «all flags on».

А:

```text
What is used?
What is provable?
What is hot?
What is cheap?
What target exists?
What constraints matter?
What representation minimizes cost?
```

Forgen выбирает answer через cost model.

---

# 54. ОПТИМИЗАЦИЯ ПО ИСПОЛЬЗОВАНИЮ

Если библиотека содержит 1000 функций, а reachable graph содержит 17, Domain artifact не должен линкувать остальные 983 без причины.

Если generic используется как `List<Float32>`, Forgen может сгенерировать специализацию только для неё.

Если функция pure и результат compile-time constant, она может исчезнуть полностью.

Если объект не escape'ится, heap allocation может исчезнуть.

Если pipeline materialization не нужна, промежуточные arrays могут исчезнуть.

---

# 55. OPTIMIZATION LEVELS

```text
local
function
module
project
whole-program
profile-guided
target-specific
domain-specific
```

`domain` должен доходить до максимальной глубины.

---

# 56. DOMAIN BUILD

```bash
forgen domain
```

Это не «release с O9».

Domain выполняет:

```text
whole-project graph build
reachability
usage analysis
specialization
cross-module optimization
representation selection
layout analysis
inlining/devirtualization
allocation elimination
pipeline fusion
vectorization
parallel analysis
runtime stripping
LTO
optional profile-guided optimization
```

---

# 57. DOMAIN И АВТОМАТИЧЕСКОЕ ПОНИМАНИЕ ЗАДАЧИ

Главная идея:

> **developer сообщает цель, compiler сам выбирает механизм.**

Например:

```dtr
intent {
    performance maximum
    memory minimum
    latency <= 2ms
    deterministic true
}
```

Forgen может выбрать:

```text
specialization
buffer reuse
different layout
parallel/sequential strategy
vectorization
allocator
runtime subset
```

Если constraints не доказаны, Forgen пишет:

```text
constraint not proven
```

и не выдаёт ложную гарантию.

---

# 58. НИКАКИХ СЛУЧАЙНЫХ 50 ФЛАГОВ

CLI остаётся простым:

```bash
forgen start
forgen debug
forgen release
forgen domain
forgen profile
forgen inspect
forgen verify
forgen test
forgen bench
forgen embedded
```

Внутренне system может иметь множество optimizer passes, но пользователь видит несколько хорошо продуманных режимов.

---

# 59. BUILD PROFILES

## start

Цель — максимально быстрая итерация.

```text
incremental
cache
moderate optimization
fast codegen
```

## debug

Цель — наблюдаемость.

```text
strong checks
diagnostics
source mapping
ownership diagnostics
```

## release

Цель — production artifact.

```text
high optimization
minimal runtime
predictable compilation cost
```

## domain

Цель — максимальная specialization.

```text
whole-program analysis
specialization
cross-module optimization
LTO
profile use
```

## verify

Цель — максимальная строгость статического контроля.

## embedded

Цель — минимальный deterministic runtime и target-specific constraints.

## profile

Цель — получить фактические execution facts.

---

# 60. ДОПОЛНИТЕЛЬНЫЕ ПРОФИЛИ

Предусмотреть внутреннюю систему:

```text
native
compact
deterministic
low-memory
latency
throughput
energy
```

Но для пользователя они по возможности формулируются через project intent и manifest, а не через длинные command-line combinations.

---

# 61. HARDWARE-AWARE OPTIMIZATION

Forgen может учитывать:

```text
CPU ISA
cache sizes
SIMD width
memory bandwidth
NUMA topology
GPU availability
NPU availability
power constraints
ABI
```

Он может генерировать multiversion code:

```text
baseline
SIMD optimized
advanced ISA optimized
fallback
```

и выбирать лучший вариант, если это безопасно и экономически оправдано по размеру бинарника.

---

# 62. PGO

```bash
forgen profile
forgen domain
```

Профиль содержит:

```text
hot functions
branch frequencies
allocation hotspots
input distribution
call frequency
```

Forgen может использовать эти данные для:

```text
hot inlining
branch layout
specialization
code placement
layout tuning
```

---

# 63. DOMAIN ДЛЯ AI-БИБЛИОТЕК

AI stack остаётся library-level.

Но Forgen видит native tensor operations благодаря:

```text
generic types
vector operations
specialized library intrinsics
compiler IR contracts
```

Пример библиотеки:

```dtr
import tensor
import ai.inference

fn infer(model Model, input Tensor<Float16>) -> Tensor<Float16> {
    model.run(input)
}
```

Domain может видеть:

```text
tensor lifetime
shape
dtype
kernel sequence
buffer reuse
device transfers
```

если library предоставляет Forgen semantic contracts.

---

# 64. AI И Forgen

Forgen предоставляет AI не отдельный язык, а:

```text
semantic context API
symbol graph
type graph
effect graph
ownership information
optimization report
semantic diff
```

Например:

```bash
forgen context --symbol User.checkout
```

AI получает минимально необходимый semantic slice.

---

# 65. AI SEMANTIC DIFF

После изменения кода:

```text
public API changed: no
new unsafe region: no
new IO effect: yes
new dependency: payments
memory behavior: unchanged
```

Это полезнее текстового diff для agent-based development.

---

# 66. AI ERROR INTERFACE

Ошибки Forgen должны иметь:

```text
human text
stable error code
location
machine-readable structure
suggested fixes
related symbols
```

Должна существовать локализация.

Первый дополнительный язык — русский.

Например:

```text
DTR-TYPE-001
Ошибка типов: ожидался `Int`, но получен `String`.
```

Английский остаётся canonical machine/debug language в toolchain internals.

Позже добавляются другие языки без изменения compiler semantics.

---

# 67. ДВУХКАНАЛЬНЫЕ ОШИБКИ

Terminal output может выглядеть красиво для человека:

```text
Ошибка в `main.dtr:18:12`

`count` имеет тип `String`, но здесь требуется `Int`.

18 │ total += count
              ^^^^^

Подсказка: преобразуйте значение явно:
    total += parseInt(count)
```

И одновременно Forgen может выдавать machine JSON:

```json
{
  "code": "DTR-TYPE-001",
  "file": "main.dtr",
  "line": 18,
  "column": 12,
  "expected": "Int",
  "actual": "String"
}
```

---

# 68. CLI UX

Для маленьких программ:

```bash
forgen run hello.dtr
```

Для проекта:

```bash
forgen start
```

Для полного artifact:

```bash
forgen domain
```

Нет обязательного project skeleton для простого script.

---

# 69. EMBEDDED

Datara должна быть пригодна для:

```text
MCU
firmware
RTOS tasks
bare-metal
industrial controllers
robotics
machine control
```

Core requirements:

```text
no mandatory GC
predictable allocation
interrupt-aware contexts
units of measure
deterministic profile
small runtime
MMIO/FFI
```

---

# 70. REAL-TIME

Уровни:

```text
best effort
soft real-time
hard real-time
```

Hard real-time только там, где toolchain реально способен доказать constraints.

Никакого маркетингового «hard real-time» без proof.

---

# 71. INDUSTRIAL RESOURCES

Intent/manifest может задавать:

```text
RAM <= 256KB
stack <= 32KB
latency <= 1ms
CPU budget <= 30%
power <= X
```

Forgen анализирует constraints и либо подтверждает их, либо сообщает, что доказательство отсутствует.

---

# 72. SECURITY

Компилятор должен различать:

```text
safe source
unsafe source
FFI boundary
foreign dependency
runtime dynamic boundary
```

`unsafe` не распространяется незаметно на весь проект.

---

# 73. PACKAGE MODEL

Package metadata:

```text
name
version
public API
target support
features
native dependencies
unsafe surface
checksums
```

Будущая система:

```text
lockfile
signed metadata
reproducible builds
binary cache
```

---

# 74. ПОЧЕМУ DATARA МОЖЕТ БЫТЬ УДОБНЕЕ RUST

Не потому что Rust «плохой».

А потому что Datara может попытаться поднять часть compiler complexity наверх и скрыть её от everyday code:

```text
Rust source
→ explicit lifetime/ownership language

Datara source
→ inferred ownership + explicit escape hatch
```

При этом внутренний compiler model остаётся строгим.

Цель — не упрощать safety, а **упрощать способ выражения safety**.

---

# 75. PERFORMANCE TARGET

Главный benchmark target:

```text
Datara ≈ Rust
```

Желаемая разница:

```text
0–2% slower — приемлемо и очень хорошо
≈ 0% — идеал
faster — отличный результат
```

Дополнительно:

```text
Datara > JS/TS на native workloads
Datara >> Python на CPU-heavy native workloads
```

Но все сравнения должны иметь:

```text
same algorithm
same input
same precision
same hardware
same I/O assumptions
same compiler settings
```

---

# 76. ПОЧЕМУ ДОСТИГНУТЬ ЭТОЙ ЦЕЛИ В ТЕОРИИ ВОЗМОЖНО

Ключ не в магическом backend.

Нужна цепочка:

```text
strong semantics
+
whole-program visibility
+
ownership/effect analysis
+
specialization
+
smart cost model
+
optimized IR
+
target-aware codegen
```

То есть язык отдаёт компилятору гораздо больше информации, чем типичный динамический runtime-oriented язык.

---

# 77. ZERO-COST ABSTRACTION

Хорошая Datara abstraction может существовать в source code:

```dtr
class Point {
    x Float
    y Float

    length() -> Float => sqrt(x*x + y*y)
}
```

а в машинном коде стать:

```text
load x
load y
mul/add
sqrt
```

без обязательного:

```text
heap allocation
vtable
object header
virtual call
```

---

# 78. DYNAMIC FEATURES ARE EXPLICIT COST BOUNDARIES

Reflection, dynamic loading, opaque plugins, unknown FFI — разрешены, но являются видимыми boundaries.

Чем меньше compiler knowledge, тем меньше доступно specialization.

Это честная модель:

```text
more dynamic → less compiler certainty
more certainty → more optimization
```

---

# 79. МИНИМАЛЬНОЕ ЯДРО

Языковое ядро должно знать только необходимое:

```text
types
variables
functions
classes
records
roles
components
behavior
control flow
modules
generics
ownership model
effects
async/parallel primitives
basic memory/FFI
```

AI/model/dataframe functionality живёт выше.

---

# 80. БИБЛИОТЕЧНАЯ СТРАТЕГИЯ

Официальные библиотеки должны использовать semantic contracts Forgen.

Например:

```text
std.io
std.cli
std.data
std.tensor
std.ai
std.hardware
```

Но Forgen умеет оптимизировать библиотечный код так же, как пользовательский source, если имеет его semantic metadata.

---

# 81. SEMANTIC CONTRACTS ДЛЯ БИБЛИОТЕК

Библиотека может сообщать:

```text
pure
read-only
no-alloc
vectorizable
parallel-safe
noalias
kernel-like
layout-preserving
```

Но compiler должен различать:

```text
claimed contract
verified contract
```

Не доверять annotation как доказательству safety.

---

# 82. COMPILER EXPLAINABILITY

`forgen inspect optimize`:

```text
calculate()
  inline: yes
  allocation removed: 3
  SIMD: enabled
  parallel: rejected
  reason: input below threshold
  specialization: Float32
```

Это важно и для человека, и для AI.

---

# 83. BENCHMARK CULTURE

Benchmark suite обязателен с ранних версий.

Нужно измерять:

```text
compile speed
runtime
memory
binary size
startup
parallel scaling
incremental rebuild
Domain build time
```

И отдельные классы workload:

```text
CLI
text processing
JSON
data processing
numeric
matrix/tensor
network
embedded
FFI
```

---

# 84. COMPILER SELF-BENCHMARK

Forgen сам должен быть benchmarked:

```text
lexing
parsing
resolution
type checking
ownership analysis
semantic graph build
IR generation
optimizer
codegen
link
incremental cache
```

Compiler speed — часть UX продукта.

---

# 85. РАСПРЕДЕЛЕНИЕ СЛОЖНОСТИ

Главная инженерная ставка:

```text
сложность не исчезает
сложность перемещается
```

Она должна уходить:

```text
от пользователя
→ в compiler analysis
→ в verifier
→ в optimizer
→ в generated code
```

Но пользователь видит её, когда возникает конфликт, а не каждую строку.

---

# 86. ТРЁХСЛОЙНЫЙ DEVELOPER EXPERIENCE

### Beginner

```dtr
name := "Alex"
users |> map(.name) |> each(out)
```

### Intermediate

```dtr
parallel for user in users {
    process(user)
}
```

### Advanced

```dtr
intent {
    latency <= 2ms
    memory <= 64MB
}

fn process(data view Data) -> Result {
    ...
}
```

Язык не меняется. Меняется глубина явности.

---

# 87. СОВМЕСТИМОСТЬ С ПРИВЫЧКАМИ

Datara должна быть знакомой на входе:

```text
{}
() =>
if / else
for
class
generic <>
imports
```

Но отличаться на уровне:

```text
behavior
role
component
flow
intent
semantic compilation
```

Это защищает от ощущения «пустышки» и одновременно даёт уникальность.

---

# 88. МОСКИРОВКА НЕ НУЖНА

Datara не должна переименовывать всё только ради бренда.

Новизна должна проявляться там, где она реально полезна:

```text
class composition
behavior splitting
flow semantics
decide/select
intent compilation
semantic graph
inferred ownership
domain specialization
AI semantic tooling
```

---

# 89. ФУНДАМЕНТАЛЬНАЯ ФОРМУЛА DATARA

```text
Readable Source
      ↓
Strong Semantics
      ↓
Compiler Knowledge
      ↓
Program-Specific Specialization
      ↓
Target-Specific Lowering
      ↓
Minimal Runtime
      ↓
Native Performance
```

---

# 90. ФУНДАМЕНТАЛЬНАЯ ФОРМУЛА FORGEN

```text
WHAT
 ↓
DATARA SEMANTICS
 ↓
GRAPH
 ↓
PROOF
 ↓
COST MODEL
 ↓
SPECIALIZATION
 ↓
OPTIMIZATION
 ↓
MACHINE
```

---

# 91. ЧТО СЧИТАЕТСЯ SUCCESS

Datara успешна, если программист может написать:

```text
один читаемый source
```

а получить:

```text
CLI binary
server binary
embedded artifact
high-performance data program
```

не меняя сам язык и не переходя в другой paradigm-specific DSL.

---

# 92. ROADMAP

## Stage 0 — language kernel

```text
lexer
parser
AST
basic type checker
variables
functions
records
classes
modules
```

## Stage 1 — semantic core

```text
generics
roles
components
behavior splitting
Result/Option
match/decide/select
```

## Stage 2 — safety

```text
ownership
borrow inference
alias analysis
effect system
unsafe/FFI
```

## Stage 3 — native backend

```text
HIR
DMIR
LLVM/native backend
linking
minimal runtime
```

## Stage 4 — performance

```text
inlining
DCE
specialization
layout analysis
vectorization
parallel analysis
```

## Stage 5 — Domain

```text
whole-program graph
PGO
cross-module optimization
runtime stripping
hardware specialization
```

## Stage 6 — ecosystem

```text
package manager
standard libs
CLI tooling
IDE/LSP
AI semantic API
embedded SDK
```

---

# 93. КРАСНЫЕ ЛИНИИ ПРОЕКТА

Нельзя жертвовать:

```text
semantic correctness
memory safety
predictability
source readability
small core
compiler explainability
```

ради:

```text
microbench marketing
syntax novelty
feature count
AI gimmicks
```

---

# 94. OPEN DESIGN BOARD

В дальнейшей спецификации нужно окончательно закрыть:

```text
1. complete lexical grammar
2. exact type inference rules
3. overload resolution
4. operator resolution
5. ownership inference algorithm
6. borrow conflict diagnostics
7. async task semantics
8. exact `decide`/`select` pattern rules
9. class `+` composition resolution
10. method conflict resolution
11. module visibility graph
12. package manifest format
13. compile-time evaluation boundary
14. macro/metaprogramming policy
15. reflection policy
16. dynamic plugin model
17. exact FFI ABI contract
18. deterministic arithmetic policy
19. hardware intrinsic interface
20. target feature detection
```

---

# 95. FINAL IDENTITY

> **Datara — язык для человека. Forgen — интеллект компиляции для машины. Semantic graph — мост между ними.**

Идея проекта не в том, чтобы написать ещё один язык с красивым синтаксисом. Идея в том, чтобы построить язык, в котором **простая программа остаётся простой на поверхности, но её внутренняя семантика достаточно богата, чтобы Forgen мог превратить её в очень специализированное, безопасное и быстрое выполнение.**
