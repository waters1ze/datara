@echo off
setlocal
title Datara Language & Forgen Compiler Setup

echo ================================================================================
echo  Datara & Forgen Compiler — 1-Click Windows Setup
echo ================================================================================
echo.

if exist "%~dp0dist\Datara-v0.1.0-Setup.exe" (
    start "" "%~dp0dist\Datara-v0.1.0-Setup.exe"
    exit /b 0
) else if exist "%~dp0installer\Datara-Setup.exe" (
    start "" "%~dp0installer\Datara-Setup.exe"
    exit /b 0
)

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1"

if %ERRORLEVEL% EQU 0 (
    echo.
    echo ================================================================================
    echo  Setup completed successfully!
    echo ================================================================================
    echo  You can now open a new Command Prompt or PowerShell and run:
    echo    forgen --help
    echo    forgen repl
    echo.
) else (
    echo.
    echo [ERROR] Installation failed. Please check the error message above.
)

pause
