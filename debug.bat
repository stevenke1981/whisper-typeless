@echo off
:: debug.bat — Run in debug mode with full logging
::
:: Options:
::   debug.bat            — build + run with debug logging
::   debug.bat nobuild    — skip build, run existing binary
::   debug.bat trace      — maximum verbosity (trace level)
::   debug.bat nocuda     — CPU-only
::   debug.bat logfile    — also save logs to logs\debug_*.log

setlocal

set "SCRIPT_DIR=%~dp0"
set "FLAGS="

:parse_args
if "%~1"=="" goto :run
if /i "%~1"=="nobuild"  set FLAGS=%FLAGS% -NoBuild
if /i "%~1"=="trace"    set FLAGS=%FLAGS% -LogLevel trace
if /i "%~1"=="info"     set FLAGS=%FLAGS% -LogLevel info
if /i "%~1"=="warn"     set FLAGS=%FLAGS% -LogLevel warn
if /i "%~1"=="nocuda"   set FLAGS=%FLAGS% -NoCuda
if /i "%~1"=="logfile"  set FLAGS=%FLAGS% -LogFile
if /i "%~1"=="windbg"   set FLAGS=%FLAGS% -WinDbg
if /i "%~1"=="perf"     set FLAGS=%FLAGS% -Perf
shift
goto :parse_args

:run
echo.
echo =============================================
echo  whisper-typeless Debug Session
echo =============================================
echo.
echo Tip: debug.bat trace logfile   ^(maximum detail^)
echo      debug.bat nobuild         ^(skip rebuild^)
echo.

where powershell >nul 2>&1
if errorlevel 1 (
    echo ERROR: PowerShell not found.
    pause
    exit /b 1
)

powershell -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%scripts\debug.ps1" %FLAGS%

set EXIT_CODE=%errorlevel%

echo.
if %EXIT_CODE%==0 (
    echo Session ended cleanly.
) else (
    echo Session exited with code %EXIT_CODE%.
)

echo.
pause
exit /b %EXIT_CODE%
