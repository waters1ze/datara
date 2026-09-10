<#
.SYNOPSIS
    Datara Language Uninstaller
#>

param(
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\Datara"
)

Write-Host "Uninstalling Datara..." -ForegroundColor Yellow

# 1. Remove from User PATH
$binDir = Join-Path $InstallDir "bin"
$userPath = [System.Environment]::GetEnvironmentVariable("PATH", "User")
if ($userPath -like "*$binDir*") {
    $parts = $userPath.Split(';') | Where-Object { $_ -ne $binDir -and $_ -ne "" }
    $cleanPath = $parts -join ';'
    [System.Environment]::SetEnvironmentVariable("PATH", $cleanPath, "User")
    [System.Environment]::SetEnvironmentVariable("DATARA_HOME", $null, "User")
    Write-Host "✓ Removed from PATH and DATARA_HOME" -ForegroundColor Green
}

# 2. Remove File Association Registry Keys
$dtrKey = "HKCU:\Software\Classes\.dtr"
if (Test-Path $dtrKey) {
    Remove-Item -Path $dtrKey -Recurse -Force
    Write-Host "✓ Removed .dtr registry association" -ForegroundColor Green
}

$progKey = "HKCU:\Software\Classes\DataraSourceFile"
if (Test-Path $progKey) {
    Remove-Item -Path $progKey -Recurse -Force
    Write-Host "✓ Removed DataraSourceFile registry class" -ForegroundColor Green
}

# 3. Notify Shell
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class ShellNotifier2 {
    [DllImport("Shell32.dll")]
    public static extern void SHChangeNotify(int eventId, int flags, IntPtr item1, IntPtr item2);
}
"@ -ErrorAction SilentlyContinue

[ShellNotifier2]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)

# 4. Remove Files
if (Test-Path $InstallDir) {
    Remove-Item -Path $InstallDir -Recurse -Force
    Write-Host "✓ Removed installation directory: $InstallDir" -ForegroundColor Green
}

Write-Host "Datara has been successfully uninstalled from your system." -ForegroundColor Green
