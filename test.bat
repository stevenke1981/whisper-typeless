@echo off
:: test.bat — Run the test suite
::
:: Options:
::   test.bat               — run all tests
::   test.bat vad           — run tests matching "vad"
::   test.bat coverage      — generate coverage report
::   test.bat nocuda        — CPU-only

setlocal

set "SCRIPT_DIR=%~dp0"
set "FLAGS="
set "FILTER="

:parse_args
if "%~1"=="" goto :run
if /i "%~1"=="coverage"  set FLAGS=%FLAGS% -Coverage
if /i "%~1"=="nocuda"    set FLAGS=%FLAGS% -NoCuda
if /i "%~1"=="verbose"   set FLAGS=%FLAGS% -Verbose
if /i "%~1"=="doc"       set FLAGS=%FLAGS% -Doc
if /i "%~1"=="failfast"  set FLAGS=%FLAGS% -FailFast
:: anything else is treated as a test filter
echo %~1 | findstr /i "coverage nocuda verbose doc failfast" >nul 2>&1
if errorlevel 1 (
    if "%FILTER%"=="" set FILTER=-Filter %~1
)
shift
goto :parse_args

:run
echo.
echo =============================================
echo  whisper-typeless Test Runner
echo =============================================
echo.

where powershell >nul 2>&1
if errorlevel 1 (
    echo ERROR: PowerShell not found.
    pause
    exit /b 1
)

powershell -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%scripts\test.ps1" %FLAGS% %FILTER%

set TEST_EXIT=%errorlevel%

echo.
if %TEST_EXIT%==0 (
    echo All tests passed.
) else (
    echo Tests FAILED.
)

echo.
pause
exit /b %TEST_EXIT%
