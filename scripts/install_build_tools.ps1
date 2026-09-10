<#
.SYNOPSIS
    Datara & Forgen - Automatic Native C/C++ Toolchain & Linker Setup for Windows
.DESCRIPTION
    Detects if a C/C++ linker (MSVC link.exe, LLVM lld-link, or MinGW gcc) is installed.
    If none is found, automatically installs Microsoft Visual Studio C++ Build Tools
    via winget or direct Microsoft bootstrapper download (Node.js style).
#>

$ErrorActionPreference = "Continue"

Write-Host "========================================================================" -ForegroundColor Cyan
Write-Host "   ____        _                                                        " -ForegroundColor Cyan
Write-Host "  |  _ \  __ _| |_ __ _ _ __ __ _    Datara Systems Language            " -ForegroundColor Cyan
Write-Host "  | | | |/ _` | __/ _` | '__/ _` |   Native Toolchain & Linker Setup    " -ForegroundColor Cyan
Write-Host "  | |_| | (_| | || (_| | | | (_| |   https://github.com/waters1ze/datara" -ForegroundColor Cyan
Write-Host "  |____/ \__,_|\__\__,_|_|  \__,_|                                      " -ForegroundColor Cyan
Write-Host "========================================================================" -ForegroundColor Cyan
Write-Host ""

function Test-RealLinker {
    # 1. Check vswhere for MSVC link.exe
    $pf = ${env:ProgramFiles(x86)}
    if (-not $pf) { $pf = $env:ProgramFiles }
    if (-not $pf) { $pf = "C:\Program Files (x86)" }
    $vswhere = Join-Path $pf "Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vswhere) {
        try {
            $vsPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
            if ($vsPath -and (Test-Path $vsPath)) {
                $msvcDir = Join-Path $vsPath "VC\Tools\MSVC"
                if (Test-Path $msvcDir) {
                    $newest = Get-ChildItem -Path $msvcDir -Directory | Sort-Object Name -Descending | Select-Object -First 1
                    if ($newest) {
                        $linkCandidates = @(
                            (Join-Path $newest.FullName "bin\Hostx64\x64\link.exe"),
                            (Join-Path $newest.FullName "bin\Hostx86\x64\link.exe"),
                            (Join-Path $newest.FullName "bin\Hostx64\x86\link.exe"),
                            (Join-Path $newest.FullName "bin\link.exe")
                        )
                        foreach ($c in $linkCandidates) {
                            if (Test-Path $c) { return @{ Name = "MSVC Linker"; Path = $c } }
                        }
                    }
                }
            }
        } catch {}
    }

    # 2. Check for lld-link.exe (LLVM)
    $lldCmd = Get-Command lld-link.exe -ErrorAction SilentlyContinue
    if ($lldCmd) { return @{ Name = "LLVM LLD Linker"; Path = $lldCmd.Source } }
    if (Test-Path "C:\Program Files\LLVM\bin\lld-link.exe") {
        return @{ Name = "LLVM LLD Linker"; Path = "C:\Program Files\LLVM\bin\lld-link.exe" }
    }

    # 3. Check for real link.exe in PATH (ignoring Git's coreutils link.exe)
    $allLinks = Get-Command link.exe -All -ErrorAction SilentlyContinue
    foreach ($cmd in $allLinks) {
        $src = $cmd.Source.ToLower()
        if (-not ($src.Contains("git\usr\bin") -or $src.Contains("git/usr/bin"))) {
            return @{ Name = "MSVC Linker (PATH)"; Path = $cmd.Source }
        }
    }

    # 4. Check for clang or gcc
    $clangCmd = Get-Command clang.exe -ErrorAction SilentlyContinue
    if ($clangCmd) { return @{ Name = "Clang Linker"; Path = $clangCmd.Source } }

    $gccCmd = Get-Command gcc.exe -ErrorAction SilentlyContinue
    if ($gccCmd) { return @{ Name = "MinGW GCC Linker"; Path = $gccCmd.Source } }

    return $null
}

Write-Host "-> Inspecting system for existing C/C++ linker..." -ForegroundColor Yellow
$foundLinker = Test-RealLinker

if ($foundLinker) {
    Write-Host "`n  [OK] $($foundLinker.Name) detected at:" -ForegroundColor Green
    Write-Host "       $($foundLinker.Path)" -ForegroundColor White
    Write-Host "`nDatara can build native .exe executables immediately." -ForegroundColor Green
    Write-Host "No further toolchain installation required." -ForegroundColor Gray
    exit 0
}

Write-Host "`n  [!] No C/C++ linker found on this system." -ForegroundColor Yellow
Write-Host "      Datara requires a C/C++ linker to produce native Windows (.exe) executables." -ForegroundColor Gray
Write-Host "      We will now install the official Microsoft C++ Build Tools (Node.js style).`n" -ForegroundColor White

# 1. Try winget
$wingetCmd = Get-Command winget.exe -ErrorAction SilentlyContinue
if (-not $wingetCmd) {
    $localWinget = "$env:LOCALAPPDATA\Microsoft\WindowsApps\winget.exe"
    if (Test-Path $localWinget) { $wingetCmd = $localWinget }
}

$installSucceeded = $false

if ($wingetCmd) {
    Write-Host "-> [1/2] Installing Microsoft C++ Build Tools via Windows Package Manager (winget)..." -ForegroundColor Cyan
    Write-Host "         Package: Microsoft.VisualStudio.2022.BuildTools (VCTools workload)" -ForegroundColor Gray
    try {
        $proc = Start-Process -FilePath "winget" -ArgumentList @(
            "install",
            "--id", "Microsoft.VisualStudio.2022.BuildTools",
            "--exact",
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--override", "`"--passive --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended`""
        ) -PassThru -Wait -NoNewWindow
        
        if ($proc.ExitCode -eq 0) {
            $installSucceeded = $true
        }
    } catch {
        Write-Host "  [WARN] winget installation failed: $_" -ForegroundColor Yellow
    }
}

# 2. Fallback to direct Microsoft vs_buildtools.exe bootstrapper
if (-not $installSucceeded) {
    Write-Host "-> [2/2] Downloading official Microsoft C++ Build Tools bootstrapper from aka.ms..." -ForegroundColor Cyan
    $bootstrapperUrl = "https://aka.ms/vs/17/release/vs_buildtools.exe"
    $tmpExe = Join-Path $env:TEMP "vs_buildtools.exe"
    
    try {
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest -Uri $bootstrapperUrl -OutFile $tmpExe -UseBasicParsing
        Write-Host "  -> Download complete ($tmpExe). Launching installer..." -ForegroundColor Green
        
        $proc = Start-Process -FilePath $tmpExe -ArgumentList @(
            "--passive",
            "--wait",
            "--norestart",
            "--add", "Microsoft.VisualStudio.Workload.VCTools",
            "--includeRecommended"
        ) -PassThru -Wait -NoNewWindow
        
        if ($proc.ExitCode -eq 0 -or $proc.ExitCode -eq 3010) {
            $installSucceeded = $true
        }
    } catch {
        Write-Host "  [ERROR] Direct bootstrapper download/installation failed: $_" -ForegroundColor Red
    }
}

# Verify installation
Write-Host "`n-> Verifying installed toolchain..." -ForegroundColor Yellow
$verify = Test-RealLinker
if ($verify) {
    Write-Host "`n========================================================================" -ForegroundColor Green
    Write-Host " [SUCCESS] $($verify.Name) is now configured!" -ForegroundColor Green
    Write-Host " Location: $($verify.Path)" -ForegroundColor White
    Write-Host " You can now run:" -ForegroundColor Cyan
    Write-Host "   datara                   (Interactive Console REPL)" -ForegroundColor White
    Write-Host "   forgen run main.dtr      (Compile & run native program)" -ForegroundColor White
    Write-Host "   forgen build main.dtr    (Generate standalone .exe)" -ForegroundColor White
    Write-Host "========================================================================" -ForegroundColor Green
    exit 0
} else {
    Write-Host "`n[NOTICE] Installation initiated. A system restart or terminal restart may be required" -ForegroundColor Yellow
    Write-Host "         for PATH environment variables to take effect." -ForegroundColor Yellow
    exit 0
}
