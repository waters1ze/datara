# PowerShell wrapper for Datara Provenance Benchmark Suite
$ErrorActionPreference = "Stop"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "  Datara (Forgen) Reproducible Provenance Benchmark Suite   " -ForegroundColor Cyan
Write-Host "============================================================" -ForegroundColor Cyan

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir

# Ensure target directories exist
if (-not (Test-Path "$RepoRoot\docs\data")) {
    New-Item -ItemType Directory -Path "$RepoRoot\docs\data" -Force | Out-Null
}
if (-not (Test-Path "$RepoRoot\docs\img")) {
    New-Item -ItemType Directory -Path "$RepoRoot\docs\img" -Force | Out-Null
}

# Verify Datara compiler is built
$DataraExe = "$RepoRoot\target\release\datara.exe"
if (-not (Test-Path $DataraExe)) {
    Write-Host "[INFO] Building Datara in release mode..." -ForegroundColor Yellow
    $Cargo = "$env:USERPROFILE\.cargo\bin\cargo.exe"
    & $Cargo build --release --bin datara
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Failed to build target\release\datara.exe"
        exit 1
    }
}

# Run Python benchmark runner
Write-Host "[INFO] Starting benchmark execution (Python runner)..." -ForegroundColor Green
python "$RepoRoot\scripts\bench_runner.py"
if ($LASTEXITCODE -ne 0) {
    Write-Error "Benchmark runner failed with exit code $LASTEXITCODE"
    exit $LASTEXITCODE
}

Write-Host "[SUCCESS] All benchmark datasets generated in docs/data/." -ForegroundColor Green
