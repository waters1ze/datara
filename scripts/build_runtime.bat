@echo off
rem Rebuild the Datara runtime object file.
rem
rem The runtime is plain C compiled with MSVC and linked into every Datara
rem executable by the Cranelift backend. It is checked in as a prebuilt .obj
rem because the build has no build.rs, so changing datara_runtime.c requires
rem re-running this script or the change will not reach any compiled program.
rem
rem Usage: scripts\build_runtime.bat

setlocal

set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
set "VCVARSALL="

if exist "%VSWHERE%" (
    for /f "usebackq tokens=*" %%i in (`"%VSWHERE%" -latest -products * -find "VC\Auxiliary\Build\vcvarsall.bat"`) do set "VCVARSALL=%%i"
)

if not defined VCVARSALL (
    if exist "%ProgramFiles(x86)%\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvarsall.bat" (
        set "VCVARSALL=%ProgramFiles(x86)%\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvarsall.bat"
    )
)

if not defined VCVARSALL (
    echo ERROR: could not locate vcvarsall.bat. Install MSVC build tools.
    exit /b 1
)

echo Using "%VCVARSALL%"
call "%VCVARSALL%" x64
if errorlevel 1 (
    echo ERROR: vcvarsall failed.
    exit /b 1
)

set "ROOT=%~dp0.."
cl /nologo /O2 /W3 /c /Fo:"%ROOT%\src\runtime\datara_runtime.obj" "%ROOT%\src\runtime\datara_runtime.c"
if errorlevel 1 (
    echo ERROR: compilation failed.
    exit /b 1
)

echo Runtime object rebuilt: %ROOT%\src\runtime\datara_runtime.obj
endlocal
