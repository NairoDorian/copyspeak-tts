# Setup Piper TTS with CUDA/GPU acceleration for CopySpeak TTS on Windows

Write-Host "=========================================" -ForegroundColor Cyan
Write-Host "  CopySpeak TTS: Piper CUDA Setup Helper " -ForegroundColor Cyan
Write-Host "=========================================" -ForegroundColor Cyan
Write-Host ""

# Check Python installation
$pythonCmd = "python3"
try {
    $pythonVersion = & $pythonCmd --version 2>&1
    Write-Host "Found Python: $pythonVersion" -ForegroundColor Green
} catch {
    try {
        $pythonCmd = "python"
        $pythonVersion = & $pythonCmd --version 2>&1
        Write-Host "Found Python: $pythonVersion" -ForegroundColor Green
    } catch {
        Write-Error "Python 3 is not installed or not in your PATH. Please install Python 3.10+ from python.org."
        exit 1
    }
}

# Install / update piper-tts with HTTP support
Write-Host "`n1. Installing piper-tts with HTTP server support..." -ForegroundColor Yellow
& $pythonCmd -m pip install --user --upgrade "piper-tts[http]"
if ($LASTEXITCODE -ne 0) {
    Write-Warning "Pip install for piper-tts[http] failed. Trying fallback..."
    & $pythonCmd -m pip install --user "piper-tts[http]"
}

# Uninstall CPU onnxruntime first to avoid import conflicts
Write-Host "`n2. Cleaning up any CPU-only onnxruntime package..." -ForegroundColor Yellow
Write-Host "Note: If you get permission errors, you may need to run this script as Administrator to remove system-wide packages." -ForegroundColor Gray
& $pythonCmd -m pip uninstall -y onnxruntime

# Install GPU version of onnxruntime
Write-Host "`n3. Installing onnxruntime-gpu..." -ForegroundColor Yellow
& $pythonCmd -m pip install --user --upgrade onnxruntime-gpu

# Install matching NVIDIA runtime packages from PyPI to avoid manual DLL setup
Write-Host "`n4. Installing official NVIDIA CUDA & cuDNN runtime libraries..." -ForegroundColor Yellow
$nvidiaPackages = @(
    "nvidia-cuda-runtime-cu12",
    "nvidia-cudnn-cu12",
    "nvidia-cublas-cu12",
    "nvidia-cufft-cu12",
    "nvidia-curand-cu12",
    "nvidia-cusolver-cu12",
    "nvidia-cusparse-cu12",
    "nvidia-nvjitlink-cu12"
)

foreach ($pkg in $nvidiaPackages) {
    Write-Host "  Installing $pkg..." -ForegroundColor Gray
    & $pythonCmd -m pip install --user --upgrade $pkg
}

# Verification Check
Write-Host "`n5. Verifying CUDA provider status in ONNX Runtime..." -ForegroundColor Yellow
$verifyScript = @"
import onnxruntime as ort
providers = ort.get_available_providers()
print("Available ONNX Runtime Execution Providers:")
for p in providers:
    print(f" - {p}")
if 'CUDAExecutionProvider' in providers:
    print("STATUS: CUDA is successfully configured!")
else:
    print("STATUS: CUDA provider is registered but DLLs are missing, or GPU is not CUDA-compatible.")
"@

& $pythonCmd -c $verifyScript

Write-Host ""
Write-Host "=========================================" -ForegroundColor Cyan
Write-Host "  Installation Completed successfully!   " -ForegroundColor Green
Write-Host "=========================================" -ForegroundColor Cyan
Write-Host "Ensure you have the latest NVIDIA drivers installed on your system."
Write-Host "You can now check the 'CUDA GPU Acceleration' checkbox in your"
Write-Host "CopySpeak settings to synthesize speech on your GPU."
Write-Host ""
