@echo off
setlocal
cd /d "%~dp0"
title Datara Setup Wizard
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0DataraSetup.ps1"
endlocal
