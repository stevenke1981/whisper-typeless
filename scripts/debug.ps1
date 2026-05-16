# debug.ps1 — Run whisper-typeless in debug mode with verbose logging
#
# Usage:
#   .\scripts\debug.ps1                         # run with default debug settings
#   .\scripts\debug.ps1 -LogLevel trace          # maximum verbosity
#   .\scripts\debug.ps1 -NoBuild                 # skip rebuild, run existing binary
#   .\scripts\debug.ps1 -NoCuda                  # CPU-only debug run
#   .\scripts\debug.ps1 -LogFile                 # also write logs to file
#   .\scripts\debug.ps1 -WinDbg                  # launch under WinDbg

param(
    [ValidateSet("error","warn","info","debug","trace")]
    [string]$LogLevel = "debug",

    [switch]$NoBuild,
    [switch]$NoCuda,
    [switch]$LogFile,
    [switch]$WinDbg,
    [switch]$Perf,           # enable performance counters
    [switch]$NoColor,        # disable ANSI color in logs
    [string]$Model = "",     # override model path for quick test
    [string]$ExtraArgs = ""  # pass extra CLI args to the binary
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path $PSScriptRoot -Parent
$LogDir      = "$ProjectRoot\logs"
$BinaryPath  = "$ProjectRoot\target\debug\whisper-typeless.exe"

function Write-Header($msg) {
    Write-Host ""
    Write-Host ("=" * 50) -ForegroundColor Magenta
    Write-Host "  $msg" -ForegroundColor Magenta
    Write-Host ("=" * 50) -ForegroundColor Magenta
}

function Write-Step($msg) {
    Write-Host ""
    Write-Host ">> $msg" -ForegroundColor Yellow
}

# ── Banner ────────────────────────────────────────────────────────────
Write-Header "whisper-typeless Debug Session"
Write-Host "  Log level : $LogLevel"
Write-Host "  Binary    : $BinaryPath"
Write-Host "  Started   : $(Get-Date -Format 'HH:mm:ss')"

# ── Environment ───────────────────────────────────────────────────────
Write-Step "Setting debug environment"
. "$PSScriptRoot\setup-env.ps1" -NoCuda:$NoCuda

# ── Logging configuration ─────────────────────────────────────────────

# Module-level log filter:
#   whisper_typeless=debug  — our code at DEBUG
#   slint=warn              — suppress Slint internals
#   wgpu=error              — suppress GPU driver spam
$logFilter = "whisper_typeless=$LogLevel,slint=warn,wgpu=error,winit=warn"

$env:RUST_LOG          = $logFilter
$env:RUST_BACKTRACE    = "full"
$env:RUST_LOG_STYLE    = if ($NoColor) { "never" } else { "always" }

# Tokio console (async task inspector) — if tokio-console is installed
# $env:TOKIO_CONSOLE_BIND = "127.0.0.1:6669"

if ($Perf) {
    $env:WHISPER_PERF = "1"
    Write-Host "   Performance counters: ON" -ForegroundColor Cyan
}

Write-Host "   RUST_LOG = $env:RUST_LOG" -ForegroundColor Gray

# ── Log file setup ────────────────────────────────────────────────────
$logPath = $null
if ($LogFile) {
    New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
    $timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
    $logPath   = "$LogDir\debug_$timestamp.log"
    Write-Host "   Log file : $logPath" -ForegroundColor Gray
}

# ── Build (unless -NoBuild) ───────────────────────────────────────────
if (-not $NoBuild) {
    Write-Step "Building debug binary"

    $buildArgs = @()
    if ($NoCuda) { $buildArgs += "-NoCuda" }

    & "$PSScriptRoot\build.ps1" @buildArgs

    if ($LASTEXITCODE -ne 0) {
        Write-Host ""
        Write-Host "Build failed. Fix errors above then retry." -ForegroundColor Red
        exit 1
    }
} else {
    if (-not (Test-Path $BinaryPath)) {
        Write-Host "Binary not found at $BinaryPath — run without -NoBuild first." -ForegroundColor Red
        exit 1
    }
    Write-Step "Skipping build, using existing binary"
}

# ── Binary size / timestamp ───────────────────────────────────────────
$binInfo  = Get-Item $BinaryPath
$binSize  = [math]::Round($binInfo.Length / 1MB, 2)
$binMtime = $binInfo.LastWriteTime.ToString("yyyy-MM-dd HH:mm:ss")
Write-Host "   Binary   : $binSize MB  (built $binMtime)" -ForegroundColor Gray

# ── Extra CLI args ────────────────────────────────────────────────────
$runArgs = @()
if ($Model -ne "") {
    $runArgs += "--model", $Model
}
if ($ExtraArgs -ne "") {
    $runArgs += $ExtraArgs.Split(" ")
}

# ── Launch ────────────────────────────────────────────────────────────
Write-Header "Launching Application"
Write-Host ""

Set-Location $ProjectRoot

if ($WinDbg) {
    $windbg = Get-Command "windbg" -ErrorAction SilentlyContinue
    if (-not $windbg) {
        Write-Host "WinDbg not found. Install via: winget install Microsoft.WinDbg" -ForegroundColor Red
        exit 1
    }
    Write-Host "Launching under WinDbg..." -ForegroundColor Magenta
    & windbg -g $BinaryPath @runArgs
} elseif ($LogFile) {
    Write-Host "Output tee'd to: $logPath" -ForegroundColor Gray
    Write-Host "Press Ctrl+C to stop." -ForegroundColor Gray
    Write-Host ""
    & $BinaryPath @runArgs 2>&1 | Tee-Object -FilePath $logPath
} else {
    Write-Host "Press Ctrl+C to stop." -ForegroundColor Gray
    Write-Host ""
    & $BinaryPath @runArgs
}

# ── Exit summary ──────────────────────────────────────────────────────
$exitCode = $LASTEXITCODE
Write-Host ""

if ($exitCode -eq 0) {
    Write-Host "Process exited cleanly (0)." -ForegroundColor Green
} else {
    Write-Host "Process exited with code $exitCode." -ForegroundColor Red

    if ($env:RUST_BACKTRACE -eq "full") {
        Write-Host ""
        Write-Host "Backtrace was enabled — scroll up for the stack trace." -ForegroundColor Yellow
    }

    Write-Host ""
    Write-Host "Debug tips:" -ForegroundColor Cyan
    Write-Host "  Increase verbosity : -LogLevel trace" -ForegroundColor Gray
    Write-Host "  Save logs to file  : -LogFile" -ForegroundColor Gray
    Write-Host "  GPU issues         : -NoCuda" -ForegroundColor Gray
    Write-Host "  Native debugger    : -WinDbg" -ForegroundColor Gray
}

if ($LogFile -and $logPath) {
    Write-Host ""
    Write-Host "Log saved to: $logPath" -ForegroundColor Cyan
}
