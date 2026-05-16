@echo off
:: setup.bat — First-time setup: check prerequisites, optionally install tools,
::             and download a default model.
::
:: Run this once before building.

setlocal

set "SCRIPT_DIR=%~dp0"

echo.
echo =============================================
echo  whisper-typeless  First-Time Setup
echo =============================================
echo.

:: ── Step 1: Prerequisites ─────────────────────────────────────────────
echo [1/3] Checking prerequisites...
echo.
powershell -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%scripts\check-prerequisites.ps1"
set CHECK_EXIT=%errorlevel%

if %CHECK_EXIT% GTR 2 (
    echo.
    echo Some prerequisites are missing. Attempting auto-install via winget...
    echo.
    powershell -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%scripts\check-prerequisites.ps1" -Fix
    set CHECK_EXIT=%errorlevel%
)

echo.

:: ── Step 2: Download default model ───────────────────────────────────
echo [2/3] Download default model (small, 466 MB)?
echo.
echo   [Y] Yes, download small model   (recommended)
echo   [T] Download tiny  model instead (75 MB, faster, lower quality)
echo   [S] Skip download (I will download later)
echo.
set /p MODEL_CHOICE="Choice [Y/T/S]: "

if /i "%MODEL_CHOICE%"=="y" (
    echo.
    echo Downloading small model...
    powershell -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%scripts\download-model.ps1" -Model small
)
if /i "%MODEL_CHOICE%"=="t" (
    echo.
    echo Downloading tiny model...
    powershell -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%scripts\download-model.ps1" -Model tiny
)
if /i "%MODEL_CHOICE%"=="s" (
    echo Skipping model download.
    echo Run later: scripts\download-model.ps1 -ListModels
)

echo.

:: ── Step 3: Quick build test ──────────────────────────────────────────
echo [3/3] Run a test build now?
echo.
set /p BUILD_CHOICE="Build now? [Y/N]: "

if /i "%BUILD_CHOICE%"=="y" (
    echo.
    call "%SCRIPT_DIR%build.bat"
)

echo.
echo =============================================
echo  Setup complete!
echo =============================================
echo.
echo  Quick reference:
echo    build.bat          ^— debug build
echo    build.bat release  ^— release build
echo    debug.bat          ^— run with debug logging
echo    test.bat           ^— run all tests
echo    scripts\download-model.ps1 -ListModels
echo.
pause
