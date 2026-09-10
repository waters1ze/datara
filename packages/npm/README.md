# @waters1ze/datara

> The Datara Programming Language and Forgen Native Compiler toolchain.

Run the Datara compiler and package manager via Node.js / NPX:

```bash
# Instant execution with npx (zero installation required)
npx @waters1ze/datara run main.dtr

# Run with LLVM peak AOT optimization
npx @waters1ze/datara build --llvm

# Launch the interactive REPL
npx @waters1ze/datara repl

# Package management with dpm
npx -p @waters1ze/datara dpm add uuid
```

## Global Installation

```bash
npm install -g @waters1ze/datara
```

After global installation, `datara`, `forgen`, and `dpm` commands are globally available in your terminal.
