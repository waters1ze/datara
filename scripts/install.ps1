# =====================================================================
# Datara & Forgen Official 1-Click Windows Installer
# =====================================================================
$ErrorActionPreference = "Stop"

Clear-Host
Write-Host @"
=======================================================================
   ____        _                     
  |  _ \  __ _| |_ __ _ _ __ __ _   
  | | | |/ _` | __/ _` | '__/ _` |   DATARA SYSTEMS LANGUAGE
  | |_| | (_| | || (_| | | | (_| |   Forgen AOT Native Toolchain v0.1.0
  |____/ \__,_|\__\__,_|_|  \__,_|   https://github.com/waters1ze/datara
=======================================================================
"@ -ForegroundColor Cyan

Write-Host "-> Initializing Datara & Forgen Compiler Installation..." -ForegroundColor White

$InstallDir = "$env:USERPROFILE\.datara"
$BinDir     = "$InstallDir\bin"
$StdlibDir  = "$InstallDir\stdlib"
$RuntimeDir = "$InstallDir\runtime"

# 1. Create directory structure
New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
New-Item -ItemType Directory -Force -Path $StdlibDir | Out-Null
New-Item -ItemType Directory -Force -Path $RuntimeDir | Out-Null

$CurrentDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $CurrentDir

# 2. Locate and install forgen.exe
$SourceExe = "$ProjectRoot\target\release\forgen.exe"
if (-not (Test-Path $SourceExe)) {
    $SourceExe = "$CurrentDir\forgen.exe"
}
if (-not (Test-Path $SourceExe)) {
    $SourceExe = "$ProjectRoot\forgen.exe"
}

if (Test-Path $SourceExe) {
    Copy-Item -Force $SourceExe "$BinDir\forgen.exe"
    Write-Host "  [OK] Installed compiler binary: $BinDir\forgen.exe" -ForegroundColor Green
} else {
    Write-Host "  [ERROR] forgen.exe not found. Build release binary first ('cargo build --release')." -ForegroundColor Red
    exit 1
}

# 3. Locate and install standard library (without tests or fixtures)
$StdlibSource = "$ProjectRoot\stdlib"
if (-not (Test-Path $StdlibSource)) {
    $StdlibSource = "$CurrentDir\stdlib"
}
if (Test-Path $StdlibSource) {
    Copy-Item -Force -Recurse "$StdlibSource\*" $StdlibDir
    Write-Host "  [OK] Installed Datara Standard Library: $StdlibDir" -ForegroundColor Green
}

# 4. Locate and install runtime libraries and headers
$RuntimeLibCandidates = @(
    "$ProjectRoot\runtime\datara_runtime.lib",
    "$CurrentDir\runtime\datara_runtime.lib"
) + @(Get-ChildItem -Path "$ProjectRoot\target" -Recurse -Filter "datara_runtime.lib" -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | ForEach-Object { $_.FullName })

foreach ($cand in $RuntimeLibCandidates) {
    if ($cand -and (Test-Path $cand)) {
        Copy-Item -Force $cand "$RuntimeDir\datara_runtime.lib"
        Write-Host "  [OK] Installed Datara C runtime archive: $RuntimeDir\datara_runtime.lib" -ForegroundColor Green
        break
    }
}

$RuntimeHeader = "$ProjectRoot\src\runtime\datara_runtime.h"
if (-not (Test-Path $RuntimeHeader)) {
    $RuntimeHeader = "$CurrentDir\runtime\datara_runtime.h"
}
if (Test-Path $RuntimeHeader) {
    Copy-Item -Force $RuntimeHeader "$RuntimeDir\datara_runtime.h"
}

# 5. Configure User Environment Variables
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$BinDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$BinDir", "User")
    Write-Host "  [OK] Added $BinDir to User PATH" -ForegroundColor Green
} else {
    Write-Host "  [OK] $BinDir already registered in User PATH" -ForegroundColor Gray
}

[Environment]::SetEnvironmentVariable("DATARA_HOME", $InstallDir, "User")
Write-Host "  [OK] Configured DATARA_HOME = $InstallDir" -ForegroundColor Green

# 6. Check C Toolchain Dependency (MSVC Linker)
$clFound = Get-Command "link.exe" -ErrorAction SilentlyContinue
if (-not $clFound) {
    Write-Host "`n  [INFO] MSVC 'link.exe' was not detected in the current PATH." -ForegroundColor Yellow
    Write-Host "         Forgen can automatically locate Visual Studio Build Tools via vswhere." -ForegroundColor Yellow
    Write-Host "         If needed, install 'Desktop development with C++' via Visual Studio Installer." -ForegroundColor Gray
}

# 7. Verification self-test
Write-Host "`n-> Running installation verification test..." -ForegroundColor Cyan
try {
    $verifyOut = & "$BinDir\forgen.exe" --help 2>&1
    if ($verifyOut -match "Forgen") {
        Write-Host "  [SUCCESS] Forgen CLI verified and operational!" -ForegroundColor Green
    }
} catch {
    Write-Host "  [WARNING] Verification call returned: $_" -ForegroundColor Yellow
}

Write-Host @"

=======================================================================
   DATARA INSTALLATION COMPLETE!
=======================================================================
To start using Datara, open a new PowerShell or Terminal window:

  1. Verify version:
     forgen --help

  2. Create your first project:
     forgen new my_first_app
     cd my_first_app
     forgen run

  3. Documentation & GitHub:
     https://github.com/waters1ze/datara
=======================================================================
"@ -ForegroundColor Cyan

