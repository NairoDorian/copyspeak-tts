# original_copyspeak — Reference Vendored Upstream

> Debugging aid. This folder is a **shallow clone of the upstream
> `ilyaizen/CopySpeak`** and is **gitignored** (`/original_copyspeak`). It exists
> only as a read-only reference for diffing the fork against upstream to recover
> behaviour that was lost during the `git merge -X ours` reconciliation.

## Why it exists

This fork (`NairoDorian/copyspeak-tts`) is based on `ilyaizen/CopySpeak` and then
diverged with the improvements below. After merging upstream via
`git merge -X ours` (fork wins), some still-needed upstream IPC commands and UI
wiring were dropped. Diffing against `original_copyspeak` is the fastest way to
see exactly what the original tool did and recover it.

```powershell
# Refresh the reference (shallow, fast)
git clone --depth 1 https://github.com/ilyaizen/CopySpeak.git original_copyspeak
```

Upstream remote is already configured in this repo:

```powershell
rtk git fetch upstream
```

## What the fork added on top of upstream

- **Persistent RAM model loading** — local engines (Kitten/Kokoro/Pocket) run
  through long-lived persistent HTTP servers (`local_tts_server.rs`,
  `piper_server.rs`) instead of spawning a fresh process per clip, so models stay
  resident in RAM between reads.
- **Piper GPU (CUDA)** — `install-piper.ps1` installs Piper with a specific,
  **order-dependent** dependency sequence so the CUDA build resolves correctly;
  `CliTtsBackend` injects `--cuda` and the NVIDIA DLL `PATH` when enabled.
- **Bumped dependencies** — engine packages, `uv` tooling and Tauri deps were
  upgraded past upstream's pins.
- **Pocket TTS** — fork-added local engine (`pocket-tts`) with
  `install-pocket.ps1`, `pocket_server.py` and a `pocket-tts` catalog entry.
- **Unified engine catalog + profiles** — `tts/catalog.rs`, profile-driven
  `engine_options`, and a single `/engines` UI (the fork's old `/engine` route was
  deleted). See `FORK_VS_UPSTREAM.md` for the full matrix.

## Debugging: how to find merge-dropped IPC

Two diffs catch the "lost in merge" class of bug (commands the frontend calls
but the backend never registered):

1. **Frontend `invoke()` vs registered commands** — extract every
   `invoke(<...>("name", …)` in `src/` and confirm each `name` appears in
   `main.rs`'s `generate_handler!`. (Watch generic-typed calls
   `invoke<{…}>("name", …)` — the `<…>` sit between `invoke` and `(`.)
2. **Defined-but-unregistered commands** — extract every `#[tauri::command] pub
   fn` in `src-tauri/src` and confirm each is listed in `main.rs`.

### Findings (fixed)

- `test_tts_engine_config` + `test_local_engine` — invoked by `/engines`
  (`engine-setup.svelte:85`/`:103`); dropped in merge. Re-added to
  `health.rs`, registered in `main.rs`. (`test_local_engine` runs a real short
  synthesis; `local_engine_spec` covers `piper`, `kitten`, `chatterbox`,
  `kokoro`, and the fork's `pocket`.)
- `list_post_processing_models` — defined in `commands/config.rs`,
  invoked by `post-processing-settings.svelte` ("Refresh models"); its
  `generate_handler!` line was dropped. Re-registered in `main.rs`.

### Known gaps not from upstream (flag, don't auto-fix)

- `get_data_dir` / `get_home_dir` — invoked by the **orphaned** `local-engine.svelte`
  (not in upstream either; no backend implementation exists). Only matter if that
  component is ever rendered.
- `speak_now_with_profile` — defined in the fork backend but unregistered **and**
  unused by the frontend (dead code, not merge-loss).

### Key reference files in `original_copyspeak`

- `src-tauri/src/commands/tts/health.rs` — original `test_tts_engine_config` /
  `test_local_engine` implementations.
- `src-tauri/src/main.rs` — original `generate_handler!` (commands
  `test_tts_engine_config`, `test_local_engine` at lines ~653 / ~705).
- `src-tauri/src/commands/config.rs` — original `list_post_processing_models`.
