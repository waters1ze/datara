# =====================================================================
# Datara & Forgen Production Multi-Platform Release Bundler
# =====================================================================
param(
    [string]$Version = ""
)

$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $ProjectRoot

# 0. Resolve Version Dynamically
if (-not $Version) {
    $cargoTomlPath = Join-Path $ProjectRoot "Cargo.toml"
    if (Test-Path $cargoTomlPath) {
        $cargoToml = Get-Content $cargoTomlPath -Raw
        if ($cargoToml -match '(?m)^version\s*=\s*"([^"]+)"') {
            $Version = $matches[1]
        }
    }
}
if (-not $Version) {
    $Version = "0.1.0"
}

$CleanVer = $Version.TrimStart('v')
$TagVer = "v$CleanVer"

Write-Host "=======================================================================" -ForegroundColor Cyan
Write-Host " Building Datara & Forgen Production Release Artifacts ($TagVer)" -ForegroundColor Cyan
Write-Host "=======================================================================" -ForegroundColor Cyan

# 1. Compile optimized release binaries
Write-Host "`n-> Building release binaries..." -ForegroundColor White
& "$env:USERPROFILE\.cargo\bin\cargo.exe" build --release --bins

$DistDir = "$ProjectRoot\dist"
if (-not (Test-Path $DistDir)) {
    New-Item -ItemType Directory -Force -Path $DistDir | Out-Null
}

# =====================================================================
# WINDOWS x86_64 RELEASE BUNDLE
# =====================================================================
$WinStaging = "$DistDir\forgen-$TagVer-windows-x64"
if (Test-Path $WinStaging) {
    Remove-Item -Recurse -Force $WinStaging
}
New-Item -ItemType Directory -Force -Path $WinStaging | Out-Null
New-Item -ItemType Directory -Force -Path "$WinStaging\bin" | Out-Null
New-Item -ItemType Directory -Force -Path "$WinStaging\stdlib" | Out-Null
New-Item -ItemType Directory -Force -Path "$WinStaging\runtime" | Out-Null

Write-Host "-> Assembling Windows x64 release package..." -ForegroundColor White

# Binaries (both in root and bin/ for maximum compatibility)
Copy-Item -Force "$ProjectRoot\target\release\forgen.exe" "$WinStaging\forgen.exe"
Copy-Item -Force "$ProjectRoot\target\release\forgen.exe" "$WinStaging\bin\forgen.exe"

if (Test-Path "$ProjectRoot\target\release\datara.exe") {
    Copy-Item -Force "$ProjectRoot\target\release\datara.exe" "$WinStaging\datara.exe"
    Copy-Item -Force "$ProjectRoot\target\release\datara.exe" "$WinStaging\bin\datara.exe"
} else {
    Copy-Item -Force "$ProjectRoot\target\release\forgen.exe" "$WinStaging\datara.exe"
    Copy-Item -Force "$ProjectRoot\target\release\forgen.exe" "$WinStaging\bin\datara.exe"
}

if (Test-Path "$ProjectRoot\target\release\dpm.exe") {
    Copy-Item -Force "$ProjectRoot\target\release\dpm.exe" "$WinStaging\dpm.exe"
    Copy-Item -Force "$ProjectRoot\target\release\dpm.exe" "$WinStaging\bin\dpm.exe"
}

# Standard Library (pure library files, no tests)
Copy-Item -Force -Recurse "$ProjectRoot\stdlib\*" "$WinStaging\stdlib"

# C Runtime headers and static archive
$RuntimeLib = (Get-ChildItem -Path "$ProjectRoot\target\release\build" -Recurse -Filter "datara_runtime.lib" -ErrorAction SilentlyContinue | Select-Object -First 1).FullName
if ($RuntimeLib -and (Test-Path $RuntimeLib)) {
    Copy-Item -Force $RuntimeLib "$WinStaging\runtime\datara_runtime.lib"
} elseif (Test-Path "$ProjectRoot\runtime\datara_runtime.lib") {
    Copy-Item -Force "$ProjectRoot\runtime\datara_runtime.lib" "$WinStaging\runtime\datara_runtime.lib"
}
Copy-Item -Force "$ProjectRoot\src\runtime\datara_runtime.h" "$WinStaging\runtime\datara_runtime.h"
Copy-Item -Force "$ProjectRoot\src\runtime\datara_runtime.c" "$WinStaging\runtime\datara_runtime.c"

# Installers & Assets
if (Test-Path "$ProjectRoot\install.ps1") { Copy-Item -Force "$ProjectRoot\install.ps1" "$WinStaging\install.ps1" }
if (Test-Path "$ProjectRoot\setup_windows.bat") { Copy-Item -Force "$ProjectRoot\setup_windows.bat" "$WinStaging\setup_windows.bat" }
if (Test-Path "$ProjectRoot\scripts\install.ps1") { Copy-Item -Force "$ProjectRoot\scripts\install.ps1" "$WinStaging\scripts_install.ps1" }
if (Test-Path "$ProjectRoot\scripts\install.sh") { Copy-Item -Force "$ProjectRoot\scripts\install.sh" "$WinStaging\install.sh" }
New-Item -ItemType Directory -Force -Path "$WinStaging\scripts" | Out-Null
if (Test-Path "$ProjectRoot\scripts\install_build_tools.ps1") { Copy-Item -Force "$ProjectRoot\scripts\install_build_tools.ps1" "$WinStaging\scripts\install_build_tools.ps1" }
if (Test-Path "$ProjectRoot\scripts\install_build_tools.bat") { Copy-Item -Force "$ProjectRoot\scripts\install_build_tools.bat" "$WinStaging\scripts\install_build_tools.bat" }
if (Test-Path "$ProjectRoot\installer\install_build_tools.bat") { Copy-Item -Force "$ProjectRoot\installer\install_build_tools.bat" "$WinStaging\install_build_tools.bat" }

# Metadata & Licenses
Copy-Item -Force "$ProjectRoot\README.md" "$WinStaging\README.md"
if (Test-Path "$ProjectRoot\LICENSE") { Copy-Item -Force "$ProjectRoot\LICENSE" "$WinStaging\LICENSE" }
if (Test-Path "$ProjectRoot\LICENSE-MIT") { Copy-Item -Force "$ProjectRoot\LICENSE-MIT" "$WinStaging\LICENSE-MIT" }
if (Test-Path "$ProjectRoot\LICENSE-APACHE") { Copy-Item -Force "$ProjectRoot\LICENSE-APACHE" "$WinStaging\LICENSE-APACHE" }
if (Test-Path "$ProjectRoot\assets\datara.ico") {
    New-Item -ItemType Directory -Force -Path "$WinStaging\assets" | Out-Null
    Copy-Item -Force "$ProjectRoot\assets\datara.ico" "$WinStaging\assets\datara.ico"
    if (Test-Path "$ProjectRoot\assets\datara-logo.png") {
        Copy-Item -Force "$ProjectRoot\assets\datara-logo.png" "$WinStaging\assets\datara-logo.png"
    }
}

# Create Windows Zip (both versioned and canonical generic name)
$WinZip = "$DistDir\forgen-$TagVer-windows-x64.zip"
$WinZipGeneric = "$DistDir\forgen-windows-x64.zip"
if (Test-Path $WinZip) { Remove-Item -Force $WinZip }
if (Test-Path $WinZipGeneric) { Remove-Item -Force $WinZipGeneric }

Compress-Archive -Path "$WinStaging\*" -DestinationPath $WinZip -Force
Copy-Item -Force $WinZip $WinZipGeneric

$WinSizeMB = [math]::Round((Get-Item $WinZip).Length / 1MB, 2)
Write-Host "  [OK] Created: $WinZip ($WinSizeMB MB)" -ForegroundColor Green
Write-Host "  [OK] Created: $WinZipGeneric" -ForegroundColor Green

Get-FileHash -Algorithm SHA256 $WinZip | Out-File "$WinZip.sha256"
Get-FileHash -Algorithm SHA256 $WinZipGeneric | Out-File "$WinZipGeneric.sha256"

# Build Standalone Windows GUI Setup.exe if csc.exe is available
$csc = "C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe"
if (-not (Test-Path $csc)) {
    $csc = "C:\Windows\Microsoft.NET\Framework\v4.0.30319\csc.exe"
}
if (Test-Path $csc) {
    Write-Host "-> Compiling native Windows Setup Wizard (Datara-Setup.exe)..." -ForegroundColor White
    $setupCsPath = "$ProjectRoot\installer\SetupWizard.cs"
    if (Test-Path $setupCsPath) {
        $setupCsContent = Get-Content $setupCsPath -Raw
        $setupCsContent = [regex]::Replace($setupCsContent, 'public const string AppVersion = "[^"]*";', "public const string AppVersion = `"$CleanVer`";")
        Set-Content -Path $setupCsPath -Value $setupCsContent -NoNewline

        & $csc /nologo /target:winexe /win32icon:"$ProjectRoot\assets\datara.ico" `
            /resource:"$WinZip,payload.zip" `
            /reference:System.dll,System.Windows.Forms.dll,System.Drawing.dll,System.IO.Compression.dll,System.IO.Compression.FileSystem.dll `
            /out:"$DistDir\Datara-Setup.exe" "$setupCsPath"

        if (Test-Path "$DistDir\Datara-Setup.exe") {
            Copy-Item -Force "$DistDir\Datara-Setup.exe" "$DistDir\Datara-$TagVer-Setup.exe"
            Get-FileHash -Algorithm SHA256 "$DistDir\Datara-Setup.exe" | Out-File "$DistDir\Datara-Setup.exe.sha256"
            Get-FileHash -Algorithm SHA256 "$DistDir\Datara-$TagVer-Setup.exe" | Out-File "$DistDir\Datara-$TagVer-Setup.exe.sha256"
            Write-Host "  [OK] Compiled: $DistDir\Datara-Setup.exe and Datara-$TagVer-Setup.exe" -ForegroundColor Green
        }
    }
}

# =====================================================================
# LINUX x86_64 RELEASE BUNDLE TEMPLATE
# =====================================================================
$LinuxStaging = "$DistDir\forgen-$TagVer-linux-x64"
if (Test-Path $LinuxStaging) { Remove-Item -Recurse -Force $LinuxStaging }
New-Item -ItemType Directory -Force -Path $LinuxStaging | Out-Null
New-Item -ItemType Directory -Force -Path "$LinuxStaging\stdlib" | Out-Null
New-Item -ItemType Directory -Force -Path "$LinuxStaging\runtime" | Out-Null

Copy-Item -Force -Recurse "$ProjectRoot\stdlib\*" "$LinuxStaging\stdlib"
Copy-Item -Force "$ProjectRoot\src\runtime\datara_runtime.h" "$LinuxStaging\runtime\datara_runtime.h"
Copy-Item -Force "$ProjectRoot\src\runtime\datara_runtime.c" "$LinuxStaging\runtime\datara_runtime.c"
Copy-Item -Force "$ProjectRoot\scripts\install.sh" "$LinuxStaging\install.sh"
Copy-Item -Force "$ProjectRoot\README.md" "$LinuxStaging\README.md"
if (Test-Path "$ProjectRoot\LICENSE") { Copy-Item -Force "$ProjectRoot\LICENSE" "$LinuxStaging\LICENSE" }

$LinuxTar = "$DistDir\forgen-$TagVer-linux-x64.zip"
$LinuxTarGeneric = "$DistDir\forgen-linux-x64.zip"
if (Test-Path $LinuxTar) { Remove-Item -Force $LinuxTar }
if (Test-Path $LinuxTarGeneric) { Remove-Item -Force $LinuxTarGeneric }

Compress-Archive -Path "$LinuxStaging\*" -DestinationPath $LinuxTar -Force
Copy-Item -Force $LinuxTar $LinuxTarGeneric
Write-Host "  [OK] Created: $LinuxTar" -ForegroundColor Green

# =====================================================================
# MACOS ARM64 / APPLE SILICON RELEASE BUNDLE TEMPLATE
# =====================================================================
$DarwinStaging = "$DistDir\forgen-$TagVer-darwin-arm64"
if (Test-Path $DarwinStaging) { Remove-Item -Recurse -Force $DarwinStaging }
New-Item -ItemType Directory -Force -Path $DarwinStaging | Out-Null
New-Item -ItemType Directory -Force -Path "$DarwinStaging\stdlib" | Out-Null
New-Item -ItemType Directory -Force -Path "$DarwinStaging\runtime" | Out-Null

Copy-Item -Force -Recurse "$ProjectRoot\stdlib\*" "$DarwinStaging\stdlib"
Copy-Item -Force "$ProjectRoot\src\runtime\datara_runtime.h" "$DarwinStaging\runtime\datara_runtime.h"
Copy-Item -Force "$ProjectRoot\src\runtime\datara_runtime.c" "$DarwinStaging\runtime\datara_runtime.c"
Copy-Item -Force "$ProjectRoot\scripts\install.sh" "$DarwinStaging\install.sh"
Copy-Item -Force "$ProjectRoot\README.md" "$DarwinStaging\README.md"
if (Test-Path "$ProjectRoot\LICENSE") { Copy-Item -Force "$ProjectRoot\LICENSE" "$DarwinStaging\LICENSE" }

$DarwinTar = "$DistDir\forgen-$TagVer-darwin-arm64.zip"
$DarwinTarGeneric = "$DistDir\forgen-darwin-arm64.zip"
if (Test-Path $DarwinTar) { Remove-Item -Force $DarwinTar }
if (Test-Path $DarwinTarGeneric) { Remove-Item -Force $DarwinTarGeneric }

Compress-Archive -Path "$DarwinStaging\*" -DestinationPath $DarwinTar -Force
Copy-Item -Force $DarwinTar $DarwinTarGeneric
Write-Host "  [OK] Created: $DarwinTar" -ForegroundColor Green

Write-Host "`n=======================================================================" -ForegroundColor Cyan
Write-Host " All distribution packages successfully assembled in dist/!" -ForegroundColor Green
Write-Host " Ready for deployment to GitHub Releases $TagVer." -ForegroundColor White
Write-Host "=======================================================================" -ForegroundColor Cyan
