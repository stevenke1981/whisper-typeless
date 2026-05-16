# download-model.ps1 — Download a whisper.cpp GGUF model
#
# Usage:
#   .\scripts\download-model.ps1 -Model small
#   .\scripts\download-model.ps1 -Model large-v3 -Mirror ModelScope
#   .\scripts\download-model.ps1 -ListModels

param(
    [string]$Model = "small",
    [ValidateSet("HuggingFace", "ModelScope")]
    [string]$Mirror = "HuggingFace",
    [switch]$ListModels,
    [string]$OutputDir = ""
)

$ErrorActionPreference = "Stop"

$Models = @{
    "tiny"             = @{ Size="75MB";  SHA256="bd577a113a864445d4c299885e0cb97d4ba92b5f" }
    "tiny.en"          = @{ Size="75MB";  SHA256="c78c86eb1a8faa21b369bcd33207cc90d64ae9df" }
    "base"             = @{ Size="142MB"; SHA256="465707469ff3a37a2b9b8d8f89f2f99de7299dac" }
    "base.en"          = @{ Size="142MB"; SHA256="137c40403d78fd54d454da0f9bd998f78703390c" }
    "small"            = @{ Size="466MB"; SHA256="55356645c2b361a969dfd0ef2c5a50d530afd8d5" }
    "small.en"         = @{ Size="466MB"; SHA256="db8a495a91d927739e50b3fc1cc4c6b8f6c2d022" }
    "medium"           = @{ Size="1.5GB"; SHA256="fd9727b6e1217c2f614f9b698455c4ffd82463b4" }
    "medium.en"        = @{ Size="1.5GB"; SHA256="8c30f0e44ce9560643ebd10bbe50cd20eafd3723" }
    "large-v2"         = @{ Size="2.9GB"; SHA256="0f4c8e34f21cf1a914c59d8b3ce882345ad349d6" }
    "large-v3"         = @{ Size="2.9GB"; SHA256="ad82bf6a9043ceed055076d0fd39f5f186ff8062" }
    "large-v3-turbo"   = @{ Size="1.6GB"; SHA256="4af2b29d7ec73d781377bfd1758ca957d842e1a4" }
}

if ($ListModels) {
    Write-Host ""
    Write-Host "Available models:" -ForegroundColor Cyan
    Write-Host ("{0,-20} {1,-8}" -f "Name", "Size")
    Write-Host ("-" * 30)
    foreach ($m in $Models.GetEnumerator() | Sort-Object Name) {
        Write-Host ("{0,-20} {1,-8}" -f $m.Key, $m.Value.Size)
    }
    Write-Host ""
    return
}

if (-not $Models.ContainsKey($Model)) {
    Write-Host "Unknown model: $Model" -ForegroundColor Red
    Write-Host "Run with -ListModels to see available options." -ForegroundColor Yellow
    exit 1
}

# Determine output directory
if ($OutputDir -eq "") {
    $localAppData = $env:LOCALAPPDATA
    $OutputDir = Join-Path $localAppData "whisper-typeless\models"
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$filename = "ggml-$Model.bin"
$destPath = Join-Path $OutputDir $filename
$tempPath = "$destPath.tmp"

$hfBase = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main"
$msBase = "https://modelscope.cn/models/ggerganov/whisper.cpp/resolve/master"

$url = if ($Mirror -eq "ModelScope") {
    "$msBase/$filename"
} else {
    "$hfBase/$filename"
}

# Check if already downloaded
if (Test-Path $destPath) {
    $existingSize = [math]::Round((Get-Item $destPath).Length / 1MB, 1)
    Write-Host "Model already exists: $destPath ($existingSize MB)" -ForegroundColor Green
    Write-Host "Delete it first to re-download." -ForegroundColor Gray
    exit 0
}

Write-Host ""
Write-Host "Downloading model: $Model" -ForegroundColor Cyan
Write-Host "  Source : $url"
Write-Host "  Dest   : $destPath"
Write-Host "  Size   : ~$($Models[$Model].Size)"
Write-Host ""

$ProgressPreference = "SilentlyContinue"  # faster downloads without PS progress bar

try {
    $client  = New-Object System.Net.WebClient
    $sw      = [System.Diagnostics.Stopwatch]::StartNew()
    $lastPct = -1

    $downloadComplete = $false

    # Use HttpClient for progress reporting
    Add-Type -AssemblyName System.Net.Http
    $httpClient = New-Object System.Net.Http.HttpClient
    $httpClient.Timeout = [System.TimeSpan]::FromHours(2)

    $response = $httpClient.GetAsync($url, [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead).Result

    if (-not $response.IsSuccessStatusCode) {
        # Try fallback mirror
        if ($Mirror -eq "HuggingFace") {
            Write-Host "HuggingFace failed ($($response.StatusCode)). Trying ModelScope..." -ForegroundColor Yellow
            $url      = "$msBase/$filename"
            $response = $httpClient.GetAsync($url, [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead).Result
        }
    }

    $totalBytes = $response.Content.Headers.ContentLength
    $stream     = $response.Content.ReadAsStreamAsync().Result
    $fileStream = [System.IO.File]::Create($tempPath)

    $buffer      = New-Object byte[] 81920
    $totalRead   = 0L
    $lastReport  = [System.Diagnostics.Stopwatch]::StartNew()

    while ($true) {
        $read = $stream.Read($buffer, 0, $buffer.Length)
        if ($read -le 0) { break }

        $fileStream.Write($buffer, 0, $read)
        $totalRead += $read

        if ($lastReport.ElapsedMilliseconds -ge 500) {
            $pct = if ($totalBytes -gt 0) {
                [int]($totalRead * 100 / $totalBytes)
            } else { 0 }

            $speedMBs = [math]::Round($totalRead / 1MB / ($sw.Elapsed.TotalSeconds + 0.001), 1)
            $readMB   = [math]::Round($totalRead / 1MB, 1)
            $totalMB  = if ($totalBytes -gt 0) { [math]::Round($totalBytes / 1MB, 1) } else { "?" }

            Write-Host "`r  [$("{0,3}" -f $pct)%] $readMB MB / $totalMB MB   $speedMBs MB/s   " -NoNewline -ForegroundColor Cyan
            $lastReport.Restart()
        }
    }

    $fileStream.Close()
    $stream.Close()
    Write-Host ""

    # Verify file exists and has size
    if (-not (Test-Path $tempPath) -or (Get-Item $tempPath).Length -eq 0) {
        throw "Downloaded file is empty or missing"
    }

    # SHA256 verification
    $expectedSha = $Models[$Model].SHA256
    if ($expectedSha) {
        Write-Host "  Verifying SHA256..." -NoNewline
        $sha = Get-FileHash -Algorithm SHA256 $tempPath | Select-Object -ExpandProperty Hash
        if ($sha.ToLower() -ne $expectedSha.ToLower()) {
            Remove-Item $tempPath -Force
            throw "SHA256 mismatch!`n  Expected: $expectedSha`n  Got     : $sha"
        }
        Write-Host " OK" -ForegroundColor Green
    }

    # Move temp → final
    Move-Item -Path $tempPath -Destination $destPath -Force

    $finalSize = [math]::Round((Get-Item $destPath).Length / 1MB, 1)
    $elapsed   = [math]::Round($sw.Elapsed.TotalSeconds)
    Write-Host ""
    Write-Host "Download complete!" -ForegroundColor Green
    Write-Host "  File    : $destPath"
    Write-Host "  Size    : $finalSize MB"
    Write-Host "  Time    : ${elapsed}s"

} catch {
    if (Test-Path $tempPath) { Remove-Item $tempPath -Force -ErrorAction SilentlyContinue }
    Write-Host ""
    Write-Host "Download failed: $_" -ForegroundColor Red
    Write-Host ""
    Write-Host "Retry with: -Mirror ModelScope" -ForegroundColor Yellow
    exit 1
} finally {
    if ($fileStream) { try { $fileStream.Dispose() } catch {} }
    if ($httpClient) { try { $httpClient.Dispose() } catch {} }
}
