# =====================================================================
# Datara & Forgen Production Multi-Platform Release Bundler
# =====================================================================
$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $ProjectRoot

Write-Host "=======================================================================" -ForegroundColor Cyan
Write-Host " Building Datara & Forgen Production Release Artifacts" -ForegroundColor Cyan
Write-Host "=======================================================================" -ForegroundColor Cyan

# 1. Compile optimized release binary
Write-Host "`n-> Building release binary..." -ForegroundColor White
& "$env:USERPROFILE\.cargo\bin\cargo.exe" build --release

$DistDir = "$ProjectRoot\dist"
if (-not (Test-Path $DistDir)) {
    New-Item -ItemType Directory -Force -Path $DistDir | Out-Null
}

# =====================================================================
# WINDOWS x86_64 RELEASE BUNDLE
# =====================================================================
$WinStaging = "$DistDir\forgen-v0.1.0-windows-x64"
if (Test-Path $WinStaging) {
    Remove-Item -Recurse -Force $WinStaging
}
New-Item -ItemType Directory -Force -Path $WinStaging | Out-Null
New-Item -ItemType Directory -Force -Path "$WinStaging\stdlib" | Out-Null
New-Item -ItemType Directory -Force -Path "$WinStaging\runtime" | Out-Null

Write-Host "-> Assembling Windows x64 release package..." -ForegroundColor White

# Binary
Copy-Item -Force "$ProjectRoot\target\release\forgen.exe" "$WinStaging\forgen.exe"

# Standard Library (pure library files, no tests)
Copy-Item -Force -Recurse "$ProjectRoot\stdlib\*" "$WinStaging\stdlib"

# C Runtime headers and static archive
$RuntimeLib = (Get-ChildItem -Path "$ProjectRoot\target\release\build" -Recurse -Filter "datara_runtime.lib" -ErrorAction SilentlyContinue | Select-Object -First 1).FullName
if ($RuntimeLib -and (Test-Path $RuntimeLib)) {
    Copy-Item -Force $RuntimeLib "$WinStaging\runtime\datara_runtime.lib"
}
Copy-Item -Force "$ProjectRoot\src\runtime\datara_runtime.h" "$WinStaging\runtime\datara_runtime.h"
Copy-Item -Force "$ProjectRoot\src\runtime\datara_runtime.c" "$WinStaging\runtime\datara_runtime.c"

# Installers
Copy-Item -Force "$ProjectRoot\scripts\install.ps1" "$WinStaging\install.ps1"
Copy-Item -Force "$ProjectRoot\scripts\install.bat" "$WinStaging\install.bat"
Copy-Item -Force "$ProjectRoot\scripts\install.sh" "$WinStaging\install.sh"

# Metadata & Licenses
Copy-Item -Force "$ProjectRoot\README.md" "$WinStaging\README.md"
Copy-Item -Force "$ProjectRoot\LICENSE" "$WinStaging\LICENSE"
Copy-Item -Force "$ProjectRoot\LICENSE-MIT" "$WinStaging\LICENSE-MIT"
Copy-Item -Force "$ProjectRoot\LICENSE-APACHE" "$WinStaging\LICENSE-APACHE"
if (Test-Path "$ProjectRoot\icon.svg") {
    Copy-Item -Force "$ProjectRoot\icon.svg" "$WinStaging\icon.svg"
}

# Create Windows Zip
$WinZip = "$DistDir\forgen-v0.1.0-windows-x64.zip"
if (Test-Path $WinZip) { Remove-Item -Force $WinZip }
Compress-Archive -Path "$WinStaging\*" -DestinationPath $WinZip -Force
$WinSizeMB = [math]::Round((Get-Item $WinZip).Length / 1MB, 2)
Write-Host "  [OK] Created: $WinZip ($WinSizeMB MB)" -ForegroundColor Green

# =====================================================================
# LINUX x86_64 RELEASE BUNDLE TEMPLATE
# =====================================================================
$LinuxStaging = "$DistDir\forgen-v0.1.0-linux-x64"
if (Test-Path $LinuxStaging) { Remove-Item -Recurse -Force $LinuxStaging }
New-Item -ItemType Directory -Force -Path $LinuxStaging | Out-Null
New-Item -ItemType Directory -Force -Path "$LinuxStaging\stdlib" | Out-Null
New-Item -ItemType Directory -Force -Path "$LinuxStaging\runtime" | Out-Null

Copy-Item -Force -Recurse "$ProjectRoot\stdlib\*" "$LinuxStaging\stdlib"
Copy-Item -Force "$ProjectRoot\src\runtime\datara_runtime.h" "$LinuxStaging\runtime\datara_runtime.h"
Copy-Item -Force "$ProjectRoot\src\runtime\datara_runtime.c" "$LinuxStaging\runtime\datara_runtime.c"
Copy-Item -Force "$ProjectRoot\scripts\install.sh" "$LinuxStaging\install.sh"
Copy-Item -Force "$ProjectRoot\README.md" "$LinuxStaging\README.md"
Copy-Item -Force "$ProjectRoot\LICENSE" "$LinuxStaging\LICENSE"

$LinuxTar = "$DistDir\forgen-v0.1.0-linux-x64.zip"
if (Test-Path $LinuxTar) { Remove-Item -Force $LinuxTar }
Compress-Archive -Path "$LinuxStaging\*" -DestinationPath $LinuxTar -Force
Write-Host "  [OK] Created: $LinuxTar" -ForegroundColor Green

# =====================================================================
# MACOS ARM64 / APPLE SILICON RELEASE BUNDLE TEMPLATE
# =====================================================================
$DarwinStaging = "$DistDir\forgen-v0.1.0-darwin-arm64"
if (Test-Path $DarwinStaging) { Remove-Item -Recurse -Force $DarwinStaging }
New-Item -ItemType Directory -Force -Path $DarwinStaging | Out-Null
New-Item -ItemType Directory -Force -Path "$DarwinStaging\stdlib" | Out-Null
New-Item -ItemType Directory -Force -Path "$DarwinStaging\runtime" | Out-Null

Copy-Item -Force -Recurse "$ProjectRoot\stdlib\*" "$DarwinStaging\stdlib"
Copy-Item -Force "$ProjectRoot\src\runtime\datara_runtime.h" "$DarwinStaging\runtime\datara_runtime.h"
Copy-Item -Force "$ProjectRoot\src\runtime\datara_runtime.c" "$DarwinStaging\runtime\datara_runtime.c"
Copy-Item -Force "$ProjectRoot\scripts\install.sh" "$DarwinStaging\install.sh"
Copy-Item -Force "$ProjectRoot\README.md" "$DarwinStaging\README.md"
Copy-Item -Force "$ProjectRoot\LICENSE" "$DarwinStaging\LICENSE"

$DarwinTar = "$DistDir\forgen-v0.1.0-darwin-arm64.zip"
if (Test-Path $DarwinTar) { Remove-Item -Force $DarwinTar }
Compress-Archive -Path "$DarwinStaging\*" -DestinationPath $DarwinTar -Force
Write-Host "  [OK] Created: $DarwinTar" -ForegroundColor Green

Write-Host "`n=======================================================================" -ForegroundColor Cyan
Write-Host " All distribution packages successfully assembled in dist/!" -ForegroundColor Green
Write-Host " Ready for deployment to GitHub Releases v0.1.0." -ForegroundColor White
Write-Host "=======================================================================" -ForegroundColor Cyan
