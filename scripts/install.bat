@echo off
setlocal
echo =======================================================================
echo    Datara & Forgen Official 1-Click Windows Installer
echo =======================================================================
echo.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1"
echo.
pause
