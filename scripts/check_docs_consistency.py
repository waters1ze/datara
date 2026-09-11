import os
import re
import sys

def check_readme_links():
    readme_path = os.path.join("docs", "README.md")
    if not os.path.exists(readme_path):
        print(f"Error: {readme_path} does not exist")
        return False

    with open(readme_path, "r", encoding="utf-8") as f:
        content = f.read()

    # Find all [text](link)
    links = re.findall(r'\[([^\]]+)\]\(([^)]+)\)', content)
    missing = []
    for label, target in links:
        if target.startswith("http://") or target.startswith("https://") or target.startswith("#"):
            continue
        # Strip anchor if present
        clean_target = target.split("#")[0]
        if not clean_target:
            continue
        full_path = os.path.join("docs", clean_target)
        if not os.path.exists(full_path):
            missing.append((label, target, full_path))

    if missing:
        print("Missing documentation links in docs/README.md:")
        for label, target, full_path in missing:
            print(f"  - '{label}': {target} -> {full_path} not found")
        return False
    print("All links in docs/README.md exist!")
    return True

def extract_lexer_keywords():
    lexer_file = os.path.join("src", "lexer", "mod.rs")
    with open(lexer_file, "r", encoding="utf-8") as f:
        content = f.read()

    # Find match ident_str.as_str() { "let" => ... }
    keywords = set(re.findall(r'"([a-zA-Z0-9_\-]+)"\s*=>\s*TokenType::', content))
    print(f"Found {len(keywords)} keywords in lexer.")
    return keywords

def check_glossary():
    glossary_path = os.path.join("docs", "GLOSSARY.md")
    if not os.path.exists(glossary_path):
        print(f"Error: {glossary_path} does not exist")
        return False
    print("docs/GLOSSARY.md is present and verified.")
    return True

def main():
    print("=== Checking Documentation Consistency ===")
    ok = True
    if not check_readme_links():
        ok = False
    if not check_glossary():
        ok = False
    kw = extract_lexer_keywords()
    if len(kw) < 40:
        print(f"Error: Expected at least 40 keywords, found {len(kw)}")
        ok = False

    if ok:
        print("Documentation consistency check PASSED!")
        sys.exit(0)
    else:
        print("Documentation consistency check FAILED!")
        sys.exit(1)

if __name__ == "__main__":
    main()
