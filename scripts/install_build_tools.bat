@echo off
setlocal
title Datara & Forgen ? Native C/C++ Build Tools & Linker Setup

echo ================================================================================
echo  Datara & Forgen Compiler ? Native C/C++ Build Tools Setup
echo ================================================================================
echo.
echo  Datara requires a C/C++ linker to produce native Windows (.exe) executables.
echo  This tool will check for an existing linker (MSVC, LLVM lld-link, MinGW)
echo  or automatically install Microsoft C++ Build Tools (Node.js style).
echo.

set "SCRIPT_DIR=%~dp0"
set "PS_SCRIPT=%SCRIPT_DIR%install_build_tools.ps1"
if not exist "%PS_SCRIPT%" set "PS_SCRIPT=%SCRIPT_DIR%..\scripts\install_build_tools.ps1"
if not exist "%PS_SCRIPT%" set "PS_SCRIPT=%SCRIPT_DIR%scripts\install_build_tools.ps1"
if not exist "%PS_SCRIPT%" (
    if defined DATARA_HOME set "PS_SCRIPT=%DATARA_HOME%\scripts\install_build_tools.ps1"
)

if exist "%PS_SCRIPT%" (
    powershell -NoProfile -ExecutionPolicy Bypass -File "%PS_SCRIPT%"
) else (
    echo Downloading and launching latest build tools setup...
    powershell -NoProfile -ExecutionPolicy Bypass -Command "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; $s = (New-Object Net.WebClient).DownloadString('https://raw.githubusercontent.com/waters1ze/datara/main/scripts/install_build_tools.ps1'); Invoke-Expression $s"
)

echo.
echo ================================================================================
echo  Setup check finished. Press any key to close this window.
echo ================================================================================
pause >nul
