# CopySpeak Fork — Changes vs Upstream (`ilyaizen/CopySpeak`)

> **Scope of this document:** an exhaustive diff of the `NairoDorian/copyspeak-tts` fork
> (branch `main`) against its upstream `ilyaizen/CopySpeak` (branch `main`).
>
> **Status at time of writing:** fork is **40 commits ahead** of upstream, **122 files changed**,
> **+13,291 / −6,539** lines (measured with `git diff --shortstat upstream/main...HEAD`).
>
> **Method:** the fork is built on the complete `tts-perf-v2` feature branch
> (`8bc5c09`), then `upstream/main` was merged with `git merge -X ours` (every conflict
> resolved **in the fork's favor**) and reconciled until `cargo build` + `bun check` passed.
> All upstream features are therefore present **and** all fork performance work is preserved.

> **Vendored upstream reference:** a shallow clone of `ilyaizen/CopySpeak` lives at
> `original_copyspeak/` (gitignored) purely for diffing. Recoveries of merge-dropped
> IPC commands (`test_local_engine`, `test_tts_engine_config`, `list_post_processing_models`)
> and the diff methodology are documented in
> [`docs_internal/original_copyspeak_reference.md`](docs_internal/original_copyspeak_reference.md).

---

## 1. Relationship to upstream

This is **not** a blind divergence. The fork:

1. Kept its entire `tts-perf-v2` history as the base (`8bc5c09` is an ancestor of `main`).
2. Merged the latest `upstream/main` (brings the upstream profile/engine overhaul, new cloud
   engines, and post-processing UI).
3. Resolved all merge conflicts with `-X ours` so upstream's changes are *absorbed* but never
   overwrite the fork's optimizations.

Result: a single branch that contains **both** upstream's recent product work **and** the fork's
TTS-performance, GPU, and hardening improvements.

---

## 2. Improvements contributed by the fork

These are changes the fork adds on top of (or instead of) upstream. Grouped by theme.

### 2.1 Local TTS engine suite + persistent RAM caching
- **Kitten TTS**, **Kokoro TTS**, **Pocket TTS** added as first-class local engines
  (`kitten_server.py`, `kokoro_server.py`, `pocket_server.py`, `src-tauri/src/tts/local_tts_server.rs`).
- **Persistent RAM caching** for all four local engines (Kitten, Piper, Kokoro, Pocket): models
  stay loaded in memory between utterances for sub-second synthesis.
  - Commit `19cb41b` — persistent RAM caching, CUDA support, dynamic voice discovery for Piper.
  - Commit `42eaedc` — persistent RAM caching for Kokoro, Kitten, and Pocket TTS.
- `piper_server.rs` rewritten (+509 lines) with streaming envelope, byte-offset pagination,
  single-lock extraction, unified HTTP client, and parallel pagination.

### 2.2 Piper GPU / CUDA acceleration
- Upgraded to **`onnxruntime-gpu 1.27.0` (CUDA 13)** — Commit `8bc5c09`.
- CUDA runtime DLL auto-linking + setup scripts:
  - `bdaacd4` — CUDA runtime DLL auto-linking and setup script for Piper GPU acceleration.
  - `c79c43e` — CPU-only and GPU setup automation scripts for Piper persistent server caching.
  - `setup-piper-cuda.ps1`, `setup-piper-cpu.ps1`, `setup-venv-cuda-v2.ps1`.
- **Critical NVIDIA DLL-injection fixes** (documented in CHANGELOG `[Unreleased]`):
  - `nvidia` namespace package `__file__` is `None` → switched to `list(nvidia.__path__)[0]`.
  - CUDA 13 deeper DLL subdirectory layout now resolved via recursive glob.
  - Hard-coded CUDA 12 subpackage names replaced with layout-agnostic discovery.

### 2.3 Pre-warm / JIT priming
- Hidden warm-up synthesis to Piper pre-warm for ONNX JIT / GPU kernel init (`9d17404`).
- Warm-up text changed from `.` to `Hello` for realistic JIT priming (`790fdec`).
- CUDA warm-up sentence to compile JIT kernels (`d659356`, in `piper_server.rs`).

### 2.4 Piper lifecycle & observability
- **Piper auto-start on config change**; CUDA toggle restart (`d659356`).
- **`/piper-status` control-server endpoint** + `get_piper_server_status` IPC command.
- **Real-time Piper model lifecycle indicator in the footer** (`78531eb`) — spinner / "Loading
  model…" / "Loading in VRAM…" / "Warming up…" / ready (green + CUDA badge) / error (red).
- `PiperStore` Svelte 5 runes store (`src/lib/stores/piper-store.svelte.ts`) listening to
  `piper-status-changed` events; `PiperStatusPayload` / `PiperServerStatus` types.

### 2.5 TTS pipeline performance
- Comprehensive pipeline optimization (`8e4b822`): connection pooling, parallel synthesis,
  pre-decode, Piper pre-warm, history bulk ops.
- Deep optimization (`767855e`): single-lock Piper, streaming envelope, byte-offset pagination,
  pipelined encoding, dead-code removal.
- `c37ad27`: single-lock extraction, unified HTTP client, parallel pagination, exponential
  backoff (+152 / −552 lines — significant dead-code removal).
- `ed4084b`, `894931f`: Piper pipeline optimization, batch I/O, mutex hygiene, WAV boundaries,
  Piper server drainage.
- `2726c69`: remove dead code, driving the build toward a clean clippy pass.
- On-demand Piper warmup + re-speak uses current config voice (`7b26827`).

### 2.6 Safety / security / lifecycle hardening
- `deed222` — critical safety, security, lifecycle, and performance improvements:
  - Replaced ~80 bare `.lock().unwrap()` calls (including inside the Win32 clipboard window
    procedure) with a `lock_or_recover!` macro to prevent poison-panic across the FFI boundary.
  - Removed `unsafe impl Send/Sync for AudioPlayer` (all fields already auto-`Send`).
  - **Control-server token length-check fix** — length XOR was truncated to `u8`; now compared
    untruncated (security).
  - Native double-copy dispatch (no full-text round-trip through the hidden webview); fixes
    triggers dropped before frontend mount (incl. first-run onboarding).
  - In-flight job preemption (`ABORT_REQUESTED` checked before acquiring the synthesis lock).
  - Audio emitted before history disk IO; Windows preroll only on first fragment.
  - Cloud TTS requests bounded with connect/overall timeouts.
  - Cartesia synthesizes `pcm_s16le` (halves payload, fixes WAV envelope extraction).
  - Blocking IPC commands moved to tokio blocking pool.
  - Atomic JSON persistence (write-to-tmp + rename) for config/history/telemetry.
  - Numerous history/pagination/fragment-queue/abort fixes (see CHANGELOG for full detail).

### 2.7 Kitten / Pocket / Kokoro support & install tooling
- `install-kittentts.ps1`, `kittentts-cli.py`, `kitten_server.py`, `kokoro_server.py`,
  `pocket_server.py`, `src-tauri/kitten_server.py`, `src-tauri/kokoro_server.py`,
  `src-tauri/pocket_server.py`.
- `test-piper-perf.ps1` — Piper performance test script.

### 2.8 Export & misc
- Source-only zip export script + dynamic archive naming (`79b27a5`, `086cfd9`, `export-zip.ps1`).
- ElevenLabs Svelte 5 slider warning fixes (`84c283a`).
- Listening-sync / pagination config-bypass / validation-test fixes (`db96f09`).

### 2.9 Dependency modernization
- `2546df2` — `reqwest` 0.12 → 0.13.
- `5e87642`, `7578c19` — frontend & backend dependencies updated to latest stable.
- Final pass (`f7c7d1f`): Rust stable 1.97.0 + `cargo update` (tauri 2.11.5, zbus 5.17, time
  0.3.53, etc.); JS/bun bumped (Svelte 5.56.4, Vite 8.1.4, Vitest 4.1.10, TypeScript ~6.0.3,
  @tauri-apps/* 2.11.x, Prettier 3.9.5, Tailwind 4.3.2).

### 2.10 Test environment fix
- Replaced the broken `jsdom` test environment with **happy-dom** and forced
  `--environment happy-dom` in the `test` / `test:watch` / `test:ui` scripts (the
  `@testing-library/svelte/vite` plugin overrides the config's `environment` field).
- Restored fork-only test files dropped during the upstream merge
  (`local-engine.test.ts`, `eng02-minimal.test.ts`, `effects-settings.svelte`,
  `routes/engine/+page.svelte`).
  - `routes/engine/+page.svelte` (and its `engine-page.svelte` component) were later
    **removed again** during the engine-UI consolidation (see §2.11); the single `/engines`
    route is now the canonical engine UI.

### 2.11 Engine UI consolidation
- Removed the duplicate `/engine` route (`src/routes/engine/+page.svelte`) and its
  `src/lib/components/engine/engine-page.svelte` component (plus its test) — the live
  `/engines` route is now the single canonical engine UI, matching upstream's layout.
- Registered **Pocket TTS** in the engine picker (`src/lib/components/engine/engine-meta.ts`
  `LOCAL_PRESETS`) and added `scripts/install-pocket.ps1` (Kyutai Pocket-TTS installer that
  reuses the shared `lib/copyspeak-engine-install.ps1` helpers), so all local engines are
  configurable from `/engines`.
- Registered previously-missing / unregistered IPC commands in `main.rs`'s `generate_handler!`:
  `install_engine`, `set_active_profile`, `list_tts_engines`, `list_tts_voices` (the last two
  implemented in the new `src-tauri/src/commands/tts/catalog.rs`).
- `speak_queued` / `speak_history_entry` now build the TTS backend from the **active voice
  profile** (`create_backend_from_effective`) instead of the stale `active_backend` mirror, so
  per-profile engine/voice options are honored at synthesis time; `hud::get_provider_voice`
  resolves the effective profile for the HUD and `set_config` re-syncs the mirror.
- `+layout.svelte` coerces the serde-skipped `playback_speed` / `pitch` settings to finite
  `1.0` defaults so a missing config field can never yield `NaN`/`undefined` playback.

---

## 3. Upstream features now included in the fork (via merge)

Because `upstream/main` was merged, the fork also contains upstream's recent product work:

- **Voice Profiles system** — `profile-manager.svelte`, `voice-picker.svelte`,
  `profile-export-dialog.svelte`, `engine-panel.svelte`, `engine-setup.svelte`, and profile
  types consolidated into `src-tauri/src/config/tts.rs`.
- **New cloud engines** — Google, Microsoft, Edge, and a generic HTTP engine
  (`src-tauri/src/tts/{google,microsoft,edge,http,catalog}.rs`).
- **Post-processing settings UI** (`post-processing-settings.svelte`).
- **New routes** — `routes/engines/+page.svelte`, `routes/voices/+page.svelte`.
- General upstream fixes/features landed between the fork's branch point and the merge.

---

## 4. File-level change summary

### Added in the fork (35 files)
- **Local TTS servers / scripts:** `kitten_server.py`, `kokoro_server.py`, `pocket_server.py`,
   `src-tauri/kitten_server.py`, `src-tauri/kokoro_server.py`, `src-tauri/pocket_server.py`,
   `install-kittentts.ps1`, `scripts/install-pocket.ps1`, `kittentts-cli.py`, `setup-piper-cpu.ps1`, `setup-piper-cuda.ps1`,
   `setup-venv-cuda-v2.ps1`, `export-zip.ps1`, `test-piper-perf.ps1`.
- **Rust backend:** `src-tauri/src/tts/local_tts_server.rs`, `src-tauri/src/tts/piper_server.rs`,
  `src-tauri/src/commands/tts/catalog.rs` (engine/voice catalog IPC commands
  `list_tts_engines` / `list_tts_voices`).
- **Engine UI (fork versions kept through the merge):** `cartesia-engine.svelte`,
  `elevenlabs-engine.svelte`, `local-engine.svelte`, `openai-engine.svelte`,
  `src/lib/components/ui/mock-info-tooltip.svelte`.
- **Engine tests:** `elevenlabs-engine.test.ts`, `local-engine.test.ts`,
  `openai-engine.test.ts`, `eng02-minimal.test.ts`.
- **Stores / mocks:** `src/lib/stores/piper-store.svelte.ts`, `src/lib/mocks/app-navigation.ts`,
  `src/lib/mocks/app-state.ts`.
- **Settings / routes:** `effects-settings.svelte`.
- **Agent tooling / docs / assets:** `skills/cli-anything-copyspeak/SKILL.md`,
  `LICENCE.txt`, `static/screen-v0.1.4.png`.

### Deleted in the fork vs upstream (3 files)
- `src-tauri/src/audio/stream.rs` — superseded by the fork's audio/fragment pipeline.
- `src-tauri/src/history_manager.rs` — consolidated into the fork's `history.rs`.
- `vitest-plugin-mock-state.ts` — intentionally removed by the fork.

---

## 5. Verification status

Run from the fork `main` branch:

| Check | Command | Result |
| ----- | ------- | ------ |
| Rust build | `cargo build --manifest-path src-tauri/Cargo.toml` | ✅ exit 0 (17 harmless warnings) |
| Frontend types | `bun check` (svelte-check) | ✅ 0 errors / 0 warnings |
| Frontend tests | `bun run test` (vitest, happy-dom) | ✅ 48 passed / 0 failed |
| Dependency freshness | `cargo update` + `bun update` | ✅ all latest within semver ranges |

> Note: `bun test` invokes bun's *native* runner and is **not** the project's test command.
> Use `bun run test` (which calls `vitest run --environment happy-dom`).
>
> The figures above were captured at the merge reconciliation and **predate** the §2.11
> engine-UI consolidation and the IPC-registration / synthesis fixes. Re-run the four checks
> to confirm the current state (the deletion of `engine-page.test.ts` in particular changes
> the vitest pass count).

---

## 6. Keeping the fork in sync with upstream

To pull future upstream work without losing the fork's optimizations:

```bash
git fetch upstream
git merge -X ours upstream/main      # fork wins conflicts
# reconcile until `cargo build` + `bun check` pass, then commit
```

Because the fork is the product of exactly this strategy, repeating it preserves both sides.
