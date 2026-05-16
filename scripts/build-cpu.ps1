# Build whisper-typeless CPU-only for both Debug and Release
# Usage: .\scripts\build-cpu.ps1

param(
    [switch]$DebugOnly,
    [switch]$ReleaseOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Build([string]$Profile, [string]$Flag) {
    Write-Host "`n=== Building $Profile (CPU) ===" -ForegroundColor Cyan
    $args = @("build", "--no-default-features")
    if ($Flag) { $args += $Flag }
    & cargo @args
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Build failed (exit $LASTEXITCODE)" -ForegroundColor Red
        exit $LASTEXITCODE
    }
    $exe = if ($Flag -eq "--release") {
        "target\release\whisper-typeless.exe"
    } else {
        "target\debug\whisper-typeless.exe"
    }
    $size = [math]::Round((Get-Item $exe).Length / 1MB, 1)
    Write-Host "OK  $exe  ($size MB)" -ForegroundColor Green
}

if (-not $ReleaseOnly) { Build "Debug"   ""          }
if (-not $DebugOnly)   { Build "Release" "--release"  }

Write-Host "`nDone." -ForegroundColor Cyan
