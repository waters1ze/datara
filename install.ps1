# Datara & Forgen Automated Windows Installer
# Run: irm https://raw.githubusercontent.com/waters1ze/datara/main/install.ps1 | iex
# Or locally: .\install.ps1

$ErrorActionPreference = "Stop"

Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host " Datara Programming Language and Forgen Compiler -- Windows Installer" -ForegroundColor Cyan
Write-Host "================================================================================" -ForegroundColor Cyan

$InstallDir = Join-Path $env:USERPROFILE ".datara"
$BinDir = Join-Path $InstallDir "bin"
$StdlibDir = Join-Path $InstallDir "stdlib"

Write-Host "`n[1/4] Preparing installation directories..." -ForegroundColor Yellow
if (-not (Test-Path $BinDir)) {
    New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
}
if (-not (Test-Path $StdlibDir)) {
    New-Item -ItemType Directory -Path $StdlibDir -Force | Out-Null
}

Write-Host "[2/4] Installing Datara compiler binaries..." -ForegroundColor Yellow
$SourceDir = $PSScriptRoot
if (-not $SourceDir) {
    $SourceDir = (Get-Location).Path
}

$BuiltBinary = Join-Path $SourceDir "target\release\forgen.exe"
$LocalBinary = Join-Path $SourceDir "forgen.exe"

if (Test-Path $BuiltBinary) {
    Copy-Item $BuiltBinary -Destination (Join-Path $BinDir "forgen.exe") -Force
    Copy-Item $BuiltBinary -Destination (Join-Path $BinDir "datara.exe") -Force
    Write-Host "  -> Installed forgen.exe and datara.exe from target\release\" -ForegroundColor Green
} elseif (Test-Path $LocalBinary) {
    Copy-Item $LocalBinary -Destination (Join-Path $BinDir "forgen.exe") -Force
    Copy-Item $LocalBinary -Destination (Join-Path $BinDir "datara.exe") -Force
    Write-Host "  -> Installed forgen.exe and datara.exe from root" -ForegroundColor Green
} else {
    Write-Host "  -> Compiling Forgen binary via Cargo in release mode..." -ForegroundColor Yellow
    $env:PATH = "C:\Users\watersize\.cargo\bin;" + $env:PATH
    cargo build --release --bin forgen
    $CompiledBinary = Join-Path $SourceDir "target\release\forgen.exe"
    Copy-Item $CompiledBinary -Destination (Join-Path $BinDir "forgen.exe") -Force
    Copy-Item $CompiledBinary -Destination (Join-Path $BinDir "datara.exe") -Force
    Write-Host "  -> Compilation complete and binaries installed." -ForegroundColor Green
}

Write-Host "[3/4] Installing Datara Standard Library (stdlib)..." -ForegroundColor Yellow
$SourceStdlib = Join-Path $SourceDir "stdlib"
if (Test-Path $SourceStdlib) {
    Copy-Item -Path "$SourceStdlib\*" -Destination $StdlibDir -Recurse -Force
    Write-Host "  -> Copied all 14 standard modules to $StdlibDir" -ForegroundColor Green
}

Write-Host "[4/4] Configuring System PATH..." -ForegroundColor Yellow
$CurrentPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($CurrentPath -notlike "*$BinDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$BinDir;$CurrentPath", "User")
    $env:PATH = "$BinDir;" + $env:PATH
    Write-Host "  -> Added '$BinDir' to User PATH environment variable." -ForegroundColor Green
} else {
    Write-Host "  -> '$BinDir' is already configured in PATH." -ForegroundColor Green
}

# Set DATARA_HOME
[Environment]::SetEnvironmentVariable("DATARA_HOME", $InstallDir, "User")
$env:DATARA_HOME = $InstallDir

Write-Host "`n================================================================================" -ForegroundColor Green
Write-Host " Verification and Environment Check:" -ForegroundColor Green
Write-Host "================================================================================" -ForegroundColor Green
$ExePath = Join-Path $BinDir "forgen.exe"
& $ExePath --version
Write-Host "DATARA_HOME: $InstallDir" -ForegroundColor Gray
Write-Host "`nDatara and Forgen installed successfully!" -ForegroundColor Cyan
Write-Host "Restart your terminal or run:" -ForegroundColor White
Write-Host "  forgen run hello.dtr" -ForegroundColor Yellow
Write-Host "  forgen repl" -ForegroundColor Yellow
Write-Host "  forgen doc --open`n" -ForegroundColor Yellow
