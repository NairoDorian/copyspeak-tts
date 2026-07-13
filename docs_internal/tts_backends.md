# TTS Backend Integration Guide

> **Last Updated:** 2026-07-12 (aligned with v0.1.10)
> **Purpose:** Reference for supported TTS engines, installers, and the profile model

---

## Table of Contents

- [TTS Backend Integration Guide](#tts-backend-integration-guide)
  - [Overview](#overview)
  - [Engine Model (Profiles)](#engine-model-profiles)
  - [Supported Backend Types](#supported-backend-types)
    - [Local CLI / Persistent HTTP Server](#local-cli--persistent-http-server)
    - [Cloud (native Rust HTTP)](#cloud-native-rust-http)
    - [Generic HTTP Server](#generic-http-server)
  - [Local Engine Installers](#local-engine-installers)
    - [Kitten TTS](#kitten-tts)
    - [Piper](#piper)
    - [Kokoro TTS](#kokoro-tts)
    - [Pocket TTS](#pocket-tts)
    - [Chatterbox](#chatterbox)
    - [Edge TTS](#edge-tts)
  - [Cloud Engines](#cloud-engines)
    - [OpenAI](#openai)
    - [ElevenLabs](#elevenlabs)
    - [Cartesia](#cartesia)
    - [Google Gemini TTS](#google-gemini-tts)
    - [Microsoft / Azure](#microsoft--azure)
  - [Backend Trait Interface](#backend-trait-interface)
  - [Adding a New Backend](#adding-a-new-backend)
  - [Troubleshooting](#troubleshooting)
  - [Performance Comparison](#performance-comparison)

---

## Overview

CopySpeak is designed as a TTS **orchestrator** — it doesn't bundle its own TTS engine. Instead, users install their preferred TTS engine (local, via `uv`, or a cloud API), and CopySpeak drives it through a **profile model**: a `VoiceProfile` (engine + voice + speed + pitch + effects + per-engine knobs) selects the backend at synthesis time. The Engine page owns only credentials, setup tests, and local-engine installers. See [`docs/profile-engine-settings.md`](../docs/profile-engine-settings.md) for the boundary.

> **Note:** The generic HTTP backend is first-class (`TtsEngine::Http`, configured per profile). It was not removed — it backs OpenAI-compatible and custom servers.

---

## Engine Model (Profiles)

Synthesis resolves an **effective request** (`engine`, `voice`, `speed`, `pitch`, `effects`, `engine_options`) from the active `VoiceProfile` (`src-tauri/src/config/tts.rs`). `TtsEngine` variants: `Local`, `Http`, `OpenAI`, `ElevenLabs`, `Cartesia`, `Google`, `Microsoft`, `Edge`. The default profile ships as **Edge** (`en-US-AvaMultilingualNeural`).

Local engines run two ways:
- **One-shot CLI** path (`tts/cli.rs`) for a single synthesis.
- **Persistent HTTP server** (`tts/local_tts_server.rs`, `tts/piper_server.rs`) that mirrors the CLI launch but keeps the model resident in RAM between utterances.

---

## Supported Backend Types

### Local CLI / Persistent HTTP Server

Local engines are installed via `uv` into `%LOCALAPPDATA%\CopySpeak\engines\<engine>`. The CLI path spawns the engine per request; the persistent server keeps it warm for sub-second synthesis.

**Placeholder Tokens** (see `tts/cli.rs` `CliTtsBackend::build_args`):

| Token        | Description                         |
| ------------ | ----------------------------------- |
| `{text}`     | The text to synthesize              |
| `{output}`   | Path to output WAV file             |
| `{voice}`    | Selected voice identifier           |
| `{data_dir}` | CopySpeak config directory          |
| `{raw_text}` | Actual text content (not file path) |
| `{engine_dir}` | `%LOCALAPPDATA%\CopySpeak\engines` (uv project root) |

### Cloud (native Rust HTTP)

Cloud engines (OpenAI, ElevenLabs, Cartesia, Google, Microsoft, Edge) are native Rust HTTP backends in `src-tauri/src/tts/`. Credentials live in global config; voices/models/knobs live in the profile.

### Generic HTTP Server

Any TTS engine exposing an HTTP API (OpenAI-compatible or custom) is configured **per profile** (`TtsEngine::Http`): URL template, method, headers, body template, voice, response format, timeout. Placeholder tokens: `{text}`, `{raw_text}`, `{voice}`, `{speed}`.

## Preset Configurations

### Kitten TTS (Default Preset)

[Kitten TTS](https://github.com/KittenML/KittenTTS) is an ultra-lightweight TTS engine (25-80MB) that runs on CPU without requiring a GPU.

**Features:**

- **Ultra-lightweight** — Model sizes from 25 MB (int8) to 80 MB
- **CPU-optimized** — ONNX-based inference runs efficiently without a GPU
- **8 built-in voices** — Bella, Jasper, Luna, Bruno, Rosie, Hugo, Kiki, Leo
- **24 kHz output** — High-quality audio at a standard sample rate
- **Apache 2.0 license** — Fully open source

**Installation:**

Run the PowerShell installer from the project root:

```powershell
./install-kittentts.ps1
```

Or manually:

```bash
pip install https://github.com/KittenML/KittenTTS/releases/download/0.8.1/kittentts-0.8.1-py3-none-any.whl
pip install soundfile
```

**Preset Configuration** (auto-applied when "Kitten TTS" preset is selected):

```json
{
  "tts": {
    "preset": "kitten-tts",
    "command": "python3",
    "args_template": [
      "{home_dir}/kittentts/kittentts-cli.py",
      "--text",
      "{raw_text}",
      "--voice",
      "{voice}",
      "--output",
      "{output}"
    ],
    "voice": "Jasper"
  }
}
```

**Available voices:**

- `Jasper` (default) — Natural male voice
- `Bella` — Warm female voice
- `Luna` — Soft female voice
- `Bruno` — Deep male voice
- `Rosie` — Cheerful female voice
- `Hugo` — Clear male voice
- `Kiki` — Playful female voice
- `Leo` — Neutral male voice

**Model variants:**

| Model                  | Parameters | Size  | Quality    |
| ---------------------- | ---------- | ----- | ---------- |
| `kitten-tts-nano-0.8`  | 15M        | 25 MB | Fast, good |
| `kitten-tts-micro-0.8` | 40M        | 41 MB | Balanced   |
| `kitten-tts-mini-0.8`  | 80M        | 80 MB | Highest    |

Default model is `nano` (fastest, smallest). Change via `--model` flag in CLI.

**Notes:**

- Models are downloaded automatically on first use from Hugging Face Hub
- First synthesis will be slower as the model downloads (~25-80MB depending on variant)
- CopySpeak uses `kitten_server.py` to keep the model loaded in RAM via a persistent HTTP server — subsequent syntheses are ~0.3s
- Playback speed is controlled via browser frontend playback rate (not at TTS generation level)

---

### Piper

[Piper](https://github.com/OHF-Voice/piper1-gpl) (piper1-gpl) is a fast, local offline neural TTS engine. CopySpeak keeps the model loaded in RAM via a persistent HTTP server (`tts/piper_server.rs`), eliminating reload latency. The Piper tab also supports a **CUDA GPU** mode (toggle in settings) for faster synthesis.

#### 1. Install (uv-based)

Run the project installer; it bootstraps `uv`, creates a uv project under `%LOCALAPPDATA%\CopySpeak\engines\piper`, and installs `piper-tts[http]`:

```powershell
./scripts/install-piper.ps1        # CPU
./scripts/install-piper.ps1 -Cuda  # GPU/CUDA (installs onnxruntime-gpu + NVIDIA deps)
```

Voices download into `%LOCALAPPDATA%\CopySpeak\engines\piper\voices\` (e.g. `en_US-joe-medium`). CopySpeak scans this directory on startup and populates the voice menu with all quality variations (low, medium, high).

```bash
# Manual voice download via the installed tool
uv run --project $env:LOCALAPPDATA\CopySpeak\engines\piper python -m piper.download_voices en_US-joe-medium
```

#### 2. Preset Configuration (applied automatically when "Piper" profile is selected):

```json
{
  "tts": {
    "profiles": [{
      "engine": "local",
      "engine_options": { "preset": "piper", "cuda": false },
      "voice": "en_US-joe-medium"
    }]
  }
}
```

Notes:
- `{data_dir}` / `{engine_dir}` placeholders resolve to the uv project root; the CLI/one-shot path and the persistent server share the same launch args built in `tts/cli.rs` / `tts/piper_server.rs`.
- `--cuda` is auto-injected into the launch args when CUDA mode is enabled.
- Playback speed and pitch are controlled via frontend playback rate (not at TTS generation level).

**Placeholder tokens:**

- `{data_dir}` — resolves automatically to `%APPDATA%\CopySpeak` (where models are stored)
- `{voice}` — model name, e.g. `en_US-joe-medium`
- `{output}` — temp WAV output path
- `{input}` — temp text input file path

**Available EN US voices (medium quality):**
`amy`, `arctic`, `bryce`, `danny`, `hfc_female`, `hfc_male`, `joe` (default),
`john`, `kathleen`, `kristin`, `kusal`, `l2arctic`, `lessac`, `libritts`,
`libritts_r`, `ljspeech`, `norman`, `reza_ibrahim`, `ryan`, `sam`

**Notes:**

- On Windows you may need `python` instead of `python3` depending on your Python installation
- Each voice requires its own `.onnx` + `.onnx.json` pair in `%APPDATA%\CopySpeak\`
- Playback speed and pitch are controlled via browser frontend playback rate (not at TTS generation level)

---

### kokoro-tts

[Kokoro TTS](https://github.com/hexgrad/kokoro) is a fast, high-quality local TTS engine (~500MB ONNX model).

**Features:**

- **11 built-in voices** across American and British English
- **24 kHz output** — high-quality audio
- **Direct Python API** — CopySpeak uses `kokoro_tts.Kokoro` API via `kokoro_server.py` to keep the model loaded in RAM

**Installation:**

```bash
pip install kokoro-tts
```

**Model files:** Download `kokoro-v1.0.onnx` and `voices-v1.0.bin` from [GitHub Releases](https://github.com/nazdridoy/kokoro-tts/releases). Place them in a `kokoro/` directory at the project root, or CopySpeak will auto-discover them in common installation paths.

**Preset Configuration:**

```json
{
  "tts": {
    "preset": "kokoro-tts",
    "command": "kokoro-tts",
    "args_template": ["{input}", "{output}", "--voice", "{voice}"],
    "voice": "af_heart"
  }
}
```

**Available Voices:**

- `af_heart`, `af_bella`, `af_nicole`, `af_sarah`, `af_sky` — American Female
- `am_adam`, `am_michael` — American Male
- `bf_emma`, `bf_isabella` — British Female
- `bm_george`, `bm_lewis` — British Male

**Notes:**

- CopySpeak uses `kokoro_server.py` to keep the ~500MB ONNX model loaded in RAM via a persistent HTTP server — synthesis is ~1.1s vs 7–9s cold start
- The server falls back to CLI subprocess if the `kokoro_tts` Python package isn't importable
- Model files are auto-discovered in `kokoro/` directory (project root), pip install paths, and system directories

---

---

### Pocket TTS

[Pocket TTS](https://github.com/kyutai-labs/pocket-tts) by Kyutai Labs is a lightweight CPU-optimized TTS engine (100M parameters) with voice cloning support.

**Features:**

- **Runs on CPU** — no GPU required, uses ~2 CPU cores
- **8+ built-in voices** — alba, marius, javert, jean, fantine, cosette, eponine, azelma
- **24 kHz output** — high-quality audio at standard sample rate
- **~6× real-time** — faster than real-time on modern CPUs
- **Voice cloning** — supports custom voice prompts from WAV files
- **Multi-language** — English, French, German, Portuguese, Italian, Spanish

**Installation:**

```powershell
./scripts/install-pocket.ps1      # uv-based installer (recommended)
```

Or manually:

```bash
pip install pocket-tts
```

**Preset Configuration:**

```json
{
  "tts": {
    "preset": "pocket-tts",
    "command": "uv",
    "args_template": [
      "run", "--project", "{engine_dir}/pocket", "python",
      "{engine_dir}/pocket/pocket_server.py", "--voice", "{voice}",
      "--text", "{raw_text}", "--output-path", "{output}"
    ],
    "voice": "alba"
  }
}
```

**Available Voices:**

- `alba` (default) — Natural English female
- `marius`, `javert`, `jean` — English male voices
- `fantine`, `cosette`, `eponine`, `azelma` — English female voices

**Notes:**

- CopySpeak uses `pocket_server.py` to keep the model loaded in RAM via `pocket_tts.TTSModel` API — synthesis is ~0.3–0.7s vs 5–16s cold start
- Also supports `pocket-tts serve` built-in FastAPI server for web interface

---

### Chatterbox

[Chatterbox](https://github.com/resemble-ai/chatterbox) is an open-source, zero-shot TTS with emotion control. Installed via `uv` (`install-chatterbox.ps1`) into `%LOCALAPPDATA%\CopySpeak\engines\chatterbox`; it runs as a local `http` profile (OpenAI-compatible server). See [`docs/engines.md`](../docs/engines.md) for the current installer/setup flow.

---

### Edge TTS (Microsoft)

[Edge TTS](https://github.com/rany2/edge-tts) uses Microsoft's online TTS service. In CopySpeak it is a **native Rust backend** (`tts/edge.rs`, `TtsEngine::Edge`) — no API key required, no local CLI. Voices are enumerated from a static catalog (`catalog.rs`) and refreshed live via the Edge API.

**Configuration:** select the `Edge-TTS` engine tab on the Engine page (no credentials). Pick a voice in the profile.

---

## Cloud Engines

### OpenAI

[OpenAI TTS](https://platform.openai.com/docs/guides/text-to-speech) is a cloud API with ~6 built-in voices and good quality. It is a **native Rust backend** (`tts/openai.rs`, `TtsEngine::OpenAI`); credentials live in global config and voice/model in the profile.

**Configuration:**

```json
{
  "tts": {
    "active_backend": "openai",
    "openai": { "api_key": "sk-...", "model": "tts-1", "voice": "alloy" }
  }
}
```

**Notes:**
- Playback speed and pitch are controlled via frontend playback rate (not at generation level).

### ElevenLabs

[ElevenLabs](https://elevenlabs.io) provides state-of-the-art AI speech synthesis with natural-sounding voices.

**Features:**

- High-quality neural TTS with emotional range
- 1000+ voices including cloned voices
- Multilingual support (29 languages)
- Voice customization (stability, similarity, style)
- Multiple output formats (MP3, PCM, FLAC, OGG)

**Configuration:**

```json
{
  "tts": {
    "active_backend": "elevenlabs",
    "elevenlabs": {
      "api_key": "your_api_key_here",
      "voice_id": "21m00Tcm4TlvDq8ikWAM",
      "model_id": "eleven_turbo_v2_5",
      "output_format": "mp3_44100_128",
      "voice_stability": 0.5,
      "voice_similarity_boost": 0.75,
      "voice_style": null,
      "use_speaker_boost": null
    }
  }
}
```

**Available Models:**

- `eleven_multilingual_v2` - Latest multilingual model (recommended)
- `eleven_multilingual_v1` - Original multilingual model
- `eleven_monolingual_v1` - English-only model
- `eleven_turbo_v2` - Fast generation, lower quality
- `eleven_turbo_v2_5` - Fastest generation

**Output Formats:**

| Format             | Quality    | File Size  | Notes                          |
| ------------------ | ---------- | ---------- | ------------------------------ |
| `mp3_44100_128`    | Good       | Medium     | **Recommended** - best balance |
| `mp3_44100_192`    | Excellent  | Large      | High quality MP3               |
| `mp3_44100_32`     | Acceptable | Small      | Compact size                   |
| `pcm_44100`        | Lossless   | Very Large | Uncompressed WAV-compatible    |
| `flac_44100`       | Lossless   | Large      | Compressed lossless            |
| `ogg_vorbis_44100` | Good       | Medium     | Open format                    |

**Voice Settings:**

| Setting                  | Range     | Default | Description                                     |
| ------------------------ | --------- | ------- | ----------------------------------------------- |
| `voice_stability`        | 0.0 - 1.0 | 0.5     | Higher = more consistent, Lower = more variable |
| `voice_similarity_boost` | 0.0 - 1.0 | 0.75    | Higher = closer to original speaker             |
| `voice_style`            | 0.0 - 1.0 | null    | Higher = more expressive (optional)             |
| `use_speaker_boost`      | bool      | null    | Improves clarity (optional)                     |

**Getting Started:**

1. Create an account at https://elevenlabs.io
2. Generate an API key at https://elevenlabs.io/app/settings/api-keys
3. In CopySpeak settings, select "ElevenLabs" as the backend
4. Enter your API key
5. Select a voice from the dropdown (voices are fetched from your account)

**Popular Voice IDs:**

- `21m00Tcm4TlvDq8ikWAM` - Rachel (calm, neutral)
- `EXAVITQu4vr4xnSDxMaL` - Bella (warm, conversational)
- `ErXwobaYiN019PkySvjV` - Antoni (friendly, warm)
- `MF3mGyEYCl7XYWbV9V6O` - Elli (expressive, versatile)
- `TxGEqnHWrfWFTfGW9XjX` - Josh (deep, professional)

**API Notes:**

- MP3 formats are playable by rodio immediately
- PCM formats are ideal for maximum quality but larger file sizes
- Playback speed and pitch are controlled via browser frontend playback rate (not at generation level)

---

## Backend Trait Interface

All backends implement the `TtsBackend` trait (`src-tauri/src/tts/mod.rs`):

```rust
pub trait TtsBackend: Send + Sync {
    /// Stable identifier for the backend.
    fn name(&self) -> &str;

    /// Synthesize text to WAV audio bytes.
    fn synthesize(&self, text: &str, voice: &str, speed: f32) -> Result<Vec<u8>, TtsError>;

    /// Check if the backend is available and properly configured.
    fn health_check(&self) -> Result<(), TtsError>;
}
```

### Error Types

```rust
pub enum TtsError {
    CommandNotFound(String),
    CommandFailed { code: i32, stderr: String },
    OutputNotFound(PathBuf),
    InvalidWav(String),
    IoError(std::io::Error),
}
```

---

## Adding a New Backend

### Step 1: Create Backend Module

Create `src-tauri/src/tts/my_backend.rs`:

```rust
use super::{TtsBackend, TtsError};
use async_trait::async_trait;

pub struct MyBackend {
    // Configuration fields
}

impl MyBackend {
    pub fn new(/* config */) -> Self {
        Self { /* ... */ }
    }
}

#[async_trait]
impl TtsBackend for MyBackend {
    async fn synthesize(
        &self,
        text: &str,
        voice: &str,
        speed: f32,
    ) -> Result<Vec<u8>, TtsError> {
        // Implementation
    }

    async fn health_check(&self) -> Result<bool, TtsError> {
        // Check if backend is available
    }
}
```

### Step 2: Register in mod.rs

```rust
// src-tauri/src/tts/mod.rs
mod cli;
mod my_backend;

pub use cli::CliBackend;
pub use my_backend::MyBackend;
```

### Step 3: Add Preset Configuration

Update `config.rs` to recognize the new preset:

```rust
pub fn backend_for_preset(preset: &str) -> Box<dyn TtsBackend> {
    match preset {
        "kokoro" => Box::new(CliBackend::kokoro_preset()),
        "my-backend" => Box::new(MyBackend::new()),
        _ => Box::new(CliBackend::from_config(config)),
    }
}
```

### Step 4: Document in This File

Add documentation above for users.

---

## Troubleshooting

### Common Issues

| Issue               | Solution                                                  |
| ------------------- | --------------------------------------------------------- |
| "Command not found" | Ensure TTS engine is installed and in PATH                |
| "Invalid WAV"       | Check that TTS engine outputs valid WAV format            |
| "Command failed"    | Check stderr output in logs for engine-specific errors    |
| Slow synthesis      | Consider a faster engine (kokoro, piper) or local install |

### Testing a Backend

```bash
# Test from command line first
kokoro-tts --text "Hello world" --output test.wav --voice af_nicole

# Check WAV file is valid
ffprobe test.wav
```

### Health Check

CopySpeak runs a health check on startup. If it fails:

1. Verify the command exists
2. Check permissions
3. Try running the command manually
4. Check logs for detailed error messages

---

## Performance Comparison

| Engine               | Speed        | Quality      | Offline | Size        | Backend   |
| -------------------- | ------------ | ------------ | ------- | ----------- | --------- |
| Kitten TTS (default) | ⚡ Very Fast | ⭐⭐⭐⭐     | ✅      | 25-80MB     | Local CLI |
| Piper (piper1-gpl)   | ⚡ Very Fast | ⭐⭐⭐⭐     | ✅      | ~60MB/voice | Local CLI |
| kokoro-tts (CLI)     | ⚡ Fast      | ⭐⭐⭐⭐     | ✅      | ~500MB      | Local CLI |
| Chatterbox           | 🐢 Medium    | ⭐⭐⭐⭐⭐   | ✅      | ~2GB        | Local CLI |
| Coqui TTS / XTTS-v2  | 🐢 Medium    | ⭐⭐⭐⭐⭐   | ✅      | ~1-2GB      | Local CLI |
| eSpeak-ng            | ⚡ Very Fast | ⭐⭐         | ✅      | ~5MB        | Local CLI |
| Kokoro server        | ⚡ Fast      | ⭐⭐⭐⭐     | ✅      | ~500MB      | HTTP      |
| Fish Speech 1.5      | ⚡ Fast      | ⭐⭐⭐⭐⭐   | ✅      | ~1GB        | HTTP      |
| Coqui TTS server     | 🐢 Medium    | ⭐⭐⭐⭐⭐   | ✅      | ~1-2GB      | HTTP      |
| Chatterbox server    | 🐢 Medium    | ⭐⭐⭐⭐⭐   | ✅      | ~2GB        | HTTP      |
| Edge TTS             | 🌐 Network   | ⭐⭐⭐⭐⭐   | ❌      | 0 (cloud)   | Local CLI |
| OpenAI               | 🌐 Network   | ⭐⭐⭐⭐⭐   | ❌      | 0 (cloud)   | Cloud API |
| ElevenLabs           | 🌐 Network   | ⭐⭐⭐⭐⭐⭐ | ❌      | 0 (cloud)   | Cloud API |

---

## Adding a New Backend

See [`docs_internal/development_guide.md`](./development_guide.md) and the steps in `engines-profiles-unification.md`. In short: implement `TtsBackend` in a new `tts/<engine>.rs`, register it in `tts/mod.rs` and the engine factory in `commands/tts/helpers.rs`, add a `catalog.rs` entry (so the Engine page renders a tab), and — for local engines — add an `install-<name>.ps1` under `scripts/`.
