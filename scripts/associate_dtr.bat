@echo off
REM ============================================================================
REM  Datara (.dtr) Windows File Association and Icon Installer
REM ============================================================================
setlocal

set "DATARA_ROOT=%~dp0.."
set "FORGEN_EXE=%DATARA_ROOT%\target\release\forgen.exe"
set "ICON_PATH=%DATARA_ROOT%\assets\datara.ico"

echo Registering .dtr extension in Windows Registry (Current User)...

REM 1. Associate .dtr with Datara.SourceFileProgID
reg add "HKCU\Software\Classes\.dtr" /ve /d "Datara.SourceFile" /f >nul
reg add "HKCU\Software\Classes\.dtr" /v "Content Type" /d "text/plain" /f >nul
reg add "HKCU\Software\Classes\.dtr" /v "PerceivedType" /d "document" /f >nul

REM 2. Define Datara.SourceFile ProgID
reg add "HKCU\Software\Classes\Datara.SourceFile" /ve /d "Datara Source File" /f >nul
reg add "HKCU\Software\Classes\Datara.SourceFile\DefaultIcon" /ve /d "%ICON_PATH%" /f >nul

REM 3. Define Open and Run actions
if exist "%FORGEN_EXE%" (
    reg add "HKCU\Software\Classes\Datara.SourceFile\shell\run" /ve /d "Run with Forgen" /f >nul
    reg add "HKCU\Software\Classes\Datara.SourceFile\shell\run\command" /ve /d "\"%FORGEN_EXE%\" run \"%%1\"" /f >nul
)

echo.
echo [OK] .dtr files are now associated with Datara Icon and Forgen runtime!
echo Icon Path: %ICON_PATH%
echo Forgen Exe: %FORGEN_EXE%
pause
