# CopySpeak Piper TTS Performance Test Script
# Measures TTS synthesis timing for CPU and CUDA modes
# Usage: .\test-piper-perf.ps1

param(
    [int]$Runs = 5,
    [string]$TestText = "This is a test sentence to be read aloud.",
    [int]$HealthTimeoutSec = 60,
    [int]$PiperTimeoutSec = 120
)

$ControlUrl = "http://127.0.0.1:43117"
$TestResults = @()

# Load control token from config (or env override)
$AuthHeader = $null
if ($env:COPYSPEAK_CONTROL_TOKEN) {
    $AuthHeader = @{ Authorization = "Bearer $env:COPYSPEAK_CONTROL_TOKEN" }
} else {
    $ConfigPath = "$env:APPDATA\CopySpeak TTS\config.json"
    if (Test-Path $ConfigPath) {
        try {
            $cfg = Get-Content $ConfigPath -Raw | ConvertFrom-Json
            if ($cfg.general.control_token) {
                $AuthHeader = @{ Authorization = "Bearer $($cfg.general.control_token)" }
            }
        } catch {}
    }
}

Write-Host "========================================" -ForegroundColor Cyan
Write-Host " CopySpeak Piper TTS Performance Test" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Runs per mode : $Runs"
Write-Host "Test text     : $TestText"
Write-Host ""

# Helper: send raw HTTP request and return response
function Send-HttpRequest {
    param([string]$Method, [string]$Path, [string]$Body = "")
    $uri = "$ControlUrl$Path"
    $headers = @{}
    if ($script:AuthHeader -and $Path -ne "/health") {
        $headers = $script:AuthHeader
    }
    try {
        if ($Method -eq "GET") {
            $response = Invoke-WebRequest -Uri $uri -Method GET -Headers $headers -TimeoutSec 5 -UseBasicParsing
            return $response.Content
        } elseif ($Method -eq "POST") {
            $response = Invoke-WebRequest -Uri $uri -Method POST -Body $Body -ContentType "application/json" -Headers $headers -TimeoutSec 30 -UseBasicParsing
            return $response.Content
        }
    } catch {
        return $null
    }
}

# Helper: get JSON field from response
function Get-JsonField {
    param([string]$Json, [string]$Field)
    try {
        $obj = $Json | ConvertFrom-Json
        return $obj.$Field
    } catch {
        return $null
    }
}

# Step 1: Wait for app health check
Write-Host "[1/4] Waiting for CopySpeak app to be ready..." -ForegroundColor Yellow
$healthStart = Get-Date
while ($true) {
    $resp = Send-HttpRequest -Method GET -Path "/health"
    if ($resp) {
        $ok = Get-JsonField -Json $resp -Field "ok"
        if ($ok) {
            $healthElapsed = ((Get-Date) - $healthStart).TotalSeconds
            Write-Host "  App ready after $([math]::Round($healthElapsed, 1))s" -ForegroundColor Green
            break
        }
    }
    if (((Get-Date) - $healthStart).TotalSeconds -gt $HealthTimeoutSec) {
        Write-Host "  ERROR: App did not start within ${HealthTimeoutSec}s" -ForegroundColor Red
        exit 1
    }
    Start-Sleep -Milliseconds 500
}

# Step 2: Wait for Piper model to be loaded
Write-Host "[2/4] Waiting for Piper model to load into RAM..." -ForegroundColor Yellow
$piperStart = Get-Date
while ($true) {
    $resp = Send-HttpRequest -Method GET -Path "/piper-status"
    if ($resp) {
        $ready = Get-JsonField -Json $resp -Field "ready"
        $model = Get-JsonField -Json $resp -Field "model"
        $cuda = Get-JsonField -Json $resp -Field "cuda"
        if ($ready) {
            $piperElapsed = ((Get-Date) - $piperStart).TotalSeconds
            Write-Host "  Piper ready! Model: $model, CUDA: $cuda, Load time: $([math]::Round($piperElapsed, 1))s" -ForegroundColor Green
            break
        }
    }
    if (((Get-Date) - $piperStart).TotalSeconds -gt $PiperTimeoutSec) {
        Write-Host "  WARNING: Piper model not ready after ${PiperTimeoutSec}s. Proceeding anyway..." -ForegroundColor Yellow
        break
    }
    Start-Sleep -Milliseconds 500
}

# Step 3: Run synthesis tests
Write-Host "[3/4] Running $Runs synthesis tests..." -ForegroundColor Yellow
$body = @{ text = $TestText } | ConvertTo-Json -Compress

for ($i = 1; $i -le $Runs; $i++) {
    Write-Host "  Run $i/$Runs..." -NoNewline
    
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $resp = Send-HttpRequest -Method POST -Path "/speak" -Body $body
    $sw.Stop()
    $httpMs = $sw.ElapsedMilliseconds
    
    if ($resp) {
        $ok = Get-JsonField -Json $resp -Field "ok"
        if ($ok) {
            Write-Host " HTTP: ${httpMs}ms" -ForegroundColor Green
        } else {
            $err = Get-JsonField -Json $resp -Field "error"
            Write-Host " FAIL: $err" -ForegroundColor Red
        }
    } else {
        Write-Host " FAIL: no response" -ForegroundColor Red
        $httpMs = -1
    }
    
    $TestResults += [PSCustomObject]@{
        Run = $i
        Mode = "CPU"
        HTTP_ms = $httpMs
    }
    
    Start-Sleep -Milliseconds 500
}

# Step 4: Get piper status for final report
Write-Host "[4/4] Final status check..." -ForegroundColor Yellow
$resp = Send-HttpRequest -Method GET -Path "/piper-status"
if ($resp) {
    $model = Get-JsonField -Json $resp -Field "model"
    $cuda = Get-JsonField -Json $resp -Field "cuda"
    $running = Get-JsonField -Json $resp -Field "running"
    Write-Host "  Model: $model | CUDA: $cuda | Running: $running" -ForegroundColor Cyan
}

# Print results table
Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host " RESULTS" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
$TestResults | Format-Table -AutoSize

$avg = ($TestResults | Where-Object { $_.HTTP_ms -gt 0 } | Measure-Object -Property HTTP_ms -Average).Average
if ($avg) {
    Write-Host ""
    Write-Host "Average HTTP round-trip time: $([math]::Round($avg, 0))ms" -ForegroundColor Cyan
    Write-Host "Note: HTTP time includes network + synthesis + playback setup." -ForegroundColor DarkGray
    Write-Host "For pure synthesis time, check history entries' synthesis_ms metadata." -ForegroundColor DarkGray
}

Write-Host ""
Write-Host "Test complete!" -ForegroundColor Green
