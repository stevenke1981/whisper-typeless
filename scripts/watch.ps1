# watch.ps1 — Auto-rebuild on file changes (requires cargo-watch)
#
# Usage:
#   .\scripts\watch.ps1            # watch + rebuild on any change
#   .\scripts\watch.ps1 -Run       # watch + rebuild + re-run
#   .\scripts\watch.ps1 -Test      # watch + re-run tests
#   .\scripts\watch.ps1 -NoCuda

param(
    [switch]$Run,
    [switch]$Test,
    [switch]$NoCuda
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path $PSScriptRoot -Parent

Write-Host ""
Write-Host "=================================================" -ForegroundColor Cyan
Write-Host "  whisper-typeless  Watch Mode" -ForegroundColor Cyan
Write-Host "=================================================" -ForegroundColor Cyan

# Ensure cargo-watch is installed
$watch = Get-Command "cargo-watch" -ErrorAction SilentlyContinue
if (-not $watch) {
    Write-Host "Installing cargo-watch..." -ForegroundColor Yellow
    cargo install cargo-watch
}

. "$PSScriptRoot\setup-env.ps1" -NoCuda:$NoCuda

Set-Location $ProjectRoot

$featureFlags = if ($NoCuda) { "--no-default-features" } else { "--features cuda" }

if ($Test) {
    Write-Host "Watching for changes — running tests on rebuild..." -ForegroundColor Cyan
    Write-Host "Press Ctrl+C to stop." -ForegroundColor Gray
    Write-Host ""
    cargo watch -x "test $featureFlags"

} elseif ($Run) {
    Write-Host "Watching for changes — rebuilding and running..." -ForegroundColor Cyan
    Write-Host "Press Ctrl+C to stop." -ForegroundColor Gray
    Write-Host ""
    $env:RUST_LOG = "whisper_typeless=debug,slint=warn"
    cargo watch -x "run $featureFlags"

} else {
    Write-Host "Watching for changes — rebuilding on save..." -ForegroundColor Cyan
    Write-Host "Press Ctrl+C to stop." -ForegroundColor Gray
    Write-Host ""
    cargo watch -x "build $featureFlags"
}
