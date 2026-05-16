# whisper-typeless release build
# 預設：嘗試 CUDA → 若失敗自動 fallback CPU
# 用法：
#   .\build-release.ps1              # 自動偵測 (CUDA → CPU fallback)
#   .\build-release.ps1 -NoCuda      # 強制 CPU-only
#   .\build-release.ps1 -Clean       # build 前先 cargo clean

param(
    [switch]$NoCuda,
    [switch]$Clean,
    [string]$Out = "dist"
)

$ErrorActionPreference = "Stop"
$BinName  = "whisper-typeless"
$DistDir  = Join-Path $PSScriptRoot $Out
$Src      = Join-Path $PSScriptRoot "target\release\$BinName.exe"

function Build([string]$Label, [string]$Features) {
    Write-Host "Building release ($Label)..." -ForegroundColor Cyan
    $args = @("build", "--release")
    if ($Features) {
        $args += "--features", $Features
    } else {
        $args += "--no-default-features"
    }
    & cargo @args
    return $LASTEXITCODE
}

if ($Clean) {
    Write-Host "Cleaning..." -ForegroundColor Yellow
    cargo clean
}

if ($NoCuda) {
    # ── 明確 CPU-only ────────────────────────────────────────
    $rc = Build "CPU-only" ""
    if ($rc -ne 0) { Write-Error "Build failed"; exit 1 }
    $label = "CPU"
} else {
    # ── 先嘗試 CUDA，失敗自動退回 CPU ────────────────────────
    Write-Host "Trying CUDA build first..." -ForegroundColor Cyan
    $rc = Build "CUDA" "cuda"
    if ($rc -ne 0) {
        Write-Host ""
        Write-Host "CUDA build failed (no NVIDIA GPU or CUDA toolkit not found)." -ForegroundColor Yellow
        Write-Host "Falling back to CPU-only build..." -ForegroundColor Yellow
        Write-Host ""
        $rc = Build "CPU-only" ""
        if ($rc -ne 0) { Write-Error "Build failed"; exit 1 }
        $label = "CPU"
    } else {
        $label = "CUDA"
    }
}

# ── 複製產出 ────────────────────────────────────────────────
New-Item -ItemType Directory -Force -Path $DistDir | Out-Null
$Dest = Join-Path $DistDir "$BinName.exe"
Copy-Item $Src $Dest -Force

$SizeMB = [math]::Round((Get-Item $Dest).Length / 1MB, 1)
Write-Host ""
Write-Host "Build complete [$label]: $Dest ($SizeMB MB)" -ForegroundColor Green
