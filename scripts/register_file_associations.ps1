param(
    [string]$InstallDir = ""
)

if (-not $InstallDir) {
    $InstallDir = (Get-Item "$PSScriptRoot\..").FullName
}

$icoPath = Join-Path $InstallDir "assets\datara.ico"
$forgenPath = Join-Path $InstallDir "target\release\forgen.exe"
if (-not (Test-Path $forgenPath)) {
    $forgenPath = Join-Path $InstallDir "bin\forgen.exe"
}

if (-not (Test-Path $icoPath)) {
    Write-Warning "Icon not found at: $icoPath"
}

Write-Host "Registering .dtr file association with Datara icon in Windows Registry..." -ForegroundColor Cyan

# 1. HKCU:\Software\Classes\.dtr
$dtrKey = "HKCU:\Software\Classes\.dtr"
if (-not (Test-Path $dtrKey)) { New-Item -Path $dtrKey -Force | Out-Null }
Set-ItemProperty -Path $dtrKey -Name "(default)" -Value "DataraSourceFile"
Set-ItemProperty -Path $dtrKey -Name "Content Type" -Value "text/plain"
Set-ItemProperty -Path $dtrKey -Name "PerceivedType" -Value "text"

# 2. HKCU:\Software\Classes\DataraSourceFile
$progKey = "HKCU:\Software\Classes\DataraSourceFile"
if (-not (Test-Path $progKey)) { New-Item -Path $progKey -Force | Out-Null }
Set-ItemProperty -Path $progKey -Name "(default)" -Value "Datara Source File"
Set-ItemProperty -Path $progKey -Name "FriendlyTypeName" -Value "Datara Source File (.dtr)"

# 3. Default Icon
$iconKey = "$progKey\DefaultIcon"
if (-not (Test-Path $iconKey)) { New-Item -Path $iconKey -Force | Out-Null }
Set-ItemProperty -Path $iconKey -Name "(default)" -Value "$icoPath,0"

# 4. Shell actions
$openCmdKey = "$progKey\shell\open\command"
if (-not (Test-Path $openCmdKey)) { New-Item -Path $openCmdKey -Force | Out-Null }
Set-ItemProperty -Path $openCmdKey -Name "(default)" -Value "`"$forgenPath`" run `"%1`""

$editCmdKey = "$progKey\shell\edit\command"
if (-not (Test-Path $editCmdKey)) { New-Item -Path $editCmdKey -Force | Out-Null }
$codeExe = (Get-Command code -ErrorAction SilentlyContinue)
if ($codeExe) {
    Set-ItemProperty -Path $editCmdKey -Name "(default)" -Value "`"$($codeExe.Source)`" `"%1`""
} else {
    Set-ItemProperty -Path $editCmdKey -Name "(default)" -Value "notepad.exe `"%1`""
}

# 5. Notify Windows Shell of association change
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class ShellNotifier {
    [DllImport("Shell32.dll")]
    public static extern void SHChangeNotify(int eventId, int flags, IntPtr item1, IntPtr item2);
}
"@ -ErrorAction SilentlyContinue

[ShellNotifier]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)

Write-Host "File association successfully registered! .dtr files now display the official Datara icon." -ForegroundColor Green
