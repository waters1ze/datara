# Datara Editor & IDE Ecosystem / Настройка подсветки в любых IDE

Язык программирования **Datara** предоставляет готовые конфигурации подсветки синтаксиса и интеграцию с Language Server Protocol (`forgen lsp`) для всех современных редакторов кода и сред разработки.

---

## Быстрый переход к вашей среде (Quick Jump)

* [Visual Studio Code / Cursor / Windsurf / VSCodium](#1-visual-studio-code--cursor--windsurf)
* [JetBrains (IntelliJ IDEA, PyCharm, CLion, RustRover)](#2-jetbrains-ides)
* [Neovim / Vim](#3-neovim--vim)
* [Sublime Text 3 / 4](#4-sublime-text)
* [Helix Editor](#5-helix)
* [Zed Editor](#6-zed)

---

## 1. Visual Studio Code / Cursor / Windsurf

Расширение расположено в каталоге [`editors/vscode/`](vscode/).

### Установка в 1 клик через CLI:
```bash
# Для VS Code:
code --install-extension editors/vscode

# Для Cursor:
cursor --install-extension editors/vscode
```

### Ручная установка:
1. Скопируйте папку `editors/vscode` в директорию расширений:
   * **Windows:** `%USERPROFILE%\.vscode\extensions\datara-language`
   * **Linux / macOS:** `~/.vscode/extensions/datara-language`
2. Перезапустите редактор. Файлы `.dtr` получат подсветку синтаксиса и фирменную иконку.

---

## 2. JetBrains IDEs
*(IntelliJ IDEA, PyCharm, CLion, RustRover, WebStorm, GoLand)*

JetBrains поддерживает формат TextMate из коробки:
1. Откройте `File` → `Settings` (`Preferences` на macOS) → `Editor` → `TextMate Bundles`.
2. Нажмите **`+`** и выберите папку `editors/vscode` из репозитория Datara.
3. Нажмите **Apply**. Подсветка `.dtr` включена мгновенно!

---

## 3. Neovim / Vim

Файлы синтаксиса расположены в [`editors/neovim/`](neovim/).

### Установка синтаксиса:
Скопируйте содержимое папки `editors/neovim/` в вашу конфигурацию:
* **Neovim:** `~/.config/nvim/` (или `%LOCALAPPDATA%\nvim\` на Windows)
* **Vim:** `~/.vim/` (или `vimfiles` на Windows)

```bash
# Linux/macOS:
cp -r editors/neovim/* ~/.config/nvim/
```

### Подключение Language Server (`forgen lsp`):
Добавьте в ваш `init.lua` (при использовании `nvim-lspconfig`):
```lua
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

if not configs.datara_lsp then
  configs.datara_lsp = {
    default_config = {
      cmd = { 'forgen', 'lsp' },
      filetypes = { 'datara' },
      root_dir = lspconfig.util.root_pattern('.git', 'forgen.toml'),
      settings = {},
    },
  }
end
lspconfig.datara_lsp.setup{}
```

---

## 4. Sublime Text 3 / 4

Файл синтаксиса: [`editors/sublime/Datara.sublime-syntax`](sublime/Datara.sublime-syntax).

1. В меню Sublime Text выберите `Preferences` → `Browse Packages...`.
2. Создайте папку `Datara` внутри открывшейся директории `Packages/`.
3. Скопируйте файл `editors/sublime/Datara.sublime-syntax` в созданную папку.
4. Готово! Файлы `.dtr` будут подсвечиваться с активной темой оформления.

---

## 5. Helix Editor

Конфигурация: [`editors/helix/languages.toml`](helix/languages.toml).

Добавьте секцию в `~/.config/helix/languages.toml`:
```toml
[[language]]
name = "datara"
scope = "source.datara"
injection-regex = "datara|dtr"
file-types = ["dtr", "datara"]
comment-token = "//"
block-comment-tokens = { start = "/*", end = "*/" }
indent = { tab-width = 4, unit = "    " }
language-servers = ["datara-lsp"]

[language-server.datara-lsp]
command = "forgen"
args = ["lsp"]
```

---

## 6. Zed Editor

1. В меню Zed: `Settings` → `Open Language Settings`.
2. Скопируйте грамматику из `editors/vscode/syntaxes/datara.tmLanguage.json`.
3. Datara LSP регистрируется командой `forgen lsp`.

---

## Фирменная Windows-иконка файлов `.dtr`

При установке через наш официальный установщик (`installer/DataraSetup.bat`), все файлы с расширением `.dtr` на вашем компьютере автоматически получают официальную иконку Datara высокого разрешения (вплоть до 256x256 пикселей с альфа-прозрачностью) и привязываются к двойному клику ("Открыть с помощью Datara" / "Редактировать в VS Code").
