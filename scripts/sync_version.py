#!/usr/bin/env python3
"""
Datara & Forgen Version Synchronization Tool
=============================================
Propagates the canonical project version from Cargo.toml across all
ecosystem manifests, packaging configs, installer scripts, and CI workflows.

Usage:
    python scripts/sync_version.py [VERSION]
    python scripts/sync_version.py            # Uses version from Cargo.toml
"""

import os
import re
import sys
import json
from pathlib import Path

def get_repo_root() -> Path:
    return Path(__file__).resolve().parent.parent

def read_canonical_version(repo_root: Path) -> str:
    cargo_toml = repo_root / "Cargo.toml"
    content = cargo_toml.read_text(encoding="utf-8")
    m = re.search(r'(?m)^version\s*=\s*"([^"]+)"', content)
    if not m:
        raise ValueError("Could not find version in Cargo.toml")
    return m.group(1).strip()

def update_file(path: Path, pattern: str, replacement: str, description: str):
    if not path.exists():
        print(f"  [SKIP] {path.name} not found ({path})")
        return False
    content = path.read_text(encoding="utf-8")
    new_content, count = re.subn(pattern, replacement, content)
    if count > 0 and new_content != content:
        path.write_text(new_content, encoding="utf-8")
        print(f"  [UPDATED] {description} ({path.relative_to(get_repo_root())}) [matched {count}]")
        return True
    else:
        print(f"  [OK] {description} is already up to date")
        return False

def sync_version(version: str):
    repo_root = get_repo_root()
    clean_ver = version.lstrip("v")
    tag_ver = f"v{clean_ver}"
    
    print(f"=======================================================================")
    print(f" Synchronizing Datara & Forgen version to: {clean_ver} ({tag_ver})")
    print(f" Repo root: {repo_root}")
    print(f"=======================================================================")
    
    # 1. Cargo.toml
    cargo_toml = repo_root / "Cargo.toml"
    update_file(
        cargo_toml,
        r'(?m)^version\s*=\s*"[^"]+"',
        f'version = "{clean_ver}"',
        "Cargo.toml [package.version]"
    )
    
    # 2. packages/npm/package.json
    npm_json = repo_root / "packages" / "npm" / "package.json"
    update_file(
        npm_json,
        r'"version":\s*"[^"]+"',
        f'"version": "{clean_ver}"',
        "packages/npm/package.json"
    )
    
    # 3. packages/pypi/pyproject.toml
    pypi_toml = repo_root / "packages" / "pypi" / "pyproject.toml"
    update_file(
        pypi_toml,
        r'(?m)^version\s*=\s*"[^"]+"',
        f'version = "{clean_ver}"',
        "packages/pypi/pyproject.toml"
    )
    
    # 4. editors/vscode/package.json
    vscode_json = repo_root / "editors" / "vscode" / "package.json"
    update_file(
        vscode_json,
        r'"version":\s*"[^"]+"',
        f'"version": "{clean_ver}"',
        "editors/vscode/package.json"
    )
    
    # 5. installer/SetupWizard.cs
    setup_cs = repo_root / "installer" / "SetupWizard.cs"
    update_file(
        setup_cs,
        r'public const string AppVersion = "[^"]*";',
        f'public const string AppVersion = "{clean_ver}";',
        "installer/SetupWizard.cs AppVersion"
    )
    
    # 6. installer/datara_setup.iss
    setup_iss = repo_root / "installer" / "datara_setup.iss"
    update_file(
        setup_iss,
        r'#define MyAppVersion "[^"]*"',
        f'#define MyAppVersion "{clean_ver}"',
        "installer/datara_setup.iss MyAppVersion"
    )
    
    # 7. installer/DataraSetup.ps1
    datara_setup_ps1 = repo_root / "installer" / "DataraSetup.ps1"
    update_file(
        datara_setup_ps1,
        r'(?m)^\$Version\s*=\s*"[^"]*"',
        f'$Version = "{clean_ver}"',
        "installer/DataraSetup.ps1 $Version"
    )
    
    # 8. packages/linux/deb/DEBIAN/control
    deb_control = repo_root / "packages" / "linux" / "deb" / "DEBIAN" / "control"
    update_file(
        deb_control,
        r'(?m)^Version:\s*.+$',
        f'Version: {clean_ver}',
        "packages/linux/deb/DEBIAN/control"
    )
    
    # 9. packages/linux/rpm/datara.spec (Release changelog)
    rpm_spec = repo_root / "packages" / "linux" / "rpm" / "datara.spec"
    if rpm_spec.exists():
        content = rpm_spec.read_text(encoding="utf-8")
        content = re.sub(r'-\s*[0-9]+\.[0-9]+\.[0-9]+-1', f'- {clean_ver}-1', content)
        rpm_spec.write_text(content, encoding="utf-8")
        print(f"  [UPDATED] packages/linux/rpm/datara.spec changelog")
        
    # 10. packaging/scoop/datara.json
    scoop_json = repo_root / "packaging" / "scoop" / "datara.json"
    if scoop_json.exists():
        content = scoop_json.read_text(encoding="utf-8")
        content = re.sub(r'"version":\s*"[^"]+"', f'"version": "{clean_ver}"', content)
        content = re.sub(r'/releases/download/v[^/]+/', f'/releases/download/{tag_ver}/', content)
        scoop_json.write_text(content, encoding="utf-8")
        print(f"  [UPDATED] packaging/scoop/datara.json")

    # 11. packaging/winget/waters1ze.Datara.yaml
    winget_yaml = repo_root / "packaging" / "winget" / "waters1ze.Datara.yaml"
    if winget_yaml.exists():
        content = winget_yaml.read_text(encoding="utf-8")
        content = re.sub(r'PackageVersion:\s*[0-9]+\.[0-9]+\.[0-9]+', f'PackageVersion: {clean_ver}', content)
        content = re.sub(r'/releases/download/v[^/]+/', f'/releases/download/{tag_ver}/', content)
        content = re.sub(r'Datara-v[0-9]+\.[0-9]+\.[0-9]+-Setup\.exe', f'Datara-v{clean_ver}-Setup.exe', content)
        winget_yaml.write_text(content, encoding="utf-8")
        print(f"  [UPDATED] packaging/winget/waters1ze.Datara.yaml")

    # 12. install.ps1 & install.sh fallbacks
    install_ps1 = repo_root / "install.ps1"
    update_file(
        install_ps1,
        r'(?m)^\$LatestTag\s*=\s*"[^"]+"',
        f'$LatestTag = "{tag_ver}"',
        "install.ps1 fallback tag"
    )
    
    install_sh = repo_root / "install.sh"
    if install_sh.exists():
        update_file(
            install_sh,
            r'(?m)^LATEST_TAG="[^"]+"',
            f'LATEST_TAG="{tag_ver}"',
            "install.sh fallback tag"
        )
        update_file(
            install_sh,
            r'<string>[0-9]+\.[0-9]+\.[0-9]+</string>',
            f'<string>{clean_ver}</string>',
            "install.sh plist version"
        )

    print(f"\n[DONE] All manifests, installers, and packaging configs synchronized with {clean_ver}!")

def main():
    repo_root = get_repo_root()
    if len(sys.argv) > 1:
        target_version = sys.argv[1].strip()
    else:
        target_version = read_canonical_version(repo_root)
    sync_version(target_version)

if __name__ == "__main__":
    main()
