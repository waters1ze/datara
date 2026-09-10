# Подключение подсветки синтаксиса Datara в JetBrains IDE
(IntelliJ IDEA, PyCharm, CLion, RustRover, WebStorm, GoLand)

Все IDE от компании JetBrains поддерживают импорт грамматик TextMate из коробки. Чтобы включить полноценную подсветку кода `.dtr`:

### Способ 1: Подключение через TextMate Bundles (Занимает 20 секунд)

1. Откройте вашу JetBrains IDE и перейдите в настройки:
   * **Windows / Linux:** `File` → `Settings` (или `Ctrl + Alt + S`)
   * **macOS:** `Preferences` → `Settings` (или `Cmd + ,`)
2. В строке поиска настроек введите **TextMate** (или выберите `Editor` → `TextMate Bundles`).
3. Нажмите иконку **`+`** (Add) в списке бандлов.
4. Выберите папку `editors/vscode` из репозитория Datara (или укажите файл `editors/vscode/syntaxes/datara.tmLanguage.json`).
5. Нажмите **Apply** и **OK**.

Теперь все файлы с расширением `.dtr` будут отображаться с подсветкой синтаксиса!

---

### Способ 2: Ассоциация типов файлов (File Types)
Если расширение `.dtr` не подхватилось автоматически:
1. `Settings` → `Editor` → `File Types`.
2. Найдите в списке `TextMate`.
3. В нижней секции `File name patterns` нажмите `+` и введите `*.dtr`.
4. Нажмите `OK`.
