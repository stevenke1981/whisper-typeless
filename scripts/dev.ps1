# dev.ps1 — whisper-typeless 開發入口
# 用法: .\scripts\dev.ps1 <check|test|lint|fmt|build|release|clean|doctor> [-Cuda] [-Vulkan]
param(
    [Parameter(Position = 0)]
    [ValidateSet('check', 'test', 'lint', 'fmt', 'build', 'release', 'clean', 'doctor')]
    [string]$Command = 'doctor',
    [switch]$Cuda,
    [switch]$Vulkan
)

$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)

# ── LIBCLANG_PATH 自動偵測 ──
function Find-LibClang {
    $cands = @($env:LIBCLANG_PATH, 'C:\Program Files\LLVM\bin', (Join-Path $env:USERPROFILE '.local\libclang'))
    $vs = Get-ChildItem 'C:\Program Files*\Microsoft Visual Studio\*\*\VC\Tools\Llvm\x64\bin' -ErrorAction SilentlyContinue
    $cands += $vs | ForEach-Object { $_.FullName }
    $cands | Where-Object { $_ -and (Test-Path (Join-Path $_ 'libclang.dll')) } | Select-Object -First 1
}
$clang = Find-LibClang
if ($clang) { $env:LIBCLANG_PATH = $clang }

# ── 目標平台：host 為 gnu 時改用 msvc ──
$extra = @()
$hostLine = (rustc -vV 2>$null | Select-String '^host:') 
if ($hostLine -and $hostLine.Line -match 'gnu') { $extra += @('--target', 'x86_64-pc-windows-msvc') }

# ── 功能旗標 ──
if ($Vulkan) {
    if (-not $env:VULKAN_SDK -or -not (Test-Path (Join-Path $env:VULKAN_SDK 'Bin\glslc.exe'))) {
        Write-Host '找不到 Vulkan SDK：請安裝 LunarG Vulkan SDK 並設定 VULKAN_SDK（需含 Bin\glslc.exe）' -ForegroundColor Red
        exit 1
    }
    $feat = @('--features', 'vulkan')
} elseif ($Cuda) {
    . (Join-Path $PSScriptRoot 'setup-env.ps1')
    $feat = @('--features', 'cuda')
} else {
    $feat = @('--no-default-features')
}

function Run([string[]]$cargoArgs) {
    Write-Host "> cargo $($cargoArgs -join ' ')" -ForegroundColor Cyan
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

switch ($Command) {
    'check'   { Run (@('check', '--all-targets') + $extra + $feat) }
    'test'    { Run (@('test') + $extra + $feat) }
    'lint'    { Run (@('clippy', '--all-targets') + $extra + $feat + @('--', '-D', 'warnings')) }
    'fmt'     { Run @('fmt', '--all') }
    'build'   { Run (@('build') + $extra + $feat) }
    'release' { Run (@('build', '--release') + $extra + $feat) }
    'clean'   { Run @('clean') }
    'doctor'  {
        function Check($name, $ok, $hint) {
            if ($ok) { Write-Host "  OK   $name" -ForegroundColor Green }
            else { Write-Host "  MISS $name  ($hint)" -ForegroundColor Yellow }
        }
        Write-Host 'Prerequisites:'
        Check 'rustc'    ([bool](Get-Command rustc -ErrorAction SilentlyContinue)) 'https://rustup.rs'
        Check 'cargo'    ([bool](Get-Command cargo -ErrorAction SilentlyContinue)) 'https://rustup.rs'
        Check 'msvc target' ([bool]((rustup target list --installed 2>$null) -match 'x86_64-pc-windows-msvc')) 'rustup target add x86_64-pc-windows-msvc'
        Check 'cmake'    ([bool](Get-Command cmake -ErrorAction SilentlyContinue)) 'install CMake and add to PATH'
        Check 'libclang' ([bool]$clang) 'install LLVM or set LIBCLANG_PATH'
        Check 'Vulkan SDK (optional, -Vulkan)' ([bool]($env:VULKAN_SDK -and (Test-Path (Join-Path $env:VULKAN_SDK 'Bin\glslc.exe')))) 'LunarG Vulkan SDK; set VULKAN_SDK'
        Check 'nvcc (optional, -Cuda)' ([bool](Get-Command nvcc -ErrorAction SilentlyContinue)) 'CUDA Toolkit; see scripts/setup-env.ps1'
        if ($clang) { Write-Host "  LIBCLANG_PATH = $clang" }
    }
}
