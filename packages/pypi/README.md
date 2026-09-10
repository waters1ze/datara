# Datara Python SDK (`pip install datara`)

Python SDK and CLI runner for the **Datara Programming Language** and **Forgen Native Compiler**.

## Installation

```bash
pip install datara
```

## CLI Usage

After installing via pip, `forgen`, `datara`, and `dpm` commands are directly available:

```bash
# Run Datara files instantly
forgen run script.dtr

# Compile with peak LLVM AOT optimization
forgen build main.dtr --llvm

# Launch interactive JIT REPL
datara repl

# Manage Datara packages
dpm add uuid
```

## Python API Usage

Accelerate Python workflows with native Datara modules:

```python
import datara

# Execute Datara scripts with zero setup:
datara.run("algorithm.dtr")

# Compile to standalone binary:
datara.build("algorithm.dtr", llvm=True)
```