# setup-env.ps1 — Configure environment variables for whisper-rs compilation
# Source this script: . .\scripts\setup-env.ps1

param(
    [switch]$NoCuda,   # force CPU-only build
    [switch]$NoCLang,  # skip LIBCLANG_PATH auto-detect
    [switch]$Verbose
)

# Never throw on missing commands — callers handle failures via return values
$ErrorActionPreference = "SilentlyContinue"

function Write-Step($msg) {
    Write-Host "  >> $msg" -ForegroundColor Cyan
}

function Write-Ok($msg) {
    Write-Host "  OK $msg" -ForegroundColor Green
}

function Write-Warn($msg) {
    Write-Host "WARN $msg" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "Setting up build environment..." -ForegroundColor Cyan

# ── LIBCLANG_PATH ─────────────────────────────────────────────────────
if (-not $NoCLang) {
    $candidates = @(
        $env:LIBCLANG_PATH,
        "C:\Program Files\LLVM\bin",
        "C:\LLVM\bin",
        "${env:ProgramFiles}\LLVM\bin"
    )

    $llvmPath = $candidates | Where-Object {
        $_ -and (Test-Path "$_\libclang.dll")
    } | Select-Object -First 1

    if ($llvmPath) {
        $env:LIBCLANG_PATH = $llvmPath
        Write-Ok "LIBCLANG_PATH = $llvmPath"
    } else {
        Write-Warn "libclang.dll not found. Install LLVM or set LIBCLANG_PATH manually."
    }
}

# ── CUDA (forced ON unless -NoCuda) ──────────────────────────────────
if ($NoCuda) {
    $env:WHISPER_CUDA = "0"
    Write-Step "CUDA disabled by -NoCuda flag"
} else {
    # Locate CUDA toolkit
    $cudaHome = $env:CUDA_PATH
    if (-not $cudaHome) {
        # Scan common install paths
        $cudaCandidates = @(
            "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.6",
            "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.4",
            "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.3",
            "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.2",
            "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.0",
            "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v11.8"
        )
        $cudaHome = $cudaCandidates | Where-Object { Test-Path "$_\bin\nvcc.exe" } | Select-Object -First 1
    }

    if ($cudaHome -and (Test-Path "$cudaHome\bin\nvcc.exe")) {
        $env:CUDA_PATH    = $cudaHome
        $env:WHISPER_CUDA = "1"

        # Add CUDA bin to PATH so cmake FindCUDA works
        if ($env:PATH -notlike "*$cudaHome\bin*") {
            $env:PATH = "$cudaHome\bin;$cudaHome\libnvvp;$env:PATH"
        }

        # Set CUDA lib path for the linker
        $env:CUDA_LIB_PATH = "$cudaHome\lib\x64"

        $nvccVer = & "$cudaHome\bin\nvcc.exe" --version 2>&1 |
                   Select-String "release" | ForEach-Object { $_.Line.Trim() }
        Write-Ok "CUDA      : $cudaHome"
        Write-Ok "nvcc      : $nvccVer"
        Write-Ok "CUDA_LIB  : $env:CUDA_LIB_PATH"

        # ── CUDA architecture detection ──────────────────────────────
        # CMake 4.x defaults CUDA_ARCHITECTURES to "native" which requires
        # the GPU to be accessible at configure time (fails in RDP/CI).
        # Use nvidia-smi to detect the real compute capability and set it
        # explicitly so cmake does not need to probe the GPU itself.
        if (-not $env:CUDAARCHS) {
            $nvidiaSmi = Get-Command "nvidia-smi" -ErrorAction SilentlyContinue
            if ($nvidiaSmi) {
                $cap = & nvidia-smi --query-gpu=compute_cap --format=csv,noheader 2>&1 |
                       Select-Object -First 1 |
                       ForEach-Object { ($_.Trim() -replace "\.", "") }   # "8.6" -> "86"
                if ($cap -match "^\d{2,3}$") {
                    $env:CUDAARCHS = $cap
                    Write-Ok "CUDAARCHS : $cap (auto-detected via nvidia-smi)"
                } else {
                    $env:CUDAARCHS = "86"
                    Write-Warn "CUDAARCHS : could not parse compute_cap, defaulting to 86 (Ampere)"
                }
            } else {
                $env:CUDAARCHS = "86"
                Write-Warn "CUDAARCHS : nvidia-smi not in PATH, defaulting to 86 (Ampere)"
            }
        } else {
            Write-Ok "CUDAARCHS : $env:CUDAARCHS (from environment)"
        }

        # CMake also reads CMAKE_CUDA_ARCHITECTURES from environment in cmake >= 3.23
        $env:CMAKE_CUDA_ARCHITECTURES = $env:CUDAARCHS

        # ── MSVC version compatibility ───────────────────────────────
        # CUDA 12.6 officially supports VS 2017–2022 only.
        # VS 2026 (MSVC 14.50 / VS 18) triggers a hard error in nvcc.
        # -allow-unsupported-compiler suppresses the version check.
        # whisper-rs-sys build.rs passes all CMAKE_* env vars to cmake,
        # so setting CMAKE_CUDA_FLAGS here overrides the default value.
        $env:CMAKE_CUDA_FLAGS = "-allow-unsupported-compiler -Xcompiler=-fPIC"
        Write-Ok "CUDA flags: -allow-unsupported-compiler (VS 2026+ compat)"

    } else {
        Write-Host ""
        Write-Host " FAIL CUDA toolkit not found." -ForegroundColor Red
        Write-Host "      Expected: C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.x" -ForegroundColor Yellow
        Write-Host "      Set CUDA_PATH manually, or use -NoCuda to build CPU-only." -ForegroundColor Yellow
        Write-Host ""
        exit 1
    }
}

# ── Rust flags ────────────────────────────────────────────────────────
# Speed up incremental builds on Windows
$env:CARGO_INCREMENTAL = "1"
$env:RUST_BACKTRACE    = "1"

# Use lld linker if available (much faster on Windows)
$lld = Get-Command "lld-link" -ErrorAction SilentlyContinue
if ($lld) {
    $env:RUSTFLAGS = "-C link-arg=-fuse-ld=lld"
    Write-Ok "lld linker enabled (faster link times)"
}

# ── OpenSSL / TLS (for reqwest) ───────────────────────────────────────
# reqwest uses rustls by default in our Cargo.toml — no OpenSSL needed

# ── PATH additions ────────────────────────────────────────────────────
$pathAdditions = @()

if ($env:LIBCLANG_PATH -and ($env:PATH -notlike "*$env:LIBCLANG_PATH*")) {
    $pathAdditions += $env:LIBCLANG_PATH
}

if ($pathAdditions) {
    $env:PATH = ($pathAdditions + $env:PATH) -join ";"
    if ($Verbose) {
        Write-Step "PATH updated with: $($pathAdditions -join ', ')"
    }
}

# ── CMake ─────────────────────────────────────────────────────────────
$cmake = Get-Command cmake -ErrorAction SilentlyContinue
if ($cmake) {
    Write-Ok "cmake = $($cmake.Source)"
} else {
    Write-Warn "cmake not found — add CMake to PATH"
}

Write-Host ""
Write-Host "Environment ready." -ForegroundColor Green
Write-Host ""

# Export summary for caller
$global:BuildEnv = @{
    CUDA    = $env:WHISPER_CUDA -eq "1"
    LibClang = $env:LIBCLANG_PATH
    LLD     = $null -ne $lld
}
