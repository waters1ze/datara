# Official Application Shell Architecture (Datara Shell / Tauri-Analogue)

**Date**: August 30, 2026  
**Status**: **APPROVED OFFICIAL ARCHITECTURE**

---

## 1. Architectural Overview & Design Philosophy

To deliver world-class developer experience without burdening the core Datara language with premature UI syntax or heavyweight widget ecosystems, the official **Datara App Shell** is architected on a clean separation of concerns:

\begin{matrix}
\boxed{\text{\textbf{Frontend UI Layer}}} & \longleftrightarrow & \boxed{\text{\textbf{Zero-Copy IPC Bridge}}} & \longleftrightarrow & \boxed{\text{\textbf{Native Shell Engine}}} & \longleftrightarrow & \boxed{\text{\textbf{Datara / Forgen Core}}} \\
\text{React + TypeScript} & & \text{Binary / JSON-RPC} & & \text{Rust Native Shell} & & \text{Compiled Datara Binaries}
\end{matrix}

### Fundamental Principles:
1. **Library & Framework, NOT Language Syntax**:
   - App Shell capabilities are provided as a system library / runtime framework (@datara/shell and datara_shell::*), **not** as bespoke keywords or language modifications inside Datara.
2. **Modern Web UI Frontend**:
   - The user interface is built using React 19 + TypeScript + Tailwind CSS / Modern CSS, leveraging rich UI ecosystems, component libraries, Monaco Editor, and standard web developer tooling.
3. **Rust Native Shell Backend**:
   - The native windowing, webview hosting (Edge WebView2 on Windows, WebKitGTK on Linux, WKWebView on macOS), file system access, and process supervision are implemented in Rust.
4. **Forgen Native Integration**:
   - Forgen services (compiler daemon, incremental analyzer, language server / LSP, native build worker) communicate directly with the native shell via high-throughput local IPC.

---

## 2. Component Layer Breakdown

### 2.1. Frontend Layer (React + TypeScript)
- **UI Framework**: React 19, TypeScript 5.5, Vite build system.
- **State Management**: Reactive Zustand stores for project tree, active editor tabs, compiler diagnostics, and terminal streams.
- **Editor Core**: Monaco Editor / CodeMirror with Datara language mode (syntax highlighting, semantic token provider, autocomplete, hover tooltips).
- **Client SDK**: @datara/shell-api exposing typed async invoke calls:
  `	ypescript
  import { invoke, listen } from '@datara/shell-api';

  // Invoke native compiler action
  const result = await invoke<CompileResult>('forgen:compile', {
    entryPath: './src/main.dtr',
    profile: 'release'
  });

  // Listen to build events
  const unlisten = listen<BuildProgressEvent>('forgen:progress', (event) => {
    console.log([Build] : %);
  });
  `

### 2.2. Zero-Copy IPC & Capability Security Model
- **Protocol**: High-speed message passing over platform webview message ports (postMessage -> windows.chrome.webview.postMessage / webkit.messageHandlers).
- **Data Serialization**: Zero-copy binary buffer transfers for large assets and AST graphs; structured JSON-RPC for command invocations.
- **Security Capabilities Matrix**:
  - Permissions are declared in datara.app.json.
  - Frontend components cannot perform unrestricted OS operations; all file and process operations pass through explicit, audited capability gates in the Rust native shell.

### 2.3. Native Shell Backend (Rust)
- **Window Management & Webview**: Cross-platform webview binding with hardware-accelerated rendering.
- **Forgen Bridge**: Direct embedded link or child-process supervisor for the Forgen compiler CLI, LSP daemon, and debugger engine.
- **Native OS Services**: Native file pickers, notifications, menu bars, hot reload file watcher (via 
otify).

---

## 3. Directory Layout of Official Shell & App Scaffold

`
datara-shell/
├── crates/
│   ├── datara-shell-core/       # [Rust] Windowing, webview host, event loop
│   ├── datara-shell-ipc/        # [Rust] Zero-copy serialization & command router
│   ├── datara-shell-security/   # [Rust] Capability permissions verifier
│   └── datara-shell-cli/        # [Rust] datara-shell init / dev / build
├── packages/
│   └── datara-shell-api/        # [TypeScript] NPM package for web frontend
└── examples/
    └── datara-studio-ide/       # Official Datara IDE application
        ├── src/                 # React + TypeScript UI
        ├── src-shell/           # Rust shell host & Forgen bridge
        └── package.json
`
