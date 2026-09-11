#!/usr/bin/env python3
"""
gen_editor_syntax.py

Generates and validates syntax definitions for VSCode, Neovim, and Sublime
directly from Datara's lexer token definitions to prevent keyword drift.

Usage:
    python scripts/gen_editor_syntax.py           # Updates editor syntax files
    python scripts/gen_editor_syntax.py --check   # Fails if files are out of date
"""

import sys
import os
import re
import json

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

def extract_keywords():
    lexer_mod = os.path.join(ROOT, "src", "lexer", "mod.rs")
    with open(lexer_mod, "r", encoding="utf-8") as f:
        content = f.read()

    # Find the keyword match block
    match = re.search(r'match ident_str\.as_str\(\)\s*\{([^}]+)\}', content)
    if not match:
        raise ValueError("Could not find keyword match in src/lexer/mod.rs")

    block = match.group(1)
    keywords = set()
    for line in block.splitlines():
        kw_match = re.search(r'"([a-zA-Z0-9_\-]+)"\s*=>\s*TokenType::', line)
        if kw_match:
            keywords.add(kw_match.group(1))

    return sorted(keywords)

def update_vscode(keywords, check=False):
    path = os.path.join(ROOT, "editors", "vscode", "syntaxes", "datara.tmLanguage.json")
    with open(path, "r", encoding="utf-8") as f:
        data = json.load(f)

    # Specific groups
    triad = ["let", "mut", "val"]
    imports = ["use", "import", "export", "as", "from", "replaces", "extends"]
    declarations = [
        "fn", "function", "class", "struct", "record", "enum", "component",
        "role", "behavior", "trait", "impl", "packet", "type", "entity"
    ]
    control = [
        "if", "else", "while", "for", "in", "loop", "return", "match",
        "break", "continue", "decide", "select", "when", "or", "with",
        "try", "catch", "where", "require", "ensure", "assert", "panic", "then"
    ]
    concurrency = ["async", "await", "task", "flow", "process", "parallel"]
    io = ["out", "err", "print", "println", "eprintln", "input"]
    modifiers = [
        "pub", "extern", "const", "view", "mutView", "mut_view", "mut-view",
        "own", "shared", "unsafe", "comptime", "wrapping", "saturating",
        "register", "asm", "bit", "bits"
    ]

    patterns = data["repository"]
    patterns["triad-declarations"]["patterns"][0]["match"] = r"\b(" + "|".join(triad) + r")\b"
    patterns["interop-keywords"]["patterns"][0]["match"] = r"\b(" + "|".join(imports) + r")\b"
    
    kw_patterns = patterns["keywords"]["patterns"]
    kw_patterns[0]["match"] = r"\b(" + "|".join(sorted(set(declarations + concurrency))) + r")\b"
    kw_patterns[1]["match"] = r"\b(" + "|".join(sorted(set(control))) + r")\b"
    kw_patterns[2]["match"] = r"\b(" + "|".join(sorted(set(io))) + r")\b"
    kw_patterns[3]["match"] = r"\b(" + "|".join(sorted(set(modifiers))) + r")\b"

    new_content = json.dumps(data, indent=2, ensure_ascii=False) + "\n"
    with open(path, "r", encoding="utf-8") as f:
        old_content = f.read()

    if check:
        if new_content != old_content:
            return False
    else:
        with open(path, "w", encoding="utf-8") as f:
            f.write(new_content)
    return True

def update_neovim(keywords, check=False):
    path = os.path.join(ROOT, "editors", "neovim", "syntax", "datara.vim")
    with open(path, "r", encoding="utf-8") as f:
        content = f.read()

    decl_kws = "fn function class struct record enum component role behavior trait impl packet type entity task flow process"
    control_kws = "if else while for in loop return match break continue decide select when or with try catch where require ensure assert panic then"
    mod_kws = "pub extern async await const view mutView mut_view own shared unsafe comptime wrapping saturating"
    
    content = re.sub(r'syn keyword dataraDecl [^\n]+', f'syn keyword dataraDecl {decl_kws}', content)
    content = re.sub(r'syn keyword dataraControl [^\n]+', f'syn keyword dataraControl {control_kws}', content)
    content = re.sub(r'syn keyword dataraModifier [^\n]+', f'syn keyword dataraModifier {mod_kws}', content)

    with open(path, "r", encoding="utf-8") as f:
        old_content = f.read()

    if check:
        if content != old_content:
            return False
    else:
        with open(path, "w", encoding="utf-8") as f:
            f.write(content)
    return True

def update_sublime(keywords, check=False):
    path = os.path.join(ROOT, "editors", "sublime", "Datara.sublime-syntax")
    with open(path, "r", encoding="utf-8") as f:
        content = f.read()

    decl_kws = "fn|function|class|struct|record|enum|component|role|behavior|trait|impl|packet|type|entity|task|flow|process"
    control_kws = "if|else|while|for|in|loop|return|match|break|continue|decide|select|when|or|with|try|catch|where|require|ensure|assert|panic|then"
    mod_kws = "pub|extern|async|await|const|view|mutView|mut_view|own|shared|unsafe|comptime|wrapping|saturating"

    content = re.sub(r"- match: '\\b\(fn\|[^\)]+\)\\b'\s+scope: keyword\.declaration\.datara",
                     f"- match: '\\b({decl_kws})\\b'\n      scope: keyword.declaration.datara", content)
    content = re.sub(r"- match: '\\b\(if\|[^\)]+\)\\b'\s+scope: keyword\.control\.datara",
                     f"- match: '\\b({control_kws})\\b'\n      scope: keyword.control.datara", content)
    content = re.sub(r"- match: '\\b\(pub\|[^\)]+\)\\b'\s+scope: storage\.modifier\.datara",
                     f"- match: '\\b({mod_kws})\\b'\n      scope: storage.modifier.datara", content)

    with open(path, "r", encoding="utf-8") as f:
        old_content = f.read()

    if check:
        if content != old_content:
            return False
    else:
        with open(path, "w", encoding="utf-8") as f:
            f.write(content)
    return True

def main():
    check_mode = "--check" in sys.argv
    keywords = extract_keywords()

    v_ok = update_vscode(keywords, check=check_mode)
    n_ok = update_neovim(keywords, check=check_mode)
    s_ok = update_sublime(keywords, check=check_mode)

    if check_mode:
        if not (v_ok and n_ok and s_ok):
            print("ERROR: Editor syntax definitions out of date! Run `python scripts/gen_editor_syntax.py`")
            sys.exit(1)
        print("OK: Editor syntax definitions are synchronized with lexer.")
    else:
        print(f"Successfully synchronized editor syntax definitions ({len(keywords)} lexer keywords).")

if __name__ == "__main__":
    main()
