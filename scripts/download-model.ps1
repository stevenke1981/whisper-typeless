# download-model.ps1 - Download a whisper.cpp GGML model
#
# Usage:
#   .\scripts\download-model.ps1 -Model small
#   .\scripts\download-model.ps1 -Model large-v3-turbo
#   .\scripts\download-model.ps1 -Model large-v3 -Mirror ModelScope
#   .\scripts\download-model.ps1 -ListModels

param(
    [string]$Model = "small",
    [ValidateSet("Auto", "HuggingFace", "ModelScope")]
    [string]$Mirror = "Auto",
    [switch]$ListModels,
    [string]$OutputDir = "",
    [string]$CatalogPath = "",
    [switch]$Force
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir

if ($CatalogPath -eq "") {
    $CatalogPath = Join-Path $RepoRoot "model-catalog.json"
}

if (-not (Test-Path -LiteralPath $CatalogPath)) {
    Write-Host "Model catalog not found: $CatalogPath" -ForegroundColor Red
    exit 1
}

$Catalog = Get-Content -LiteralPath $CatalogPath -Raw | ConvertFrom-Json
$Models = @($Catalog.models)

function Get-ModelUrls {
    param(
        [Parameter(Mandatory = $true)] $Entry,
        [Parameter(Mandatory = $true)] [string] $Source
    )

    $urls = @()
    if ($Entry.download_url) {
        $urls += [string]$Entry.download_url
    }
    if ($Entry.mirror_urls) {
        foreach ($url in @($Entry.mirror_urls)) {
            if ($url) {
                $urls += [string]$url
            }
        }
    }

    $urls = @($urls | Select-Object -Unique)

    if ($Source -eq "HuggingFace") {
        return @($urls | Where-Object { $_ -like "*huggingface.co*" })
    }
    if ($Source -eq "ModelScope") {
        return @($urls | Where-Object { $_ -like "*modelscope.cn*" })
    }

    return $urls
}

function Get-ModelSizeLabel {
    param([Parameter(Mandatory = $true)] $Entry)

    $diskMb = [int]$Entry.size.disk_mb
    if ($diskMb -ge 1024) {
        return ("{0:N1} GB" -f ($diskMb / 1024))
    }

    return "$diskMb MB"
}

if ($ListModels) {
    Write-Host ""
    Write-Host "Available models:" -ForegroundColor Cyan
    Write-Host ("{0,-20} {1,-10} {2}" -f "Name", "Size", "Primary URL")
    Write-Host ("-" * 110)
    foreach ($entry in $Models | Sort-Object id) {
        $sizeLabel = Get-ModelSizeLabel -Entry $entry
        Write-Host ("{0,-20} {1,-10} {2}" -f $entry.id, $sizeLabel, $entry.download_url)
    }
    Write-Host ""
    return
}

$modelInfo = $Models | Where-Object { $_.id -eq $Model } | Select-Object -First 1

if (-not $modelInfo) {
    Write-Host "Unknown model: $Model" -ForegroundColor Red
    Write-Host "Run with -ListModels to see available options." -ForegroundColor Yellow
    exit 1
}

if ($OutputDir -eq "") {
    $localAppData = $env:LOCALAPPDATA
    $OutputDir = Join-Path $localAppData "whisper-typeless\models"
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$filename = "ggml-$Model.bin"
$destPath = Join-Path $OutputDir $filename
$tempPath = "$destPath.tmp"
$expectedSha = [string]$modelInfo.sha1
$urls = @(Get-ModelUrls -Entry $modelInfo -Source $Mirror)

if (-not $urls -or $urls.Count -eq 0) {
    Write-Host "No download URL for model '$Model' and source '$Mirror'." -ForegroundColor Red
    exit 1
}

if (Test-Path -LiteralPath $destPath) {
    $existingSize = [math]::Round((Get-Item -LiteralPath $destPath).Length / 1MB, 1)
    if ($expectedSha) {
        $existingSha = (Get-FileHash -Algorithm SHA1 -LiteralPath $destPath).Hash.ToLower()
        if ($existingSha -eq $expectedSha.ToLower()) {
            Write-Host "Model already exists: $destPath ($existingSize MB)" -ForegroundColor Green
            exit 0
        }

        Write-Host "Existing model checksum mismatch: $destPath" -ForegroundColor Yellow
        Write-Host "  Expected: $expectedSha"
        Write-Host "  Got     : $existingSha"
        if (-not $Force) {
            Write-Host "Run again with -Force to replace it." -ForegroundColor Yellow
            exit 1
        }
    }

    if ($Force) {
        Remove-Item -LiteralPath $destPath -Force
    }
    else {
        Write-Host "Model already exists: $destPath ($existingSize MB)" -ForegroundColor Green
        Write-Host "Delete it first or run with -Force to re-download." -ForegroundColor Gray
        exit 0
    }
}

Write-Host ""
Write-Host "Downloading model: $Model" -ForegroundColor Cyan
Write-Host "  Catalog: $CatalogPath"
Write-Host "  Dest   : $destPath"
Write-Host "  Size   : ~$(Get-ModelSizeLabel -Entry $modelInfo)"
Write-Host "  SHA1   : $expectedSha"
Write-Host ""

$ProgressPreference = "SilentlyContinue"

Add-Type -AssemblyName System.Net.Http
$httpClient = $null
$fileStream = $null
$stream = $null
$success = $false
$lastError = $null

try {
    $httpClient = New-Object System.Net.Http.HttpClient
    $httpClient.Timeout = [System.TimeSpan]::FromHours(2)

    foreach ($url in $urls) {
        if (Test-Path -LiteralPath $tempPath) {
            Remove-Item -LiteralPath $tempPath -Force -ErrorAction SilentlyContinue
        }

        Write-Host "Source : $url"
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        $fileStream = $null
        $stream = $null

        try {
            $response = $httpClient.GetAsync($url, [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead).Result
            if (-not $response.IsSuccessStatusCode) {
                throw "HTTP $($response.StatusCode)"
            }

            $totalBytes = $response.Content.Headers.ContentLength
            $stream = $response.Content.ReadAsStreamAsync().Result
            $fileStream = [System.IO.File]::Create($tempPath)

            $buffer = New-Object byte[] 81920
            $totalRead = 0L
            $lastReport = [System.Diagnostics.Stopwatch]::StartNew()

            while ($true) {
                $read = $stream.Read($buffer, 0, $buffer.Length)
                if ($read -le 0) {
                    break
                }

                $fileStream.Write($buffer, 0, $read)
                $totalRead += $read

                if ($lastReport.ElapsedMilliseconds -ge 500) {
                    $pct = 0
                    if ($totalBytes -and $totalBytes -gt 0) {
                        $pct = [int]($totalRead * 100 / $totalBytes)
                    }

                    $speedMBs = [math]::Round($totalRead / 1MB / ($sw.Elapsed.TotalSeconds + 0.001), 1)
                    $readMB = [math]::Round($totalRead / 1MB, 1)
                    $totalMB = "?"
                    if ($totalBytes -and $totalBytes -gt 0) {
                        $totalMB = [math]::Round($totalBytes / 1MB, 1)
                    }

                    Write-Host "`r  [$("{0,3}" -f $pct)%] $readMB MB / $totalMB MB   $speedMBs MB/s   " -NoNewline -ForegroundColor Cyan
                    $lastReport.Restart()
                }
            }

            $fileStream.Close()
            $fileStream = $null
            $stream.Close()
            $stream = $null
            Write-Host ""

            if ((-not (Test-Path -LiteralPath $tempPath)) -or ((Get-Item -LiteralPath $tempPath).Length -eq 0)) {
                throw "Downloaded file is empty or missing"
            }

            if ($expectedSha) {
                Write-Host "  Verifying SHA1..." -NoNewline
                $sha = (Get-FileHash -Algorithm SHA1 -LiteralPath $tempPath).Hash.ToLower()
                if ($sha -ne $expectedSha.ToLower()) {
                    throw "SHA1 mismatch! Expected: $expectedSha Got: $sha"
                }
                Write-Host " OK" -ForegroundColor Green
            }

            Move-Item -LiteralPath $tempPath -Destination $destPath -Force

            $finalSize = [math]::Round((Get-Item -LiteralPath $destPath).Length / 1MB, 1)
            $elapsed = [math]::Round($sw.Elapsed.TotalSeconds)
            Write-Host ""
            Write-Host "Download complete!" -ForegroundColor Green
            Write-Host "  File    : $destPath"
            Write-Host "  Size    : $finalSize MB"
            Write-Host "  Time    : ${elapsed}s"
            $success = $true
            break
        }
        catch {
            $lastError = $_
            Write-Host ""
            Write-Host "Source failed: $_" -ForegroundColor Yellow
            if ($null -ne $fileStream) {
                try { $fileStream.Dispose() } catch {}
                $fileStream = $null
            }
            if ($null -ne $stream) {
                try { $stream.Dispose() } catch {}
                $stream = $null
            }
            if (Test-Path -LiteralPath $tempPath) {
                Remove-Item -LiteralPath $tempPath -Force -ErrorAction SilentlyContinue
            }
        }
    }
}
finally {
    if ($null -ne $fileStream) {
        try { $fileStream.Dispose() } catch {}
    }
    if ($null -ne $stream) {
        try { $stream.Dispose() } catch {}
    }
    if ($null -ne $httpClient) {
        try { $httpClient.Dispose() } catch {}
    }
}

if (-not $success) {
    Write-Host ""
    Write-Host "Download failed: $lastError" -ForegroundColor Red
    Write-Host "Check or update the URL in: $CatalogPath" -ForegroundColor Yellow
    exit 1
}
