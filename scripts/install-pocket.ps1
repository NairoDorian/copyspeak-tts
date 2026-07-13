#Requires -Version 5.1
<#
.SYNOPSIS
    Installs Pocket TTS (pocket-tts CLI) for CopySpeak via uv.

.DESCRIPTION
    Installs pocket-tts as a uv tool so the `pocket-tts` binary is on PATH.
    Pocket TTS (Kyutai Labs) is a compact, CPU-optimized offline engine with
    voice selection via CLI. Unlike Kokoro/Piper it does not require separate
    model files to be staged by this installer — the package resolves its
    weights on first use.

    Prompts for an optional default voice, baked into the profile snippet
    (defaults to "alba").

.PARAMETER Force
    Reinstall even if pocket-tts is already available.

.PARAMETER SmokeTest
    Verify the binary is on PATH after install.

.EXAMPLE
    ./scripts/install-pocket.ps1
    ./scripts/install-pocket.ps1 -SmokeTest
#>

param(
    [switch]$Force,
    [switch]$SmokeTest
)

$ErrorActionPreference = "Stop"

. "$PSScriptRoot/lib/copyspeak-engine-install.ps1"

Write-EngineBanner -Title "Pocket TTS Installer"

Require-Uv

# Interactive force prompt: -Force bypasses; a blank Enter keeps the install.
$alreadyInstalled = [bool](Get-Command pocket-tts -ErrorAction SilentlyContinue)
$effectiveForce = if ($Force) {
    $true
} elseif (-not $alreadyInstalled) {
    $false
} else {
    Get-Confirmation -Prompt "pocket-tts is already installed. Reinstall from scratch?" -DefaultYes:$false
}

if (-not $effectiveForce -and $alreadyInstalled) {
    Write-Host "  pocket-tts already installed." -ForegroundColor Green
    Write-Host "  Use -Force or answer Yes to reinstall." -ForegroundColor Yellow
} else {
    Write-Host "  Installing pocket-tts via uv tool..." -ForegroundColor Gray
    Invoke-Uv tool install pocket-tts --force
}

# Pocket ships a small set of built-in voices; "alba" is a sensible default.
$pocketVoices = @(
    @{ Id = "alba";   Label = "Alba (default)" },
    @{ Id = "diamond"; Label = "Diamond" },
    @{ Id = "sky";    Label = "Sky" },
    @{ Id = "amber";  Label = "Amber" }
)
$chosenVoice = Select-VoiceFromMenu -Title "Pick a default Pocket TTS voice" -Voices $pocketVoices -Default "alba"

$profileJson = @"
{
  "schema_version": 1,
  "id": "pocket-local",
  "name": "Pocket (Local)",
  "engine": "local",
  "voice": "$chosenVoice",
  "speed": 1.0,
  "pitch": 1.0,
  "effects": { "enabled": false, "active_effect": "none" },
  "engine_options": {
    "engine": "local",
    "preset": "pocket-tts",
    "command": "pocket-tts",
    "args_template": []
  }
}
"@

if ($SmokeTest -and (Get-Command pocket-tts -ErrorAction SilentlyContinue)) {
    Write-Host ""
    Write-Host "  Smoke test (binary on PATH)..." -ForegroundColor Yellow
    pocket-tts --help | Out-Null
    if (-not $?) { Write-Host "  Smoke test FAILED." -ForegroundColor Red; exit 1 }
}

Write-Host ""
Write-Host "  Pocket TTS installed." -ForegroundColor Green
Write-ProfileSnippet -Json $profileJson
