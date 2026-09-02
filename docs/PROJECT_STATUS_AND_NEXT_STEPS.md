# Datara + Forgen: текущее состояние и следующий этап

**Дата среза:** 31 августа 2026  
**Рабочая директория:** `D:\DATARA\datara + forgen`  
**Цель документа:** зафиксировать, что реально реализовано, что было исправлено, как проверять проект и какие работы нельзя считать завершёнными.

## 1. Короткий итог

Forgen уже не является только макетом: в проекте есть Rust-компилятор, DMIR с базовыми блоками и терминаторами, Cranelift native backend, линкерный путь Windows, тесты и реальные исправления циклов, короткого замыкания, лексера и ошибочного backend fallback.

Но проект ещё нельзя называть полностью готовым языком и нельзя утверждать достижение parity с Rust. Часть старых оптимизаторных подсистем была аналитической или отчётной, хотя писала `Applied`. Главная текущая работа — отделить доказанные преобразования от кандидатов и не пропускать в отчёты неподтверждённые SIMD, parallel, async, fusion, layout и PGO claims.

**Главный принцип:** строка в trace не доказывает оптимизацию. Доказательство — это изменение canonical DMIR/CFG или backend IR, успешная native-компиляция и сохранение результата/ошибок/порядка эффектов.

## 2. Что было исправлено ранее и реально подтверждено

### 2.1 Native compilation

Активная цепочка:

```text
.dtr source
 -> lexer/parser
 -> DMIR
 -> optimizer
 -> Cranelift IR
 -> COFF/object
 -> MSVC linker
 -> native Windows executable
```

В качестве основного backend используется `src/codegen/cranelift/backend.rs`. Старый C#/`csc.exe` путь из прежних отчётов в текущем `src/` не является активным native backend.

### 2.2 Циклы и CFG

- `for` по диапазонам переводится в реальный CFG с header/body/exit.
- `while` и `loop` используют `Terminator::Branch`/`Terminator::CondBranch`.
- Возврат внутри тела цикла не перезаписывается обратным переходом.
- `Function::set_back_edge` устанавливает back edge только для блока, который ещё имеет fall-through.
- LICM работает по natural loops настоящего CFG, а не только по legacy `Inst::WhileLoop`.
- Деление и остаток не поднимаются из цикла, потому что это может изменить момент trap при нулевом числе итераций.
- Старый небезопасный unrolling удалён/отключён: старый вариант дублировал инструкции без fresh SSA IDs, корректной CFG-реструктуризации и SSA-renaming.

### 2.3 Short-circuit logic

`&&` и `||` больше не проходят как обычный арифметический `BinOp`.

Текущая логика:

1. вычислить левую часть;
2. перейти в short-circuit block или RHS block;
3. вычислить RHS только при необходимости;
4. нормализовать результат в boolean-like `0/1`;
5. объединить ветки в merge block.

Проверяется пропуск побочных эффектов и защита от деления на ноль.

### 2.4 Fail-closed behavior

- Лексер больше не молча выбрасывает неизвестные символы.
- Lone `&` и `|` отвергаются; `&&` и `||` разрешены.
- UTF-8 BOM в начале файла принимается.
- Неизвестный integer/float operator в Cranelift backend вызывает ошибку, а не подменяется на `+`.
- `forgen profile` больше не должен печатать, что программа исполнялась, если реального запуска не было.
- SAE больше не выдаёт автоматически `SIMDVectorized`, `ParallelThreadPool` или `AsyncTaskReactor`, если backend их не умеет эмитить.

## 3. Что было изменено в текущем оптимизационном проходе

### 3.1 `src/optimizer/pipeline_fusion.rs`

**До:** найденные `map`/`filter` calls и цепочки `BinOp` записывались как `Applied`, хотя IR не менялся.

**Сейчас:**

- pipeline-shaped кандидаты только обнаруживаются;
- решение записывается как `Rejected`;
- инструкции не меняются;
- функция возвращает `0` изменений;
- optimizer не входит в ложную итерацию «изменений».

Для настоящего `Applied` потребуется отдельное DMIR-представление fused iterator/stream или проверенная CFG-трансформация и backend lowering.

### 3.2 `src/optimizer/memory.rs`

#### SROA helper

Escape analysis завершается до решения. Helper больше не заявляет полное устранение allocation заранее и не scalarize-ит escaping aggregate.

Полное структурное SROA-преобразование остаётся в `src/optimizer/mod.rs`: для proven non-escaping `StructInit` действительно удаляется, а `GetField` заменяется forwarding-значением. Это можно считать применённым только если проверяется фактическое отсутствие `StructInit` в DMIR.

#### BCE

До этого `<`/`<=` сравнения принимались за bounds checks и считались удалёнными.

Сейчас:

- обычное сравнение не считается bounds check;
- если в DMIR нет явной пары array-access + bounds-check, записывается `Rejected`;
- IR не удаляется;
- функция возвращает `0`.

### 3.3 `src/optimizer/scalar.rs`

CSE ограничен одним basic block. Это консервативный, но безопасный вариант: определение выражения, использованное повторно, находится в том же блоке и локально доминирует последующее использование.

Глобальный CSE между блоками пока не готов: ему нужны dominance proof через `ControlFlowGraph::dominates`, обработка merge points и проверка invalidation/side effects.

### 3.4 `src/optimizer/cost_model.rs`

Следующие методы теперь не могут сами по себе породить ложное `Applied`:

- `evaluate_loop_unroll` — candidate only / rejected;
- `evaluate_parallelization` — analytical candidate, no parallel code emitted;
- `evaluate_vectorization` — analytical eligibility, но backend lowering отсутствует.

Threshold, latency и throughput в cost model — оценки, а не измерения и не доказательство emitted code.

### 3.5 `src/optimizer/adaptive/`

Representation, layout, pipeline и dispatch records переведены в candidate semantics, если отдельного DMIR/backend rewrite нет. Например:

- `Candidate:PromoteToScalarSSA`;
- `Candidate:StackLocalPlacement`;
- `Candidate:TransformToStructOfArrays`;
- `Candidate:TransformToAoSoA(8)`;
- `Candidate:SingleFusedLoop`;
- аналитические AVX2 и parallel plans.

`ExecutionAdapter` выбирает `SequentialScalar`, потому что это единственная стратегия, подключённая к текущему native lowering path.

### 3.6 `src/pgo.rs`

`ProfileData.source` различает:

- `static` — call-site/call-graph оценка компилятора, не число исполнений;
- `runtime` — профиль, собранный реальной instrumentation + execution цепочкой.

Только `runtime` может вызвать `apply_pgo_boost(true)` и получить `PGO / Applied`. Static profile получает `Rejected` и не меняет optimization budgets.

Наблюдение branch bias пока не меняет layout CFG; поэтому запись не должна называться применённым branch reordering.

## 4. Что считается реально применённой оптимизацией

| Pass | Текущий статус | Что является доказательством |
|---|---|---|
| Constant folding | **Applied при наличии фактической замены** | `BinOp` заменён на `Const*`, native результат сохранён |
| Local CSE | **Applied в пределах блока** | повторный `BinOp` заменён на `copy`, операнд доминирует |
| DCE/reachability | **Applied при удалении инструкции/символа** | instruction/function отсутствует после pass |
| LICM | **Applied для доказанной invariant instruction** | instruction физически вне loop body, результат сохранён |
| SROA | **Applied только для proven non-escaping path** | `StructInit` исчез из DMIR, `GetField` forwarding сохранён, native output совпал |
| Pipeline fusion | **Rejected/candidate** | fused IR/backend lowering ещё отсутствует |
| BCE | **Rejected/candidate** | explicit access/check IR ещё отсутствует |
| SIMD | **Not emitted** | vector lowering отсутствует |
| Automatic parallel loop | **Not emitted** | compiler CFG не подключён к thread pool |
| Async reactor | **Not emitted** | async runtime lowering отсутствует |
| Loop unrolling | **Disabled** | нет sound fresh-SSA/CFG unroller |
| Runtime PGO | **Ready only for runtime-proven profile path** | source=`runtime`, instrumentation evidence, changed budget/IR |

## 5. Как правильно смотреть проект

### 5.1 Сначала читать нормативные документы

Порядок:

1. `D:\DATARA\Учет\datara версии\Философия.md` — граница core/library и концептуальная модель.
2. `D:\DATARA\Учет\datara версии\Спецификация языка.md` — syntax/semantics.
3. `D:\DATARA\Учет\datara версии\Архитектура Компилятора.md` — требуемые compiler stages.
4. `D:\DATARA\Учет\datara версии\План.md` — порядок реализации.
5. `docs/AUDIT_OPTIMIZATION_FIXES.md` — текущий optimization truth gate.

Нормативный документ отвечает на вопрос «как должно быть». Source отвечает на вопрос «что сейчас есть».

### 5.2 Затем проверять реальный pipeline по слоям

| Слой | Смотреть |
|---|---|
| Lexer | `src/lexer/mod.rs` |
| Parser/AST | `src/parser/`, `src/ast/` |
| Driver | `src/driver.rs` |
| DMIR and CFG | `src/dmir/mod.rs`, `src/dmir/cfg.rs` |
| Optimizer orchestration | `src/optimizer/mod.rs` |
| Loops | `src/optimizer/loops.rs` |
| Scalar | `src/optimizer/scalar.rs` |
| Memory | `src/optimizer/memory.rs` |
| Adaptive analysis | `src/optimizer/adaptive/` |
| Native backend | `src/codegen/cranelift/backend.rs` |
| PGO | `src/pgo.rs` |
| CLI | `src/cli.rs` |
| Semantic graph | `src/semantic_graph/mod.rs` |

### 5.3 Ищущий вопрос для каждого pass

Для каждого оптимизатора нужно ответить:

1. Какие exact preconditions?
2. Где хранится исходный IR?
3. Какие инструкции/blocks реально меняются?
4. Как доказывается dominance и type correctness?
5. Сохраняются ли side effects, evaluation order, traps и ABI?
6. Что делает backend с полученным IR?
7. Где native output/exit code проверяется?
8. Что будет при zero-trip loop, aliasing, escape и error path?

Если ответ заканчивается на «записали trace», оптимизация не доказана.

## 6. Команды проверки

Запускать из `D:\DATARA\datara + forgen`.

```bash
"$HOME/.cargo/bin/cargo.exe" fmt --all -- --check
"$HOME/.cargo/bin/cargo.exe" check
"$HOME/.cargo/bin/cargo.exe" test --release -j 2
```

`-j 2` на Windows используется намеренно: он снижает вероятность гонок/lock failures при одновременной линковке.

Точечные проверки:

```bash
"$HOME/.cargo/bin/cargo.exe" test --release test_optimizer_licm_proof -- --nocapture
"$HOME/.cargo/bin/cargo.exe" test --release test_optimizer_golden -- --nocapture
"$HOME/.cargo/bin/cargo.exe" test --release test_semantic_adaptation_engine -- --nocapture
"$HOME/.cargo/bin/cargo.exe" test --release test_logical_operators -- --nocapture
"$HOME/.cargo/bin/cargo.exe" test --release test_lexer_unknown_characters -- --nocapture
"$HOME/.cargo/bin/cargo.exe" test --release test_pgo -- --nocapture
```

Тест проходит только если одновременно выполнены три условия:

- компиляция Rust-теста успешна;
- native Datara executable собран и запущен;
- output/exit code соответствуют ожидаемым значениям.

## 7. Как читать optimization trace

Допустимые решения:

- `Applied` — физическая трансформация уже произошла и проверена;
- `Rejected` — кандидат рассмотрен, но transformation не выполняется;
- `Candidate` — аналитическая возможность, не emitted code;
- `Preserved` — IR сохранён намеренно, например из-за escape/trap/effect;
- `Unknown` — доказательств недостаточно.

Нельзя использовать слова `zero cost`, `SIMD`, `parallel speedup`, `single fused loop`, `0 allocations` как факт только потому, что они есть в аналитическом расчёте.

## 8. Что ещё обязательно реализовать

### 8.1 Сначала закрыть truth/verification gate

1. Добавить единый DMIR verifier: value definitions, use-before-definition, terminator targets, block reachability, type consistency.
2. Проверять verifier до и после каждого mutating pass.
3. Формализовать `Applied/Candidate/Rejected/Preserved` для всех optimizer logs.
4. Удалить/изолировать legacy compound `Inst::WhileLoop` и `Inst::TryCatch`, чтобы они не выглядели как active backend IR.
5. Добавить native structural tests для каждого pass, а не только trace assertions.
6. Переписать stale docs, которые сейчас называют неполные подсистемы готовыми.

### 8.2 Затем завершить compiler core

1. Настоящий iterator protocol для всех iterable values.
2. Полная семантика `Option` и `Result`.
3. `try/catch` или окончательное решение о границе с `Result` и lowering.
4. Modules/imports/exports и cycle diagnostics.
5. HIR/DGraph boundary без placeholder metadata.
6. Ownership/borrow proofs с escape и mutation cases.
7. Effect proofs для IO, network, async и parallel.
8. Structured diagnostics со стабильными кодами и spans.
9. Formatter и documentation tooling.

### 8.3 Потом concurrency и advanced optimization

1. `parallel`/`parallel for` только после formal effect/error/cancellation semantics.
2. Async runtime и cancellation semantics.
3. Runtime PGO instrumentation, profile validation и versioning.
4. SIMD lowering только с реальными vector instructions в backend и target-feature checks.
5. Thread-pool lowering только с реальным join/error propagation path.
6. Pipeline fusion только через реальный iterator IR/CFG rewrite.
7. BCE только после явного representation array access/check.
8. Sound loop unrolling с fresh SSA, CFG duplication, remainder handling и verifier.

## 9. Benchmark gate

Старый `docs/FORGEN_PERFORMANCE_PARITY_FINAL.md` нельзя считать доказательством: он заявлял parity/freeze, real threads, SIMD и fusion без достаточной structural evidence.

Новый benchmark harness должен:

- использовать эквивалентные алгоритмы и входы;
- получать runtime input, чтобы не дать compiler constant-fold весь workload;
- делать observable output/checksum;
- разделять compile time, process startup и kernel runtime;
- повторять измерения и публиковать raw data;
- записывать target, mode, compiler revision, trip count, binary size, memory и correctness;
- отдельно показывать, что именно было оптимизировано в IR.

Пока эта методика не выполнена, допустимы только exploratory measurements. Нельзя писать «Datara быстрее Rust» или «performance parity reached».

## 10. Рекомендуемый первый полноценный этап

**Stage 1: Verified native core**

### Scope

- стабильный lexer/parser/DMIR путь;
- explicit CFG verifier;
- verified constant folding, local CSE, DCE, LICM и proven SROA;
- native Cranelift execution;
- diagnostics и regression tests;
- honest optimization trace;
- reproducible benchmark harness.

### Non-goals

- SIMD;
- automatic parallel lowering;
- async reactor;
- pipeline fusion;
- global CSE;
- generalized BCE;
- IDE/LSP;
- AI/ML core syntax.

### Acceptance criteria

1. `cargo fmt --check`, `cargo check` и release tests проходят.
2. Каждый mutating pass запускает verifier до/после.
3. Каждая запись `Applied` связана с observable DMIR/backend delta.
4. LICM, CSE, SROA, DCE и constant folding имеют structural tests.
5. Native output и trap/error behavior сохраняются.
6. Performance report содержит только воспроизводимые measurements.
7. Gap documents отражают реальный статус, а не желаемую архитектуру.

## 11. Важные текущие риски

- Рабочее дерево Git сейчас состоит из untracked файлов, поэтому полезного baseline diff нет. Нужен первый осмысленный commit после стабилизации audit artifacts.
- В корне и `tmp_verify/` лежат временные `.exe`, `.obj`, `.dtr` и benchmark artifacts. Их нельзя смешивать с доказательной частью проекта.
- Старые benchmark tests содержат рекламные формулировки и не должны автоматически считаться performance proof.
- Концепты ещё требуют решений по `import/use`, `model`, `with` vs `from Base + Component`, `fn/function`, `Result` vs `try/catch`, boolean coercion, integer overflow, numeric promotion, module cycles, async cancellation, parallel errors и ABI/packed layout.

## 12. Итоговый рабочий порядок

```text
1. Доказать native core и DMIR verifier
2. Довести optimization reports до honest status
3. Перезапустить structural + native tests
4. Переписать stale audit/performance docs
5. Зафиксировать решения по language contradictions
6. Написать и утвердить Stage 1 implementation plan
7. Реализовать Stage 1 acceptance criteria
8. Только затем добавлять iterator/modules/Result/ownership/effects/concurrency
9. SIMD/parallel/async/fusion — только после реального lowering
```

Это состояние проекта: фундамент уже функционирует, но заявления нужно оценивать строже, чем наличие классов, enum-ов и trace records. Следующий правильный шаг — не добавлять рекламные оптимизации, а закрыть verifier, документацию и доказательства native transformations.

## 13. Дополнение (сессия 31.08.2026): surface syntax, loop-closure fix, project sanity

Полный suite после этого дополнения: **61 test binary, 0 failures** (`cargo test --release -j 2`).

### 13.1 Исправления компилятора

1. **Сборка была сломана.** Новые файлы `src/optimizer/blocks.rs` и `src/optimizer/idioms.rs` не компилировались (несуществующее поле отчёта, borrow-ошибки в `apply_closure`, неверная проверка константы инкремента). Исправлено; в `OptimizationReport` добавлено поле `unreachable_blocks_removed`.
2. **Loop-closure писал неверный результат (главный баг сессии).** Пасс `recognize_accumulation_idioms` заменял accumulation-цикл (`sum 0..n`) на closed form, но `bound_value` был определён ВНУТРИ удаляемого loop header. Бэкенд материализовал висячий ValueId как молчаливый `0`: все бенчмарки циклов печатали `0` вместо `499999500000`. Фикс: bound заново материализуется в preheader под fresh id (`with_dest`), отказ трансформации, если bound не LoadVar/константа. Regression-тесты: `tests/test_optimizer_loop_closure.rs` (включая `<=` bound и случай, где в теле есть дополнительная работа — она обязана ОТКЛОНЯТЬ закрытие).
3. **Soundness-сужения пасса:** тело цикла теперь должно быть РОВНО каноническими 8 инструкциями (лишняя запись в переменную раньше терялась бы); заголовок цикла — только чистые производители значения; `/` и `%` в цикле запрещают закрытие (исчезновение возможного trap — изменение поведения).
4. **Небезопасный silent fallback бэкенда.** Отсутствующее значение в `val_map` материализовалось как `iconst 0` — это маскировало ошибки типа этого бага. Оставлено как есть (fallback), но закрытие источника висячих значений сделано в самом пассе.

### 13.2 Surface syntax (модель «entity + component + role + behavior + process»)

Реализовано как чистый сахар поверх существующего ядра — без новых сущностей в IR:

- `then` — separator пайплайнов, полный аналог `|>` (token `Then`); lowered DMIR байт-в-байт совпадает с `|>` (тест `test_then_pipeline_matches_pipe_pipeline`); поддерживается многострочная форма `obj then f() then g()`.
- `entity` — алиас `class` (data-bearing type declaration).
- `process` — алиас `flow` (typed pipeline function).
- `with A, B, C` — композиция. Обнаружено и исправлено: **списки через запятую раньше не парсились вообще** (работал только `with A with B`).
- Компоненты инлайнятся в layout класса на resolve-этапе (поля + методы), роли проверяются как capability-контракт (класс обязан реализовать методы роли — напрямую или через behavior, который мержится раньше проверки).
- Компоненты/роли НЕ создают runtime-объектов — это compile-time composition, что соответствует принципу «маленькое ядро — умный компилятор».

Новые диагностики (вместо молчаливого неверного поведения):
- `E-ROLE-UNSATISFIED` — уже существовал; подтверждён тестом.
- `E-COMPONENT-FIELD-CLASH` / `E-COMPONENT-METHOD-CLASH` — конфликт полей/методов компонента с классом раньше молча перезаписывал поле.
- Неподдерживаемая стадия пайплайна (не `call`) теперь ошибка typecheck, а не молчаливое отбрасывание.
- `Duplicate function` — дубликаты топ-уровневых функций (актуально при merge нескольких файлов проекта) раньше молча перезаписывали друг друга.

### 13.3 Большие проекты

- Обход файлов в `ProjectDiscovery::collect_dtr_files` теперь детерминирован (лексикографическая сортировка) — порядок компиляции больше не зависит от порядка FS.
- Тесты: `tests/test_project_sanity.rs`.

### 13.4 Что осталось (не претерпело изменений в этой сессии)

- Модульная система с import/export и cycle diagnostics — merge файлов в одну плоскую программу остаётся промежуточным решением.
- iterator protocol, полная семантика `Option`/`Result`/`try-catch`, threaded type inference через стадии пайплайна.
- Все пункты разделов 8.1–8.3 (verifier, concurrency, SIMD и т.д.) — в силе.

## 14. Сессия: аудит тестов + три критических фикса корректности

Полный прогон: **63 test binaries, 131 test, 0 failures** (`cargo test --release -j 2`), `cargo fmt --check` чистый.

### 14.1 Критические фиксы корректности компилятора (главное)

Эти баги означали, что язык **молча выдавал неверные результаты или падал** — приоритет был отдан им перед новыми фичами.

1. **Мутация полей класса не работала вообще.** Присваивание `this.field = v` молча выбрасывалось на lowering: `Inst::SetField` никогда не эмитился, бэкенд его поддерживал, но IR до него не доходило. Каждый метод, мутирующий состояние объекта, был no-op. После фикса включилась и вторая часть бага: SROA (`memory.rs`) форвардила стартовое значение поля сквозь `MethodCall`, игнорируя alias-корень escape-набора — фиксировано (SetField/MethodCall помечают объект escaping по alias-корню). Тесты: `tests/test_field_mutation.rs` (3 теста).
2. **Typecheck не проверял типы аргументов пайплайна.** Стадии типизировались в отрыве от piped-значения (оно считалось `Int`), и `value |> fn(param: UnrelatedType)` компилировалось, а сгенерированный exe падал с access violation. Теперь piped-значение участвует в проверке арности и типов каждой стадии. Тесты: `tests/test_pipeline_typecheck.rs`.
3. **Методные вызовы типизировались как `String`.** Fallback в `check_expr` для `obj.method(args)` возвращал String независимо от реального типа — теперь тип берётся из таблицы методов класса.
4. **Loop-closure pass был несостоятельным при ненулевых стартах.** Закрытая форма предполагала, что аккумулятор стартует с того же значения, что и индуктор (`sum 0..n`), и не проверяла `trip <= 0` (мусор при пустом цикле). Пасс переписан: раздельные константы старта аккумулятора и индуктора, guard `trip > 0` с уходом в exit-блок (семантически тождественно нулевой итерации), reject `s = s + s`. Отчёт пасса теперь честный: `loops_closed` вместо трёх фейковых инкрементов других счётчиков. Тесты: `tests/test_optimizer_loop_closure.rs` (8 тестов: `<=` bound, ненулевые старты, zero-trip, reject division/extra work/computed bound).

### 14.2 Оживление пассов и честность бенчмарков

- **CSE был фактически мёртв**: ключ по ValueId никогда не совпадал, т.к. каждый `LoadVar` даёт свежий ValueId. Переписан на value numbering по (имя переменной, версия). Тест: `test_advanced_cse_optimization` проверяет физическое устранение дубля (`3 BinOps -> 2`) в финальном DMIR.
- `test_optimizer_advanced` переведён с trace-ассертов на структурные IR-ассерты (LICM: инвариант физически отсутствует в теле цикла).
- `bench_multilanguage_matrix`: удалена фейковая TS-колонка (`node_time * 1.02`), `unwrap_or(0.0)` заменён на `n/a`, ворклоады приведены к эквивалентности (XOR-шаг, которого нет в Datara, удалён из бейслайнов; невыразимые категории помечены n/a), добавлена проверка корректности stdout каждой Datara-задачи (поймала бы регрессию dangling-ValueId), warm-up, cleanup.
- Удалён мёртвый планировщик `adaptive::strategy.rs` (решения никуда не поступали, PIC-ветка недостижима) — тест удалён вместе с модулем.
- `test_performance_microbenchmarks`: вакуумный ассерт `contains(...) || !is_empty()` ужесточён до точного значения; `test_graph_scale` переведён с тайминг-порогов (флаки) на проверку корректности графа; `test_incremental_multimodule` теперь проверяет настоящую транзитивную инвалидацию (`IncrementalCache::is_tree_fresh` с обходом зависимостей и обработкой циклов), а не выдуманную семантику.

### 14.3 Примеры

- `examples/07_entity_process_model.dtr` — модель «entity + component + role + process + then» (перекликается с предложением из обсуждения): компоненты инлайнятся в entity, role проверяется компилятором, `then`-цепочка со свободными и методными стадиями, нативный запуск подтверждён.
- Все примеры 01–07 прогнаны нативно после ужесточения typecheck.

### 14.4 Честные ограничения (не жертвуя корректностью)

- `SetField` поддерживает только identifier-цели (`obj.f = v`); вложенные пути (`a.b.c = v`) требуют access-path rewriting.
- Typecheck пайплайна не разрешает generics через стадии (TypeParam-параметры пропускаются без биндинга).
- Bool печатается как `1`/`0` — строковое представление `true`/`false` не реализовано.
- CSE остаётся локальным (per-block) — глобальный требует dominance proof в verifier.

## 15. Сессия: настоящая семантика `?` (Result/Option propagation) — закрытие п. 8.2.2

Полный прогон: **61 test binaries, 0 failures** (`cargo test --release -j 2`), `cargo fmt --check` чистый, примеры 01–07 прогнаны нативно.

### 15.1 Главный фикс: `?` перестал быть no-op

До этого `Expr::ErrorPropagate` на lowering был pass-through: возвращался сам объект `Outcome/Maybe`, а не payload. Провалившийся результат использовался как значение — мусор по указателю вниз по течению, никакой ранней отдачи ошибки.

Теперь lowering строит настоящий CFG для каждого `?`-сайта:

```text
val = <operand>                  (Outcome<T> / Maybe<T>)
flag = val.is_success|is_some    (GetField, Bool)
cond flag ? ok : err
err:  return val                 (zero-copy: провалившийся объект становится результатом функции)
ok:   payload = val.value        (GetField, тип с подстановкой generic'ов)
      -> merge                   (payload — значение выражения)
```

Решение, какой конкретно representation у операнда (`Outcome` против `Maybe`), принимает type checker и записывает в `propagation_sites` по span'у; отсутствие записи на lowering — внутренняя ошибка (`panic!`), а не тихий fallback.

### 15.2 Правила type checker'а (все — hard errors, без неявной магии)

1. Операнд `?` обязан быть Result-подобным (`T!E`, `Result<T,E>`, `Outcome<T>`) или Option-подобным (`T?`, `Option<T>`, `Maybe<T>`) — иначе ошибка, а не тихая передача значения насквозь.
2. `?` разрешён только в функции/методе, чья сигнатура возвращает тот же вид (Result↔Result, Option↔Option) с совместимым payload — иначе «нечего пропагировать».
3. `T!E` — error-канал обязан быть `String` (у `Outcome<T>` фиксированное поле `error_msg: String`; другой канал не имеет представления).
4. `return` в функции с Result/Option-сигнатурой обязан возвращать совместимый `Outcome/Maybe` — голый payload отвергается («молча потерять error-канал» больше нельзя). Неявной обёртки/коэрции нет.

`Result<T,E>` в generic-форме резолвится в `DataraType::Result` — абстрактно то же, что `T!E`.

### 15.3 Representation-фиксы lowering

- `repr_type_string`: сигнатуры `T!E`/`Result<T,E>` → `Outcome<T>`, `T?`/`Option<T>` → `Maybe<T>`. Раньше `full_type_name()` сводил `Int!String` к `Int`, и возвращённый Outcome-объект трактовался бэкендом как скаляр.
- `member_field_repr`: тип `obj.field` резолвится через type checker с подстановкой generic'ов (`r.value` на `Outcome<String>` — это `String`, а не шаблонный `T`). Без него поля generic-инстансов теряли тип.
- SROA: terminator теперь участвует в escape-анализе — `return s` держит структуру живой (раньше пассы по `instructions` не видели `Terminator::Return` и удаляли возвращаемые объекты).
- Backend: `copy` со скопированным Bool сохраняет флаг boolean-like — `out m.is_some` печатает `true`/`false`, а не `1`/`0` (частично закрывает п. 14.4 про Bool-печать: флаги и SROA-forwarded Bool печатаются как `true`/`false`).

### 15.4 CLI

`forgen check` теперь выполняет resolve модулей так же, как реальная компиляция — раньше `check` отвергал валидные stdlib-импорты («Unknown class»).

### 15.5 Тесты

`tests/test_result_propagation.rs` — 8 контрактов:
- позитивные: success/error пути `?`, `Maybe`/`?`, sugar `Int!String` end-to-end;
- негативные: `?` на не-Result операнде; `?` в функции без Result/Option-сигнатуры; вид сигнатуры не совпадает с видом операнда (`Outcome`→`Maybe`); error-канал `Int!Int`; голый `return payload` из Result-функции.

### 15.6 Что осталось из семантики Option/Result

- `decide`/match по `Outcome`/`Maybe` с exhaustiveness-проверкой.
- Convenience-конструкторы (`Outcome.ok(v)` / `Outcome.err(msg)`) — сейчас только явные struct literals.
