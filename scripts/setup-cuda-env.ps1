# CUDA build environment setup for whisper-typeless
# RTX 3060 Ti (sm_86), CUDA 12.6, VS 2026 / MSVC 14.50
#
# Usage: . .\scripts\setup-cuda-env.ps1   (dot-source to persist in current shell)

# ── GPU architecture ──────────────────────────────────────────────────────────
# Auto-detect via nvidia-smi; fall back to sm_86 (RTX 3060 Ti) if unavailable
$detected = $null
try {
    $cap = & nvidia-smi --query-gpu=compute_cap --format=csv,noheader 2>$null |
           Select-Object -First 1
    if ($cap -match '(\d+)\.(\d+)') {
        $detected = "$($Matches[1])$($Matches[2])"   # e.g. "8.6" → "86"
    }
} catch {}

$arch = if ($detected) { $detected } else { "86" }
$env:CMAKE_CUDA_ARCHITECTURES = $arch
Write-Host "CMAKE_CUDA_ARCHITECTURES = $arch"

# ── VS 2026 / MSVC 14.50+ compatibility ───────────────────────────────────────
# CUDA 12.6 only officially supports up to VS 2022.
# -allow-unsupported-compiler bypasses the version check.
# -Xcompiler=-fPIC is whisper-rs-sys's default and must be preserved here
# because this env var completely overrides the CMake default.
$env:CMAKE_CUDA_FLAGS = "-allow-unsupported-compiler -Xcompiler=-fPIC"
Write-Host "CMAKE_CUDA_FLAGS = $($env:CMAKE_CUDA_FLAGS)"

Write-Host "CUDA environment ready."
