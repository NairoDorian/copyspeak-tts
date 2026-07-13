# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Test Engine button in `openai-engine.svelte` and `elevenlabs-engine.svelte`** — Both cloud engine
  components now expose an in-component "Test Engine" button (Testing… / success / error states),
  conditionally rendered only when the engine is the active backend. Mirrors `local-engine.svelte`.
  All 48 frontend tests pass.
- **RTK (Rust Token Killer) tooling** — Installed `rtk v0.42.4` globally. Added install instruction
  (`cargo install --git https://github.com/rtk-ai/rtk`) to `AGENTS.md` and `README.md`.

### Fixed

- **Engine install + catalog IPC commands were unregistered / missing** — `install_engine`
  (called by onboarding and the engine setup UI) was never added to Tauri's `generate_handler!`,
  so engine installers could not launch. Added the missing `list_tts_engines` and
  `list_tts_voices` commands (backed by `tts/catalog.rs`) that the Voice Profiles UI depends on,
  and registered `set_active_profile` so the `profiles::*` glob re-export is no longer dead.
- **Streaming (`speak_queued`) ignored per-profile engine options** — The queued path built its
  backend from the legacy global per-engine config instead of the active profile's
  `engine_options` (model id, output format, voice overrides). It now uses
  `create_backend_from_effective` like `speak_now`, and derives the active engine from the active
  profile rather than the stale `active_backend` mirror.
- **`active_backend` mirror went stale after a full-config save** — `set_config` now calls
  `sync_active_backend_mirror` before persisting, keeping telemetry keys, history filenames and
  the HUD provider label correct. `speak_history_entry` no longer reads the dead mirror binding.
- **HUD provider/voice label read legacy fields** — `hud::get_provider_voice` now resolves the
  active profile (engine, local preset, voice) instead of the empty `preset`/`active_backend`
  mirror, so the HUD shows the real engine (Piper/Kokoro/Pocket/…) and voice.
- **Playback speed/pitch could be `undefined`** — `src/routes/+layout.svelte` coerces the
  serde-skipped `playback_speed`/`pitch` to finite defaults (1.0) instead of passing `undefined`
  into the playback store.
- **Consolidated engine UI onto `/engines`** — Removed the fork's duplicate `/engine` route and
  `engine-page.svelte` (plus `engine-page.test.ts`); navigation already points at upstream's
  `/engines` (`engine-setup.svelte` → `engine-panel.svelte`), which is now the single source of
  truth. Pocket TTS — previously only reachable from the deleted page and missing its installer —
  is now registered in `engine-meta` `LOCAL_PRESETS` and gains `install-pocket.ps1` (mapped in
  `install.rs`), so all five local engines (Kitten, Piper, Kokoro, Pocket, Chatterbox) are
  configurable from `/engines`. `local-engine.svelte` is now only exercised by its test
  (orphaned UI). Updated `FORK_VS_UPSTREAM.md` accordingly.
- **`clear_history` orphaned every cached audio file** — `clear_history` now deletes each entry's
  `output_path` from disk (mirroring `delete_history_entry`) instead of leaving WAVs to accumulate.
- **Audio-format conversion used fixed temp-file names (race)** — `convert_audio_format` now uses a
  process/sequence-unique temp path per call, preventing concurrent conversions from clobbering each
  other's input/output.
- **Groq credential check could hang indefinitely** — `check_groq_credentials` now builds its blocking
  HTTP client with a 10s timeout.
- **Backward seek was broken** — `AudioPlayer::seek_relative` took a `Duration` (always non-negative) so
  the subtract branch was unreachable; it now takes a signed `i32` and `skip_backward` actually seeks back.
- **CSV history export produced malformed rows** — Fields containing commas, quotes or newlines are now
  RFC 4180 quoted/escaped instead of only swapping `,` → `;`.
- **Engine "Test" buttons called unregistered IPC commands** — The `/engines` UI invokes
  `test_tts_engine_config` (cloud engines, `engine-setup.svelte:85`) and `test_local_engine`
  (local engines, `engine-setup.svelte:103`); both were dropped during the upstream merge and
  produced `Command test_local_engine not found` at runtime. Re-added both to `health.rs`
  (registered in `main.rs`), porting the upstream real-synthesis local test and adding a `pocket`
  fixture (the fork's added local engine) that routes through the persistent `pocket_server.py`.
- **`list_post_processing_models` was defined but never registered** — The command
  exists in `commands/config.rs` (used by `post-processing-settings.svelte` →
  "Refresh models") but its `generate_handler!` line was dropped in the upstream
  merge, so the model list errored with `Command list_post_processing_models not
  found`. Re-added the registration in `main.rs`.

### Fixed

- **Rust borrow-after-move in `synthesis.rs`** — `app.state::<FragmentQueue>()` borrowed `app` while
  the subsequent `speak_queued_internal(app, ...)` consumed it. Fixed by cloning `app` before `.state()`
  (`app.clone().state::<...>()`).

- **WAV formatting and stream truncation tests** — Added unit tests to `wav.rs` verifying that sample rate mismatches are correctly rejected on WAV concatenation, and that truncated or corrupted audio streams are parsed robustly.

### Changed

- **Unified streaming playback for all long texts** — Refactored `speak_now` to delegate to the fragment-by-fragment queue path (`speak_queued_internal`) when pagination criteria are met and file output is disabled. This unifies hotkey triggers and the Play page UI to stream audio immediately instead of waiting for full sequential synthesis.
- **Fast Stopped Python health checks** — Optimized health checks for python-based CLI engines when Stopped. Spawns a fast python environment check (`python -c "import <module>"`) to verify interpreter and package availability in milliseconds, eliminating the multi-second model-loading overhead of a test synthesis.

### Maintenance

- **Restored OpenAI and ElevenLabs component tests** — Re-imported and adapted the thorough original test suites (15 tests each) to verify settings inputs, validation states, and UI alert handling under Svelte 5 and the mock environment.
- **Consolidated Piper voice listings** — Moved duplicated voice lists in `cli.rs` into a single `const PIPER_KNOWN_VOICES` source of truth.


- **Dependency modernization (all ecosystems)** — Rust stable 1.97.0 + `cargo update`
  (tauri 2.11.5, zbus 5.17, time 0.3.53, etc.); JS/bun bumped to latest within semver ranges
  (@sveltejs/kit 2.69, svelte 5.56.4, vite 8.1.4, vitest 4.1.10, typescript ~6.0.3,
  @tauri-apps/* 2.11.x, prettier 3.9.5, tailwindcss 4.3.2). `reqwest` previously moved 0.12 → 0.13.
- **Test environment fix** — Replaced the broken `jsdom` test environment with **happy-dom** and
  forced `--environment happy-dom` in the `test` / `test:watch` / `test:ui` scripts (the
  `@testing-library/svelte/vite` plugin overrides the config `environment` field). `bun run test`
  now passes all 48 frontend tests.
- **Restored fork-only files dropped by the upstream merge** — `install-kittentts.ps1`,
  `kittentts-cli.py`, `skills/cli-anything-copyspeak/SKILL.md`, `local-engine.test.ts`,
  `eng02-minimal.test.ts`, `effects-settings.svelte`, `routes/engine/+page.svelte`,
  `LICENCE.txt`, `static/screen-v0.1.4.png`.

### Added

- **`setup-venv-cuda-v2.ps1` — CUDA 13 GPU environment script** — New PowerShell setup script that creates a clean `.venv-v2` Python environment targeting `onnxruntime-gpu 1.27.0` (stable PyPI, CUDA 13). Installs the correct NVIDIA CUDA 13 runtime wheels (`nvidia-cuda-runtime`, `nvidia-cublas`, `nvidia-cufft`, `nvidia-cusolver`, `nvidia-cusparse`, `nvidia-nvjitlink`, `nvidia-cudnn-cu13`) with verified `win_amd64` wheels only, and runs an end-to-end CUDA provider verification matching the runtime DLL injection logic.

### Fixed

- **NVIDIA DLL injection broken for namespace packages** — `get_nvidia_dll_paths` in both `piper_server.rs` and `cli.rs` used `nvidia.__file__` to locate the NVIDIA site-packages directory; `nvidia` is a Python namespace package whose `__file__` is always `None`, causing a silent `TypeError` crash, the function returning `None`, and zero PATH injection happening — meaning the Piper/onnxruntime child process could never find CUDA or cuDNN DLLs. Fixed to use `list(nvidia.__path__)[0]`.
- **CUDA 13 DLL subdirectory not injected into PATH** — CUDA 13 wheels consolidate all runtime DLLs one level deeper than CUDA 12 (`nvidia/cu13/bin/x86_64/*.dll` vs `nvidia/<pkg>/bin/*.dll`). The glob pattern `nvidia/*/bin` found the directory but not the DLLs inside it. Extended the glob to also collect immediate subdirectories of each `bin/` folder, so both layouts resolve correctly.
- **`get_nvidia_dll_paths` hard-coded CUDA 12 subpackage names** — The Python snippet enumerated a fixed list of nine CUDA 12 subdirectory names (`cublas`, `cuda_runtime`, `cudnn`, etc.) which do not exist in the CUDA 13 wheel layout. Replaced with a `glob.glob` over all subdirectories of the `nvidia` namespace root, making DLL discovery layout-agnostic and forward-compatible with future NVIDIA packaging changes.
- **Local TTS persistent HTTP server never started under `uv`** — `local_tts_server.rs` only recognized `python`/`py` interpreters and looked for dev scripts next to the binary, so with the default `command: "uv"` the model-kept-in-RAM server never launched and every kitten/kokoro/pocket utterance fell back to a slow one-shot CLI. `build_server_launch` now mirrors the working one-shot CLI path: `uv run --project <engine_dir>/<engine> python <engine>_server.py`. `install-kittentts.ps1` now also ships `kitten_server.py` into the uv project so the persistent server can launch. Added unit tests for the launch builder.
- **Markdown sanitization ignored per-feature config toggles** — `strip_markdown` accepted a `MarkdownSanitizationConfig` but never read it, so disabling e.g. `strip_inline_code` had no effect. Each pass is now gated on its `config` toggle. Also fixed a pre-existing `cargo test` compile error (a test called `strip_markdown` with the wrong arity) and two `validate()` tests that were stale after the profile migration (they poked the legacy global `active_backend`/`command`; validation now runs against the active profile).
- **GUI froze / became unclickable on first launch** — `audioEl.playbackRate` was assigned `undefined` because `PlaybackConfig` serializes `playback_speed`/`pitch` with `#[serde(skip_serializing)]` (legacy, migrated to profiles), producing `NaN` inside the playback `$effect` and tearing down the entire Svelte tree (mouse, keyboard, and TAB all dead). `syncPlaybackConfig` and the play-page effect now coerce to finite defaults (volume 100, speed/pitch 1.0); the HUD window gained `core:window:allow-show`/`allow-hide`/`allow-set-focus` (fixes `window.show not allowed on window "hud"`).

### Added

- **`set_playback_state` IPC command** — The frontend `<audio>` element now reports playback state to the backend (`AudioPlayer::set_playback_state_reported`), reviving the tray busy icon, tray-click pause/resume during playback, and backend HUD auto-hide, which were dead because the backend tracked a rodio `Sink` that no code path ever created.
- **In-flight job preemption** — `speak_now`/`speak_queued`/`speak_history_entry` set `ABORT_REQUESTED` before acquiring the global synthesis lock, so a new double-copy/hotkey trigger cancels the running job at its next fragment boundary instead of queuing behind the entire remaining synthesis (previously up to minutes of dead wait on long local batches).
- **Piper re-prewarm after abort** — `do_abort_synthesis` now restarts the Piper server in the background after unloading it, so the next utterance doesn't pay a multi-second cold start.

### Changed

- **Audio emitted before history disk IO** — In all four synthesis paths (`handle_playback_output`, sequential and parallel queued synthesis, `speak_history_entry`), the `audio-ready`/`audio-fragment-ready` emit now happens before the WAV file write and the full `history.json` rewrite, removing disk IO from the copy-to-first-audio path.
- **Native double-copy dispatch** — The clipboard listener now calls `speak_queued` directly (`spawn_speak_queued`) instead of round-tripping the full text through the hidden main webview via a `speak-request` event; this removes two full-text IPC serializations per trigger and fixes triggers being silently dropped before the frontend finished mounting (including the first-run session, where the listener was never registered after onboarding).
- **Windows audio preroll only on the first fragment** — Continuation fragments of a streamed playback skip the 200ms silence preroll and its decode→re-encode, removing a fixed 200ms gap per fragment; they now play through the raw-bytes fast path.
- **Clipboard-change event payload capped** — Every system-wide copy used to serialize the full clipboard text (multi-MB) to all webviews even when listening was paused; the event now carries a 200-char preview and rapid duplicate events (multiple `WM_CLIPBOARDUPDATE` per Ctrl+C) are deduplicated.
- **Cloud TTS requests bounded** — OpenAI, ElevenLabs, Cartesia, and Groq post-process clients now set `connect_timeout(10s)` and an overall request `timeout` (120s TTS / 30s Groq); a hung request can no longer hold the global synthesis lock forever.
- **Cartesia synthesizes `pcm_s16le` instead of `pcm_f32le`** — Halves download and IPC payload size, and produces format-1 WAV that the app's own WAV parser accepts (float WAV is format 3, which broke envelope extraction, duration, and fragment concatenation).
- **Blocking IPC commands made async** — `replay_cached`, `play_history_entry`, `list_elevenlabs_voices`, `get_elevenlabs_voice_by_id`, `check_*_credentials`, and `test_tts_engine` ran file IO, multi-MB base64 encoding, or full network roundtrips (and even CLI test synthesis) on the WebView2 main thread, freezing the UI; they now run on tokio's blocking pool. The voices commands also no longer hold the `AppConfig` mutex across the network fetch (which blocked double-copy detection).
- **Hotkey/tray speak path sanitizes like double-copy** — `spawn_speak` now applies `sanitize_text` and `max_text_length` truncation to clipboard text, so the same copy is spoken identically regardless of trigger mechanism (and the clipboard is read once instead of twice).
- **Atomic JSON persistence** — `config.json`, `history.json`, and `telemetry.json` are written via write-to-tmp + rename; a crash mid-write can no longer truncate them (loaders silently reset to defaults, losing API keys / up to 1000 history entries).
- **`audio-fragment-ready` honors `is_final`** — The fragment queue no longer declares playback complete whenever it momentarily drains; if synthesis is running behind playback it waits for the next fragment, fixing the HUD disappearing mid-utterance. A `pagination:failed` listener stops playback cleanly when synthesis dies mid-stream.
- **Faster fragment decode path** — base64 decode uses an indexed loop instead of a per-byte callback plus redundant copy, and `_originalBytes` is kept on a separate buffer from the one `decodeAudioData` detaches (the raw-bytes fast path previously served an empty Blob).
- **HUD amplitude loop gated on `hud.enabled`** — The 60fps FFT + cross-window IPC waveform stream no longer runs when the HUD is disabled.
- **Hot-path console logging removed** — Dropped unconditional `console.log` calls from the audio fragment pipeline and play-page config effect.

### Fixed

- **ElevenLabs slider warnings** — Resolved Svelte 5 non-reactive property binding warnings on stability and similarity boost sliders by implementing the local state + `$effect` + `onchange` pattern.
- **Abort now actually stops local sequential synthesis** — The sequential fragment loop only checked the queue stop flag (set by a command no frontend code calls), never `ABORT_REQUESTED`; aborting a multi-fragment local batch killed the Piper server, paid a cold CLI model reload to re-synthesize the aborted fragment, restarted the server, and kept speaking. The loop now honors the abort flag, and `CliTtsBackend` skips the CLI fallback (and redundant server unloads) when the failure was abort-induced.
- **Aborted `speak_now` no longer emits empty audio** — An aborted paginated synthesis returned `Ok(empty)` and proceeded to record poisoned telemetry (skewing ETA estimates and adaptive fragment sizing), persist a bogus `success: true` history entry with a zero-byte audio file (which the cache lookup then replayed for the same text forever), re-show the HUD, and ship empty base64 to the frontend.
- **User abort of parallel synthesis reported as failure** — The abort branch now sets `is_aborted`, so aborts return `Ok` and stop emitting a contradictory `pagination:failed` after `pagination:stopped`.
- **`FragmentQueue` stop flag was a one-way latch** — One `stop_queue` call permanently short-circuited every subsequent sequential synthesis until app restart; `clear()` now re-arms the flag.
- **Stop race in playback store** — `handleStop` during an in-flight decode/render let the stale fragment start playing after the user stopped it; a generation token now invalidates in-flight work.
- **Fragment queue stall on decode error** — A failed `decodeAudioData` left `isProcessing` stuck forever, blocking all subsequent playback; the queue now advances past the failed fragment.
- **Stale-config auto-save in play page** — The debounced auto-save `$effect` persisted the whole (possibly stale) config on every page mount and silently reverted engine switches saved by the footer; it is now gated on a load-time snapshot and the page reloads on `config-changed`.
- **Engine availability indicators never updated** — `Map` mutations inside `$state` are not reactive in Svelte 5; replaced with `SvelteMap`.
- **`list_history` sort inverted** — `sort_order: "newest"` (the default) returned the oldest entries; it also cloned all 1000 entries before paginating (now paginates first).
- **Duplicate history entry IDs** — `generate_id()` derived its "random" suffix purely from the millisecond timestamp, so fragments added in the same millisecond collided (corrupting file tracking and delete/replay-by-id); a monotonic counter now guarantees uniqueness.
- **History file-tracking drift** — `cleanup_old_entries`, `delete_history_entry`, and `delete_history_batch` removed entries and files but never unlinked `file_tracking` maps, inflating `history.json` and statistics unboundedly; the 1000-entry rotation now also deletes the evicted entry's audio file instead of orphaning it forever.
- **Orphan cleanup deleted user exports** — The daily orphaned-file scan ran over the directory `export_history` writes into and flagged every untracked file; exports (`.json`/`.csv`) and temp files are now excluded.
- **Pagination bypass on sparse punctuation** — A trailing remainder larger than `fragment_size` was pushed as a single fragment (e.g. "Hi. " + 10k unpunctuated chars became ONE fragment, making time-to-first-audio equal full synthesis); it is now force-split like the lone-fragment case.
- **`*` bullet lists lost sentence pauses** — `strip_bold_italic` ran before `strip_lists` and destroyed `*` bullet markers, so list items never received their terminal period.
- **Mutex poisoning hardening completed** — Replaced the remaining 80 bare `.lock().unwrap()` calls (including one inside the Win32 clipboard window procedure, where a poisoned mutex would panic across an FFI boundary and abort the process) with the `lock_or_recover!` macro.
- **Win32 clipboard memory leak** — `set_clipboard_text` now frees the `GlobalAlloc`'d handle on the lock-failure and `SetClipboardData`-failure paths (ownership only transfers on success).
- **Removed `unsafe impl Send/Sync for AudioPlayer`** — All fields are auto-`Send`; the handle is always used behind a `Mutex`, so the manual unsafe impls bypassed compiler checking for no benefit.

### Security

- **Control server token length check** — The constant-time token comparison truncated the length XOR to `u8`, accepting Authorization headers whose length differs by a multiple of 256; lengths are now compared untruncated.

- **Piper model lifecycle status indicator in footer** — The bottom-left footer now shows real-time Piper model state: a spinner with "Loading model..." during ONNX model load, "Loading in VRAM..." (CUDA) or "Warming up..." (CPU) during JIT compilation, a green dot with model name + CUDA badge when ready, and a red dot with error message on failure. Status transitions are emitted from the Rust backend via `piper-status-changed` events at each phase (loading → warming_up → ready → error → stopped).
- **`get_piper_server_status` IPC command** — New Tauri IPC command `get_piper_server_status` returns `PiperServerStatus` (running, model, port, cuda, ready) for frontend polling on initial load.
- **`PiperStore` reactive frontend store** — New `piper-store.svelte.ts` (Svelte 5 runes) tracks model lifecycle state, listens to `piper-status-changed` events in real time, and exposes getters (`isLoading`, `isWarmingUp`, `isReady`, `isError`, `isStopped`, `statusLabel`).
- **`PiperStatusPayload` and `PiperServerStatus` TypeScript types** — Added type definitions for model status events and IPC responses in `src/lib/types.ts`.

- **Persistent Piper TTS RAM caching** — Keeps the Piper voice model loaded in RAM using a background HTTP server process (`piper.http_server`) on a free localhost port. Speeds up consecutive synthesis triggers to near-instantaneous.
- **CUDA/GPU acceleration for Piper** — Added a CUDA option to the UI and configuration, enabling GPU-accelerated local inference if available.
- **PowerShell setup scripts** — Added `setup-piper-cpu.ps1` and `setup-piper-cuda.ps1` scripts to automate installing `piper-tts[http]` (for persistent RAM caching) along with their corresponding CPU or GPU/NVIDIA library dependencies.
- **Automatic CUDA DLL path discovery** — Rust backend now queries Python on startup to locate pip-installed NVIDIA runtime paths (like `cublas`, `cudnn`, etc.) and adds them to the environment `PATH` when spawning the server, making GPU acceleration work fully out-of-the-box on Windows.
- **Dynamic local Piper voice discovery** — Automatically scans the `piper-voices` folder and lists all downloaded quality variations (low, medium, high) dynamically in the dropdown.
- **"Unload Model" system tray action** — Added a menu item to the tray context menu to manually terminate the background server and unload the model from RAM.
- **New Rust IPC commands** — Added `get_local_piper_voices` to discover available models and `unload_piper_model` to allow manual unloading of the cached model.
- **Server teardown hooks** — Hooked server termination into Tauri's `RunEvent::Exit` so the background python server is always cleaned up when the app is closed.
- **History bulk operations** — Added multi-select checkboxes to every history entry, a "Clear All" button, and a bulk actions toolbar (Select All, Clear Selection, Export Selected, Delete Selected with double-click confirmation) wired through the existing `HistoryBulkActions` component.
- **History "Clear All" confirmation dialog** — Added an `AlertDialog` confirmation before wiping the entire history, with new i18n keys (`history.clearAll`, `history.clearAllDialogTitle`, `history.clearAllDescription`, `history.confirmClearAll`).
- **Reusable HTTP connection pooling for all cloud TTS backends** — Each backend (`ElevenLabsTtsBackend`, `OpenAiTtsBackend`, `CartesiaTtsBackend`, and Groq post-process) now holds a single `reqwest::Client` with `tcp_nodelay`, `tcp_keepalive(60s)`, and `pool_max_idle_per_host(2)` configured in `new()`, eliminating TLS handshake + TCP connection setup per synthesis request.
- **Precomputed `Bearer` header in OpenAI backend** — The `Authorization: Bearer <key>` header is now computed once in `OpenAiTtsBackend::new()` and reused across all requests, avoiding per-request `format!()` allocations.
- **NVIDIA DLL paths cached via `OnceLock`** — The expensive `python -c "import nvidia..."` subprocess call that discovers GPU library paths now runs only once per app lifetime, cached in a static `OnceLock<Option<String>>`.
- **Adaptive fragment sizing from telemetry** — `pagination::adaptive_fragment_size()` uses per-engine `chars_per_ms` telemetry (3+ samples required) to dynamically scale the pagination fragment size: fast engines (>1.0 chars/ms) get 3× the base size (capped at 2000), moderate engines (0.3–1.0 chars/ms) get 2× (capped at 1500), slow or unknown engines keep the default.
- **Parallel cloud fragment synthesis** — For cloud backends (OpenAI, ElevenLabs, Cartesia) with multiple paginated fragments, `speak_queued` now synthesizes up to 3 fragments concurrently via `tokio::task::JoinSet`, then emits results in index order. Local Piper backend continues to use sequential loop to avoid server process/thread contention.
- **Pre-decode next fragment during playback** — When a fragment starts playing, the `PlaybackStore` asynchronously base64-decodes and `decodeAudioData()` the next queued fragment in the background, so when the current fragment ends the next one is ready to play instantly with zero decode gap.
- **Piper server pre-warm at app startup** — When the active backend is Local with the `piper` preset, `prewarm_piper_server()` spawns the HTTP server in a background thread at app launch, loading the voice model into RAM before the user ever triggers synthesis.
- **Piper warm-up synthesis** — After the server reports ready, `prewarm_piper_server()` sends a hidden synthesis to force ONNX Runtime JIT compilation and GPU kernel init, eliminating a ~1.6s cold-start penalty on the first real request.
- **Piper server auto-restart on config change** — `restart_piper_server()` kills and respawns the Piper server when voice or CUDA settings change, with a `PIPER_WARMING` atomic guard preventing duplicate servers.
- **Piper server status endpoint** — `get_piper_server_status()` returns `PiperServerStatus` (running, model, port, cuda, ready) for the control server `/piper-status` health check.
- **Piper performance test script** — `test-piper-perf.ps1` automates synthesis timing measurements via the control server API.
- **WAV and Serialization Regression Tests** — Added comprehensive unit test suites in `wav.rs` covering truncated WAV buffers, invalid headers, and sample extraction. Added JSON request payload serialization tests in `cli.rs`.

- **Persistent RAM caching for Kokoro, Kitten, and Pocket TTS** — All three local TTS engines now keep their models loaded in RAM between utterances via persistent Python HTTP server processes, matching the existing Piper pattern. A generalized `local_tts_server.rs` state machine manages per-engine server lifecycle (start, health-poll, warmup, unload) with generation counters to prevent zombie processes.
  - **Kokoro** — `kokoro_server.py` uses `kokoro_tts.Kokoro` API to load the ~500MB ONNX model once. Synthesis time dropped from 7–9s (cold CLI) to ~1.1s (RAM persistent).
  - **Kitten** — `kitten_server.py` uses `kittentts.KittenTTS` API to load the 25–80MB model once. Synthesis time dropped from 7–14s to ~0.3s.
  - **Pocket** — `pocket_server.py` uses `pocket_tts.TTSModel` API to load the model once. Synthesis time dropped from 5–16s to ~0.3–0.7s.
  - **Automatic Python interpreter resolution** — `resolve_python_command()` detects whether the user-configured command is a Python interpreter; for non-Python commands (`kokoro-tts`, `pocket-tts`), it probes `python`/`python3`/`py` to find a suitable interpreter for running the server scripts.
  - **Auto model-path discovery for Kokoro** — `find_kokoro_models()` now searches the project-root `kokoro/` directory (relative to `CARGO_MANIFEST_DIR` and `current_dir`) in addition to system pip install paths, so dev-environment model files are found automatically.
  - **Engine-switch lifecycle** — On config change, old engine servers are unloaded and new ones prewarmed. Abort triggers unload+re-prewarm for the active engine. All servers are cleaned up on app exit.

### Changed

- **Frontend Dependency Upgrades** — Upgraded Svelte to `5.56.1`, `@sveltejs/kit` to `2.63.0`, Vite to `8.0.16`, Vitest to `4.1.8`, and all Tauri frontend modules to their latest v2 releases.
- **Backend Cargo Upgrades** — Upgraded Rust crate dependencies (`dirs` to v6, `flexi_logger` to v0.31, `winreg` to v0.56, `chrono`, and `log` to their latest patch versions).
- **CLI synthesis engine** — Intercepts synthesis calls for Piper to route them via the running local HTTP server instead of spawning a new process for every synthesis. Adds fallback to standard CLI execution if server synthesis fails.
- **Piper HTTP server health-check poll interval** — Uses exponential backoff starting at 100ms and doubling up to 1600ms, reducing CPU wake-ups during server startup.
- **Piper speed control** — The `_speed` parameter is now passed as `length_scale` in the Piper HTTP API JSON body, allowing playback speed adjustments at the synthesis level.
- **Piper server `reqwest::blocking::Client` reuse** — The health-check poll client is now stored in `PiperServerState` and reused for all subsequent synthesis HTTP requests instead of creating a fresh `Client::new()` each time.
- **Backend created once per `speak_queued`** — The `create_backend()` call was hoisted outside the pagination loop so that the same `Arc<Box<dyn TtsBackend>>` (with its shared connection pool) is reused for all fragments in a paginated synthesis.
- **base64 encoding moved to `spawn_blocking`** — `emit_audio_ready()` and `emit_audio_fragment()` are now async functions that run base64 encoding on tokio's blocking thread pool, preventing CPU-intensive encoding from stalling the async worker thread.
- **`history-updated` event batched** — The `history-updated` Tauri event is now emitted once at the end of all fragment synthesis instead of per-fragment, avoiding repeated frontend re-renders.
- **HUD event emission** — Removed `std::thread::spawn` + 50ms `thread::sleep` pattern from `show_hud`, `show_hud_synthesizing`, and `show_hud_playback`. Events are now emitted synchronously with direct `app.emit()`, eliminating OS thread creation overhead per HUD display.
- **Windows audio preroll** — Reduced from 1200ms to 200ms of near-silent sine wave prepended to audio playback on Windows, reducing dead air before speech starts by 1 second.
- **`AudioContext` pre-warmed at startup** — `PlaybackStore.setupListeners()` now creates the `AudioContext` and calls `resume()` immediately at app startup instead of lazily on first playback, avoiding a 50–200ms cold-start delay.
- **Clipboard config read** — Combined two separate `AppConfig` mutex locks (for `trigger_window_ms`/`max_text_length` and for `sanitization_config`) into a single lock acquisition in `clipboard.rs:on_change`, reducing mutex contention.
- **Cleanup pass conditional** — `cleanup_artifacts()` in the sanitization pipeline now runs a single pass and only re-runs if the text actually changed, instead of unconditionally running twice.
- **Piper server lifecycle management** — Moved to a robust, generation-aware state machine in `piper_server.rs` to coordinate startup and prevent duplicate server processes.
- **Piper HTTP client builder** — Configured with `tcp_nodelay(true)`, `connect_timeout(2s)`, and `pool_max_idle_per_host(2)` to optimize connection reuse.
- **Piper synthesis timing instrumentation** — Every synthesis call logs `[Piper] Synthesis — total:Xms (req:Yms read:Zms) size:N B chars:N voice:X cuda:bool` for visibility into HTTP vs data transfer time.
- **TTS pipeline timing breakdown** — `handle_playback_output()` logs `[TTS] Pipeline — synth:Xms env:Xms hist:Xms emit:Xms total_post:Xms` showing exactly where post-synthesis time goes.
- **Synthesis timing always visible** — Changed synthesis completion from `log::debug!` (debug-mode only) to `log::info!` so millisecond timings are always in console output.
- **Audio thread poll interval** — Increased the audio playback monitor's channel receive timeout from 50ms to 200ms, reducing idle CPU wake-ups by 4×.
- **Piper log prefix standardized** — All Piper-related log messages use `[Piper]` prefix for consistency across prewarm, synthesis, restart, and health-check paths.
- **Piper server port+client extracted in a single Mutex lock** — `synthesize_via_server()` now returns a `(port, client)` tuple from one lock acquisition, eliminating a second lock/unlock cycle that cloned the HTTP client handle separately.
- **Sequential local Piper synthesis** — `speak_queued` routes the Piper preset sequentially to avoid head-of-line blocking and Python HTTP server single-threaded contention.

### Fixed

- **Parallel fragment synthesis processes all fragments** — `synthesize_queued_parallel` now loops with a max-concurrency cap of 3 until every fragment is synthesized, fixing a bug where `fragments.len().min(3)` silently dropped fragments beyond the first three. Emit order remains sequential.
- **Piper prewarm/synthesis race condition** — Added `PIPER_WARMING` atomic flag with `ClearWarming` RAII drop guard. `synthesize_via_server()` polls this flag before starting a new server, eliminating a race where `restart_piper_server`'s background prewarm thread and a foreground synthesis call would start two separate server processes simultaneously.
- **System Tray and Configuration Sync** — Fixed the listening state toggle sync by making `set_listening` IPC command update and persist the `AppConfig` to disk and emit the `config-changed` event.
- **Listening State Initialization** — Initialized the tray listening menu item label dynamically on startup using the user's persistent config instead of hardcoding `"● Listening"`.
- **Chrono timestamp allocation** — Moved `chrono::Local::now().format(...)` inside the `is_debug_mode()` guard in `clipboard.rs`, avoiding a heap allocation on every clipboard change in release builds.
- **Pagination Configuration Bypass** — Fixed `synthesize_paginated` to respect the user's settings by passing `pagination_config` from active configuration instead of hardcoding `PaginationConfig::default()`.
- **Config Validation Unit Tests** — Explicitly set `active_backend = TtsEngine::Local` in local validation tests in `tests.rs` to prevent failure when the default project engine is configured to a non-local engine.
- **CLI TTS Engine Health Check** — Fixed the pre-existing health check to dynamically find and use any downloaded local `.onnx` voice in the user's voice folder, resolving failure errors complaining about a missing `"Rosie"` voice.
- **Clippy and Panic Fixes in CLI TTS backend** — Resolved option unwrap panic vector in Piper server port logic and simplified file extension checks using `is_some_and`.
- **CJK punctuation pagination panics** — Resolved Rust panics on non-ASCII delimiters (e.g. Spanish inverted punctuation, CJK delimiters `。！？`) by replacing raw byte slicing in `pagination.rs` with safe `str::get()` and using `char::len_utf8()` for word boundaries instead of assuming 1-byte offsets.
- **Mutex Poisoning Recovery** — Replaced direct `.lock().unwrap()` calls with robust recovery via `lock_or_recover!` macro or `.unwrap_or_else(|p| p.into_inner())` across `synthesis.rs`, `fragment_queue.rs`, `telemetry.rs`, and `piper_server.rs`, preventing thread panics from bricking the entire application state.
- **WAV Envelope Bounds Checks** — Clamped data size parsing in `wav.rs` to the actual file size to prevent out-of-bounds panics on truncated WAV headers, corrupt files, or streaming buffers.
- **Piper Server Pipe Drainage** — Spawns dedicated background reader threads for local Piper HTTP processes' stdout and stderr channels, preventing processes from freezing when OS output buffers fill up.
- **Frontend Playback URL Cache Leak** — Moved cache invalidation logic before the pre-decoded branch in `playback-store.svelte.ts`, resolving a bug where pre-decoded fragments would play stale cached audio URLs.
- **WAV Concatenation Artifacts** — Modified `concat_wav_files()` to respect the declared chunk size of the first WAV fragment, preventing trailing metadata blocks (like INFO/LIST) from being appended as loud static/PCM noise.

### Performance

- **Piper unified HTTP client** — Replaced ad-hoc `build_client()` with a `OnceLock<reqwest::Client>` singleton configured with `tcp_nodelay(true)` and `pool_max_idle_per_host(2)`. All Piper call sites (prewarm warm-up, server storage, synthesis) share the same connection pool via `.clone()`.
- **Piper HTTP response double-buffer eliminated** — `synthesize_via_server()` now calls `response.bytes()?.to_vec()` directly for zero-copy `Bytes`→`Vec` conversion, avoiding pre-allocation + `extend_from_slice()` copy.
- **Piper health-poll exponential backoff** — Replaced fixed 50ms/200ms delays with 100→200→400→800→1600ms exponential backoff in both `prewarm_piper_server()` and `synthesize_via_server()`, reducing CPU wake-ups during server startup.
- **Windows PATH expansion cached** — Wrapped `get_expanded_path()` in a `OnceLock<String>`; the 20-path iteration now computes once per process lifetime instead of on every CLI synthesis call.
- **Removed dead `AudioCommand::Play/Pause/Resume`** — Removed 3 variants, their match arms in the audio thread loop, and the `AudioPlayer::play()/pause()/resume()/is_paused()` methods (never called externally; playback uses `TogglePause`/`Stop` only).
- **Removed dead `SynthesisProgressEvent`** — 12-line struct never constructed; frontend uses a separate `SynthesisProgressPayload` type for the `hud:synthesis-progress` event.
- **Removed dead `FragmentQueue` methods** — Removed `add_fragment`, `is_empty`, `current_fragment`, `get_fragment`, `get_audio`, `has_audio`, `next`, `previous`, `clear_stop_flag`, `pause`, `resume`, `start` (~120 lines of untested-in-production queue navigation API) plus their test suite, reducing from 17 test cases to 7 focused tests.
- **Removed unused `Cursor`/`Decoder` imports** — Cleaned up imports in `audio/player.rs` after `play()` method removal.

- **base64 decode optimization** — Replaced the manual `for` loop over `charCodeAt(i)` with `Uint8Array.from(binary, c => c.charCodeAt(0))` in both `handleAudioReady` and `predecodeNextFragment`, leveraging V8's internal typed-array fast path for ~2M fewer JavaScript VM bytecode iterations per fragment.
- **WAV conversion optimization** — Rewrote `audioBufferToWavBlob()` to use a single `Int16Array` view over the output buffer with interleaved channel writes, replacing the inner-per-sample `DataView.setInt16()` loop for ~10× faster PCM sample writing.
- **Reduced `ArrayBuffer` copies** — Removed the redundant `arrayBuffer.slice(0)` copy in `handleAudioReady`; `decodeAudioData` now reads directly from the original buffer and `_originalBytes` is set once (not twice).
- **Analyser `Uint8Array` reuse** — `AudioAnalyser.start()` now allocates the frequency data `Uint8Array` once at `setup()` and reuses it every frame in the rAF loop instead of calling `new Uint8Array(...)` 60 times per second.
- **OpenAI header precomputation** — `format!("Bearer {}", api_key)` is now computed once in `new()` and stored as `auth_header: String`, avoiding a per-request allocation.
- **Tauri Plugin Registration dedup** — Removed a duplicate registration of `tauri-plugin-global-shortcut` builder in `main.rs`.
- **WAV envelope extraction: streaming RMS** — `extract_envelope()` now computes RMS in a single pass over raw PCM data without allocating an intermediate `Vec<f32>`. For short audio (<0.5s), processes every frame; for longer audio, decimates to at most `num_bars × 256` samples per bar. Eliminates ~880KB allocation for typical TTS outputs.
- **PCM frame decoder inlined** — `decode_frame_mono()` marked `#[inline(always)]` with per-bit-depth fast paths, eliminating function call overhead in the hot envelope extraction loop.
- **Pagination: no `Vec<char>` allocation** — `paginate_text()`, `detect_sentence_boundaries()`, and `force_split()` now operate on `&str` byte offsets via `char_indices()` instead of materializing the entire text as `Vec<char>`, saving ~400KB for 100k-char inputs.
- **Sequential fragment encoding pipelined** — `synthesize_queued_sequential()` spawns fragment N's base64 encoding in a background `tokio::task` while synthesizing fragment N+1, overlapping CPU-bound encoding with I/O-bound synthesis on the blocking thread pool.
- **Removed dead `WavStreamSource`** — Deleted `src-tauri/src/audio/stream.rs` (254 lines), `AudioCommand::PlayStreaming` variant, and `play_streaming()` method — all dead code from a pre-HTTP stdout-streaming approach.
- **Removed dead `read_pcm_samples`/`compute_rms`** — Deleted two functions replaced by the streaming `extract_envelope` + inline `decode_frame_mono`.
- **Cargo release profile** — Added `opt-level = 3`, `lto = true`, `codegen-units = 1`, and `strip = true` to `[profile.release]` in `Cargo.toml`.
- **Cleaned `#[allow(dead_code)]` annotations** — Removed file-level `#![allow(dead_code)]` from `fragment_queue.rs` and unnecessary annotations from `TtsError`, `Voice`, `TtsBackend` trait methods, `SynthesisProgressEvent`, and `AudioPlayer` fields/methods.

- **Piper removed from parallel synthesis** — `is_parallel_capable` no longer includes the Local+piper preset. Piper's HTTP server is single-threaded Python; concurrent requests cause head-of-line blocking with no speed gain. Sequential synthesis reduces server contention.
- **Piper health-check poll client reused** — `synthesize_via_server()` now uses the global `get_piper_client()` singleton for health-check polling instead of building a new `reqwest::blocking::Client` per server start, avoiding a redundant TCP handshake.
- **Piper server mutex lock scope minimized** — The `PIPER_SERVER` mutex is released before the HTTP synthesis request. `synthesize_via_server()` extracts `(port, client)` under lock, then performs the synthesis outside the critical section. Verified via new `lock:0ms` log metric on warm-server calls.
- **Envelope extraction offloaded to blocking thread** — `extract_envelope_async()` wraps WAV parsing in `spawn_blocking`, preventing large audio files from blocking the tokio async worker thread. All 4 call sites updated.
- **History saves batched in paginated synthesis** — `add_entry_with_batch()` accepts `skip_save: bool`; `synthesize_queued_sequential()` and `synthesize_queued_parallel()` set `skip_save=true` per-fragment and call `history::save()` once at the end. N fragments → 1 disk write instead of N.
- **Telemetry saves debounced** — `record_sample()` now persists to disk every 10 samples via an `AtomicU32` counter instead of on every synthesis call, reducing disk I/O by 90%.
- **Double-spawn eliminated in fragment emit** — `spawn_fragment_emit()` now uses a single `spawn_blocking` call (encoding + emit) instead of nested `spawn(async { spawn_blocking(...) })`, saving one task allocation per fragment.
- **Hot-path functions inlined** — Added `#[inline]` to `create_backend()`, `voice_for_backend()`, and `engine_str()`.
- **Piper warmup text extended** — Warm-up synthesis text increased from 5 chars (`"Hello"`) to 80 chars for more thorough ONNX Runtime JIT/GPU kernel warmup.

### Changed

- **Speed parameter threaded through synthesis** — `synthesize_async()` now accepts `speed: f32` and all call sites pass `config.playback.playback_speed`. Piper HTTP receives it as `length_scale`; previously hardcoded to `1.0`.
- **Piper synthesis timing expanded** — Log format now includes `lock_ms` (mutex hold time), `poll_attempts` (health-check retries), and `spawn_ms` (process spawn time) in addition to the existing `req_ms`/`read_ms` breakdown.
- **Duplicate tray/hotkey speak code consolidated** — Extracted a shared `spawn_speak(&AppHandle)` helper in `main.rs`, eliminating 28 lines of duplicate state extraction across the tray menu handler and global hotkey handler.
- **History cleanup deferred on startup** — The background cleanup service now sleeps 30s before its first run instead of executing immediately, avoiding disk I/O during app launch.
- **Synthesis calls always pass speed** — `synthesize_async()`, `synthesize_paginated()`, `synthesize_queued_sequential()`, and `synthesize_queued_parallel()` all accept and propagate the `speed` parameter.

### Removed

- **`history_manager.rs`** — Deleted the entire 385-line file (`HistoryManager` struct with `#![allow(dead_code)]`). The managed state was created in `main.rs` but never consumed by any Tauri command (all history operations use `HistoryLog` directly).
- **Dead functions in `history.rs`** — Removed `create_entry()`, `add_entry()`, `add_entry_complete()`, `update_file_size()`, `format_file_size()`, and `get_total_file_size_human()` (~80 lines of never-called API).
- **Dead methods in `pagination.rs`** — Removed `TextFragment::is_first()`, `is_last()`, and `label()` (~20 lines, all `#[allow(dead_code)]`).
- **Dead function in `telemetry.rs`** — Removed `get_bucket_label()` (debug formatting, never called).
- **Dead method in `config/output.rs`** — Removed `AudioFormat::from_extension()` (`#[allow(dead_code)]`).
- **Unused macro in `logging.rs`** — Removed `debug_log!` macro (never invoked).
- **Dead import in `main.rs`** — Removed `mod history_manager;` declaration.

- **Removed `is_autostart_enabled()`** — 29-line function plus its test, never called outside its own test.
- **Removed `ElevenLabsOutputFormat::is_playable_by_rodio()`** — 16-line method, never called.
- **Removed `ElevenLabsTtsBackend::get_voices()`** — 15-line method wrapping `list_voices()` into `Vec<Voice>`, never called.
- **Removed `ElevenLabsTtsBackend::resolve_voice_name()`** — 3-line instance method wrapper for static resolver, never called.
- **Removed `TimingSample` struct** — 7-line struct in `telemetry.rs`, never constructed.
- **Removed `HistoryLog::get_entry_by_file_path_mut()`** — 9-line method, never called.
- **Removed `HistoryLog::get_file_path()`** — 8-line method, never called.
- **Removed `HistoryLog::get_entry_file_size()`** — 3-line method, never called.
- **Removed `HistoryLog::update_file_format()`** — 11-line method, never called.

### Changed

- **Compile-time dead code verification** — Removed `#![allow(dead_code)]` from `impl HistoryLog` block; 4 genuinely dead methods identified and removed. Remaining `#[allow(dead_code)]` annotations are now scoped to individual items with explicit justification (trait dynamic dispatch, test-only helpers).
- **Derivable `Default` impls replaced** — `EffectId`, `CloseBehavior`, `AppearanceMode`, `AudioFormat`, `StorageMode`, `TtsEngine`, `ElevenLabsOutputFormat`, and `ElevenLabsVoice` now use `#[derive(Default)]` with `#[default]` variant markers instead of manual `impl Default` blocks (clippy `derivable_impls`).
- **Collapsed nested conditionals and streamlined expressions** — `if is_sentence_end(c) { if !is_abbreviation_at(...) { ... } }` collapsed into a single condition; `map_or(false, ...)` replaced with `is_some_and()`; `sort_by` replaced with `sort_by_key`; `match` for single branch replaced with `if`; manual range checks replaced with `contains()`; consecutive `str::replace` calls merged; manual `div_ceil` implementations replaced with `.div_ceil()`; `useless_format` and `useless_conversion` calls replaced with `.to_string()` or direct expressions (clippy `collapsible_if`, `unnecessary_map_or`, `unnecessary_sort_by`, `single_match`, `manual_range_contains`, `collapsible_str_replace`, `manual_div_ceil`, `useless_format`, `useless_conversion`).
- **Type annotations tightened** — `&PathBuf` → `&Path` in `write_to_temp()`, `&Vec<u8>` → `&[u8]` in `spawn_fragment_emit()`, `std::u32::MAX` → `u32::MAX` (clippy `ptr_arg`, `legacy_numeric_constants`).
- **Lint scope reduced** — `#[allow(clippy::too_many_arguments)]` moved from module-level to individual function annotations on 8 heavy-parameter functions; `#[allow(clippy::match_like_matches_macro)]` and `#[allow(clippy::needless_range_loop)]` scoped to single functions in `clipboard.rs` and `audio/wav.rs` respectively.

### Fixed

- **On-demand Piper server restart missing warmup** — `synthesize_via_server()` now runs a hidden warmup synthesis after starting a new server on-demand (when the loaded voice/model doesn't match the requested one). Previously, only `prewarm_piper_server()` (config-triggered) ran the warmup; voice-mismatch restarts inside the synthesis path skipped it, making the first real request pay the 1–4s ONNX JIT/GPU init penalty on top of the server start time.
- **`speak_history_entry` used history entry's voice instead of current config** — The re-speak button now uses `voice_for_backend(current_config)` rather than the voice stored in the history entry, so it re-synthesizes text with the currently selected voice, backend, and speed settings as intended.
- **Piper warmup text** — Warmup synthesis uses a substantial sentence on CUDA to compile JIT GPU kernels, and `"Hello"` (5 chars) on CPU, reducing warmup time.

### Added

- **Pagination fragment size validation** — `PaginationConfig::validate()` clamps `fragment_size` to 50..5000, wired into `AppConfig::validate()`, preventing hand-edited configs from producing empty or single-character fragments.
- **Empty fragment filter** — `paginate_text()` now filters out whitespace-only fragments with a safety-net `retain()` at the end, and `force_split()` skips trimmed-empty chunks. Fixes a bug where inter-sentence whitespace at tiny fragment sizes produced empty fragments that crashed the Piper server (`ValueError("No text provided")`).
- **Pagination regression tests** — Added `no_empty_or_whitespace_fragments_at_any_size`, `no_character_loss_any_size_unicode`, and `zwj_emoji_4byte_combining_never_panic` tests covering fragment sizes 1–500 with CJK, Devanagari, Cyrillic, ZWJ emoji, and combining marks.

### Changed

- **Piper speed is playback-only** — Removed dead `playback_speed`/`speed` plumbing from 8 call sites across `synthesis.rs`. `length_scale` is always `1.0` at synthesis time; the frontend applies speed via `playbackRate` on the `<audio>` element. `synthesize_async()` no longer takes a speed parameter. Updated `test_piper_request_body_serialization` to assert `length_scale == 1.0`.
- **Piper server restart keyed on command/CUDA only** — Voice changes no longer trigger a full kill-and-restart of the Piper HTTP server (it lazy-loads voices per request from `--data-dir`). `piper_server_changed` in `set_config` now only checks `command` and `cuda`.
- **Piper model unloaded on engine switch** — Switching away from the Piper preset (to OpenAI, ElevenLabs, Cartesia, etc.) calls `unload_piper_model_internal()`, releasing hundreds of MB of RAM/VRAM that would otherwise linger until app exit.
- **Adaptive request timeout for Piper synthesis** — Per-request HTTP deadline via `RequestBuilder::timeout()` scales with text length: `clamp(5s + chars × per_char_ms, 10s, 180s)`, using 5ms/char for CUDA and 30ms/char for CPU. Prevents a wedged ONNX session from holding the global synthesis queue lock forever.
- **Pre-flight voice model check** — `synthesize_via_server()` now stats the expected `.onnx` file before talking to the Piper server. If missing, returns a clear error with download instructions. Prevents silent fallback to the default voice (Piper HTTP server returns 200 for unknown voices).
- **`is_piper()` uses preset field** — Changed from substring heuristic over the command name to the ground-truth `preset` field on `CliTtsBackend`. Eliminates false positives from commands/paths containing "piper" (e.g. `bagpiper`). The `preset` string is threaded from `TtsConfig` through `create_backend()`.
- **Health check fast path for Piper** — When the persistent server is Ready, `health_check()` pings `GET /voices` with a 2s deadline instead of spawning a full CLI synthesis (seconds of model load). When `Starting`, returns `Ok` immediately. Falls back to the full probe only when `Stopped`.
- **Hotkey/tray speak routes long texts to `speak_queued`** — `spawn_speak()` checks pagination config and routes long clipboard texts through `speak_queued` for streaming fragment-by-fragment playback (lower time-to-first-audio). Short texts and file-output mode continue using `speak_now`.
- **`duration_ms` shipped in `AudioFragmentEvent`** — Backend parses audio duration from the WAV header and ships it in the fragment event payload, enabling the frontend to skip `decodeAudioData` for duration-only needs when pitch=1 and no effect.

### Fixed

- **`ensure_running` dead-server branch leaked processes** — Replaced the `Arc::try_unwrap`/dummy-`cmd` fallback with kill-through-the-Mutex + `wait()` + generation bump. The old code spawned a leaking `cmd.exe` on Windows (panic on Linux/macOS) and could orphan the real Python server when `try_unwrap` failed due to concurrent `Arc` clones.
- **`unload_piper_model` shelled out to `taskkill`/`kill -9`** — Now uses `Child::kill()` + `Child::wait()` through the `Arc<Mutex<Child>>`, eliminating external process spawning, PATH dependency, and asynchronous kill (which raced follow-up restarts). `Starting` state is now cancelled via generation bump instead of being silently ignored.
- **Control server token generation was weak** — Token now generated with OS CSPRNG (`getrandom`) producing a 32-hex-char random value instead of `DefaultHasher` (SipHash with zero keys) over low-entropy inputs (`Instant`, `SystemTime`, stack address).
- **Control server token locked out repo's own clients** — All three first-party clients (`.pi` extension, Claude hook script, `test-piper-perf.ps1`) now read the token from `config.json` (or `COPYSPEAK_CONTROL_TOKEN` env override) and send `Authorization: Bearer <token>`. `GET /health` is now unauthenticated.
- **Control server token compared non-constant-time** — Replaced `auth == expected` string comparison with byte-wise XOR fold constant-time compare.
- **Parallel synthesis reported success after failure** — `synthesize_queued_parallel` now returns `Result<(), String>`. On fragment error → emits `fragment-failed`, aborts the set, returns `Err`. On `JoinSet` panic → stores a synthetic error, returns `Err`. Caller emits `pagination:complete` only on `Ok(())` and `pagination:failed` on `Err`, unifying the contract with the sequential path.
- **Abort couldn't reach the Piper server path** — `do_abort_synthesis()` now calls `unload_piper_model_internal()`, so any blocked `send()` errors out immediately instead of waiting for the per-request timeout.
- **Mid-stream Piper read errors silently dropped** — `response.bytes()` read failures in `synthesize_via_server()` now call `unload_piper_model()` so the next utterance gets a fresh server.
- **`concat_wav_files` silently corrupted mismatched formats** — Now validates that all fragments share the same sample rate, channel count, and bit depth, returning a clear error instead of stamping the first header over foreign PCM. Fixed off-by-one in the skip-warning fragment label.
- **RIFF chunk iteration missed odd-size pad byte** — `parse_wav_header()` now accounts for the RIFF spec's odd-chunk-size pad byte (`chunk_size & 1`), preventing desync on WAVs with LIST/INFO metadata chunks.
- **Undeclared MSRV** — Added `rust-version = "1.87"` to `Cargo.toml` so contributors on older toolchains get a clear version error instead of cryptic manifest parse failures from edition-2024 dependencies.

### Breaking Changes

- **Control server `/speak` and `/piper-status` now require authentication** — Any external automation sending `POST /speak` or `GET /piper-status` to `127.0.0.1:43117` must include `Authorization: Bearer <token>` where `<token>` is `general.control_token` from `config.json`, or set the `COPYSPEAK_CONTROL_TOKEN` environment variable. `GET /health` remains unauthenticated.

### Dependencies

- **Upgraded `windows` 0.58 → 0.62** — `SetClipboardData` and `CreateWindowExW` now take `Option<HANDLE>`/`Option<HWND>`/`Option<HINSTANCE>` instead of raw handle types. Updated 3 call sites in `clipboard.rs`.
- **Upgraded `reqwest` 0.12 → 0.13** — `RequestBuilder::query()` removed; moved `output_format` query parameter into the URL string in `elevenlabs.rs`.
- **Upgraded `tempfile` 3.10 → 3** (resolves to 3.27.0) — semver-compatible, no code changes.
- **Upgraded `flexi_logger` to 0.31.9**, `regex` to 1.12.4, `uuid` to 1.23.3, `wasm-bindgen` to 0.2.123, `bitflags` to 2.13.0, `serde_with` to 3.21.0 via `cargo update`.
- **Frontend** — `svelte` 5.56.1 → 5.56.3, `@sveltejs/kit` 2.63.0 → 2.64.0, `prettier` 3.8.3 → 3.8.4, `@types/node` 25.9.1 → 25.9.2 via `bun update`.

## [0.1.5] - 2026-05-20

### Added

- **LLM post-processing (Groq Cloud)** — Optional pass between sanitize and TTS synthesis that rewrites copied text into concise, listener-friendly speech tailored for software developers. Off by default. Configure under Settings → Advanced → LLM Post-Processing.
  - New `PostProcessConfig` (`enabled`, `api_key`, `model`, `prompt`) in `AppConfig`; config schema version bumped to `0.1.5`.
  - New Rust module `post_process` (`process`, `try_process`) wraps Groq's OpenAI-compatible `/chat/completions`.
  - New IPC command `check_groq_credentials` validates the key via `GET /models`.
  - Hooked into `speak_now` and `speak_queued` after the cfg snapshot, before pagination. LLM failures fall back to the original text and never block synthesis.
  - Hardcoded model dropdown: `openai/gpt-oss-20b`, `llama-3.3-70b-versatile`, `llama-3.1-8b-instant`.

### Changed

- **LLM post-processing default prompt** — Switched to a terse caveman-style rewrite prompt with a 3 bullet/point maximum.

### Fixed

- **CopySpeak TTS Pi extension** — Routes final Pi responses through the running app's sanitization, max-length, LLM post-processing, effects, and TTS pipeline instead of filtering/truncating in the extension.
- **Vercel landing page** — Updated the displayed version, screenshot asset, and removed the double-copy hero tagline.

## [0.1.4] - 2026-05-20

### Added

- **CopySpeak TTS Claude Code hook** — Added `scripts/claude-copyspeak-hook.mjs` to speak Claude Code `Stop`/`SubagentStop` assistant responses through the CopySpeak TTS control server.

### Changed

- **CopySpeak TTS Pi extension** — Disabled speaking Pi thinking blocks by default and expanded status text to show only non-default assistant/thinking/activity modes.

### Fixed

- **CopySpeak TTS Pi extension** — Removed the stale `.pi/extensions/copyspeak-voice` extension so only `/copyspeak` is registered.
- **Vercel deployments** — Added a repository `ignoreCommand` that runs production builds and skips preview builds.

## [0.1.3] - 2026-05-19

### Added

- **Update controls in settings** — Added the footer update status/check/install control below the automatic update-check setting.

### Fixed

- **CopySpeak TTS Pi extension** — Renamed the Pi command/extension path to `copyspeak` and shortened its Pi status text to `on`/`off`.
- **Vercel landing page** — Re-enabled non-English locale registration and footer language switching, and restored page scrolling despite the desktop app's global hidden body overflow.
- **Windows audio wake-up** — Add a low-level preroll to desktop playback on Windows so the audio device wakes before speech or radio effects begin.
- **About settings layout** — Removed the stale import/export separator and aligned About rows with the shared `SettingRow` spacing.

## [0.1.2] - 2026-05-18

### Added

- **Audio Effects system** — Frontend-only post-processing applied to TTS playback
  - New `EffectsConfig` (Rust + TS) persisted in `AppConfig` with `enabled` and `active_effect`
  - New Effects settings tab and conditional main-menu Effects tab (gated by `effects.enabled`)
  - New `/effects` route with live effect selector and preview button
  - **Walkie-talkie effect** — Narrow radio EQ, subtle saturation, light AM wobble, normalized PTT clicks, and low static under the voice
  - **8-bit Game Boy effect** — 4-bit sample quantization resampled to 11025 Hz for crunchy retro voice
  - `Effect` interface and registry in `src/lib/stores/playback/effects/` for extensibility
  - Effects render inside `OfflineAudioContext` and integrate with existing pitch-shift pipeline; results cached per `{pitch, effect}` pair

### Changed

- **Unified web and desktop SvelteKit app** — Consolidated the former `src-web` landing page into the main `src` app
  - Added Vercel environment detection via `import.meta.env.VITE_IS_VERCEL`
  - Route layout now renders the marketing landing page on Vercel and the Tauri app shell locally/in desktop builds
  - Removed the redundant `src-web` SvelteKit project

### Fixed

- **CopySpeak TTS Pi extension** — Switched Pi speech triggering from clipboard double-copy writes to the local CopySpeak TTS control server, avoiding primer speech and Windows clipboard failures.
- **CopySpeak TTS Pi extension** — Disabled activity/tool announcements by default so normal use only speaks final assistant responses unless `/copyspeak activity on` is enabled.
- **CopySpeak TTS Pi extension** — Now speaks only once after an agent run completes and no longer auto-launches CopySpeak TTS unless `COPYSPEAK_PI_LAUNCH=1` is set.
- **CopySpeak TTS Pi extension** — Added a two-minute duplicate speech guard to avoid charging TTS credits for repeated final messages.
- **CopySpeak TTS Pi extension** — Uses the running app's engine/effect settings by default and can include Pi thinking blocks in spoken assistant responses.
- **CopySpeak TTS Pi extension** — Speaks Pi thinking blocks as soon as each thinking block finishes streaming, while avoiding replaying those blocks in the final response.
- **CopySpeak TTS control server** — Fixed `Content-Length` parsing so `/speak` accepts normal HTTP POST bodies from Pi, curl, and other clients.
- **CopySpeak TTS control server** — `/speak` now waits for speech generation to complete before responding, allowing Pi extension requests to queue synthesis instead of overlapping.
- **Playback queue** — Single `audio-ready` events now use the existing fragment queue so Pi-generated thinking and final responses play sequentially instead of interrupting each other.
- **Global playback settings** — Sync playback volume, speed, pitch, and effects during app startup so Pi control-server speech uses the configured walkie-talkie effect outside the Play page.

## [0.1.1] - 2026-05-15

### Added

- **Audio Effects system** — Frontend-only post-processing applied to TTS playback
  - New `EffectsConfig` (Rust + TS) persisted in `AppConfig` with `enabled` and `active_effect`
  - New Effects settings tab and conditional main-menu Effects tab (gated by `effects.enabled`)
  - New `/effects` route with live effect selector and preview button
  - **Walkie-talkie effect** — Narrow radio EQ, subtle saturation, light AM wobble, normalized PTT clicks, and low static under the voice
  - **8-bit Game Boy effect** — 4-bit sample quantization resampled to 11025 Hz for crunchy retro voice
  - `Effect` interface and registry in `src/lib/stores/playback/effects/` for extensibility
  - Effects render inside `OfflineAudioContext` and integrate with existing pitch-shift pipeline; results cached per `{pitch, effect}` pair

- **Cartesia onboarding verification** — Onboarding now accepts a Cartesia API key and validates it via `check_cartesia_credentials` without synthesis.

- **Cartesia TTS backend** — Added Cartesia Sonic 3.5 as a cloud TTS engine
  - Added `CartesiaConfig`, `TtsEngine::Cartesia`, and `CartesiaTtsBackend`
  - Added Cartesia engine settings UI with model, voice ID, and output format controls

### Changed

- **Unified web and desktop SvelteKit app** — Consolidated the former `src-web` landing page into the main `src` app
  - Added Vercel environment detection via `import.meta.env.VITE_IS_VERCEL`
  - Route layout now renders the marketing landing page on Vercel and the Tauri app shell locally/in desktop builds
  - Removed the redundant `src-web` SvelteKit project
- **Default TTS engine** — New configs now default to Cartesia Sonic 3.5 with the Katie voice
- **Default pagination fragment size** — New configs now use `fragment_size: 500`
- **Engine picker order** — Cartesia now appears first in engine settings and footer selector
- **Cartesia voice selection** — Cartesia settings now show resolved voice names with a manual voice ID fallback
- **Onboarding flow** — First-run setup now focuses on Cartesia Cloud instead of local Kitten TTS installation

### Fixed

- **CopySpeak TTS Pi extension** — Switched Pi speech triggering from clipboard double-copy writes to the local CopySpeak TTS control server, avoiding primer speech and Windows clipboard failures.
- **CopySpeak TTS Pi extension** — Disabled activity/tool announcements by default so normal use only speaks final assistant responses unless `/copyspeak activity on` is enabled.
- **CopySpeak TTS control server** — Fixed `Content-Length` parsing so `/speak` accepts normal HTTP POST bodies from Pi, curl, and other clients.

## [0.1.0] - 2026-03-27

### Added

- **Global hotkey speak-from-clipboard** — Hotkey now triggers TTS directly from clipboard content
  - Added handler in global-shortcut plugin to call `speak_from_clipboard` on hotkey press
  - Logs hotkey trigger events for debugging

- **Dedicated History page** — New `/history` route for viewing all TTS generations
  - Moved history from play page to its own route
  - Conditionally shown in nav when history is enabled

- **SettingRow component** — Reusable settings row with label, tooltip, and consistent layout
  - Applied across all settings components for uniform UI

- **Live debug logs viewer** — Real-time log tail in About section when debug mode enabled
  - Shows last 20 lines, auto-refreshes every 2s

### Fixed

- **CopySpeak TTS Pi extension** — Reworked clipboard triggering to serialize double-copy events and avoid repeated trigger loops; startup now avoids focusing an already-running CopySpeak TTS instance.

- **Windows CLI backend PATH resolution** — Expanded PATH for finding Python/uv tools on Windows
  - Added `get_expanded_path()` to include common Python and uv installation paths
  - Fixes "executable not found" errors on clean Windows installations

### Changed

- **Settings page consolidation** — Major restructure from 8 sections to 3 tabs (General, Advanced, About)
  - Continuous scroll with scroll-spy navigation
  - Removed staggered loading (WebView2 crash workaround no longer needed)
  - HUD settings moved to General section as dropdown
  - Pagination/Sanitization moved to Advanced tab
- **Window size increased** — 675x540 → 775x640 for better content visibility
- **Hotkey capture redesign** — Cleaner UI with Kbd components and arrow key symbols (↑↓←→)
- **Quick-settings redesign** — Larger controls with clearer labels (Volume, Speed, Pitch)
- **App shell refactor** — Grid-based layout for better content distribution
- **Removed `show_notifications`** config field — Unused setting cleaned up
- **Default hotkey shortcut** — Changed from `Super+Shift+A` to `Win+Shift+A` for Windows clarity
- **Hotkey error messages** — Updated to use "Win" instead of "Win/Super" for consistency
- **Hotkey logging** — Added structured logging with `[Hotkey]` prefix for registration attempts and config changes
- **Border radius system** — Simplified radius variables for sharper brutalist aesthetic
  - `--radius-sm: 2px`, `--radius-md: var(--radius)`, `--radius-lg: 4px`, `--radius-xl: 6px`
  - Theme toggle and UI components updated to use `rounded-sm` instead of `rounded-none`
- **Logging noise reduction** — Suppressed verbose debug logs from tauri_plugin_updater and reqwest
- **Engine page layout refactor** — Moved badges to header section for cleaner UI
- **Progress bar animation** — Converted from JavaScript interval to CSS animation for smoother performance
- **Default Kokoro voice** — Changed from `af_heart` to `adam`
- **Internationalization** — Temporarily disabled language switcher, hardcoded to English during development

## [0.0.5] - 2026-03-24

### Added

- **Global hotkey configuration** — Configurable keyboard shortcut to trigger TTS
  - `hotkey` config field with modifier + key format (e.g., `"Ctrl+Space"`)
  - Hotkey capture component in settings UI
  - Backend IPC: `register_hotkey` with global-shortcut plugin
  - Hotkey re-registration on config change

- **Listening toggle** — Enable/disable clipboard monitoring via `listen_enabled` config
  - Toggle in quick-settings dropdown and app-footer
  - Backend IPC: `set_listening`, `get_listening` commands
  - Persisted to config, synced via `config-changed` event

### Fixed

- **HUD progress bar and marquee timing** — Accurate playback duration via cross-window event
  - HUD window and main window have separate JS contexts with separate `hudStore` instances
  - `playbackStore` in main window decodes audio via Web Audio API to get accurate duration
  - Emits `hud:audio-duration` event which HUD window receives and updates its `hudStore`
  - Progress now shows accurate percentage based on `AudioBuffer.duration`
  - Marquee animation timing now matches actual playback duration
  - ElevenLabs MP3 duration now accurately determined via Web Audio decode (not server estimate)

- **Audio playback on clean Windows 11** — AudioContext now resumes if suspended
  - Web Audio API requires user gesture to activate AudioContext on fresh profiles
  - Added `audioCtx.resume()` call when state is "suspended" in playback-store

## [0.0.3] - 2026-03-22

### Fixed

- **KittenTTS installer** now works on clean Windows 11 without Python pre-installed
  - Embeds installer scripts in binary and extracts to temp directory at runtime
  - Auto-detects any Python 3.x version, offers winget installation if not found
  - PowerShell window now visible with success/failure feedback before pause
  - Default config now uses `py -3.12` to ensure kittentts runs on same Python version used by installer
  - Health check detects `ModuleNotFoundError` with actionable error message
  - Fixed health check using invalid voice "test" instead of "Rosie"

## [0.0.2] - 2026-03-21

### Added

- **HUD playback enhancements**
  - Progress bar animation synced to audio duration
  - Marquee scrolling text for long speech content
  - `duration_ms` field in `HudSynthesizingPayload` for synthesis duration tracking

### Fixed

- Removed duplicate `$effect` in hud-playback-content component
- Removed debug `console.log` statement from production code

## [0.0.1] - 2026-03-20

### Added

- **Core TTS functionality** — Clipboard-triggered text-to-speech with multiple engine support
  - Double-copy trigger: copy twice within 1.5s to speak selected text
  - Hotkey trigger: configurable keyboard shortcut
  - Manual trigger: paste/play from UI

- **Multiple TTS engines**
  - **Kitten TTS** (default): Ultra-lightweight CPU-optimized ONNX inference, 8 built-in voices
  - **Piper TTS**: Local CLI engine with 20+ EN US voices
  - **Kokoro TTS**: Local CLI engine with multiple voices
  - **OpenAI TTS**: Cloud API with 9 voices (alloy, ash, coral, echo, fable, onyx, nova, shimmer, verse)
  - **ElevenLabs TTS**: Cloud API with voice library support

- **HUD overlay** — Floating heads-up display showing playback status, waveform visualization, and engine info
  - Real-time waveform visualization with 16-bar equalizer
  - Progress tracking for paginated synthesis
  - Click-through transparent overlay

- **History management** — Persistent history of TTS generations with playback
  - Audio files saved in native format (WAV/MP3/OGG/FLAC)
  - Fragmented copy grouping for paginated text
  - Batch playback and deletion

- **Settings system**
  - General: auto-start, debug mode, language (EN/ES with full i18n support)
  - Playback: speed (0.25x–4x), pitch (0.5x–2x), volume
  - Triggers: double-copy window, hotkey configuration
  - Sanitization: markdown stripping, text normalization

- **Auto-updater** — Check and install updates from GitHub Releases

- **Internationalization (i18n)** — Full localization with English and Spanish support, RTL layout ready

### Breaking Changes

- **HTTP TTS engine removed** — HTTP endpoint backend removed in favor of CLI and cloud engines
- **SSML support removed** — SSML markup passthrough feature removed
- **Streaming TTS mode removed** — Simplified to paginated synthesis only

[Unreleased]: https://github.com/ilyaizen/copyspeak-tts/compare/v0.1.5...HEAD
[0.1.5]: https://github.com/ilyaizen/copyspeak-tts/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/ilyaizen/copyspeak-tts/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/ilyaizen/copyspeak-tts/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/ilyaizen/copyspeak-tts/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/ilyaizen/copyspeak-tts/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/ilyaizen/copyspeak-tts/compare/v0.0.5...v0.1.0
[0.0.5]: https://github.com/ilyaizen/copyspeak-tts/compare/v0.0.3...v0.0.5
[0.0.3]: https://github.com/ilyaizen/copyspeak-tts/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/ilyaizen/copyspeak-tts/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/ilyaizen/copyspeak-tts/releases/tag/v0.0.1
