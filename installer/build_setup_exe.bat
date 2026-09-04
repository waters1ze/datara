@echo off
setlocal
cd /d "%~dp0"
title Building Datara-Setup.exe

echo [1/3] Packaging installer payload...
powershell -NoProfile -ExecutionPolicy Bypass -Command "$p = 'payload'; if (Test-Path $p) { rm -r -fo $p }; md $p\bin,$p\assets | Out-Null; cp ..\target\release\forgen.exe $p\bin\ -fo; cp ..\target\release\datara.exe $p\bin\ -fo; cp ..\target\release\dpm.exe $p\bin\ -fo; cp ..\assets\datara.ico $p\assets\ -fo; cp ..\assets\datara-logo.png $p\assets\ -fo; cp ..\stdlib $p\ -r -fo; cp ..\runtime $p\ -r -fo; cp ..\scripts $p\ -r -fo; cp ..\editors $p\ -r -fo; if (Test-Path payload.zip) { rm payload.zip -fo }; Compress-Archive -Path $p\* -DestinationPath payload.zip -CompressionLevel Optimal; rm -r -fo $p"

echo [2/3] Compiling standalone Datara-Setup.exe...
C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe /nologo /target:winexe /win32icon:"..\assets\datara.ico" /resource:payload.zip,payload.zip /reference:System.dll,System.Windows.Forms.dll,System.Drawing.dll,System.IO.Compression.dll,System.IO.Compression.FileSystem.dll /out:"Datara-Setup.exe" SetupWizard.cs

echo [3/3] Cleaning up...
if exist payload.zip del /f /q payload.zip

if exist "Datara-Setup.exe" (
    echo.
    echo ================================================================================
    echo  Successfully generated Datara-Setup.exe!
    echo ================================================================================
) else (
    echo [ERROR] Failed to compile Datara-Setup.exe
)
