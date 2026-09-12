# @datara-lang/datara

> The Datara Programming Language and Forgen Native Compiler toolchain.

Run the Datara compiler and package manager via Node.js / NPX:

```bash
# Instant execution with npx (zero installation required)
npx @datara-lang/datara run main.dtr

# Run with LLVM peak AOT optimization
npx @datara-lang/datara build --llvm

# Launch the interactive REPL
npx @datara-lang/datara repl

# Package management with dpm
npx -p @datara-lang/datara dpm add uuid
```

## Global Installation

```bash
npm install -g @datara-lang/datara
```

After global installation, `datara`, `forgen`, and `dpm` commands are globally available in your terminal.
