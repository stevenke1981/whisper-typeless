# whisper-typeless release build
#
# Default: CUDA release build.
# Use -NoCuda for CPU-only, or -AllowCpuFallback to try CPU if CUDA fails.
#
# Usage:
#   .\build-release.ps1
#   .\build-release.ps1 -Clean
#   .\build-release.ps1 -NoCuda
#   .\build-release.ps1 -AllowCpuFallback

param(
    [switch]$NoCuda,
    [switch]$Clean,
    [switch]$AllowCpuFallback,
    [string]$Out = "dist"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ProjectRoot = $PSScriptRoot
$BinName = "whisper-typeless"
$DistDir = Join-Path $ProjectRoot $Out
$ReleaseExe = Join-Path $ProjectRoot "target\release\$BinName.exe"

function Invoke-ReleaseBuild {
    param(
        [string]$Label,
        [bool]$UseCuda
    )

    Write-Host "Building release [$Label]..." -ForegroundColor Cyan

    if ($UseCuda) {
        . (Join-Path $ProjectRoot "scripts\setup-env.ps1") -NoCuda:$false
        $args = @("build", "--release", "--features", "cuda")
    } else {
        . (Join-Path $ProjectRoot "scripts\setup-env.ps1") -NoCuda:$true
        $args = @("build", "--release", "--no-default-features")
    }

    Write-Host "cargo $($args -join ' ')" -ForegroundColor Gray
    & cargo @args
    return $LASTEXITCODE
}

function Get-FileSizeMb {
    param([string]$Path)
    $item = Get-Item -LiteralPath $Path
    return [math]::Round($item.Length / 1MB, 2)
}

Set-Location $ProjectRoot

if ($Clean) {
    Write-Host "Cleaning..." -ForegroundColor Yellow
    cargo clean
}

$label = "CUDA"
$exitCode = 0

if ($NoCuda) {
    $label = "CPU"
    $exitCode = Invoke-ReleaseBuild -Label "CPU-only" -UseCuda:$false
} else {
    $exitCode = Invoke-ReleaseBuild -Label "CUDA" -UseCuda:$true

    if (($exitCode -ne 0) -and $AllowCpuFallback) {
        Write-Host ""
        Write-Host "CUDA build failed. Trying CPU-only fallback..." -ForegroundColor Yellow
        $label = "CPU"
        $exitCode = Invoke-ReleaseBuild -Label "CPU-only fallback" -UseCuda:$false
    }
}

if ($exitCode -ne 0) {
    Write-Error "Release build failed."
    exit 1
}

if (-not (Test-Path -LiteralPath $ReleaseExe)) {
    Write-Error "Release binary not found: $ReleaseExe"
    exit 1
}

New-Item -ItemType Directory -Force -Path $DistDir | Out-Null
$dest = Join-Path $DistDir "$BinName.exe"
Copy-Item -LiteralPath $ReleaseExe -Destination $dest -Force

$catalog = Join-Path $ProjectRoot "model-catalog.json"
if (Test-Path -LiteralPath $catalog) {
    Copy-Item -LiteralPath $catalog -Destination (Join-Path $DistDir "model-catalog.json") -Force
}

$catalogDoc = Join-Path $ProjectRoot "MODEL_CATALOG.md"
if (Test-Path -LiteralPath $catalogDoc) {
    Copy-Item -LiteralPath $catalogDoc -Destination (Join-Path $DistDir "MODEL_CATALOG.md") -Force
}

$releaseSize = Get-FileSizeMb -Path $ReleaseExe
$distSize = Get-FileSizeMb -Path $dest

Write-Host ""
Write-Host "Build complete [$label]" -ForegroundColor Green
Write-Host "  Release : $ReleaseExe ($releaseSize MB)"
Write-Host "  Dist    : $dest ($distSize MB)"
Write-Host "  Profile : opt-level=z, lto=true, strip=symbols, panic=abort"
