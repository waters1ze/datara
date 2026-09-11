@echo off
setlocal
cd /d %~dp0
echo =======================================================
echo          Datara & Forgen Compiler Installer
echo =======================================================
echo.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File %~dp0install.ps1
if %ERRORLEVEL% NEQ 0 (
    echo.
    echo [ERROR] Installation failed with exit code %ERRORLEVEL%.
    pause
    exit /b %ERRORLEVEL%
)
echo.
echo [OK] Datara has been installed successfully!
pause
