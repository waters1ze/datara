@echo off
REM ============================================================================
REM  Datara VS Code Extension Installer
REM  Installs syntax highlighting, language configuration, and icons for .dtr
REM ============================================================================
setlocal

set "EXT_SOURCE=%~dp0..\editors\vscode"
set "TARGET_DIR=%USERPROFILE%\.vscode\extensions\datara-language-0.1.0"

echo Installing Datara VS Code Extension...
echo Source: %EXT_SOURCE%
echo Target: %TARGET_DIR%

if not exist "%USERPROFILE%\.vscode\extensions" (
    mkdir "%USERPROFILE%\.vscode\extensions"
)

if exist "%TARGET_DIR%" (
    echo Removing previous version...
    rmdir /S /Q "%TARGET_DIR%"
)

mkdir "%TARGET_DIR%"
xcopy /E /I /Y "%EXT_SOURCE%" "%TARGET_DIR%" >nul

echo.
echo [OK] Datara Language Extension successfully installed for VS Code!
echo Restart or reload VS Code to enable syntax highlighting for all .dtr files.
pause
