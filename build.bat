@echo off
:: build.bat — Quick build launcher (double-click or run from cmd)
::
:: Options passed through to build.ps1:
::   build.bat           — debug build
::   build.bat release   — release build
::   build.bat clean     — clean then debug build
::   build.bat nocuda    — CPU-only debug build

setlocal

set "SCRIPT_DIR=%~dp0"
set "FLAGS="

:: Parse simple positional args
:parse_args
if "%~1"=="" goto :run
if /i "%~1"=="release" set FLAGS=%FLAGS% -Release
if /i "%~1"=="clean"   set FLAGS=%FLAGS% -Clean
if /i "%~1"=="nocuda"  set FLAGS=%FLAGS% -NoCuda
if /i "%~1"=="verbose" set FLAGS=%FLAGS% -Verbose
if /i "%~1"=="package" set FLAGS=%FLAGS% -Package
if /i "%~1"=="timings" set FLAGS=%FLAGS% -Timings
shift
goto :parse_args

:run
echo.
echo =============================================
echo  whisper-typeless Build
echo =============================================
echo.

:: Check PowerShell is available
where powershell >nul 2>&1
if errorlevel 1 (
    echo ERROR: PowerShell not found. Please install PowerShell 5+.
    pause
    exit /b 1
)

powershell -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%scripts\build.ps1" %FLAGS%

set BUILD_EXIT=%errorlevel%

echo.
if %BUILD_EXIT%==0 (
    echo Build succeeded.
) else (
    echo Build FAILED with exit code %BUILD_EXIT%.
    echo.
    echo See error messages above. Common fixes:
    echo   - Install cmake   : winget install Kitware.CMake
    echo   - Install LLVM    : winget install LLVM.LLVM
    echo   - CPU-only build  : build.bat nocuda
)

echo.
pause
exit /b %BUILD_EXIT%
