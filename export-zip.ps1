# AZ_AgentZero: Export source-only zip (no build artifacts)
param(
    [string]$OutputPath = "..\AZ_AgentZero-source.zip"
)

$ErrorActionPreference = "Stop"
$ROOT = $PSScriptRoot

# Try to locate 7z in common installation paths
$7zPaths = @(
    "$env:ProgramFiles\7-Zip\7z.exe",
    "${env:ProgramFiles(x86)}\7-Zip\7z.exe",
    "$env:LOCALAPPDATA\Programs\7-Zip\7z.exe",
    "$env:LOCALAPPDATA\Microsoft\WindowsApps\7z.exe"
)
$7z = $null
foreach ($p in $7zPaths) {
    if (Test-Path $p) { $7z = $p; break }
}
if (-not $7z) {
    # Try PATH
    $7z = (Get-Command "7z" -ErrorAction SilentlyContinue).Source
}
if (-not $7z) {
    Write-Host "7-Zip not found. Install from https://7-zip.org/ or ensure '7z' is in PATH." -ForegroundColor Red
    exit 1
}

Write-Host "Creating source-only archive of AZ_AgentZero..." -ForegroundColor Yellow

# Include docs now tracked in git; exclude large/cache/build/temp artifacts
& $7z a -tzip $OutputPath "$ROOT\*" `
    -xr!".git" -xr!"models" -xr!"node_modules" -xr!"venv" -xr!".venv" `
    -xr!"target" -xr!"dist" -xr!"build" -xr!"__pycache__" -xr!"*.pyc" `
    -xr!".setup-cache" -xr!"*.log" -xr!"Cargo.lock" -xr!"package-lock.json" `
    -xr!"*.tar.gz" -xr!".DS_Store" -xr!"Thumbs.db" -xr!"debug" -xr!"release" `
    -xr!"*.exe" -xr!"*.dll" -xr!"*.pdb" -xr!"*.o" -xr!"*.so" -xr!"*.dylib" `
    -xr!"*.onnx" -xr!"*.onnx.json" -xr!"vocab.txt" `
    -xr!".svelte-kit" -xr!"package" -xr!".env" -xr!".env.*" `
    -xr!"vite.config.*.timestamp-*" -xr!"bun.lock" -xr!".obsidian" `
    -xr!"skills-lock.json" -xr!".agents" -xr!".claude" -xr!".kilocode" `
    -xr!".gemini" -xr!"archive" -xr!".agent" -xr!".vercel" `
    -xr!".ruff_cache" -xr!".tauri" -xr!".opencode" -xr!"openspec" `
    -xr!"plans" -xr!"DO_NOT_TOUCH" -xr!"schemas"

if ($LASTEXITCODE -eq 0) {
    Write-Host "Archive created: $OutputPath" -ForegroundColor Green
} else {
    Write-Host "Failed to create archive" -ForegroundColor Red
    exit 1
}
