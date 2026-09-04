#!/usr/bin/env python3
"""
Generates complete and partitioned code dumps of the Datara/Forgen repository
for LLM code review and architectural analysis.
"""

import os
import subprocess
import sys

def build_tree(paths):
    tree = {}
    for path in paths:
        parts = path.replace("\\", "/").split("/")
        current = tree
        for part in parts:
            current = current.setdefault(part, {})

    lines = []
    def recurse(node, prefix=""):
        items = sorted(node.keys())
        for i, item in enumerate(items):
            is_last = (i == len(items) - 1)
            connector = "\\-- " if is_last else "|-- "
            lines.append(f"{prefix}{connector}{item}")
            child_prefix = prefix + ("    " if is_last else "|   ")
            recurse(node[item], child_prefix)

    recurse(tree)
    return "\n".join(lines)

def main():
    root = os.path.abspath(os.path.dirname(os.path.dirname(__file__)))
    os.chdir(root)

    print(f"Working in: {root}")

    # Fetch tracked files with UTF-8 and raw unquoted paths
    output = subprocess.check_output(
        ["git", "-c", "core.quotepath=false", "ls-files"],
        text=True,
        encoding="utf-8"
    )
    all_tracked = [f.strip() for f in output.splitlines() if f.strip()]

    ignore_exts = {
        ".png", ".jpg", ".jpeg", ".gif", ".ico", ".icns", ".lock",
        ".bin", ".wasm", ".exe", ".dll", ".zip", ".tar.gz", ".deb",
        ".rpm", ".vsix", ".whl", ".tgz", ".lib", ".pdb", ".so", ".dylib"
    }
    ignore_files = {"cargo.lock", "package-lock.json", "pnpm-lock.yaml", "yarn.lock"}

    valid_files = []
    for f in all_tracked:
        ext = os.path.splitext(f)[1].lower()
        base = os.path.basename(f).lower()
        if ext in ignore_exts or base in ignore_files:
            continue
        if os.path.isfile(f):
            valid_files.append(f)

    # Sort files naturally
    valid_files.sort()

    part1, part2, part3, part4 = [], [], [], []

    def is_part2(fl):
        return any(fl.startswith(p) for p in ["src/lexer/", "src/parser/", "src/ast/", "src/diagnostics/"])

    def is_part3(fl):
        return any(fl.startswith(p) for p in [
            "src/resolver/", "src/types/", "src/dmir/", "src/optimizer/",
            "src/ownership/", "src/effects/", "src/semantic_graph/",
            "src/codegen/", "src/runtime/", "runtime/"
        ]) or fl in ["src/driver.rs", "src/lib.rs"]

    def is_part4(fl):
        return any(fl.startswith(p) for p in [
            "stdlib/", "src/cli.rs", "src/repl/", "src/lsp/", "src/fmt/",
            "src/doc/", "src/lint/", "src/project/", "src/export/",
            "src/bin/", "tests/", "examples/", "benchmarks/", "scripts/", "editors/"
        ])

    for f in valid_files:
        fl = f.replace("\\", "/").lower()
        if is_part2(fl):
            part2.append(f)
        elif is_part3(fl):
            part3.append(f)
        elif is_part4(fl):
            part4.append(f)
        else:
            part1.append(f)

    project_tree = build_tree(valid_files)

    def write_dump(output_filename, title, file_list, include_tree=False):
        print(f"Writing {output_filename} ({len(file_list)} files)...")
        with open(output_filename, "w", encoding="utf-8") as out:
            out.write("=" * 80 + "\n")
            out.write(f" DATARA & FORGEN CODEBASE DUMP: {title}\n")
            out.write(f" Files included: {len(file_list)}\n")
            out.write("=" * 80 + "\n\n")

            if include_tree:
                out.write("PROJECT FILE TREE:\n")
                out.write("=" * 80 + "\n")
                out.write(project_tree + "\n\n")
                out.write("=" * 80 + "\n\n")

            out.write("TABLE OF CONTENTS:\n")
            for idx, f in enumerate(file_list, 1):
                sz = os.path.getsize(f)
                out.write(f"  {idx:3d}. {f:<60} ({sz/1024:6.1f} KB)\n")
            out.write("\n" + "=" * 80 + "\n\n")

            for f in file_list:
                rel = f.replace("\\", "/")
                out.write("=" * 80 + "\n")
                out.write(f"===== {rel} =====\n")
                out.write("=" * 80 + "\n")
                try:
                    with open(f, "r", encoding="utf-8", errors="replace") as fin:
                        content = fin.read()
                    out.write(content)
                except Exception as err:
                    out.write(f"[ERROR READING FILE: {err}]\n")
                out.write("\n\n")

        size_kb = os.path.getsize(output_filename) / 1024
        print(f"Created {output_filename}: {size_kb:.1f} KB ({size_kb/1024:.2f} MB)")

    # 1. Full dump
    write_dump(
        "dump.txt",
        "COMPLETE REPOSITORY SOURCE CODE",
        valid_files,
        include_tree=True
    )

    # 2. Part 1: Overview, Architecture, Build, Configs
    write_dump(
        "dump_part1_overview_and_build.txt",
        "PART 1 - Project Tree, README, Build Configuration & Architecture Docs",
        part1,
        include_tree=True
    )

    # 3. Part 2: Frontend (Lexer, Parser, AST, Diagnostics)
    write_dump(
        "dump_part2_frontend_lexer_parser_ast.txt",
        "PART 2 - Compiler Frontend (Lexer, Parser, AST, Diagnostics)",
        part2,
        include_tree=False
    )

    # 4. Part 3: Backend (Semantics, Types, DMIR, Optimizations, Codegen, Runtime)
    write_dump(
        "dump_part3_backend_semantics_ir_codegen_runtime.txt",
        "PART 3 - Semantics, Type Checking, DMIR, Codegen (Cranelift/LLVM/JIT) & Runtime",
        part3,
        include_tree=False
    )

    # 5. Part 4: Stdlib, REPL, LSP, CLI, Tests & Examples
    write_dump(
        "dump_part4_stdlib_lsp_repl_cli_tests.txt",
        "PART 4 - Standard Library (.dtr), REPL, LSP, CLI, Tests & Examples",
        part4,
        include_tree=False
    )

    print("\nAll dump files generated successfully!")

if __name__ == "__main__":
    main()
