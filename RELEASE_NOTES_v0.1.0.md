# 🚀 Datara v0.1.0 Release Notes: The Genesis Release

We are thrilled to announce the official public release of **Datara v0.1.0** and the **`forgen`** AOT native compiler toolchain!

Datara is a high-performance, compiled Post-OOP systems and application programming language designed for high-frequency trading, cloud microservices, game development, and scientific computing. It combines the ergonomic speed and elegance of modern languages with bare-metal machine execution and zero garbage collection pauses.

---

## 🌟 Highlights of v0.1.0

### 1. Dual-Engine Compiler Architecture (`forgen`)
- **Fast-Dev Cranelift JIT:** Instant 30–50ms cold compilation from source to native CPU instructions.
- **Production LLVM AOT (`--llvm`):** Whole-program optimization with Clang `-O3`, Link-Time Optimization (LTO), and adaptive SIMD loop auto-vectorization.
- **Universal CPU Portability:** Baseline target `generic_x86_64` (SSE2) guarantees binaries execute cleanly on 100% of x86_64 CPUs without illegal instruction crashes, with dynamic feature detection for AVX2/AVX-512 and ARM64 NEON.

### 2. The Evidence Gate Formal Optimizer
- **Closed-Form Loop Folding ($O(1)$):** Automatically converts linear induction loops into instant mathematical closed-form arithmetic ($\sum_{i=1}^N i = \frac{N(N+1)}{2}$).
- **Mutable Aggregate Scalarization (SROA):** Explodes mutable structs and classes into CPU scalar registers with zero heap allocations.
- **Wire-Blit Polyhedral String Fusion:** Eliminates intermediate string buffers and reallocation overhead in interpolation and formatting chains.
- **Branchless Select Transformation:** Replaces unpredictable branches with hardware conditional moves (`cmov`/`csel`).

### 3. Post-OOP & Data-Oriented Programming (DOP)
- **Decoupled Data & Logic:** `class`, `behavior`, `entity`, `role`, `component`, and `packet` separate memory layouts from behavior dispatch.
- **Monomorphic Direct Dispatch:** Class and behavior method calls compile to direct CALL instructions without vtable pointer indirection.
- **Algebraic Data Types & Pattern Matching:** First-class tagged `enum` with payload variants (`enum Shape { Circle(Float), Rect(Float, Float) }`) and exhaustiveness-checked `match` expressions.

### 4. Interactive Zero-Latency REPL & Windows Start Menu Integration
- **Auto-Launch REPL:** Running `datara` or `forgen` without arguments launches the interactive JIT console in less than 5 milliseconds.
- **Windows Start Menu Integration:** Fully registered as `Datara 0.1.0 (64-bit)` in the Windows Start Menu and Windows Search, matching the native experience of Python.
- **File Associations:** Files ending in `.dtr` display the official 6-layer high-resolution Datara icon across Windows Explorer, macOS Finder, and Linux desktop environments (GNOME/KDE).

### 5. Universal Editor & IDE Ecosystem
- Ready-to-use syntax highlighting and Language Server Protocol (`forgen lsp`) packages for:
  - **VS Code / Cursor / Windsurf** (`editors/vscode/`)
  - **JetBrains IDEs (IntelliJ, PyCharm, CLion, RustRover)** (`editors/jetbrains/`)
  - **Sublime Text 3 / 4** (`editors/sublime/`)
  - **Neovim / Vim** (`editors/neovim/`)
  - **Helix Editor** (`editors/helix/`)
  - **Zed Editor**

---

## 📦 Downloads & Installation

### Windows (64-bit)
- **GUI Installer (Recommended):** [Datara-v0.1.0-Setup.exe](https://github.com/waters1ze/datara/releases/download/v0.1.0/Datara-v0.1.0-Setup.exe) (5.4 MB)
- **Portable Zip:** [forgen-windows-x64.zip](https://github.com/waters1ze/datara/releases/download/v0.1.0/forgen-windows-x64.zip)
- **PowerShell One-Liner:**
  ```powershell
  irm https://raw.githubusercontent.com/waters1ze/datara/main/install.ps1 | iex
  ```
- **Scoop:** `scoop install https://raw.githubusercontent.com/waters1ze/datara/main/packaging/scoop/datara.json`
- **Winget:** `winget install waters1ze.Datara`

### Linux & macOS
- **Unix Shell One-Liner:**
  ```bash
  curl -fsSL https://raw.githubusercontent.com/waters1ze/datara/main/install.sh | bash
  ```
- **Linux x86_64:** [forgen-linux-x64.tar.gz](https://github.com/waters1ze/datara/releases/download/v0.1.0/forgen-linux-x64.tar.gz)
- **macOS Apple Silicon:** [forgen-darwin-arm64.tar.gz](https://github.com/waters1ze/datara/releases/download/v0.1.0/forgen-darwin-arm64.tar.gz)
- **macOS Intel:** [forgen-darwin-x64.tar.gz](https://github.com/waters1ze/datara/releases/download/v0.1.0/forgen-darwin-x64.tar.gz)
- **Homebrew:** `brew install waters1ze/tap/datara`
- **Arch Linux AUR:** `yay -S datara-bin`

---

## 🛡️ Quality & Verification
- **Test Suite:** 88 automated integration and compiler test suites passing (100% green).
- **Formal Evidence Gate:** All SSA optimizations mathematically validated.
- **License:** Dual-licensed under Apache-2.0 / MIT.
