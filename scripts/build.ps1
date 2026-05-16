# build.ps1 — Build whisper-typeless for Windows
#
# Usage:
#   .\scripts\build.ps1              # debug build
#   .\scripts\build.ps1 -Release     # optimized release build
#   .\scripts\build.ps1 -NoCuda      # CPU-only
#   .\scripts\build.ps1 -Clean       # cargo clean first
#   .\scripts\build.ps1 -Release -Package   # build + create installer zip

param(
    [switch]$Release,
    [switch]$NoCuda,
    [switch]$Clean,
    [switch]$Package,
    [switch]$Verbose,
    [switch]$Timings    # show per-crate compile times
)

Set-StrictMode -Version Latest
# "Continue" so missing tools report via our own error messages, not PS exceptions
$ErrorActionPreference = "Continue"
$startTime = Get-Date

$ProjectRoot = Split-Path $PSScriptRoot -Parent

function Write-Header($msg) {
    Write-Host ""
    Write-Host ("=" * 50) -ForegroundColor Cyan
    Write-Host "  $msg" -ForegroundColor Cyan
    Write-Host ("=" * 50) -ForegroundColor Cyan
}

function Write-Step($msg) {
    Write-Host ""
    Write-Host ">> $msg" -ForegroundColor Yellow
}

function Write-Ok($msg) {
    Write-Host "   OK  $msg" -ForegroundColor Green
}

function Write-Fail($msg) {
    Write-Host " FAIL  $msg" -ForegroundColor Red
}

function Write-Warn($msg) {
    Write-Host " WARN  $msg" -ForegroundColor Yellow
}

# ── Banner ────────────────────────────────────────────────────────────
Write-Header "whisper-typeless Build Script"
Write-Host "  Mode    : $(if ($Release) { 'RELEASE' } else { 'DEBUG' })"
Write-Host "  CUDA    : $(if ($NoCuda) { 'disabled (emergency fallback)' } else { 'enabled (RTX/NVIDIA required)' })"
Write-Host "  Root    : $ProjectRoot"
Write-Host "  Started : $(Get-Date -Format 'HH:mm:ss')"

# ── Inject common tool paths so Explorer-launched .bat sessions work ──
$toolPaths = @(
    "$env:USERPROFILE\.cargo\bin",          # rustc / cargo
    "C:\Program Files\CMake\bin",           # cmake
    "C:\Program Files\Git\bin",             # git
    "C:\Program Files\Git\cmd",             # git (alternate)
    "C:\Program Files\LLVM\bin"             # clang / libclang
)
foreach ($p in $toolPaths) {
    if ((Test-Path $p) -and ($env:PATH -notlike "*$p*")) {
        $env:PATH = "$p;$env:PATH"
    }
}

# ── Setup environment ─────────────────────────────────────────────────
Write-Step "Configuring build environment"
. "$PSScriptRoot\setup-env.ps1" -NoCuda:$NoCuda -Verbose:$Verbose

# ── Prerequisites check ───────────────────────────────────────────────
Write-Step "Checking prerequisites"
& "$PSScriptRoot\check-prerequisites.ps1" -NoCuda:$NoCuda
$checkExit = $LASTEXITCODE
# exit codes: 0 = all good, 1-2 = warnings only, 3+ = hard failures
if ($checkExit -ge 3) {
    Write-Fail "Critical prerequisites missing. Run: .\scripts\check-prerequisites.ps1 -Fix"
    exit 1
}

# ── Change to project root ────────────────────────────────────────────
Set-Location $ProjectRoot

# ── Clean ─────────────────────────────────────────────────────────────
if ($Clean) {
    Write-Step "Cleaning previous build artifacts"
    cargo clean
    Write-Ok "Clean complete"
}

# ── Build arguments ───────────────────────────────────────────────────
$cargoArgs = @("build")

if ($Release) {
    $cargoArgs += "--release"
}

if ($NoCuda) {
    $cargoArgs += "--no-default-features"
} else {
    $cargoArgs += "--features", "cuda"
}

if ($Timings) {
    $cargoArgs += "--timings"
}

if ($Verbose) {
    $cargoArgs += "--verbose"
}

# ── Run build ─────────────────────────────────────────────────────────
$featDisplay = if ($NoCuda) { "cpu-only (--no-default-features)" } else { "cuda" }
Write-Step "Building (features: $featDisplay)"
Write-Host "  cargo $($cargoArgs -join ' ')" -ForegroundColor Gray
Write-Host ""

$buildOutput = $null
$buildSuccess = $true

try {
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) {
        $buildSuccess = $false
    }
} catch {
    $buildSuccess = $false
    Write-Fail "Build threw an exception: $_"
}

# ── Result ────────────────────────────────────────────────────────────
$elapsed = (Get-Date) - $startTime
$elapsedStr = "{0:mm\:ss}" -f $elapsed

Write-Header "Build $(if ($buildSuccess) { 'SUCCEEDED' } else { 'FAILED' })"

if (-not $buildSuccess) {
    Write-Host ""
    Write-Host "Common fixes:" -ForegroundColor Yellow
    Write-Host "  - Missing cmake     : winget install Kitware.CMake" -ForegroundColor Gray
    Write-Host "  - Missing LLVM      : winget install LLVM.LLVM" -ForegroundColor Gray
    Write-Host "  - LIBCLANG_PATH     : set LIBCLANG_PATH=C:\Program Files\LLVM\bin" -ForegroundColor Gray
    Write-Host "  - CUDA errors       : build with -NoCuda flag" -ForegroundColor Gray
    Write-Host "  - Slint errors      : check ui/ .slint files syntax" -ForegroundColor Gray
    Write-Host ""
    exit 1
}

# ── Binary info ───────────────────────────────────────────────────────
$profile   = if ($Release) { "release" } else { "debug" }
$exePath   = "$ProjectRoot\target\$profile\whisper-typeless.exe"

if (Test-Path $exePath) {
    $exeInfo = Get-Item $exePath
    $sizeMB  = [math]::Round($exeInfo.Length / 1MB, 2)
    Write-Ok "Binary  : $exePath"
    Write-Ok "Size    : $sizeMB MB"
    Write-Ok "Elapsed : $elapsedStr"
} else {
    Write-Warn "Binary not found at expected path: $exePath"
}

# ── Package ───────────────────────────────────────────────────────────
if ($Package -and $Release) {
    Write-Step "Packaging release"

    $distDir = "$ProjectRoot\dist"
    New-Item -ItemType Directory -Force -Path $distDir | Out-Null

    $zipName = "whisper-typeless-windows-x64.zip"
    $zipPath = "$distDir\$zipName"

    $filesToPack = @(
        $exePath,
        "$ProjectRoot\README.md"
    )

    # Include templates directory if exists
    if (Test-Path "$ProjectRoot\templates") {
        $filesToPack += "$ProjectRoot\templates"
    }

    Compress-Archive -Path $filesToPack -DestinationPath $zipPath -Force
    Write-Ok "Package : $zipPath"
}

Write-Host ""
