# scripts/clean_artifacts.ps1
# Cleans generated build artifacts, test binaries, PDBs, object files, and temporary outputs from the repo root.
$ErrorActionPreference = "SilentlyContinue"

$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path (Join-Path $repoRoot "Cargo.toml"))) {
    $repoRoot = (Get-Location).Path
}

Write-Host "Cleaning generated artifacts from: $repoRoot" -ForegroundColor Cyan

$count = 0
Get-ChildItem -Path $repoRoot -File | Where-Object {
    $ext = $_.Extension.ToLower()
    $name = $_.Name.ToLower()
    $ext -in @(".exe", ".pdb", ".obj", ".lib", ".ilk") -or $name -like "tmp_*"
} | ForEach-Object {
    Remove-Item -LiteralPath $_.FullName -Force -ErrorAction SilentlyContinue
    $count++
}

Write-Host "Cleaned $count artifact files from root." -ForegroundColor Green
