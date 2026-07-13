# CopySpeak Project Overview

**Last Updated:** 2026-07-12 (aligned with v0.1.10)

## What This Is

CopySpeak is a Windows desktop application that orchestrates AI Text-to-Speech engines. It silently monitors the clipboard and reads text aloud when the user copies the same text twice in quick succession (double-copy trigger), via a global hotkey, or manually. It runs in the system tray, staying out of the way until needed.

## Core Value

**Double-copy → instant speech must be flawless.** If the trigger misfires or the voice takes too long, the app is useless.

## Stack

- **Frontend**: Svelte 5 + SvelteKit + Tailwind CSS v4 + shadcn-svelte + mode-watcher
- **Backend**: Rust (Tauri v2)
- **IPC**: `commands/` modules → `main.rs` → frontend via `@tauri-apps/api`
- **State**: `Mutex<T>` via Tauri's `app.manage()`

## Constraints

- **Platform**: Windows only — uses Win32 clipboard APIs; no cross-platform requirement
- **Tech Stack**: Must remain Tauri v2 + Svelte 5; no framework changes
- **Lightweight**: App should stay minimal in tray; no background CPU drain

## Current State

The app is at **v0.1.10** and is in active development on the `main` branch. The core clipboard-to-speech flow is complete and working. A **profile model** drives synthesis: users create named voice profiles (engine + voice + speed + pitch + effects + per-engine knobs) and switch between them from the footer. The Engine page owns only credentials, setup tests, and local-engine installers; voices/models/knobs live in profiles (see [`profile-engine-settings.md`](../docs/profile-engine-settings.md)).

Supported engines (see [`docs/engines.md`](../docs/engines.md) for the full matrix): cloud — Edge-TTS, OpenAI, ElevenLabs, Cartesia, Google Gemini TTS, Microsoft / Azure; local — Kitten, Piper, Kokoro, Pocket, Chatterbox (installed via `uv` into `%LOCALAPPDATA%\CopySpeak\engines\<engine>`). A generic HTTP backend is also available for OpenAI-compatible / custom servers.

## Key Decisions

| Decision                          | Rationale                                                                     | Status          |
| --------------------------------- | ----------------------------------------------------------------------------- | --------------- |
| Double-copy trigger (not hotkey)  | Zero-friction; no shortcut to memorize                                        | ✓ Good          |
| HUD overlay with waveform         | Real-time visual feedback during playback and clipboard operations             | ✓ Implemented   |
| Brutalist UI design               | Distinctive aesthetic, hard edges, muted palette                              | ✓ Good          |
| Engine route (`/engines`)    | Engine config too complex for Settings; deserves its own page                  | ✓ Complete      |
| Profile model over single backend | Swap engine+voice+speed+pitch+effects as one unit; cleaner config boundary    | ✓ Implemented   |
| `uv`-managed local engines        | Avoid system-Python assumptions; isolate engine environments                  | ✓ Implemented   |
| HTTP as first-class backend       | Many local models eventually expose a server; keep it supported per-profile   | ✓ Implemented   |

## Existing Documentation

- **[Architecture](architecture.md)**
- **[Requirements & Traceability](requirements.md)**
- **[Development Guide](development_guide.md)**
- **[TTS Backends](tts_backends.md)**
- **[Engines & Profiles](engines-profiles-unification.md)**
- **[Brutalist Design](brutalist_design.md)**
- **[Roadmap](roadmap.md)**
- **[HUD Overlay](hud-overlay.md)**
- **[Event System](event-system.md)**

## Fork Context

This is the `NairoDorian/copyspeak-tts` fork; upstream is `ilyaizen/CopySpeak`. See [`FORK_VS_UPSTREAM.md`](../FORK_VS_UPSTREAM.md) for the relationship and divergence history.
