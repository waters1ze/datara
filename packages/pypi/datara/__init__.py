"""
Datara Programming Language - Python SDK & Native Compiler Interface
"""

import subprocess
import os
import sys
import shutil

__version__ = "0.1.0"

def _find_binary(name: str) -> str:
    exe_name = f"{name}.exe" if sys.platform == "win32" else name
    
    # 1. Check inside package directory (if bundled)
    pkg_bin = os.path.join(os.path.dirname(__file__), "bin", exe_name)
    if os.path.isfile(pkg_bin):
        return pkg_bin
        
    # 2. Check ~/.datara/bin
    home_bin = os.path.expanduser(f"~/.datara/bin/{exe_name}")
    if os.path.isfile(home_bin):
        return home_bin
        
    # 3. Check system PATH
    found = shutil.which(name)
    if found:
        return found
        
    return exe_name

def run(target: str, *args) -> subprocess.CompletedProcess:
    """Run a Datara file or project with native JIT (< 50ms)"""
    bin_path = _find_binary("forgen")
    cmd = [bin_path, "run", target] + list(args)
    return subprocess.run(cmd, check=True)

def build(target: str, llvm: bool = False, *args) -> subprocess.CompletedProcess:
    """Compile a Datara file or project to a native standalone executable"""
    bin_path = _find_binary("forgen")
    cmd = [bin_path, "build", target]
    if llvm:
        cmd.append("--llvm")
    cmd.extend(args)
    return subprocess.run(cmd, check=True)

def repl():
    """Launch the Datara zero-latency interactive JIT REPL"""
    bin_path = _find_binary("datara")
    subprocess.run([bin_path, "repl"])