@echo off
setlocal
title Datara Language & Forgen Compiler Setup

echo ================================================================================
echo  Datara & Forgen Compiler — 1-Click Windows Setup
echo ================================================================================
echo.

for %%F in ("%~dp0dist\Datara-Setup.exe" "%~dp0dist\Datara*Setup*.exe" "%~dp0installer\Datara-Setup.exe") do (
    if exist "%%~fF" (
        start "" "%%~fF"
        exit /b 0
    )
)

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1"

if %ERRORLEVEL% EQU 0 (
    echo.
    echo ================================================================================
    echo  Setup completed successfully!
    echo ================================================================================
    echo  You can now launch Datara from your Desktop / Start Menu or run:
    echo    datara                 (Interactive REPL Console)
    echo    forgen setup-tools     (C/C++ Build Tools & Linker)
    echo    forgen run main.dtr    (Compile & run code)
    echo.
) else (
    echo.
    echo [ERROR] Installation failed. Please check the error message above.
)

pause
