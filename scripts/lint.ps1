# lint.ps1 — Run Clippy lints and rustfmt check
#
# Usage:
#   .\scripts\lint.ps1          # clippy + fmt check
#   .\scripts\lint.ps1 -Fix     # auto-fix what can be fixed
#   .\scripts\lint.ps1 -NoCuda

param(
    [switch]$Fix,
    [switch]$NoCuda,
    [switch]$Strict    # enable pedantic lints
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

Write-Header "whisper-typeless Lint Check"

. "$PSScriptRoot\setup-env.ps1" -NoCuda:$NoCuda

Set-Location $ProjectRoot

$featureArgs = if ($NoCuda) { @("--no-default-features") } else { @("--features", "cuda") }
$featureDisplay = if ($NoCuda) { "cpu-only (--no-default-features)" } else { "cuda" }
$issues   = 0

# ── rustfmt ───────────────────────────────────────────────────────────
Write-Host ""
Write-Host ">> rustfmt" -ForegroundColor Yellow

if ($Fix) {
    cargo fmt
    Write-Host "   Formatting applied." -ForegroundColor Green
} else {
    cargo fmt --check
    if ($LASTEXITCODE -ne 0) {
        Write-Host "   Format issues found. Run with -Fix to auto-format." -ForegroundColor Red
        $issues++
    } else {
        Write-Host "   OK" -ForegroundColor Green
    }
}

# ── Clippy ────────────────────────────────────────────────────────────
Write-Host ""
Write-Host ">> clippy" -ForegroundColor Yellow
Write-Host "   features: $featureDisplay" -ForegroundColor Gray

$clippyArgs = @("clippy") + $featureArgs + @("--", "-D", "warnings")

if ($Strict) {
    $clippyArgs += "-W", "clippy::pedantic"
    $clippyArgs += "-W", "clippy::nursery"
}

if ($Fix) {
    $clippyArgs = @("clippy", "--fix") + $featureArgs + @("--allow-staged")
}

cargo @clippyArgs

if ($LASTEXITCODE -ne 0) {
    $issues++
}

# ── Summary ───────────────────────────────────────────────────────────
Write-Header "Lint $(if ($issues -eq 0) { 'PASSED' } else { 'FAILED' })"

if ($issues -gt 0) {
    Write-Host "  $issues check(s) failed." -ForegroundColor Red
    Write-Host "  Run with -Fix to auto-correct formatting." -ForegroundColor Yellow
    exit 1
} else {
    Write-Host "  All clean." -ForegroundColor Green
}
Write-Host ""
