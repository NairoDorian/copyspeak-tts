# TTS Engines, Voice Profiles, and Windows Installers Plan

> **Date:** 2026-06-22  
> **Status:** Planning / ready for implementation  
> **Scope:** Google Gemini TTS, Microsoft MAI-Voice-2, local Chatterbox, Windows-compatible engine setup, `uv`-based installers, and swappable voice profiles.

## Goal

Add a clean, Windows-first TTS engine system that supports:

1. Two new cloud engines:
   - Google Gemini TTS.
   - Microsoft MAI-Voice-2 from Azure AI Foundry / `microsoft.ai`.
2. One new local engine:
   - Resemble AI Chatterbox from <https://github.com/resemble-ai/chatterbox>.
3. A profile system where users can swap `engine + voice + speed + pitch + effects` as one unit.
4. Compatibility with the existing HTTP-serving TTS path.
5. Custom PowerShell installers/prototyping commands for Python-dependent engines, using `uv` as a hard requirement.
6. A simple migration path from the current single default TTS setup into multi-profile config.

## Non-goals

- Do not redesign all TTS settings at once.
- Do not build a plugin framework yet.
- Do not bundle Python in the first pass.
- Do not hide cloud/local engines behind one huge generic config blob.
- Do not introduce speculative metadata fields that are not used by current UI, synthesis, or install flows.
- Do not move API keys into secure storage yet. This project is alpha; plaintext app config is acceptable for now.
- Do not break existing local/OpenAI/ElevenLabs/Cartesia configs.

## Current context

Relevant existing code:

- `src-tauri/src/config/tts.rs`
  - `TtsEngine = Local | OpenAI | ElevenLabs | Cartesia`
  - `TtsConfig` stores one active backend and provider-specific nested configs.
  - Local CLI config currently has `preset`, `command`, `args_template`, `voice`.
- `src-tauri/src/tts/mod.rs`
  - Small `TtsBackend` trait: `name`, `synthesize`, `health_check`, `file_extension`, `voice_display_name`.
- `src-tauri/src/tts/cli.rs`
  - CLI runner already handles `{input}`, `{output}`, `{voice}`, `{raw_text}`, `{data_dir}`, `{home_dir}`.
  - Current Windows PATH workaround is brittle: it guesses Python/Scripts locations and tool installs.
- `src-tauri/src/commands/tts/helpers.rs`
  - Central backend factory and voice/engine display helpers.
- `src-tauri/src/commands/tts/synthesis.rs`
  - Synthesis currently resolves a backend from `cfg.tts.active_backend`, resolves one voice, synthesizes, plays, caches, and records history.
- `src/lib/types.ts`
  - Frontend mirrors backend config types.
- `src/lib/stores/playback/effects/`
  - Effects currently include `none`, `walkie_talkie`, `game_boy`.
- `src-tauri/src/control_server.rs`
  - HTTP control server can override `engine` and `effect`, but not a full voice profile.
- `docs_internal/tts_backends.md`
  - Existing TTS integration guide already frames CopySpeak as an orchestrator, not a bundled engine.
- `install-kittentts.ps1`
  - Existing PowerShell installer is useful but should not be copied forward as-is. It manages system Python manually; new installers should require/use `uv`.

Important repo rule from `AGENTS.md`:

- Do not run checks/build/tests without explicit user confirmation.
- This plan can define verification commands, but implementation should ask before running them.

## Architecture decision

Use a **hybrid engine model**:

- Cloud engines are native Rust HTTP backends.
- Local Python engines are isolated behind either:
  - the existing CLI backend, or
  - a local HTTP server profile when the engine serves OpenAI-compatible or custom HTTP.
- `uv` owns Python environments and tool execution.
- Voice profiles sit above engines and select the active backend/config/effects/playback parameters.

Why this matters:

- Cloud APIs do not need Python. Forcing cloud through Python is fake portability and real failure surface.
- Local Python engines should not poison the app with system Python assumptions.
- Profiles should not duplicate provider secrets or installer internals.
- HTTP-serving TTS must be first-class, because many local model projects eventually expose a server.

## Proposed config model

### Versioned TTS config

Add a schema version so migration is explicit and testable.

Modify `src-tauri/src/config/tts.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TtsConfig {
    pub schema_version: u32,
    pub active_backend: TtsEngine,

    // New profile layer
    pub active_profile_id: String,
    pub profiles: Vec<VoiceProfile>,

    // Existing local config, retained for compatibility/migration
    pub preset: String,
    pub command: String,
    pub args_template: Vec<String>,
    pub voice: String,

    // Provider configs
    pub openai: OpenAIConfig,
    pub elevenlabs: ElevenLabsConfig,
    pub cartesia: CartesiaConfig,
    pub google: GoogleTtsConfig,
    pub microsoft: MicrosoftTtsConfig,
    pub http: HttpTtsConfig,
}
```

Keep the old fields for now. That is not elegant, but it is the least dangerous migration path. New code should read through profiles; old fields remain as compatibility/source-of-truth during migration and as fallback.

### Engine enum

Extend `TtsEngine`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TtsEngine {
    Local,
    Http,
    OpenAI,
    ElevenLabs,
    Cartesia,
    Google,
    Microsoft,
}
```

Use `Http`, not `LocalHttp`, because profiles can name the actual local server. The backend type is HTTP; locality is just endpoint choice.

### Voice profile model

Create profile structs in `src-tauri/src/config/tts.rs` first. Split to `src-tauri/src/config/profiles.rs` later only if the file becomes noisy.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceProfile {
    pub id: String,
    pub name: String,
    pub engine: TtsEngine,
    pub voice: String,
    pub speed: f32,
    pub pitch: f32,
    pub effects: ProfileEffects,
    pub engine_options: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileEffects {
    pub enabled: bool,
    pub active_effect: crate::config::EffectId,
}
```

Defaults:

```rust
impl Default for VoiceProfile {
    fn default() -> Self {
        Self {
            id: "default".into(),
            name: "Default".into(),
            engine: TtsEngine::Cartesia,
            voice: "f786b574-daa5-4673-aa0c-cbe3e8534c02".into(),
            speed: 1.0,
            pitch: 1.0,
            effects: ProfileEffects::default(),
            engine_options: serde_json::json!({}),
        }
    }
}
```

Why `engine_options: serde_json::Value` is acceptable here:

- The shared profile contract is small and stable.
- Provider-specific knobs vary wildly.
- We should not add generic fields like `emotion`, `style`, `temperature`, `stability`, `similarity`, `seed`, etc. until the UI actually uses them.
- Provider config structs remain typed. Profile-specific overrides can stay sparse JSON until a field proves common.

### Profile JSON import/export

Profile files should be portable and compatible with HTTP-serving TTS.

Example:

```json
{
  "schema_version": 1,
  "id": "chatterbox-local-amy-walkie",
  "name": "Chatterbox Amy Walkie",
  "engine": "http",
  "voice": "amy.wav",
  "speed": 1.05,
  "pitch": 1.0,
  "effects": {
    "enabled": true,
    "active_effect": "walkie_talkie"
  },
  "engine_options": {
    "http_profile_id": "chatterbox-local",
    "model": "chatterbox",
    "response_format": "wav"
  }
}
```

Rules:

- Exported profiles do not include API keys by default.
- Profile import validates `id`, `name`, `engine`, `voice`, finite speed/pitch, and supported effect ID.
- If a profile references a missing HTTP/server config, import it but mark it as not ready in UI.
- Avoid one profile file containing the entire app config. That becomes backup/restore, not a voice profile.

## Migration plan

Current config has a single active engine/voice/effects setup. Migrate it into one default profile.

Add migration logic near config load/save, likely `src-tauri/src/config/mod.rs` or the existing config normalization point.

Pseudo-flow:

```rust
pub fn migrate_tts_config(mut tts: TtsConfig) -> TtsConfig {
    if tts.schema_version == 0 || tts.profiles.is_empty() {
        let voice = match tts.active_backend {
            TtsEngine::Local => tts.voice.clone(),
            TtsEngine::OpenAI => tts.openai.voice.clone(),
            TtsEngine::ElevenLabs => tts.elevenlabs.voice_id.clone(),
            TtsEngine::Cartesia => tts.cartesia.voice_id.clone(),
            TtsEngine::Google => tts.google.voice_name.clone(),
            TtsEngine::Microsoft => tts.microsoft.voice_name.clone(),
            TtsEngine::Http => tts.http.voice.clone(),
        };

        tts.active_profile_id = "default".into();
        tts.profiles = vec![VoiceProfile {
            id: "default".into(),
            name: "Default".into(),
            engine: tts.active_backend.clone(),
            voice,
            speed: 1.0,
            pitch: 1.0,
            effects: ProfileEffects::default(),
            engine_options: serde_json::json!({}),
        }];
        tts.schema_version = 1;
    }

    tts
}
```

Then update synthesis to resolve the active profile before backend creation:

1. Load `cfg.tts`.
2. Find `active_profile_id`.
3. If missing, fall back to first profile.
4. If no profiles, synthesize with legacy fields and log a warning.
5. Use profile `engine`, `voice`, `speed`, `pitch`, `effects`.

Do not mutate global app `effects` every time a profile is used. Instead, build an effective synthesis request object.

```rust
struct EffectiveTtsRequest {
    engine: TtsEngine,
    voice: String,
    speed: f32,
    pitch: f32,
    effects: ProfileEffects,
}
```

That object prevents profile switching from being a config side-effect soup.

## Engine backend additions

### Google Gemini TTS

Assumption: target Gemini TTS, not legacy Google Cloud Text-to-Speech. Confirm exact API endpoint during implementation from official Google docs.

Config:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GoogleTtsConfig {
    pub api_key: String,
    pub model: String,
    pub voice_name: String,
    pub output_format: String,
}
```

Defaults:

```rust
impl Default for GoogleTtsConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "gemini-2.5-flash-preview-tts".into(),
            voice_name: "Kore".into(),
            output_format: "wav".into(),
        }
    }
}
```

Implementation file:

- Create `src-tauri/src/tts/google.rs`.
- Export `pub mod google;` in `src-tauri/src/tts/mod.rs`.
- Add `GoogleTtsBackend` implementing `TtsBackend`.
- Use native Rust HTTP via `reqwest`.
- Return WAV bytes if the API can produce WAV/PCM directly.
- If the response is base64 PCM, add one small conversion helper that wraps PCM into WAV. Do not leak this into all backends.

Backend responsibilities:

- Validate API key non-empty in `health_check`.
- Map provider errors to `TtsError::Http`.
- Do not log API key or full request headers.
- Log model, voice, text length, status, elapsed time.

Open question:

- Need official Google Gemini TTS docs before implementation. Network fetch was unavailable during this planning pass, so endpoint/schema must be checked before coding.

### Microsoft MAI-Voice-2

Target source provided by user:

- <https://microsoft-foundry.github.io/forgebook/notebook/mai-voice-2/>

Network fetch failed from this environment during planning (`getaddrinfo failed`), so implementation must first inspect this page locally/in browser or from a network-enabled session.

Config:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MicrosoftTtsConfig {
    pub api_key: String,
    pub endpoint: String,
    pub model: String,
    pub voice_name: String,
    pub output_format: String,
}
```

Defaults should be conservative:

```rust
impl Default for MicrosoftTtsConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            endpoint: String::new(),
            model: "mai-voice-2".into(),
            voice_name: String::new(),
            output_format: "wav".into(),
        }
    }
}
```

Why `endpoint` default is empty:

- Azure AI Foundry deployments often have user/project-specific endpoints.
- Hardcoding a guessed endpoint is how alpha apps become haunted.

Implementation file:

- Create `src-tauri/src/tts/microsoft.rs`.
- Export `pub mod microsoft;`.
- Add `MicrosoftTtsBackend` implementing `TtsBackend`.
- Support at minimum:
  - API key header.
  - endpoint URL.
  - model/deployment name.
  - voice name.
  - output format.

Open questions for implementation:

- Exact auth header name.
- Exact request body.
- Whether response is binary audio, base64 audio JSON, or streaming chunks.
- Whether `model` means Foundry model name or deployment name.

### Chatterbox local

Target:

- <https://github.com/resemble-ai/chatterbox>

Use `uv`. Do not depend on system Python or pip.

Support two modes:

1. **CLI mode** using existing `Local` backend.
2. **HTTP mode** using the new/normalized `Http` backend.

Recommended first implementation: CLI wrapper installed into a per-engine uv-managed directory. HTTP server can follow once the CLI path is reliable.

Proposed local layout:

```text
%LOCALAPPDATA%\CopySpeak\engines\chatterbox\
  pyproject.toml
  uv.lock
  .venv\
  scripts\
    copyspeak-chatterbox.py
  voices\
    default.wav
  output\
```

PowerShell installer:

- Create `scripts/install-chatterbox.ps1`.
- Require `uv`.
- Do not install Python manually in this script.
- Use `uv python install` only if a compatible Python is missing and `uv` supports it cleanly.
- Create a `pyproject.toml` in engine dir.
- Add Chatterbox dependency from GitHub or PyPI depending on current upstream packaging.
- Write a stable wrapper script `copyspeak-chatterbox.py`.
- Print the exact CopySpeak CLI config on success.

Prototype commands:

```powershell
uv --version
$EngineDir = Join-Path $env:LOCALAPPDATA "CopySpeak\engines\chatterbox"
New-Item -ItemType Directory -Force $EngineDir | Out-Null
Set-Location $EngineDir
uv init --bare
uv add git+https://github.com/resemble-ai/chatterbox.git
uv run python .\scripts\copyspeak-chatterbox.py --text "Hello from Chatterbox" --voice default --output .\test.wav
```

CopySpeak local config for Chatterbox CLI:

```json
{
  "tts": {
    "active_backend": "local",
    "preset": "chatterbox",
    "command": "uv",
    "args_template": [
      "run",
      "--project",
      "{engine_dir}/chatterbox",
      "python",
      "scripts/copyspeak-chatterbox.py",
      "--text-file",
      "{input}",
      "--voice",
      "{voice}",
      "--output",
      "{output}"
    ],
    "voice": "default"
  }
}
```

This requires adding `{engine_dir}` placeholder to `CliTtsBackend::build_args()`.

Do not add five path placeholders. Add one:

```rust
{engine_dir} => %LOCALAPPDATA%/CopySpeak/engines
```

## Generic HTTP backend cleanup

Docs mention HTTP TTS, but current visible backend enum/code does not expose a first-class `Http` backend. Make it real.

Add:

- `HttpTtsConfig` in `src-tauri/src/config/tts.rs`.
- `src-tauri/src/tts/http.rs` implementing `TtsBackend`.
- `TtsEngine::Http` variant.
- Frontend type `"http"`.

Config:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HttpTtsConfig {
    pub profile_id: String,
    pub url_template: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body_template: Option<String>,
    pub voice: String,
    pub response_format: String,
    pub timeout_secs: u64,
}
```

YAGNI guard:

- Do not build a full request-templating language.
- Keep current placeholders: `{text}`, `{raw_text}`, `{input}`, `{voice}`, `{speed}`, `{pitch}`.
- If an engine needs complex JSON, its wrapper/server should normalize that, not the app.

## Installer strategy

### Stage 1: dev bootstrap installers

Create:

- `scripts/install-uv.ps1`
- `scripts/install-chatterbox.ps1`
- `scripts/install-kittentts-uv.ps1` or refactor existing `install-kittentts.ps1`
- `scripts/test-engine.ps1`

Rules:

- All Python engines use `uv`.
- Scripts fail early if `uv` is missing, except `install-uv.ps1`.
- Scripts print exact commands they run.
- Scripts produce a final CopySpeak config/profile snippet.
- Scripts should be idempotent with `-Force` for reinstall.
- Scripts should not edit app config automatically in v1. Print/apply later with explicit UI/IPC.

Common helper functions should live in:

- `scripts/lib/copyspeak-engine-install.ps1`

Functions:

```powershell
Require-Uv
Get-CopySpeakEngineRoot
New-EngineProject
Invoke-Uv
Test-AudioFile
Write-ProfileSnippet
```

Keep helpers boring. No PowerShell framework cosplay.

### Stage 2: end-user installer

After engine contracts stabilize:

- Integrate engine setup into Tauri installer or first-run onboarding.
- Let user choose:
  - Cloud only.
  - Lightweight local KittenTTS.
  - Higher quality local Chatterbox.
- Detect `uv` and offer install.
- Keep big model downloads explicit.

## Frontend plan

### Types

Update `src/lib/types.ts`:

- Extend `TtsEngine` union with `"http" | "google" | "microsoft"`.
- Add `GoogleTtsConfig`.
- Add `MicrosoftTtsConfig`.
- Add `HttpTtsConfig`.
- Add `VoiceProfile` and `ProfileEffects`.
- Add fields to `TtsConfig`.

### UI pages/components

Likely files:

- `src/routes/engine/+page.svelte`
- `src/lib/components/settings-page.svelte`
- `src/lib/components/quick-settings.svelte`
- `src/lib/components/hud/hud-playback-content.svelte`
- Any current engine-specific components under `src/lib/components/engine/` if present.

Add profile UI in the smallest useful shape:

1. Profile selector.
2. Rename profile.
3. Duplicate active profile.
4. Delete non-default profile.
5. Engine dropdown.
6. Voice input/select.
7. Speed slider.
8. Pitch slider.
9. Effect selector.
10. Import/export profile JSON.

Do not build drag reorder, folders, tags, favorites, cloud sync, profile marketplace, etc. Not now.

### HTTP-serving compatibility

A profile with `engine: "http"` should point to the shared HTTP TTS config/profile. For v1, one HTTP config is acceptable. If multiple HTTP servers become necessary, add `http_profiles: Vec<HttpTtsConfig>` later.

Do not prematurely build multi-server management unless the first implementation needs it.

## Backend task breakdown

### Task 1: Add config structs and migrations

Files:

- Modify `src-tauri/src/config/tts.rs`.
- Modify config load/migration code in `src-tauri/src/config/mod.rs` if needed.
- Modify tests in `src-tauri/src/config/tests.rs`.

Steps:

1. Add `schema_version`, `VoiceProfile`, `ProfileEffects`.
2. Add `Http`, `Google`, `Microsoft` to `TtsEngine`.
3. Add provider config structs with defaults.
4. Add migration from legacy single config to one `default` profile.
5. Add focused tests for default config and migration.

### Task 2: Add effective profile resolution

Files:

- Modify `src-tauri/src/commands/tts/helpers.rs`.
- Modify `src-tauri/src/commands/tts/synthesis.rs`.

Steps:

1. Add `EffectiveTtsRequest` helper.
2. Resolve active profile by ID.
3. Use profile engine/voice/speed in synthesis.
4. Keep legacy fallback.
5. Include profile ID/name in history metadata.

### Task 3: Add HTTP backend

Files:

- Create `src-tauri/src/tts/http.rs`.
- Modify `src-tauri/src/tts/mod.rs`.
- Modify `src-tauri/src/commands/tts/helpers.rs`.

Steps:

1. Implement templated HTTP request.
2. Support binary response.
3. Support JSON base64 response only if needed by actual server. Otherwise skip.
4. Add health check that validates URL presence, not full synthesis.

### Task 4: Add Google backend

Files:

- Create `src-tauri/src/tts/google.rs`.
- Modify `src-tauri/src/tts/mod.rs`.
- Modify `src-tauri/src/config/tts.rs`.
- Modify `src-tauri/src/commands/tts/helpers.rs`.
- Add commands only if voice listing is available and needed.

Prerequisite:

- Verify official Gemini TTS endpoint/schema.

Steps:

1. Implement request body from docs.
2. Implement binary/base64 audio extraction.
3. Add WAV wrapping if needed.
4. Add health check.
5. Add basic credential check IPC if consistent with existing providers.

### Task 5: Add Microsoft backend

Files:

- Create `src-tauri/src/tts/microsoft.rs`.
- Modify `src-tauri/src/tts/mod.rs`.
- Modify `src-tauri/src/config/tts.rs`.
- Modify `src-tauri/src/commands/tts/helpers.rs`.

Prerequisite:

- Inspect the provided MAI-Voice-2 ForgeBook docs.

Steps:

1. Confirm endpoint/auth/body/response.
2. Implement minimal backend.
3. Keep endpoint user-configurable.
4. Add health check.
5. Add credential check IPC only if useful.

### Task 6: Normalize CLI backend around uv

Files:

- Modify `src-tauri/src/tts/cli.rs`.
- Modify docs/internal backend docs.

Steps:

1. Add `{engine_dir}` placeholder.
2. Stop expanding PATH with a pile of guessed Python paths for new presets.
3. Prefer explicit `uv run --project ...` commands in presets/installers.
4. Keep old PATH expansion temporarily for backward compatibility.
5. Document deprecation of system-Python-style presets.

### Task 7: Chatterbox wrapper and installer

Files:

- Create `scripts/install-chatterbox.ps1`.
- Create `scripts/lib/copyspeak-engine-install.ps1`.
- Create `scripts/chatterbox/copyspeak-chatterbox.py` or generate this into engine dir from installer.
- Update `docs_internal/tts_backends.md`.

Steps:

1. Require `uv`.
2. Create engine dir under `%LOCALAPPDATA%\CopySpeak\engines\chatterbox`.
3. Install Chatterbox dependency.
4. Write wrapper script.
5. Run one synthesis smoke test only after user approves checks/manual verification.
6. Print profile JSON snippet.

### Task 8: Profile frontend

Files:

- Modify `src/lib/types.ts`.
- Modify engine/settings pages.
- Add profile component(s), likely under `src/lib/components/engine/`.

Steps:

1. Add TS interfaces.
2. Add active profile selector.
3. Add create/duplicate/delete actions.
4. Add engine/voice/speed/pitch/effect fields.
5. Add import/export JSON.
6. Keep provider advanced settings where they already are.

### Task 9: Control server profile compatibility

Files:

- Modify `src-tauri/src/control_server.rs`.

Steps:

1. Extend `SpeakRequest` with `profile: Option<String>`.
2. If profile is present, resolve it and use it for that request.
3. Keep existing `engine` and `effect` overrides as backward-compatible shorthand.
4. Do not save request-level profile overrides into config unless explicitly requested.

Proposed request:

```json
{
  "text": "Hello",
  "profile": "chatterbox-local-amy-walkie"
}
```

## Testing / validation plan

Ask before running these, per repo rules.

Rust/config:

```bash
cd src-tauri && cargo test config
cd src-tauri && cargo test tts
```

Frontend:

```bash
bun run test
bun run check
```

Manual smoke tests:

1. Existing config loads and auto-creates one default profile.
2. Existing Cartesia/OpenAI/ElevenLabs/local configs still synthesize.
3. Profile switch changes engine/voice/speed/effect together.
4. Exported profile JSON imports into a fresh config.
5. HTTP profile can call an OpenAI-compatible local TTS server.
6. Chatterbox installer creates uv project and wrapper.
7. Chatterbox wrapper produces a valid WAV.
8. Google backend speaks with Gemini TTS using a real key.
9. Microsoft backend speaks with MAI-Voice-2 using a real endpoint/key.
10. History records profile metadata without breaking existing history UI.

## Risks and sharp edges

- **Provider docs volatility:** Gemini TTS and MAI-Voice-2 may still be preview APIs. Keep implementations small and isolated.
- **Audio formats:** Some APIs may return PCM/base64/streaming instead of WAV. Add conversion only at the backend boundary.
- **Python dependency hell:** Chatterbox may require specific Python/CUDA/Torch versions. `uv` helps, but GPU installs may still be ugly.
- **Windows PATH:** Stop trying to guess every Python Scripts dir. Explicit `uv run --project` is cleaner.
- **Profile side effects:** Profiles must not mutate global config on every synthesis. Resolve effective request instead.
- **Secrets in profiles:** User accepted plaintext app config, but exported profiles should still omit API keys by default. Otherwise sharing a profile leaks keys. Dumb footgun.
- **UI sprawl:** Engine settings can become a swamp. Keep v1 profile UI compact.

## Recommended implementation order

1. Config schema + migration.
2. Effective profile resolution in synthesis.
3. Frontend types + minimal profile selector/editor.
4. HTTP backend first-class support.
5. `uv` CLI placeholder/install helper foundation.
6. Chatterbox installer/wrapper.
7. Google Gemini backend.
8. Microsoft MAI-Voice-2 backend.
9. Polish docs and onboarding.

This order is intentional. Profiles and HTTP are the foundation. Cloud engines are then just additional providers, not architectural surgery every time a new model drops.

## Documentation updates after implementation

Update:

- `docs_internal/tts_backends.md`
- `docs_internal/architecture.md`
- `docs_internal/development_guide.md`
- `docs_internal/roadmap.md`
- `CHANGELOG.md`

Add examples for:

- Profile JSON.
- Chatterbox `uv` install.
- Google Gemini TTS config.
- Microsoft MAI-Voice-2 config.
- HTTP-serving TTS profile.

## Final take

The wrong abstraction is “one more provider panel per engine forever.” That scales badly.

The right abstraction is:

```text
Profile -> effective synthesis request -> backend -> audio bytes -> effects/playback/history
```

Keep the profile layer tiny. Keep providers isolated. Use `uv` for Python engines. Make HTTP real. Then adding engines stops being a rewrite and becomes boring plumbing.
