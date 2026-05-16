# check-prerequisites.ps1 — Verify all build dependencies are present
# Compatible with PowerShell 5.1 and PowerShell 7+

param(
    [switch]$Fix,    # attempt to install missing tools via winget
    [switch]$NoCuda  # skip CUDA as a required prerequisite
)

# SilentlyContinue so missing commands don't throw; we handle failures manually
$ErrorActionPreference = "SilentlyContinue"

$pass = "[  OK  ]"
$fail = "[ FAIL ]"
$warn = "[ WARN ]"
$info = "[ INFO ]"

$issues = 0

# ── Helpers ───────────────────────────────────────────────────────────

# Find an executable: first check PATH, then probe common install dirs
function Find-Exe {
    param([string]$Name, [string[]]$CommonPaths = @())

    $cmd = Get-Command $Name -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }

    foreach ($p in $CommonPaths) {
        $full = Join-Path $p "$Name.exe"
        if (Test-Path $full) { return $full }
    }
    return $null
}

function Get-ToolVersion {
    param([string]$ExePath, [string[]]$Arguments, [string]$Pattern)
    try {
        $out = & $ExePath @Arguments 2>&1 | Out-String
        if ($out -match $Pattern) { return $Matches[0] }
    } catch {}
    return $null
}

# GPU info — works on both PS5 (Get-WmiObject) and PS7 (Get-CimInstance)
function Get-NvidiaGpuName {
    try {
        if (Get-Command Get-CimInstance -ErrorAction SilentlyContinue) {
            return Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue |
                   Where-Object { $_.Name -match "NVIDIA" } |
                   Select-Object -First 1 -ExpandProperty Name
        } else {
            return Get-WmiObject Win32_VideoController -ErrorAction SilentlyContinue |
                   Where-Object { $_.Name -match "NVIDIA" } |
                   Select-Object -First 1 -ExpandProperty Name
        }
    } catch {
        return $null
    }
}

# ── Banner ────────────────────────────────────────────────────────────
Write-Host ""
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host "  whisper-typeless - Prerequisites Check" -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host ""

# ── Rust ──────────────────────────────────────────────────────────────
Write-Host "-- Rust Toolchain --------------------------------------"

$rustupDir  = "$env:USERPROFILE\.cargo\bin"
$rustcPaths = @($rustupDir, "C:\Users\$env:USERNAME\.cargo\bin")

$rustcExe = Find-Exe "rustc" $rustcPaths
$cargoExe = Find-Exe "cargo" $rustcPaths

if ($rustcExe) {
    $rustVer = Get-ToolVersion $rustcExe @("--version") "rustc [\d\.]+"
    Write-Host "$pass rustc   : $rustVer  ($rustcExe)"

    # Add cargo bin to PATH for this session if it wasn't already there
    if ($env:PATH -notlike "*$rustupDir*") {
        $env:PATH = "$rustupDir;$env:PATH"
    }

    # MSRV check (only when rustc is available)
    $minorStr = ($rustVer -replace "rustc 1\.(\d+)\..*", '$1')
    if ($minorStr -match "^\d+$" -and [int]$minorStr -lt 78) {
        Write-Host "$warn   Rust 1.78+ required. Run: rustup update" -ForegroundColor Yellow
    }
} else {
    Write-Host "$fail rustc   : NOT FOUND" -ForegroundColor Red
    Write-Host "       Install from: https://rustup.rs" -ForegroundColor Yellow
    Write-Host "       After install, open a NEW terminal window." -ForegroundColor Yellow
    $issues++
}

if ($cargoExe) {
    $cargoVer = Get-ToolVersion $cargoExe @("--version") "cargo [\d\.]+"
    Write-Host "$pass cargo   : $cargoVer"
} else {
    Write-Host "$fail cargo   : NOT FOUND" -ForegroundColor Red
    if (-not $rustcExe) {
        Write-Host "       (Install rustc first — cargo is bundled with it)" -ForegroundColor Gray
    }
    $issues++
}

Write-Host ""

# ── Build Tools ───────────────────────────────────────────────────────
Write-Host "-- Build Tools -----------------------------------------"

# CMake
$cmakePaths = @(
    "C:\Program Files\CMake\bin",
    "C:\Program Files (x86)\CMake\bin",
    "$env:ProgramFiles\CMake\bin"
)
$cmakeExe = Find-Exe "cmake" $cmakePaths

if ($cmakeExe) {
    $cmakeVer = Get-ToolVersion $cmakeExe @("--version") "cmake version [\d\.]+"
    Write-Host "$pass cmake   : $cmakeVer  ($cmakeExe)"
    if ($env:PATH -notlike "*$(Split-Path $cmakeExe)*") {
        $env:PATH = "$(Split-Path $cmakeExe);$env:PATH"
    }
} else {
    Write-Host "$fail cmake   : NOT FOUND" -ForegroundColor Red
    Write-Host "       Install: winget install Kitware.CMake" -ForegroundColor Yellow
    if ($Fix) { winget install --id Kitware.CMake -e --accept-package-agreements }
    $issues++
}

# LLVM / Clang
$llvmPaths = @(
    "C:\Program Files\LLVM\bin",
    "C:\LLVM\bin",
    "$env:ProgramFiles\LLVM\bin"
)
$clangExe = Find-Exe "clang" $llvmPaths

if ($clangExe) {
    $clangVer = Get-ToolVersion $clangExe @("--version") "clang version [\d\.]+"
    Write-Host "$pass clang   : $clangVer"
} else {
    # clang not in PATH but libclang.dll may still exist
    $libclang = $llvmPaths | Where-Object { Test-Path "$_\libclang.dll" } | Select-Object -First 1
    if ($libclang) {
        Write-Host "$warn clang   : not in PATH, but libclang.dll found at $libclang" -ForegroundColor Yellow
        Write-Host "       bindgen will work; add $libclang to PATH for full LLVM tooling." -ForegroundColor Gray
    } else {
        Write-Host "$fail clang   : NOT FOUND (required for whisper-rs bindgen)" -ForegroundColor Red
        Write-Host "       Install: winget install LLVM.LLVM" -ForegroundColor Yellow
        if ($Fix) { winget install --id LLVM.LLVM -e --accept-package-agreements }
        $issues++
    }
}

# MSVC
$vsPaths = @(
    "$env:ProgramFiles\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC",
    "$env:ProgramFiles\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC",
    "$env:ProgramFiles\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC",
    "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC"
)
$msvcFound = Find-Exe "cl" @() # check PATH first
$vsDir     = $vsPaths | Where-Object { Test-Path $_ } | Select-Object -First 1

if ($msvcFound) {
    Write-Host "$pass MSVC    : cl.exe in PATH"
} elseif ($vsDir) {
    Write-Host "$pass MSVC    : Visual Studio Build Tools found"
    Write-Host "       $vsDir" -ForegroundColor Gray
    Write-Host "       Tip: run scripts from 'Developer PowerShell for VS 2022' for full PATH." -ForegroundColor Gray
} else {
    Write-Host "$warn MSVC    : Visual Studio Build Tools not found" -ForegroundColor Yellow
    Write-Host "       Install: winget install Microsoft.VisualStudio.2022.BuildTools" -ForegroundColor Yellow
    if ($Fix) { winget install --id Microsoft.VisualStudio.2022.BuildTools -e --accept-package-agreements }
}

# Git
$gitPaths = @(
    "C:\Program Files\Git\bin",
    "C:\Program Files\Git\cmd",
    "$env:ProgramFiles\Git\bin"
)
$gitExe = Find-Exe "git" $gitPaths

if ($gitExe) {
    $gitVer = Get-ToolVersion $gitExe @("--version") "git version [\d\.]+"
    Write-Host "$pass git     : $gitVer"
} else {
    Write-Host "$fail git     : NOT FOUND" -ForegroundColor Red
    Write-Host "       Install: winget install Git.Git" -ForegroundColor Yellow
    if ($Fix) { winget install --id Git.Git -e --accept-package-agreements }
    $issues++
}

Write-Host ""

# ── CUDA ──────────────────────────────────────────────────────────────
Write-Host "-- GPU / CUDA $(if ($NoCuda) { '(optional for CPU-only)' } else { '(required)' }) ---------------------------"

$cudaHome = $env:CUDA_PATH
if (-not $cudaHome) {
    $cudaCandidates = @(
        "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.6",
        "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.5",
        "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.4",
        "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.3",
        "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.2",
        "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.0",
        "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v11.8"
    )
    $cudaHome = $cudaCandidates | Where-Object { Test-Path "$_\bin\nvcc.exe" } | Select-Object -First 1
}

if ($cudaHome -and (Test-Path "$cudaHome\bin\nvcc.exe")) {
    $nvccVer = & "$cudaHome\bin\nvcc.exe" --version 2>&1 |
               Select-String "release" |
               ForEach-Object { ($_.Line -split ",")[1].Trim() }
    Write-Host "$pass CUDA    : $cudaHome"
    Write-Host "$pass nvcc    : $nvccVer"
    $env:CUDA_PATH    = $cudaHome
    $env:WHISPER_CUDA = "1"
} else {
    if ($NoCuda) {
        Write-Host "$info CUDA    : skipped for CPU-only build" -ForegroundColor Gray
    } else {
        Write-Host "$fail CUDA    : NOT FOUND" -ForegroundColor Red
        Write-Host "       Install: https://developer.nvidia.com/cuda-downloads" -ForegroundColor Yellow
        Write-Host "       CPU-only fallback: .\scripts\build.ps1 -NoCuda" -ForegroundColor Yellow
        $issues++
    }
}

# GPU detection — Get-CimInstance (PS7) with fallback to Get-WmiObject (PS5)
$gpuName = Get-NvidiaGpuName
if ($gpuName) {
    Write-Host "$pass GPU     : $gpuName"
} else {
    Write-Host "$info GPU     : No NVIDIA GPU detected (or WMI unavailable)" -ForegroundColor Gray
}

Write-Host ""

# ── Environment Variables ─────────────────────────────────────────────
Write-Host "-- Environment Variables --------------------------------"

$llvmBin = $env:LIBCLANG_PATH
if (-not $llvmBin) {
    $llvmBin = $llvmPaths | Where-Object { Test-Path "$_\libclang.dll" } | Select-Object -First 1
}

if ($llvmBin -and (Test-Path "$llvmBin\libclang.dll")) {
    $env:LIBCLANG_PATH = $llvmBin
    Write-Host "$pass LIBCLANG_PATH : $llvmBin"
} else {
    Write-Host "$fail LIBCLANG_PATH : libclang.dll not found" -ForegroundColor Red
    Write-Host "       Install LLVM: winget install LLVM.LLVM" -ForegroundColor Yellow
    $issues++
}

Write-Host ""

# ── PowerShell version ────────────────────────────────────────────────
$psVer = $PSVersionTable.PSVersion
Write-Host "$info PowerShell : $($psVer.Major).$($psVer.Minor) ($($PSVersionTable.PSEdition))"

Write-Host ""

# ── Summary ───────────────────────────────────────────────────────────
# Exit codes: 0 = all good, 1-2 = minor warnings (build proceeds),
#             3+ = critical failures (build aborted)
Write-Host "==================================================" -ForegroundColor Cyan
if ($issues -eq 0) {
    Write-Host "  All prerequisites satisfied. Ready to build!" -ForegroundColor Green
} elseif ($issues -lt 3) {
    Write-Host "  $issues warning(s) only. Build will proceed." -ForegroundColor Yellow
    Write-Host "  Run with -Fix to auto-install missing items." -ForegroundColor Gray
} else {
    Write-Host "  $issues critical issue(s) found." -ForegroundColor Red
    Write-Host ""
    Write-Host "  Quick fix (run in an elevated terminal):" -ForegroundColor Yellow
    Write-Host "    winget install Rustlang.Rustup Kitware.CMake LLVM.LLVM Git.Git" -ForegroundColor White
    Write-Host "  Then open a NEW terminal window and retry." -ForegroundColor Yellow
}
Write-Host "==================================================" -ForegroundColor Cyan
Write-Host ""

exit $issues
