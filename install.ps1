<#
.SYNOPSIS
    Datara Programming Language & Forgen Compiler - Official Universal Windows Terminal Installer
.DESCRIPTION
    Dynamically installs the latest release of Datara, standard library, and file associations.
    Run via: irm https://raw.githubusercontent.com/waters1ze/datara/main/install.ps1 | iex
#>

$ErrorActionPreference = "Stop"

[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

Write-Host "================================================================================" -ForegroundColor Cyan
Write-Host "   ____        _                     " -ForegroundColor Cyan
Write-Host "  |  _ \  __ _| |_ __ _ _ __ __ _    Datara Systems Language" -ForegroundColor Cyan
Write-Host "  | | | |/ _` | __/ _` | '__/ _` |   Forgen AOT Native Toolchain" -ForegroundColor Cyan
Write-Host "  | |_| | (_| | || (_| | | | (_| |   Universal Windows Installer" -ForegroundColor Cyan
Write-Host "  |____/ \__,_|\__\__,_|_|  \__,_|   https://github.com/waters1ze/datara" -ForegroundColor Cyan
Write-Host "================================================================================" -ForegroundColor Cyan

# 1. Prepare Target Directory Structure
$InstallDir = Join-Path $env:USERPROFILE ".datara"
$BinDir     = Join-Path $InstallDir "bin"
$StdlibDir  = Join-Path $InstallDir "stdlib"
$AssetsDir  = Join-Path $InstallDir "assets"

Write-Host "`n[1/5] Preparing installation directories..." -ForegroundColor Yellow
New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
New-Item -ItemType Directory -Force -Path $StdlibDir | Out-Null
New-Item -ItemType Directory -Force -Path $AssetsDir | Out-Null

# 2. Determine Version Dynamically from GitHub API
Write-Host "[2/5] Resolving latest Datara version..." -ForegroundColor Yellow
$Repo = "waters1ze/datara"
$ApiUrl = "https://api.github.com/repos/$Repo/releases/latest"
$LatestTag = "v0.1.0"
$DownloadUrl = ""

try {
    $releaseInfo = Invoke-RestMethod -Uri $ApiUrl -Headers @{ "User-Agent" = "Datara-Installer" } -TimeoutSec 6 -ErrorAction Stop
    if ($releaseInfo.tag_name) {
        $LatestTag = $releaseInfo.tag_name
        Write-Host "  -> Detected latest release: $LatestTag" -ForegroundColor Green
        
        # Check for windows zip or exe asset
        $asset = $releaseInfo.assets | Where-Object { $_.name -like "*windows-x64*.zip" -or $_.name -like "*Setup.exe" } | Select-Object -First 1
        if ($asset) {
            $DownloadUrl = $asset.browser_download_url
        }
    }
} catch {
    Write-Host "  -> Using release profile: $LatestTag (GitHub API offline or unreleased tag)" -ForegroundColor Gray
}

# 3. Obtain Binaries (Local checkout -> Download -> Cargo build fallback)
Write-Host "[3/5] Installing compiler binaries and runtime..." -ForegroundColor Yellow
$InstalledSuccessfully = $false

# 3a. Check if running inside local repository
$ScriptDir = if ($PSScriptRoot) { $PSScriptRoot } else { (Get-Location).Path }
$LocalCandidates = @(
    (Join-Path $ScriptDir "target\release\forgen.exe"),
    (Join-Path $ScriptDir "..\target\release\forgen.exe"),
    (Join-Path $ScriptDir "bin\forgen.exe"),
    (Join-Path $ScriptDir "forgen.exe")
)

foreach ($cand in $LocalCandidates) {
    if (Test-Path $cand) {
        Copy-Item -Path $cand -Destination (Join-Path $BinDir "forgen.exe") -Force
        Copy-Item -Path $cand -Destination (Join-Path $BinDir "datara.exe") -Force
        $dpmLocal = Join-Path (Split-Path $cand) "dpm.exe"
        if (Test-Path $dpmLocal) {
            Copy-Item -Path $dpmLocal -Destination (Join-Path $BinDir "dpm.exe") -Force
        } else {
            Copy-Item -Path $cand -Destination (Join-Path $BinDir "dpm.exe") -Force
        }
        Write-Host "  -> Installed toolchain binaries (forgen, datara, dpm) from local source: $cand" -ForegroundColor Green
        $InstalledSuccessfully = $true
        break
    }
}

# 3b. Download prebuilt release if not found locally
if (-not $InstalledSuccessfully -and $DownloadUrl) {
    Write-Host "  -> Downloading prebuilt release package from GitHub..." -ForegroundColor Cyan
    $tempZip = Join-Path $env:TEMP "datara-$LatestTag.zip"
    try {
        Invoke-WebRequest -Uri $DownloadUrl -OutFile $tempZip -TimeoutSec 60
        Expand-Archive -Path $tempZip -DestinationPath $env:TEMP\datara_extracted -Force
        
        $extractedExe = Get-ChildItem -Path $env:TEMP\datara_extracted -Recurse -Filter "forgen.exe" | Select-Object -First 1
        if ($extractedExe) {
            Copy-Item -Path $extractedExe.FullName -Destination (Join-Path $BinDir "forgen.exe") -Force
            Copy-Item -Path $extractedExe.FullName -Destination (Join-Path $BinDir "datara.exe") -Force
            $extractedDpm = Get-ChildItem -Path $env:TEMP\datara_extracted -Recurse -Filter "dpm.exe" | Select-Object -First 1
            if ($extractedDpm) {
                Copy-Item -Path $extractedDpm.FullName -Destination (Join-Path $BinDir "dpm.exe") -Force
            } else {
                Copy-Item -Path $extractedExe.FullName -Destination (Join-Path $BinDir "dpm.exe") -Force
            }
            $InstalledSuccessfully = $true
            Write-Host "  -> Downloaded and installed $LatestTag binaries successfully." -ForegroundColor Green
        }
    } catch {
        Write-Host "  -> Download failed, attempting fallback to Cargo compilation..." -ForegroundColor Gray
    } finally {
        if (Test-Path $tempZip) { Remove-Item $tempZip -Force }
        if (Test-Path "$env:TEMP\datara_extracted") { Remove-Item "$env:TEMP\datara_extracted" -Recurse -Force }
    }
}

# 3c. Fallback to Cargo if local build exists or cargo is on system
if (-not $InstalledSuccessfully) {
    # Dynamically locate cargo without any hardcoded user paths
    $cargoCmd = Get-Command cargo -ErrorAction SilentlyContinue
    $cargoExe = if ($cargoCmd) { $cargoCmd.Source } else { Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe" }
    
    if (Test-Path $cargoExe) {
        Write-Host "  -> Building Datara toolchain from source using Cargo..." -ForegroundColor Cyan
        & $cargoExe build --release
        $builtExe = Join-Path $ScriptDir "target\release\forgen.exe"
        if (Test-Path $builtExe) {
            Copy-Item -Path $builtExe -Destination (Join-Path $BinDir "forgen.exe") -Force
            Copy-Item -Path $builtExe -Destination (Join-Path $BinDir "datara.exe") -Force
            $builtDpm = Join-Path $ScriptDir "target\release\dpm.exe"
            if (Test-Path $builtDpm) {
                Copy-Item -Path $builtDpm -Destination (Join-Path $BinDir "dpm.exe") -Force
            }
            $InstalledSuccessfully = $true
            Write-Host "  -> Cargo compilation succeeded and binaries installed." -ForegroundColor Green
        }
    }
}

if (-not $InstalledSuccessfully) {
    Write-Error "Failed to install forgen.exe. Please download Datara-Setup.exe or run 'cargo build --release'."
    exit 1
}

# 4. Install Standard Library & Assets
Write-Host "[4/5] Installing Standard Library and official icons..." -ForegroundColor Yellow
$StdlibCandidates = @(
    (Join-Path $ScriptDir "stdlib"),
    (Join-Path $ScriptDir "..\stdlib"),
    (Join-Path (Get-Location).Path "stdlib")
)

foreach ($cand in $StdlibCandidates) {
    if (Test-Path $cand) {
        Copy-Item -Path "$cand\*" -Destination $StdlibDir -Recurse -Force
        Write-Host "  -> Installed standard library modules from $cand" -ForegroundColor Green
        break
    }
}

# Copy Icons & Register File Association
$IconCandidates = @(
    (Join-Path $ScriptDir "assets\datara.ico"),
    (Join-Path $ScriptDir "..\assets\datara.ico")
)
$RegisteredIcon = ""
foreach ($ico in $IconCandidates) {
    if (Test-Path $ico) {
        Copy-Item -Path $ico -Destination (Join-Path $AssetsDir "datara.ico") -Force
        $RegisteredIcon = Join-Path $AssetsDir "datara.ico"
        break
    }
}

if ($RegisteredIcon -and (Test-Path $RegisteredIcon)) {
    try {
        $forgenBin = Join-Path $BinDir "forgen.exe"
        # HKCU:\Software\Classes\.dtr
        $dtrKey = "HKCU:\Software\Classes\.dtr"
        if (-not (Test-Path $dtrKey)) { New-Item -Path $dtrKey -Force | Out-Null }
        Set-ItemProperty -Path $dtrKey -Name "(default)" -Value "DataraSourceFile"
        Set-ItemProperty -Path $dtrKey -Name "FriendlyTypeName" -Value "Datara Source File"
        Set-ItemProperty -Path $dtrKey -Name "Content Type" -Value "text/plain"
        Set-ItemProperty -Path $dtrKey -Name "PerceivedType" -Value "text"

        # HKCU:\Software\Classes\DataraSourceFile
        $progKey = "HKCU:\Software\Classes\DataraSourceFile"
        if (-not (Test-Path $progKey)) { New-Item -Path $progKey -Force | Out-Null }
        Set-ItemProperty -Path $progKey -Name "(default)" -Value "Datara Source File"
        Set-ItemProperty -Path $progKey -Name "FriendlyTypeName" -Value "Datara Source File"
        
        $iconKey = "$progKey\DefaultIcon"
        if (-not (Test-Path $iconKey)) { New-Item -Path $iconKey -Force | Out-Null }
        Set-ItemProperty -Path $iconKey -Name "(default)" -Value "$RegisteredIcon"

        # Also set on .dtr\DefaultIcon directly
        $dtrIconKey = "$dtrKey\DefaultIcon"
        if (-not (Test-Path $dtrIconKey)) { New-Item -Path $dtrIconKey -Force | Out-Null }
        Set-ItemProperty -Path $dtrIconKey -Name "(default)" -Value "$RegisteredIcon"

        # Also set on Datara.SourceFile
        $dotProgKey = "HKCU:\Software\Classes\Datara.SourceFile"
        if (-not (Test-Path $dotProgKey)) { New-Item -Path $dotProgKey -Force | Out-Null }
        Set-ItemProperty -Path $dotProgKey -Name "(default)" -Value "Datara Source File"
        Set-ItemProperty -Path $dotProgKey -Name "FriendlyTypeName" -Value "Datara Source File"

        $dotProgIconKey = "$dotProgKey\DefaultIcon"
        if (-not (Test-Path $dotProgIconKey)) { New-Item -Path $dotProgIconKey -Force | Out-Null }
        Set-ItemProperty -Path $dotProgIconKey -Name "(default)" -Value "$RegisteredIcon"

        $cmdKey = "$progKey\shell\open\command"
        if (-not (Test-Path $cmdKey)) { New-Item -Path $cmdKey -Force | Out-Null }
        Set-ItemProperty -Path $cmdKey -Name "(default)" -Value "`"$forgenBin`" run `"%1`""

        # Notify Shell
        Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class ShellNotifierTerm {
    [DllImport("Shell32.dll")]
    public static extern void SHChangeNotify(int eventId, int flags, IntPtr item1, IntPtr item2);
}
"@ -ErrorAction SilentlyContinue
        [ShellNotifierTerm]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)
        Write-Host "  -> Registered .dtr file association with official Datara icon." -ForegroundColor Green
    } catch { }
}

# 4b. Copy Build Tools Scripts & Create Shortcuts
$ScriptsDst = Join-Path $InstallDir "scripts"
New-Item -ItemType Directory -Force -Path $ScriptsDst | Out-Null
$btCandidates = @(
    (Join-Path $ScriptDir "scripts\install_build_tools.ps1"),
    (Join-Path $ScriptDir "install_build_tools.ps1"),
    (Join-Path $ScriptDir "scripts\install_build_tools.bat"),
    (Join-Path $ScriptDir "install_build_tools.bat")
)
foreach ($bt in $btCandidates) {
    if (Test-Path $bt) { Copy-Item -Path $bt -Destination $ScriptsDst -Force }
}

try {
    $ws = New-Object -ComObject WScript.Shell
    $programsDir = [Environment]::GetFolderPath([Environment+SpecialFolder]::Programs)
    $startMenuDir = Join-Path $programsDir "Datara"
    New-Item -ItemType Directory -Force -Path $startMenuDir | Out-Null
    $desktopDir = [Environment]::GetFolderPath([Environment+SpecialFolder]::DesktopDirectory)
    $dataraBin = Join-Path $BinDir "datara.exe"

    # 1. Start Menu Interactive Console
    $s1 = $ws.CreateShortcut((Join-Path $startMenuDir "Datara (Interactive Console).lnk"))
    $s1.TargetPath = $dataraBin
    $s1.WorkingDirectory = $env:USERPROFILE
    if ($RegisteredIcon) { $s1.IconLocation = "$RegisteredIcon,0" }
    $s1.Description = "Datara Interactive Programming Console (REPL)"
    $s1.Save()

    # 2. Start Menu Command Prompt
    $s2 = $ws.CreateShortcut((Join-Path $startMenuDir "Datara Command Prompt.lnk"))
    $s2.TargetPath = "cmd.exe"
    $s2.Arguments = "/K `"title Datara Developer Console & prompt `$P`$G & set PATH=$BinDir;%PATH%`""
    $s2.WorkingDirectory = $env:USERPROFILE
    if ($RegisteredIcon) { $s2.IconLocation = "$RegisteredIcon,0" }
    $s2.Description = "Command Prompt configured with Datara environment"
    $s2.Save()

    # 3. Desktop Shortcut
    $s3 = $ws.CreateShortcut((Join-Path $desktopDir "Datara.lnk"))
    $s3.TargetPath = $dataraBin
    $s3.WorkingDirectory = $env:USERPROFILE
    if ($RegisteredIcon) { $s3.IconLocation = "$RegisteredIcon,0" }
    $s3.Description = "Datara Interactive Programming Console"
    $s3.Save()
    Write-Host "  -> Created Start Menu and Desktop shortcuts for Datara Console." -ForegroundColor Green
} catch { }

# 5. Configure PATH Environment Variable
Write-Host "[5/5] Configuring environment variables..." -ForegroundColor Yellow
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$BinDir*") {
    $NewPath = "$BinDir;$UserPath"
    [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
    $env:PATH = "$BinDir;" + $env:PATH
    Write-Host "  -> Added '$BinDir' to User PATH environment variable." -ForegroundColor Green
} else {
    Write-Host "  -> '$BinDir' is already configured in User PATH." -ForegroundColor Green
}

[Environment]::SetEnvironmentVariable("DATARA_HOME", $InstallDir, "User")
$env:DATARA_HOME = $InstallDir
Write-Host "  -> Configured DATARA_HOME = $InstallDir" -ForegroundColor Green

# Verification & Toolchain Check
Write-Host "`n================================================================================" -ForegroundColor Green
Write-Host " Verification and Toolchain Status:" -ForegroundColor Green
Write-Host "================================================================================" -ForegroundColor Green
$ExePath = Join-Path $BinDir "forgen.exe"
if (Test-Path $ExePath) {
    & $ExePath --version
}

# Linker detection
$hasLinker = $false
$allLinks = Get-Command link.exe -All -ErrorAction SilentlyContinue
foreach ($l in $allLinks) {
    $src = $l.Source.ToLower()
    if (-not ($src.Contains("git\usr\bin") -or $src.Contains("git/usr/bin"))) {
        $hasLinker = $true
        break
    }
}
if (-not $hasLinker) {
    if ((Get-Command lld-link.exe -ErrorAction SilentlyContinue) -or (Test-Path "C:\Program Files\LLVM\bin\lld-link.exe") -or (Get-Command gcc.exe -ErrorAction SilentlyContinue)) {
        $hasLinker = $true
    }
}
if (-not $hasLinker) {
    $pf86 = ${env:ProgramFiles(x86)}
    if (-not $pf86) { $pf86 = "C:\Program Files (x86)" }
    $vswhere = Join-Path $pf86 "Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vswhere) {
        $vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
        if ($vs -and (Test-Path (Join-Path $vs "VC\Tools\MSVC"))) { $hasLinker = $true }
    }
}

if ($hasLinker) {
    Write-Host "`n[OK] C/C++ native linker is detected and ready." -ForegroundColor Green
} else {
    Write-Host "`n[!] Notice: No C/C++ linker detected on this system." -ForegroundColor Yellow
    Write-Host "    Datara requires a linker to compile native Windows (.exe) executables." -ForegroundColor Gray
    Write-Host "    To automatically download and install build tools (Node.js style), run:" -ForegroundColor White
    Write-Host "      forgen setup-tools" -ForegroundColor Cyan
    Write-Host "    or run: $InstallDir\scripts\install_build_tools.bat`n" -ForegroundColor Gray
}

Write-Host "DATARA_HOME: $InstallDir" -ForegroundColor Gray
Write-Host "`nDatara and Forgen installed successfully!" -ForegroundColor Cyan
Write-Host "To start immediately, launch Datara from your Desktop / Start Menu or run:" -ForegroundColor White
Write-Host "  datara        (Interactive REPL Console)" -ForegroundColor Yellow
Write-Host "  forgen repl   (Interactive REPL Console)" -ForegroundColor Yellow
Write-Host "  forgen run main.dtr`n" -ForegroundColor Yellow
