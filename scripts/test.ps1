# test.ps1 — Run the test suite
#
# Usage:
#   .\scripts\test.ps1                    # run all tests
#   .\scripts\test.ps1 -Filter "vad"      # run tests matching pattern
#   .\scripts\test.ps1 -Coverage          # generate coverage report (requires cargo-llvm-cov)
#   .\scripts\test.ps1 -Doc               # also run doc tests
#   .\scripts\test.ps1 -NoCuda            # CPU-only

param(
    [string]$Filter   = "",
    [switch]$Coverage,
    [switch]$Doc,
    [switch]$NoCuda,
    [switch]$Verbose,
    [switch]$FailFast
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path $PSScriptRoot -Parent

function Write-Header($msg) {
    Write-Host ""
    Write-Host ("=" * 50) -ForegroundColor Cyan
    Write-Host "  $msg" -ForegroundColor Cyan
    Write-Host ("=" * 50) -ForegroundColor Cyan
}

Write-Header "whisper-typeless Test Runner"

# Environment
. "$PSScriptRoot\setup-env.ps1" -NoCuda:$NoCuda
$env:RUST_LOG       = "whisper_typeless=debug"
$env:RUST_BACKTRACE = "1"

Set-Location $ProjectRoot

$featureArgs = if ($NoCuda) { @("--no-default-features") } else { @("--features", "cuda") }
$featureDisplay = if ($NoCuda) { "cpu-only (--no-default-features)" } else { "cuda" }

# ── Coverage ─────────────────────────────────────────────────────────
if ($Coverage) {
    $cov = Get-Command "cargo-llvm-cov" -ErrorAction SilentlyContinue
    if (-not $cov) {
        Write-Host "Installing cargo-llvm-cov..." -ForegroundColor Yellow
        cargo install cargo-llvm-cov
    }

    Write-Host ""
    Write-Host "Running tests with coverage..." -ForegroundColor Cyan

    $coverageArgs = @("llvm-cov") + $featureArgs + @(
        "--lcov",
        "--output-path", "target\lcov.info"
    )

    if ($Filter) { $coverageArgs += "--test-threads=1"; $coverageArgs += $Filter }

    cargo @coverageArgs

    Write-Host ""
    Write-Host "Coverage report: target\lcov.info" -ForegroundColor Green

    # Also output a summary
    cargo llvm-cov report @featureArgs 2>&1 | Select-String "TOTAL"
    return
}

# ── Standard test run ─────────────────────────────────────────────────
$testArgs = @("test") + $featureArgs

if ($Filter)   { $testArgs += $Filter }
if ($Verbose)  { $testArgs += "--", "--nocapture" }
if ($FailFast) { $testArgs += "--", "-q" }
if ($Doc)      { $testArgs += "--doc" }

Write-Host ""
Write-Host "  features: $featureDisplay" -ForegroundColor Gray
Write-Host "  cargo $($testArgs -join ' ')" -ForegroundColor Gray
Write-Host ""

cargo @testArgs

$exitCode = $LASTEXITCODE

Write-Header "Tests $(if ($exitCode -eq 0) { 'PASSED' } else { 'FAILED' })"

if ($exitCode -ne 0) {
    Write-Host ""
    Write-Host "  Run specific test  : -Filter <name>" -ForegroundColor Yellow
    Write-Host "  Show stdout        : -Verbose" -ForegroundColor Yellow
    Write-Host "  Stop on first fail : -FailFast" -ForegroundColor Yellow
    exit 1
}

Write-Host ""
Write-Host "  All tests passed." -ForegroundColor Green

# Coverage reminder
if (-not $Coverage) {
    Write-Host "  Tip: run with -Coverage to check coverage >= 80%" -ForegroundColor Gray
}
Write-Host ""
