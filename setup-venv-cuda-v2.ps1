# Setup CopySpeak TTS Python Virtual Environment (V2) with CUDA 13 GPU support
# Uses onnxruntime-gpu 1.27.0+ (stable PyPI, CUDA 13) with explicit NVIDIA cu13
# runtime packages so piper_server.rs's get_nvidia_dll_paths() can inject DLLs.
# Requires uv to be installed.

$ErrorActionPreference = "Continue"

Write-Host "================================================================" -ForegroundColor Cyan
Write-Host "  CopySpeak TTS: GPU .venv-v2 Setup Helper (CUDA 13 stable)      " -ForegroundColor Cyan
Write-Host "================================================================" -ForegroundColor Cyan
Write-Host ""

# Ensure we are in the script directory
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
if ($scriptDir) {
    Set-Location $scriptDir
}

# 1. Create virtual environment using uv
Write-Host "1. Creating/Verifying virtual environment (.venv-v2)..." -ForegroundColor Yellow
if (-not (Test-Path ".venv-v2")) {
    & uv venv .venv-v2 --python 3.12
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Failed to create virtual environment via uv."
        exit 1
    }
} else {
    Write-Host "  .venv-v2 already exists. Proceeding..." -ForegroundColor Gray
}

# Helper function to install packages
function Install-Pkg {
    param(
        [string]$Name,
        [string[]]$Packages
    )
    Write-Host "  Installing $Name..." -ForegroundColor Gray
    & uv pip install --python .venv-v2 $Packages
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Failed to install $Name."
        exit 1
    }
}

# Helper function to uninstall a package
function Uninstall-Pkg {
    param(
        [string]$Name,
        [string]$Package
    )
    Write-Host "  Uninstalling $Name..." -ForegroundColor Gray
    & uv pip uninstall --python .venv-v2 $Package
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "Uninstall of $Package failed or was not installed. Continuing..."
    }
}

# 2. Install base project dependencies
Write-Host "`n2. Installing base dependencies (kokoro, pocket-tts, soundfile, numpy, click)..." -ForegroundColor Yellow
Install-Pkg "base dependencies" @("kokoro", "pocket-tts", "soundfile", "numpy", "click")

# 3. Install KittenTTS wheel
Write-Host "`n3. Installing KittenTTS wheel..." -ForegroundColor Yellow
$KittenTTSWheel = "https://github.com/KittenML/KittenTTS/releases/download/0.8.1/kittentts-0.8.1-py3-none-any.whl"
Install-Pkg "KittenTTS wheel" @($KittenTTSWheel)

# 4. Install local agent-harness package in editable mode
Write-Host "`n4. Installing local agent-harness package..." -ForegroundColor Yellow
if (Test-Path "agent-harness") {
    Install-Pkg "agent-harness" @("-e", "./agent-harness")
} else {
    Write-Warning "agent-harness directory not found. Skipping."
}

# 5. Install piper-tts LAST (which pulls in CPU onnxruntime as a dependency)
Write-Host "`n5. Installing piper-tts[http]..." -ForegroundColor Yellow
Install-Pkg "piper-tts[http]" @("piper-tts[http]")

# 6. Install build/runtime dependencies
Write-Host "`n6. Installing build/runtime dependencies (coloredlogs, flatbuffers, packaging, protobuf, sympy, sentencepiece)..." -ForegroundColor Yellow
Install-Pkg "build/runtime deps" @("coloredlogs", "flatbuffers", "packaging", "protobuf", "sympy", "sentencepiece")

# 7. Remove CPU-only onnxruntime pulled in by piper-tts
#    We must completely nuke the onnxruntime directories to avoid namespace conflicts
#    (onnxruntime-gpu and onnxruntime share the same import namespace).
Write-Host "`n7. Removing CPU-only onnxruntime..." -ForegroundColor Yellow
Uninstall-Pkg "CPU onnxruntime" "onnxruntime"
Write-Host "  Cleaning leftover onnxruntime directories from site-packages..." -ForegroundColor Gray
Remove-Item -Recurse -Force ".venv-v2\Lib\site-packages\onnxruntime" -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force ".venv-v2\Lib\site-packages\onnxruntime_gpu*" -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force ".venv-v2\Lib\site-packages\onnxruntime*.dist-info" -ErrorAction SilentlyContinue

# 8. Install onnxruntime-gpu (stable PyPI).
Write-Host "`n8. Installing onnxruntime-gpu (stable PyPI)..." -ForegroundColor Yellow
& uv pip install --python .venv-v2 --no-cache --force-reinstall onnxruntime-gpu
if ($LASTEXITCODE -ne 0) {
    Write-Error "Failed to install onnxruntime-gpu."
    exit 1
}

# 9. Install NVIDIA CUDA 13 runtime libraries.
#    Only bare names have real win_amd64 DLL wheels (the -cu13 suffixed ones
#    are Linux-only stubs). nvidia-cudnn is the exception: only -cu13 has Windows.
#    nvidia-curand has NO Windows wheel at all on PyPI — omitted intentionally.
Write-Host "`n9. Installing NVIDIA CUDA 13 runtime libraries..." -ForegroundColor Yellow
$cuda13Packages = @(
    "nvidia-cuda-runtime",   # 13.0.48  win_amd64
    "nvidia-cudnn-cu13",     # 9.13.x   win_amd64 (only form with Windows wheel)
    "nvidia-cublas",         # 13.0.0   win_amd64
    "nvidia-cufft",          # 12.0.0   win_amd64 (12.x is CUDA-13-compat)
    "nvidia-cusolver",       # 12.0.3   win_amd64 (12.x is CUDA-13-compat)
    "nvidia-cusparse",       # 12.6.2   win_amd64 (12.x is CUDA-13-compat)
    "nvidia-nvjitlink"       # 13.0.39  win_amd64
    # nvidia-curand intentionally omitted: no win_amd64 wheel exists on PyPI
)

foreach ($pkg in $cuda13Packages) {
    Install-Pkg "$pkg" @($pkg)
}

# 10. Verification — mirrors exactly what piper_server.rs does at runtime.
Write-Host "`n10. Verifying ONNX Runtime CUDA provider..." -ForegroundColor Yellow
$verifyScript = @"
import os, sys, glob

# --- DLL injection (mirrors get_nvidia_dll_paths in piper_server.rs) ----------
try:
    import nvidia
    nvidia_dir = list(nvidia.__path__)[0]
    injected = []
    for bin_path in glob.glob(os.path.join(nvidia_dir, '*', 'bin')) + glob.glob(os.path.join(nvidia_dir, '*', 'bin', '*')):
        if os.path.isdir(bin_path):
            os.add_dll_directory(bin_path)
            injected.append(bin_path)
    print(f'Injected {len(injected)} NVIDIA DLL director(ies).')
except ImportError:
    print('WARNING: nvidia namespace package not found — DLL injection skipped.')

# --- ORT provider check -------------------------------------------------------
import onnxruntime as ort
providers = ort.get_available_providers()
print(f'\nONNX Runtime version : {ort.__version__}')
print('Available Execution Providers:')
for p in providers:
    print(f'  - {p}')
if 'CUDAExecutionProvider' in providers:
    print('\nSTATUS: SUCCESS — CUDAExecutionProvider is available!')
else:
    print('\nSTATUS: CUDAExecutionProvider NOT detected.')
    print('  Possible causes: driver too old for CUDA 13, or DLLs failed to load.')
    sys.exit(1)
"@

$verifyScript | & .venv-v2\Scripts\python.exe
$verifyExitCode = $LASTEXITCODE

Write-Host ""
Write-Host "=================================================================" -ForegroundColor Cyan
if ($verifyExitCode -eq 0) {
    Write-Host "  .venv-v2 GPU setup completed successfully!                    " -ForegroundColor Green
    Write-Host "  Point your CopySpeak config to:" -ForegroundColor Green
    Write-Host "    $(Resolve-Path '.venv-v2\Scripts\python.exe')" -ForegroundColor Green
} else {
    Write-Host "  Setup finished but CUDA provider was NOT detected.            " -ForegroundColor Yellow
    Write-Host "  TTS will fall back to CPU mode.                               " -ForegroundColor Yellow
}
Write-Host "=================================================================" -ForegroundColor Cyan
Write-Host ""
