# CopySpeak TTS — `tts-perf-v2` Deep Review & Improvement Plan (v3)

> **Reviewed:** `NairoDorian/copyspeak-tts` @ branch `tts-perf-v2`, HEAD `894931f` ("refactor(backend): standardize mutex hygiene, WAV boundaries, and piper server drainage") — 21 commits ahead of upstream.
> **Against:** `ilyaizen/copyspeak-tts` @ `main` (`f466314`) — confirmed to be the fork's exact merge base (0 upstream commits missed).
> **Diff size:** 67 files, **+4,092 / −3,280** lines.
> **Review date:** 2026-06-10.
> **Relationship to the two plan files already in the repo root:** those documents reviewed commit `7b26827` and earlier. Several of their findings have since been *acted on* (the code even carries their IDs as comments: `// H2:`, `// H5:`, `// P3:`, `// S1:`…). **This document audits the quality of those fixes at the current HEAD**, confirms which are genuinely resolved, identifies where the fix itself introduced new problems, and adds findings neither previous review caught. Once you've absorbed this file, the two older ones can be deleted from the repo (see Q3).

---

## 0. Executive verdict

The architecture is right and most of it now works. The persistent `piper.http_server` + RAM model residency is the correct design for low-latency local TTS, and the latest three commits fixed the majority of the previously-reported critical bugs **properly** — I verified the pagination rewrite, the WAV-bounds hardening, and the stale-predecode fix by *executing* them, not just reading them (see §1).

What remains falls into three buckets:

1. **Half-finished fixes.** The speed-inversion bug was *defused* (by hard-coding `1.0`) rather than *resolved* — the inverted `length_scale` mapping is still in the code, dead `playback_speed` plumbing threads through eight call sites, and a unit test now *enshrines the wrong semantics*. One careless future refactor re-detonates it. Similarly, the server-lifecycle race fix replaced one race with a genuinely dangerous fallback (`Command::new("cmd").spawn().unwrap()` — a leaked process on Windows, a guaranteed panic everywhere else).
2. **The new security feature breaks your own product.** The control-server bearer token (a good idea) is generated with a non-cryptographic hash, compared non-constant-time, and — most importantly — **none of the three first-party clients in this very repo send it**: the `.pi` extension, the Claude hook script, and your own brand-new `test-piper-perf.ps1` all get `401` after first run.
3. **Lifecycle gaps in the server design.** Switching away from the Piper preset leaks the Python server (RAM/VRAM held until app exit); changing voices triggers a full kill-and-restart that the Piper HTTP API makes unnecessary (it lazy-loads voices per request — verified against the upstream `piper1-gpl` source); a hung server wedges synthesis forever because the HTTP client has no total timeout; and the Abort button cannot touch an in-flight server request at all.

Nothing here invalidates the fork's direction. Fix R1–R7, and this branch is materially better than upstream on every axis it set out to improve.

---

## 1. Method — what was actually executed, not just read

To keep this review honest, here is what was *run* versus *read*:

**Executed:**

- Cloned both repos; computed the merge base (`f466314`); diffed the full tree; confirmed the fork is exactly 21 commits ahead and 0 behind.
- **Extracted `pagination.rs` and `audio/wav.rs` (with a stubbed `PaginationConfig` and `AmplitudeEnvelope`) into a standalone crate and ran the fork's complete embedded test suite: 51/51 pass on rustc 1.82**, including the new Unicode regression tests (CJK, curly quotes, Spanish). The previously-reported "pagination panics on non-ASCII" finding is **genuinely fixed** at HEAD.
- Wrote and ran **11 additional adversarial tests** against the same extracted code (ZWJ emoji, 4-byte scalars, combining marks after delimiters, Devanagari danda, fullwidth CJK delimiters, abbreviation-at-EOF, force-split at sizes 1–50, truncated WAV streams, RIFF odd-chunk padding, mismatched-sample-rate concat, PCM-preservation in concat). Results: **one real bug found** (empty fragments at tiny fragment sizes — H1), one silent-corruption case **demonstrated** (mismatched-rate concat — M1), everything else held: no panics, no character loss at any fragment size, truncation clamps correctly, concat preserves PCM byte-for-byte.
- Behavior-diffed the fork's pagination against the **original implementation compiled side-by-side** on ASCII inputs across four fragment sizes: no divergence in produced fragments. The rewrite is a faithful, faster re-implementation.
- Fetched the **upstream `piper1-gpl` `http_server.py` source** and verified every API assumption the fork makes: `POST /` accepts `text`, optional `voice`, `length_scale`, `noise_scale`, `noise_w_scale`; `GET /voices` exists (your health check is valid); unknown voices are **lazy-loaded from `--data-dir` and cached in RAM**; missing voices **silently fall back to the default voice with HTTP 200** (→ R6); `length_scale` is **phoneme duration** — bigger = slower (→ R1); the server keeps every loaded voice in a dict forever (→ R5c).
- Diffed the complete **Tauri command surface**: zero commands removed, two added (`get_local_piper_voices`, `unload_piper_model`). Cross-checked every frontend `invoke("…")` string against the backend: **no orphaned IPC calls**. The IPC contract is intact.
- Verified each previously-claimed "dead code removal" still has zero callers at HEAD (`history_manager.rs`, `audio/stream.rs`, the removed `AudioPlayer::play/pause/resume`, `FragmentQueue` navigation methods, `SynthesisProgressEvent`). All genuinely dead. `AudioPlayer` itself is still live (stop/pause/skip/volume/state relay) — correctly kept.
- Grepped the repo's own HTTP clients (`.pi/extensions/copyspeak/index.ts`, `scripts/claude-copyspeak-hook.mjs`, `test-piper-perf.ps1`) for `Authorization` headers: **none send one** (→ R4).
- Counted tests: `elevenlabs-engine.test.ts` 15 → 1, `openai-engine.test.ts` 15 → 1 (28 cases still deleted at HEAD); `engine-page` 4 → 4 and `local-engine` 18 → 18 preserved (→ H3).
- Attempted a full `cargo check` of `src-tauri` on Linux (installed webkit2gtk/GTK deps + rustc 1.82 from Ubuntu): blocked by the dependency graph requiring **edition 2024** crates (`dlopen2_derive 0.4.3`, `serde_spanned 1.1.1`, ICU 2.x stack). This is itself a finding: the fork compiles only on Rust ≥ 1.85 toolchains, and the code additionally uses `u32::is_multiple_of` (stabilized **1.87**) with **no `rust-version` declared** (→ H2).

**Read in full:** `piper_server.rs`, `cli.rs`, `synthesis.rs` (all 1,319 lines), `config.rs` command, `control_server.rs`, `main.rs` diff, `wav.rs`, `pagination.rs`, `voices.rs`, `playback-store.svelte.ts`, `fragment-queue.ts`, `app-footer.svelte`, `local-engine.svelte`, both setup scripts, the perf script, telemetry/hud/clipboard/history diffs.

**Not possible in this environment:** running the Tauri app, Windows-specific runtime behavior (CUDA paths, `CREATE_NO_WINDOW`, tray), and the Vitest suite. Items depending on those are tagged **[verify on Windows]**.

---

## 2. Scorecard — previous findings vs. the code at HEAD `894931f`

| Prior ID | Finding | Status at HEAD | Notes |
|---|---|---|---|
| C1 | Piper speed inverted + double-applied | ⚠️ **Defused, not fixed** | `synthesize_async` hard-codes `1.0`, so nothing fires — but the inverted mapping, the dead plumbing, and a test asserting the wrong semantics all remain. → **R1** |
| C2 | Pagination panics on non-ASCII | ✅ **Fixed & verified by execution** | Full rewrite on byte offsets with `char_indices`/`len_utf8`; 51 embedded + 9 adversarial tests pass; no char loss at any size. One residual edge: → **H1** |
| C3 | Stale pre-decoded fragment replays previous audio | ✅ **Fixed & verified by reading** | Decoded buffer now travels *with* the `QueuedFragment`; `_cachedPitchUrl` invalidated per fragment; `buildPlaybackUrl` null-guards `_originalBytes`. Minor residual inefficiency: → **M6** |
| C4 | Server stdout/stderr piped but never drained → freeze | ✅ **Fixed** | Both streams drained on dedicated threads; stderr kept in a 30-line ring buffer surfaced in error messages. Good. |
| C5 | Envelope panics on truncated WAVs | ✅ **Fixed & verified by execution** | `data` chunk clamped to file length; my truncated-stream test passes for both `get_wav_duration` and `extract_envelope`. |
| C6 | Parallel synthesis waits for all fragments; drops failures silently | 🟡 **Half fixed** | Now streams in index order as results land (good). But a failed fragment still ends with `pagination:complete` + `Ok(())` → **R7**. |
| H1 | Server lifecycle races / orphaned processes | ⚠️ **Fix introduced a worse bug** | Generation counter + state machine is sound, but the dead-server branch contains `Command::new("cmd").spawn().unwrap()` → **R2**. |
| H2 | No timeout on Piper HTTP; abort can't reach server path | ❌ **Open** | Comment says "H2" but only `connect_timeout(2s)` was added; no total timeout; `ACTIVE_CLI_PID` never set on the server path → **R3**. |
| H3 | Any TTS change restarts the server | 🟡 **Narrowed, two gaps** | Now keyed on command/voice/cuda/preset/backend. But voice change still restarts needlessly, and switching *away* leaks the server → **R5**. |
| H4 | `adaptive_fragment_size` thresholds unreachable | 🟡 **Mostly still true** | Measured: thresholds are 0.5 / 5.0 chars-per-**ms** (= 500 / 5,000 chars-per-second). Realistic Piper-CPU is ~0.05–0.2; only fast CUDA crosses the first threshold. → **M3** |
| H5 | 15 s readiness too tight for CUDA; "ready" accepted any TCP response | ✅ **Fixed** | 60 s CUDA / 15 s CPU budget, exponential backoff (100→1600 ms), checks `GET /voices` for 2xx, detects premature child exit and surfaces the stderr tail. Solid. |
| H6 | Engine switch wipes the chosen voice | ✅ **Fixed** | `lastVoiceByPreset` map in `app-footer.svelte` restores the per-preset voice. |
| H7 | `set_audio` write-only RAM sink | ✅ **Removed** | Queue API is coherent; every remaining method has callers. |
| A2 | Extract `piper_server.rs` | ✅ **Done** | Module exists; `cli.rs` is a thin delegate. |
| S1 | Control-server auth token | ⚠️ **Implemented, breaks clients, weak RNG** | → **R4**. |
| T (#4) | OpenAI/ElevenLabs test suites gutted (~28 cases) | ❌ **Open** | Still 15→1 and 15→1 at HEAD → **H3 (new numbering)**. |
| #6 | Telemetry loses up to 9 samples on exit | ✅ **Fixed** | `SAVE_EVERY_N_SAMPLES = 10` + flush in `RunEvent::Exit`. |
| #7 | `Int16Array` native-endian WAV write | 🟡 **Accepted with comment** | Justified as LE-only client platforms. Fine pragmatically; keep the comment. |
| #9 | HUD 50 ms timing-delay removal | ✅ **Kept removed**, no regression signal. |

Also independently re-verified as genuinely good and worth protecting in review: the duplicate `tauri_plugin_global_shortcut` registration fix, `listen_enabled` persistence + tray-label sync via `config-changed`, the single-lock clipboard sanitization hoist, deferred history saves with one flush per batch, `RunEvent::Exit` Piper teardown, the tray "Unload Model" item, and the dynamic `get_local_piper_voices` discovery.

---

## 3. CRITICAL — fix before merging or shipping (R-series)

### R1. Finish the speed fix: the inverted `length_scale` is still loaded, and a test now asserts the wrong semantics

**Where:**
- `src-tauri/src/tts/cli.rs` — `synthesize_via_server`: `let body = serde_json::json!({ "text": text, "voice": voice, "length_scale": speed });`
- `src-tauri/src/commands/tts/synthesis.rs:64-67` — `synthesize_async(_speed: f32)` → `backend.synthesize(&text, &voice, 1.0)`
- ~8 call sites threading `cfg.playback.playback_speed` into `synthesize_async` (lines 344, 358, 365, 488, 815, 839, 924, 1047, 1269) — all ignored.
- `src-tauri/src/tts/cli.rs` tests — `test_piper_request_body_serialization` asserts `body["length_scale"] == speed`.
- `src/lib/stores/playback-store.svelte.ts:272, 342` — frontend still applies `audioEl.playbackRate = speed` (the *actual*, working speed mechanism).

**What's wrong.** The earlier review found speed both inverted (Piper's `length_scale` is phoneme *duration*: 2.0 = twice as **slow** — re-verified against `piper1-gpl` `http_server.py`, which feeds it straight into `SynthesisConfig`) and double-applied. The "fix" at HEAD was to hard-code `1.0` inside `synthesize_async`. That makes the bug unobservable, but the codebase now actively lies about itself:

1. Every caller passes `playback_speed` into a parameter named `_speed` that is discarded. A reader (or you, in three months) will assume synthesis-time speed works.
2. The first person who "fixes" the underscore re-enables **inverted speed + double application simultaneously**: at the default 1.35×, Piper would synthesize 35 % *more* audio frames (slower, more expensive — directly against this branch's purpose) and the browser would time-stretch it back. At slider max (`speed.clamp(0.25, 4.0)` in `commands/playback.rs`), 4× the synthesis work for ~1× perceived speed.
3. The unit test *codifies* `length_scale == speed`, so the wrong mapping is now "protected" by CI.

**Fix — pick one policy, then delete the other path entirely:**

*Option A (recommended — smallest, preserves today's behavior):* speed is a **playback-only** concept.
- `synthesize_async(backend, text, voice)` — remove the parameter.
- Delete `playback_speed` from every tuple-destructure in `synthesis.rs` that only feeds it.
- In `cli.rs`, drop `length_scale` from the request body (or send the explicit constant `1.0` with a comment) and change `synthesize(&self, text, voice, _speed)` back to an ignored param on the trait, or remove it from the trait signature if no backend uses it.
- Fix the test to assert the body *omits* `length_scale` (or equals `1.0`).

*Option B (synthesis-time speed for Piper — better audio quality than time-stretch):*
- `length_scale = (1.0 / speed).clamp(0.25, 4.0)`.
- Frontend: set `playbackRate = 1.0` **when the active engine is Piper** (otherwise cloud engines lose speed entirely).
- Add `speed` to the `speak_now` history-cache key (currently `(text, voice, engine)` — a cached 1.0× entry would replay verbatim at a 2.0× setting) and to the telemetry key, since `synthesis_ms` would now vary with speed and pollute `chars_per_ms`.
- HUD ETA (`hudStore.setSpeed`) must be told which regime is active.

Option B is a feature; Option A is a cleanup. Either is fine — the only unacceptable state is the current half-and-half.

---

### R2. `ensure_running`'s dead-server branch spawns a dummy `cmd` process — panics off-Windows, leaks a process on Windows, and can orphan the real server

**Where:** `src-tauri/src/tts/piper_server.rs`, `ensure_running`, the `ServerState::Ready` arm:

```rust
let server_owned = Arc::try_unwrap(active).ok().unwrap_or_else(|| {
    // Fallback if still referenced
    ActiveServer {
        child: Mutex::new(Command::new("cmd").spawn().unwrap()), // dummy
        port: 0, ...
    }
});
if server_owned.port != 0 { /* kill + wait */ }
*state = ServerState::Stopped;
```

**What's wrong, concretely:**

1. `Arc::try_unwrap` **can** fail here: any concurrent `ensure_running` caller clones the `Arc` in its own `Ready` arm before you re-acquire the lock. When it fails:
   - **Windows:** an interactive `cmd.exe` is spawned *without* `CREATE_NO_WINDOW` (this call site doesn't set creation flags) — a real console window can flash — and the process is **never killed** (port == 0 skips the kill). One leaked `cmd.exe` per occurrence.
   - **Linux/macOS:** `Command::new("cmd")` fails to spawn → `.unwrap()` → **panic inside `ensure_running`**, which runs on a `spawn_blocking` thread inside a Tauri command. The fork is Windows-first, but the rest of the file carefully `#[cfg(windows)]`-guards platform code — this line silently breaks any future cross-platform build at runtime, not compile time.
   - Worst of all, when `try_unwrap` fails the **real** dead-or-mismatched Piper child is *not* killed by this thread; state flips to `Stopped`, the other `Arc` holders drop their clones after their own checks, and the Python process (holding the model in RAM/VRAM) is orphaned until app exit.

2. The entire unwrap dance is unnecessary. `ActiveServer.child` is already a `Mutex<Child>` — you can kill **through** the `Arc`, exactly like `unload_piper_model` in the same file already does.

**Fix (drop-in):**

```rust
// In the Ready arm, after re-verifying Arc::ptr_eq under the lock:
if let ServerState::Ready(curr) = &*state {
    if Arc::ptr_eq(curr, &active) {
        log::info!("[Piper] Killing dead/mismatched server on port {}", active.port);
        {
            let mut child = active.child.lock().unwrap_or_else(|p| p.into_inner());
            let _ = child.kill();
            let _ = child.wait(); // reap; prevents zombies on Unix
        }
        *state = ServerState::Stopped;
    }
}
```

No `try_unwrap`, no dummy process, no platform divergence, and the real child is always reaped. While here: bump `CURRENT_GENERATION` so a `Starting` thread racing in cannot resurrect the same configuration unobserved.

---

### R3. No total timeout on Piper synthesis; Abort cannot touch the server path; a wedged server freezes `speak_now` and holds the global queue lock

**Where:**
- `piper_server.rs::get_piper_client()` — `connect_timeout(2s)` only; **no `.timeout(...)`**. The code comment says this implements "H2", but the prior H2 asked for a *request* deadline.
- `cli.rs::synthesize`/`synthesize_via_server` — on the server path, `crate::ACTIVE_CLI_PID` is never set (it's only set in the CLI-fallback branch), so `do_abort_synthesis` (`main.rs:254`) has nothing to kill; `ABORT_REQUESTED` is only polled **between** fragments in `synthesize_queued_sequential`/`synthesize_paginated`.
- `speak_now`/`speak_queued` hold `app.state::<tokio::sync::Mutex<()>>()` for the duration — a hung request therefore blocks **every subsequent speak**, including the hotkey path (`spawn_speak`).

**Failure mode in practice:** Piper accepts the TCP connection (so `connect_timeout` passes) and then never replies — e.g., ONNX wedged after a driver hiccup, or a 20,000-char fragment on CPU. The blocking `send()` waits forever; the user presses Abort and nothing happens; every further hotkey press queues behind the dead one until app restart. The CLI era didn't have this: the spawned process had a PID and Abort killed it.

**Fix (three layers, all cheap):**

1. **Adaptive request deadline.** A fixed timeout either kills legitimate long synthesis or is too lax. Scale with input: `timeout = clamp(5s + text_chars * 30ms_cpu_or_5ms_cuda, 10s, 180s)` — build a per-request client or use `reqwest::blocking::RequestBuilder::timeout(...)` (per-request timeout is supported on blocking requests and overrides the client default).
2. **Make Abort effective:** the simplest semantics matching the old behavior — in `do_abort_synthesis`, if the active backend is Piper, call `unload_piper_model_internal()` (kill the server; the blocked `send()` errors out immediately; the next utterance prewarms again). Cheap on CPU; on CUDA you pay the reload, but the user asked to stop. Alternative: keep the server and just let the per-request timeout fire — but then Abort still feels dead for up to the deadline. Killing is honest.
3. **On `response.bytes()` mid-stream error** (today only `send()` errors trigger unload), also mark the server unhealthy — a half-written body usually means a dying server.

While in there: the `ensure_running` waiter's 65 s ceiling vs. the starter's 60 s CUDA budget *plus* warmup (your own log shows CUDA warmups of several seconds) means a caller can time out moments before the server flips to Ready, surfacing a scary error for a server that then works on retry. Either raise the waiter to `start_budget + warmup_allowance + margin` (e.g., 90 s for CUDA) or have the Starting state expose the actual start `Instant` so waiters compute the real remaining budget.

---

### R4. The control-server token locks out the repo's own clients, and the token itself is guessable

**Where:**
- Generation: `src-tauri/src/main.rs:309-318` — `DefaultHasher` over `Instant::now()`, `SystemTime::now()`, and a stack address.
- Enforcement: `src-tauri/src/control_server.rs::parse_request` — `auth == expected_auth` string compare, applied to **every** route including `GET /health` and `GET /piper-status`.
- Clients that never send it (all verified by grep):
  - `.pi/extensions/copyspeak/index.ts:170` → `POST /speak`
  - `scripts/claude-copyspeak-hook.mjs:140` → `POST /speak`
  - `test-piper-perf.ps1:28-31, 74, 100, 128` → `GET /piper-status`, `POST /speak` — **your own new perf-measurement script cannot talk to the build it was written for.**

**Why the token quality matters even on localhost:** the stated threat is *other local processes* (different user, sandboxed app) reaching `127.0.0.1:43117`. Against that adversary, `DefaultHasher::new()` is SipHash with **fixed zero keys** — fully deterministic given the inputs — and the inputs are low-entropy: `SystemTime` is bracketed to within seconds by the config file's own creation timestamp sitting next to the token, and the address contributes a handful of ASLR bits. An offline brute force over plausible timestamps is feasible. If the token isn't worth generating properly, it isn't worth having.

**Fix:**

1. Generate with the OS CSPRNG — `getrandom = "0.2"` (tiny, no_std-friendly):
   ```rust
   let mut buf = [0u8; 16];
   getrandom::getrandom(&mut buf).expect("OS RNG");
   let token = buf.iter().map(|b| format!("{:02x}", b)).collect::<String>();
   ```
2. Constant-time compare (`subtle` crate, or byte-wise `|=` xor fold). One line.
3. **Exempt `GET /health`** (it leaks nothing and external liveness probes are its whole purpose). Keep `/speak` and `/piper-status` gated.
4. **Update all three clients** to read the token from the config file (its location is stable: same dir the app writes) and send `Authorization: Bearer <token>`; honor a `COPYSPEAK_CONTROL_TOKEN` env override like the existing `COPYSPEAK_CONTROL_URL`. Document the header in `docs_internal/` and the README section that advertises the control API.
5. Document the migration: anyone with external automation against `/speak` breaks on first launch of this build. A `CHANGELOG` "Breaking" entry is mandatory.

---

### R5. Server lifecycle gaps: leak on engine switch, pointless restart on voice change, unbounded voice cache

**Where:** `src-tauri/src/commands/config.rs:104-135, 187-199`; `piper_server.rs` (`StartingConfig` has no `voice` field — correct! — but `restart_piper_server` is keyed on voice anyway); upstream `http_server.py` (`loaded_voices` dict grows monotonically).

Three related problems:

**(a) Leak on switching away.** `tts_for_server` is `Some` only when the *new* config is `Local`+`piper`. Switch to OpenAI/ElevenLabs/Cartesia (or another local preset) and `piper_server_changed` is true but nothing runs — the Python server, with the full model resident (hundreds of MB RAM; on CUDA, VRAM too), keeps running until app exit or a manual tray "Unload Model". Users who toggle engines will accumulate exactly one zombie footprint, silently.

**Fix:** in `set_config`, when the *old* config was piper-active and the *new* one is not → `unload_piper_model_internal()` (or, friendlier: start a 5-minute idle timer and unload then, so quick A/B switching doesn't thrash the model).

**(b) Voice change ≠ restart.** The Piper HTTP server you're targeting loads the requested `voice` from `--data-dir` *per request* and keeps it cached — I verified this in the upstream source. Your synthesis body already sends `"voice": voice`. So changing the voice in the UI requires **zero** server action; today it kills the warm server, respawns Python, reloads a model, and re-runs warmup — throwing away the entire RAM-persistency benefit at the exact moment the user is experimenting with voices. Restart should be keyed **only** on `command` or `cuda` (data_dir is currently constant). Remove `voice` (and `preset`, when the preset stays piper) from `piper_server_changed`; pass the *current* voice through on each request as you already do.

Consequences to handle once you do this:
- First utterance after a voice switch pays that voice's load inside the request (seconds on CUDA). Optional polish: fire a background warmup `POST /` with the new voice on config change (no restart — just a primer request).
- `ActiveServer.model_name` / the `/piper-status` `model` field becomes "startup voice", which is misleading. Either track "last requested voice" or rename the field to `default_model`.

**(c) Unbounded `loaded_voices`.** Upstream caches every distinct voice forever. A user who samples ten voices holds ten ONNX sessions in RAM (and VRAM under CUDA). You can't fix upstream from here, but you can bound it: after N distinct voices (say 3) or on a low-memory signal, do a cheap restart with the current voice as the new default. At minimum document it next to the CUDA toggle.

---

### R6. Missing voice model → Piper silently speaks the **wrong voice** with HTTP 200; the helpful error from the CLI era is gone

**Where:** upstream `http_server.py` `app_synthesize`: unknown `voice` → `_LOGGER.warning(...); voice = default_voice` → 200 OK. Fork: `cli.rs::synthesize_via_server` treats any 2xx as success. The carefully-written "voice model not found — run `piper.download_voices …`" error in the CLI fallback (`cli.rs`, the `Unable to find voice` branch) can now only trigger if the *server* path fails first.

**User-visible symptom:** select a voice whose `.onnx` was deleted/never downloaded → playback proceeds in the default voice. No error, no hint. With (b) above implemented (no restart on voice change), this becomes the *normal* path for a missing voice.

**Fix (client-side, no server changes needed):** before the POST, `stat` the expected model file — you already have the exact logic in `spawn_start_thread`:

```rust
let model = std::path::Path::new(&data_dir).join(format!("{voice}.onnx"));
if !model.exists() && !std::path::Path::new(voice).exists() {
    return Err(TtsError::CommandFailed(format!(
        "Piper voice model not found: {voice}\n\nDownload it with:\n  python -m piper.download_voices {voice}\n\nThen place the .onnx and .onnx.json in:\n  {data_dir}"
    )));
}
```

One filesystem stat per utterance is free, and it restores (and improves) the old UX. Bonus: it also means `get_local_piper_voices` and reality can't drift mid-session.

---

### R7. Parallel queued synthesis reports success after a failure

**Where:** `src-tauri/src/commands/tts/synthesis.rs` — `synthesize_queued_parallel` returns `()`; on a fragment error it emits `pagination:fragment-failed`, `abort_all()`s, and `break`s — and then control returns to `speak_queued`, which unconditionally logs `"All {} fragments synthesized and streamed"` and emits **`pagination:complete`**, returning `Ok(())` to the invoker. A *panicked* task is even quieter: its slot stays `None`, the stall-guard `break`s, and not even `fragment-failed` fires.

Meanwhile the **sequential** path propagates `Err(...)` and never emits `complete`. The two paths' contracts with the frontend have diverged; any UI logic keyed on `pagination:complete` (progress teardown, "done" toasts, batch markers) will treat half-finished cloud batches as successful.

**Fix:** make the parallel helper return `Result<(), String>`:
- fragment error → emit `fragment-failed`, abort the set, **return `Err`** (and have `speak_queued` skip `pagination:complete`, emitting `pagination:stopped` or a new `pagination:failed` instead — pick one and use it in both paths);
- `JoinSet` panic → same treatment, with the panic message;
- user abort → today both paths emit `pagination:stopped`; keep that, but also skip `complete` (currently the parallel abort path *also* falls through to `complete`).

This is ~20 lines and makes the two pipelines behaviorally identical from the frontend's perspective.

---

## 4. HIGH — correctness, resource safety, contributor safety (H-series)

### H1. `paginate_text` emits **empty fragments** at small fragment sizes — reproduced

**Evidence (executed against your code, extracted verbatim):**

```
REPRO "Hi! Ok."   size=1 → ["H", "i", "!", "", "O", "k", "."]
REPRO "A. B. C."  size=1 → ["A", ".", "", "B", ".", "", "C", "."]
REPRO "Hi!  Double space. End." size=2 → ["Hi", "!", "", "Do", "ub", ...]
```

The inter-sentence whitespace becomes a trimmed-to-empty fragment. Downstream: an empty `text` POSTed to the Piper server raises `ValueError("No text provided")` → HTTP 500 → your code falls back to the slow CLI spawn → which also produces nothing → `"Fragment N synthesis failed"` **aborts the entire utterance**.

**Reachability:** the settings UI is a preset dropdown so it can't produce 1–2, and `adaptive_fragment_size` only ever *grows* (measured: 500 → 1000 → 1500; never below the configured value) — but the config file is hand-editable, `PaginationConfig` is **absent from the `validate()` chain** in `config/mod.rs` (trigger/tts/hud/history/hotkey are validated; pagination is not), and nothing filters fragments before synthesis. Good news: my strict no-char-loss test passes at every size — only *emptiness*, never *loss*.

**Fix (belt and suspenders, ~6 lines total):**
1. End of `paginate_text`: `fragments.retain(|f| !f.text.trim().is_empty());` then re-index `index`/`total` (or filter before constructing).
2. Add `PaginationConfig::validate()` clamping `fragment_size` to e.g. `50..=5000`, and wire it into `AppConfig::validate()`.
3. Keep my repro cases as regression tests (Appendix B).

### H2. Undeclared MSRV: the crate silently requires Rust ≥ 1.87

`telemetry.rs:295` uses `u32::is_multiple_of` (**stabilized 1.87**, June 2025), and the resolved dependency graph (`dlopen2_derive 0.4.3`, `serde_spanned 1.1.1`, ICU 2.x) requires **edition 2024 / Rust ≥ 1.85**. I hit this attempting `cargo check` on 1.82: contributors on anything but a current toolchain get cryptic *manifest parse* errors that point at random dependencies, not at your code. `Cargo.toml` declares no `rust-version`, and `.github/workflows/build-windows.yml` doesn't pin a toolchain.

**Fix:** add `rust-version = "1.87"` to `[package]` (cargo then reports the real problem in one line), pin `dtolnay/rust-toolchain@<version>` (or `stable`) in CI, and consider committing `Cargo.lock` — Tauri apps are binaries; a lockfile makes builds reproducible and would have made this whole class of issue visible in review.

### H3. The OpenAI & ElevenLabs component test suites are still gutted (15 → 1 each)

Counted at HEAD: `openai-engine.test.ts` and `elevenlabs-engine.test.ts` each retain a single smoke test; the original's 28 deleted cases covered API-key persistence, voice/model/format selection, error rendering, and save flows — exactly the surfaces the new `src/lib/mocks/` + `test-setup.ts` infrastructure was built to serve (and which `local-engine.test.ts`, 18/18 preserved, proves works). The deletion was a migration shortcut, not a judgment that the behavior stopped mattering.

**Fix:** port the 28 cases onto the new mock infra. They exist in upstream git history (`git show f466314:src/lib/components/engine/openai-engine.test.ts`); most need only the import/mock preamble swapped. Budget: an afternoon. Until then, CI green on these two files is meaningless.

### H4. `unload_piper_model` shells out to `taskkill`/`kill -9`, doesn't reap, and can't cancel a `Starting` server

**Where:** `piper_server.rs::unload_piper_model`.

- Spawning external `taskkill /F /PID` (Windows) / `kill -9` (Unix) is strictly worse than `Child::kill()` (same `TerminateProcess`/`SIGKILL` underneath): it's slower, PATH-dependent, **asynchronous** (the function returns before the process dies — racing a follow-up `restart_piper_server`'s prewarm), and runs while **holding the state lock**.
- Neither branch `wait()`s → defunct zombies on Unix.
- If the state is `Starting`, the function returns `false` and the in-flight start completes into `Ready` — so tray "Unload Model" pressed during a CUDA warmup is silently ignored, and exit-time cleanup (`RunEvent::Exit` calls this) can leak a just-starting server.

**Fix:** `child.lock().kill(); child.lock().wait();` through the Arc (no taskkill), and on `Starting` bump `CURRENT_GENERATION` + set `Stopped` — the start thread's generation checks already kill the child at the next poll/pre-Ready gate, which is exactly what they're for.

### H5. Piper "health check" still spawns a full CLI synthesis — seconds of model load that the persistent server exists to avoid

**Where:** `cli.rs::health_check` (Python branch): writes a temp file, spawns `python -m piper … -f out.wav` with a real voice, waits for a complete synthesis (CPU model load ≈ several seconds; CUDA worse) — and `cmd.output()` has **no timeout**, so a wedged Python hangs whatever UI action triggered the check. The engine page calls this via `test_tts_engine` on visit/availability refresh, so the cost is user-facing.

**Fix:** if the server state is `Ready` → `GET /voices` with a 2 s deadline and return Ok. If `Starting` → return Ok("starting"). Only when `Stopped` fall back to a *cheap* probe: `python -c "import piper"` with a 10 s timeout (verifies interpreter + package without loading a model), plus the existing `.onnx`-presence directory scan. This turns a multi-second blocking check into milliseconds and removes the hang window. (Also dedupe the voice-name list that's currently pasted into three different error strings — one `const PIPER_KNOWN_VOICES` and done.)

### H6. `is_piper()` is a substring heuristic over command *and* args — false positives reroute non-Piper engines through the Piper server

**Where:** `cli.rs::is_piper`: `command.to_lowercase().contains("piper") || args_template.iter().any(|arg| arg.contains("piper"))`.

A custom engine whose path merely contains the substring (`C:\Users\piperowski\tts.exe`, `D:\tools\bagpiper\…`), or any template that references `{data_dir}` *after expansion* (it expands to `…\piper-voices`! — check: expansion happens in `build_args`, the heuristic runs on the raw template, so today only literal "piper" in the template triggers, but the next person to "fix" ordering won't know that) gets routed into `ensure_running`, which will dutifully try `your-command -m piper.http_server …`. You already have a ground-truth signal: `tts_config.preset == "piper"` is what `config.rs` and `main.rs` key on.

**Fix:** thread the preset (or an explicit `engine_kind: PiperHttp | GenericCli` enum) into `CliTtsBackend::new` and make `is_piper()` read it. Keep the substring check only as a logged fallback for hand-rolled configs, or drop it.

### H7. The hotkey path still waits for the **entire** long text before any audio (`speak_now` + concat)

**Where:** `main.rs::spawn_speak` → `commands::speak_now` → `synthesize_paginated` → sequential fragments → `concat_wav_files` → one `emit_audio_ready` at the very end. Time-to-first-audio for a 5,000-char clipboard ≈ full synthesis time, even though `speak_queued` (used by the Play page) already streams fragment-by-fragment with pre-decode overlap. Inherited from upstream — but it's the single biggest *perceived-latency* lever left, and "real-time generation" is this branch's mission statement.

**Fix:** route the trigger/hotkey path to `speak_queued` when `should_paginate(...)` (one condition in `spawn_speak`, or inside `speak_now` delegate). Mind the differences you must preserve: `speak_now`'s history-cache hit (keep it — check cache first, fall through to queued), and file-output mode (`output_config.enabled` must keep using the concat path, since it writes one file). Estimated: small change, dramatic UX win on long texts.

---

## 5. MEDIUM — robustness & polish (M-series)

**M1. `concat_wav_files` silently corrupts on mismatched formats.** Demonstrated by execution: concatenating 22 050 Hz + 44 100 Hz fragments returns Ok with the first header stamped over all PCM (the 44.1 k tail plays at half speed). Unreachable today (one voice per batch) — but R5(b) makes per-request voices routine, and a future "change voice mid-queue" feature would hit it. Fix: parse each fragment's `fmt` and **error** on `sample_rate/channels/bits` mismatch (`"fragment 3 is 44100 Hz, expected 22050"`), which is also the honest behavior for the file-export path. Also fix the off-by-one in the skip log (`idx + 1` labels the 2nd fragment "Fragment 1") — `idx + 2`, or enumerate from 1.

**M2. RIFF chunk iteration ignores the spec's odd-size pad byte.** `offset += 8 + chunk_size` lands on the pad byte after any odd-sized chunk (LIST/INFO metadata), desyncing the scan; my padded-LIST test happened to survive by luck (garbage chunk-id parse overshot the end *after* `data` was already found in that layout — reorder the chunks and it fails). Piper's `wave`-module output is always even-aligned, so today this is theoretical; one line buys correctness for arbitrary WAVs (history re-import, future engines): `offset += 8 + chunk_size + (chunk_size & 1);`

**M3. `adaptive_fragment_size` thresholds remain ~unreachable.** Measured behavior at config 500: `< 0.5` chars/ms → 500; `0.5..5.0` → 1000; `≥ 5.0` → 1500. Realistic throughputs: Piper-CPU ≈ 0.05–0.2 chars/ms; cloud APIs ≈ 0.1–0.3 (network-bound); only warm Piper-CUDA plausibly crosses 0.5. Either recalibrate (e.g., grow at ≥ 0.15, cap by *measured* first-fragment latency target: `size ≈ target_ttfa_ms × chars_per_ms`) or delete the feature — half-alive heuristics are worse than none because they make telemetry-debugging confusing.

**M4. `get_free_port` TOCTOU.** Bind-probe-release then hand the port to Python: another process can grab it in the gap. Localhost, rare — but the failure (server exits "address in use") currently burns the whole 15/60 s readiness budget before erroring. Cheap mitigation: detect `try_wait` premature-exit (you already do) and **retry once with a fresh port** before giving up.

**M5. Warmup covers only the startup voice.** With R5(b), the first utterance after a voice switch pays the model load inside a user-visible request. Optional: on voice-change, fire-and-forget a warmup `POST /` with the new voice (the request body is two lines; no restart involved).

**M6. Pre-decoded fragments re-encode PCM→WAV even at neutral pitch/no effect.** In `handleAudioReady`, the fast path nulls `_originalBytes`, so `buildPlaybackUrl` falls into the `audioBufferToWavBlob` branch — an extra encode per fragment and (for MP3 cloud engines) a silent transcode-to-WAV of the blob. Keep the original bytes alongside `decodedBuffer` on the `QueuedFragment` and prefer them when pitch == 1 && !effect, matching the non-predecoded path.

**M7. `/piper-status` and the perf script.** After R4 lands, add the Bearer header to `test-piper-perf.ps1` (it's broken at HEAD anyway), and have it print the warmup-vs-warm split it was built to demonstrate (first call after `unload` vs. steady state).

**M8. `windows` crate is an unconditional dependency.** It compiles to stubs elsewhere but bloats non-Windows builds and resolution. Move it under `[target.'cfg(windows)'.dependencies]` alongside `winreg`.

**M9. `strip = true` in release profile.** You lose symbolicated backtraces from user crash reports. Either keep and accept, or `split-debuginfo = "packed"` + archive the PDB per release in CI (Windows builds emit PDBs regardless; the workflow just needs to upload them).

**M10. Production `console.log` noise.** `playback-store.svelte.ts` logs every fragment (base64 lengths, pre-decode notices) unconditionally. Gate behind the existing debug flag or strip.

**M11. Exit cleanup only covers graceful exits.** `RunEvent::Exit` won't fire on task-manager kill / crash, orphaning the Python server. Acceptable, but cheap insurance on Windows is a Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` wrapping the child — the OS reaps it no matter how the app dies. (The `windows` crate you already depend on exposes it.)

**M12. UI gap:** `unload_piper_model` is tray-only. A small "Unload model (free RAM/VRAM)" button next to the CUDA toggle in `local-engine.svelte` — with the status from `/piper-status`'s Tauri-side equivalent — would make the feature discoverable. (Add a `get_piper_status` **Tauri command**; the frontend shouldn't loop back through the HTTP control server + token for its own backend's state.)

---

## 6. PERFORMANCE — lock in the wins, then the next real levers

**Keep (verified good):** persistent server + warmup; pooled `reqwest` clients with `tcp_nodelay`; exponential-backoff readiness polling; single-pass decimated envelope; pipelined base64 emit (`pending_emit` overlap) in the sequential path; per-fragment pre-decode on the frontend; deferred history/telemetry saves; `OnceLock` NVIDIA-path discovery; `[profile.release] lto/codegen-units=1`.

**Next, in order of user-perceived impact:**

1. **TTFA for the hotkey path** = H7. This dwarfs everything else for long clipboard texts.
2. **Stop paying decode for duration.** Both `handleAudioReady` and the backend compute duration; the backend already parses the WAV header in microseconds (`get_wav_duration`). Ship `duration_ms` inside the `AudioFragmentEvent` payload and drop the *mandatory* `decodeAudioData` when pitch == 1 && !effect && HUD has a duration — decode only becomes a pitch/effect prerequisite. Saves ~tens of ms and a full PCM copy (≈ 10 MB/min of audio) per fragment.
3. **Skip the envelope when the HUD won't draw it.** Already done for `hud.enabled == false` — extend the same gate to "HUD hidden/minimized" if that state is knowable.
4. **Re-route Abort + adaptive timeout** (R3) — not throughput, but it converts worst-case "forever" into bounded latency, which *is* performance to the user.
5. **Measure before further micro-opts.** You already log `synth/req/read/env/hist/emit` timings — formalize: `test-piper-perf.ps1` (post-R4 fix) should emit a one-line CSV per run (cold, warm, per-voice-switch) so future PRs can claim numbers. Suggested fixed corpus: 50 / 500 / 2 500 chars × CPU & CUDA.
6. **Optional/large — true streaming synthesis.** The Flask server returns one complete WAV; first-byte ≈ full synthesis of the fragment. Piper's Python API (`PiperVoice.synthesize`) yields per-sentence chunks — a ~40-line custom sidecar (`copyspeak_piper_server.py`, shipped in the repo and launched instead of `piper.http_server`) could stream `audio/L16` chunks and let the frontend start playback after sentence 1. This is the endgame for "real-time", but it adds a component you own — only take it after R/H items are done and #1 has landed (which already captures most of the win via fragment streaming).
7. **Base64-over-IPC (prior A5)** remains the known tax: ~33 % size + encode/decode per fragment. Tauri v2 alternatives: serve audio via a custom `asset:`-style protocol handler from the cached file path, or `tauri::ipc::Response` raw bytes. Worth a prototype after #2 (which removes half the copies anyway).

---

## 7. SECURITY

1. **R4 in full** (CSPRNG token, constant-time compare, `/health` exemption, client updates, breaking-change note).
2. **Config file perms:** the token lives in plaintext config — fine for the threat model (same-user processes can read it by definition), but say so in a comment; don't let a future "sync settings to cloud" feature ship it.
3. **Bind surfaces verified good:** control server on `127.0.0.1:43117`; Piper spawned with `--host 127.0.0.1` (upstream's default is `0.0.0.0` — your explicit override is load-bearing; add a test or at least a comment saying *never remove this flag*).
4. **`get_expanded_path` / NVIDIA `PATH` prepending:** you prepend pip-discovered DLL dirs to `PATH` for the child. The dirs come from the user's own Python env, so it's self-attack only — acceptable; just keep it scoped to the child `Command` (it is) and never `std::env::set_var` it process-wide.
5. **Werkzeug dev server:** single-user localhost is its sanctioned use; document that the port is random and loopback-only. Don't expose a "remote Piper host" setting later without revisiting auth on *that* hop.
6. **`parse_piper_voice_label` / `clean_name`** handle attacker-named files in the voices dir; they're pure string ops — fine. The control server caps body via `max_text_length` — fine.

---

## 8. TESTING — make the suite trustworthy again

1. **Restore the 28 engine tests** (H3) on the new mock infra. `git show f466314:<file>` is your source.
2. **Adopt Appendix B** as `src-tauri/tests/pagination_adversarial.rs` (it needs only `pub`-visibility on `adaptive_fragment_size`, already `pub`). It encodes: empty-fragment regression (H1), no-char-loss invariant at sizes 1→500, ZWJ/4-byte/combining-mark safety, concat PCM preservation, truncated-WAV clamping, mismatched-rate detection (will go red→green when M1 lands — that's the point).
3. **Fix the lying test:** `test_piper_request_body_serialization` must assert the *decided* R1 policy, not `length_scale == speed`.
4. **CI:** the Windows workflow should run `cargo test` and `cargo clippy -- -D warnings` (the fork's "0 warnings" claim is currently unenforced), plus `npm test`. Pin the Rust toolchain (H2). Add a 5-line job that greps the three control-API clients for `Authorization` so R4 can't regress.
5. **One integration smoke worth its weight:** a `#[ignore]`d Rust test (run manually/on Windows CI) that boots `piper.http_server` against a checked-in tiny `.onnx`-less fixture? Not feasible without a model — instead, a **mock Flask-shaped server** (20 lines of `tiny_http` in dev-deps) exercising `ensure_running`'s state machine: Ready-path reuse, config-mismatch restart, premature-exit stderr surfacing, generation supersession. The state machine is the riskiest concurrent code in the repo and currently has zero tests.

---

## 9. CODE QUALITY & HOUSEKEEPING (Q-series)

- **Q1.** Delete the dead `speed` plumbing per R1; while there, `synthesize_paginated`'s `_total_estimate`/`_avg_confidence` params are recomputed inside the function — drop them.
- **Q2.** One `const PIPER_KNOWN_VOICES: &[&str]` instead of three pasted lists in error strings (cli.rs ×3).
- **Q3.** Remove `copyspeak-tts-fork-improvement-plan.md` and `copyspeak-tts-fork-improvement-plan (2).md` from the repo root (the space-in-filename one breaks naive scripts); planning docs belong in `.planning/` like upstream's, or out of tree. Replace with this file if you want it in-repo.
- **Q4.** CHANGELOG: the fork's entries describe states the code has since moved past (e.g., warmup text "Hello" → now CUDA-conditional sentence; restart-on-any-TTS-change → narrowed). Squash the perf entries into one accurate block for the eventual PR; per-commit archaeology belongs in git.
- **Q5.** `recent-history.svelte`'s new bulk-select/clear-all/export is a real feature hiding in a perf branch — split it into its own PR with the `en.json` strings and a couple of component tests; it'll review faster and won't hold the perf work hostage.
- **Q6.** `#[derive(Default)]` + `#[default]` enum migrations, `lock_or_recover!`, `Listener` import, `clippy` cleanups: all good, keep.
- **Q7.** `docs_internal/tts_backends.md` was updated for the server design — add one paragraph on lifecycle (when it starts/stops/restarts, the unload paths, and the R5 idle policy once chosen) so the next contributor doesn't re-derive it from `piper_server.rs`.
- **Q8.** Two `get_nvidia_dll_paths` copies exist (cli.rs and piper_server.rs, both `OnceLock`d). Fold into one `pub(crate)` fn in `piper_server` (or a small `win_util` module).

---

## 10. SUGGESTED EXECUTION ORDER

**Phase 0 — same-day safety (R2, R4-clients, H1-filter):**
☐ Replace the `Arc::try_unwrap`/dummy-`cmd` block with kill-through-the-Mutex (+ `wait()`), bump generation.
☐ Add `Authorization` to `.pi` extension, hook script, perf script; exempt `GET /health`.
☐ `retain(!trim().is_empty())` in `paginate_text` + the H1 regression tests.

**Phase 1 — finish the half-fixes (R1, R3, R7, H4):**
☐ Decide speed policy (recommend A), delete the plumbing, fix the test.
☐ Per-request adaptive timeout; Abort kills the server when Piper is active; mid-stream read errors mark unhealthy.
☐ Parallel path returns `Result`; unify completion/failed/stopped event contract with sequential; cover with a small async test.
☐ `unload` via `Child::kill`+`wait`; cancel `Starting` via generation bump.

**Phase 2 — lifecycle correctness (R5, R6, H5, H6):**
☐ Unload (or idle-timer) on switching away from piper; restart keyed on `command`/`cuda` only; status field renamed `default_model` or tracks last-used.
☐ Pre-flight `.onnx` existence check with the friendly download error.
☐ Health check: server-ping fast path + `import piper` probe with timeout; dedupe voice list (Q2).
☐ `is_piper` → preset-driven.

**Phase 3 — guardrails (H2, H3, M1, M2, §8):**
☐ `rust-version = "1.87"`, toolchain pinned in CI, consider committing `Cargo.lock`.
☐ Restore the 28 engine tests; adopt Appendix B; clippy `-D warnings` + `cargo test` in CI.
☐ concat format validation; RIFF pad byte; log off-by-one.

**Phase 4 — the next perf wins (H7, §6.2, M5–M7):**
☐ Hotkey long-text → `speak_queued` streaming (preserve cache-hit + file-output behavior).
☐ `duration_ms` in fragment payload; decode only when pitch/effect demands.
☐ Voice-switch warmup primer; perf script CSV mode; then *measure* before anything in §6.6–6.7.

**Pre-merge checklist:** every box in Phases 0–2 green; `cargo clippy -D warnings` & full test suites pass on Windows CI; a fresh-profile manual pass on Windows covering: cold start → hotkey speak (short & 3 000-char), voice switch (no restart, correct voice, missing-voice error), CUDA toggle both ways, Abort mid-long-synthesis (≤ 1 s to silence), engine switch away (RAM released), tray Unload during warmup, app exit (no `python.exe` left), `.pi`/hook scripts speak successfully, perf script prints status + timings.

---

## Appendix A — evidence & repro commands

```bash
git clone https://github.com/ilyaizen/copyspeak-tts original
git clone -b tts-perf-v2 https://github.com/NairoDorian/copyspeak-tts fork
cd fork && git merge-base HEAD <original>/main        # → f466314

# Empty-fragment repro (H1): extract pagination.rs + stub PaginationConfig{enabled,fragment_size},
# then:  paginate_text("Hi! Ok.", &cfg(1))  →  ["H","i","!","","O","k","."]

# length_scale ground truth (R1/R6/R5b): OHF-Voice/piper1-gpl src/piper/http_server.py —
#   POST / accepts {text, voice?, length_scale?...}; unknown voice → warning + default voice + 200;
#   GET /voices lists data-dir models; loaded_voices dict never evicts.

# Token clients lacking auth (R4):
grep -rn "Authorization" .pi/ scripts/ test-piper-perf.ps1     # → no matches

# Test gutting (H3):
git show f466314:src/lib/components/engine/openai-engine.test.ts | grep -c "it("   # 15
grep -c "it(" src/lib/components/engine/openai-engine.test.ts                       # 1

# MSRV (H2): rustc 1.82 → dep-graph 'edition2024' manifest errors; telemetry.rs uses
# u32::is_multiple_of (stable 1.87).
```

Adversarial run summary (rustc 1.82, fork code verbatim): **51/51 embedded tests pass**; adversarial: 9/11 pass, 1 fail = H1 empty fragments (real), 1 fail = harness arithmetic (mine, fixed); strict whitespace-normalized **no-char-loss holds at fragment sizes 1–500** across emoji/ZWJ/Devanagari/CJK/Cyrillic inputs; fork-vs-original fragment outputs **identical** on ASCII at sizes 10/30/80/500; truncated-WAV duration+envelope clamp correctly; concat preserves PCM byte-exactly and **silently accepts mismatched sample rates** (M1); `adaptive_fragment_size(cfg 500)` → 500/1000/1500 at 0.05/0.5/5.0 chars-per-ms (M3).

## Appendix B — drop-in regression tests

Save as `src-tauri/tests/pagination_adversarial.rs` (adjust the crate name in imports to your lib target, or inline into `pagination.rs`'s `mod tests`):

```rust
use copyspeak_tts::config::PaginationConfig;
use copyspeak_tts::pagination::paginate_text;

fn cfg(size: u32) -> PaginationConfig { PaginationConfig { enabled: true, fragment_size: size } }

#[test]
fn no_empty_or_whitespace_fragments_at_any_size() {           // H1
    for t in ["Hi! Ok.", "A. B. C.", "One? Two!", "Hi!  Double space. End.",
              "Hello 👨‍👩‍👧‍👦 world! Done 🎉. Next!"] {
        for size in [1u32, 2, 3, 5, 10, 50, 500] {
            for f in paginate_text(t, &cfg(size)) {
                assert!(!f.text.trim().is_empty(),
                    "empty fragment: text={t:?} size={size} frags={:?}",
                    paginate_text(t, &cfg(size)).iter().map(|x| x.text.clone()).collect::<Vec<_>>());
            }
        }
    }
}

#[test]
fn no_character_loss_any_size_unicode() {
    let t = "First. वाक्य दो। Третье! 四番目の文。Fifth? Mr. Smith. $3.50 today. e.g. this.";
    let norm = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    for size in [1u32, 8, 25, 60, 200, 500] {
        let re: String = paginate_text(t, &cfg(size)).iter().map(|f| f.text.as_str()).collect();
        assert_eq!(norm(&re), norm(t), "char loss at size {size}");
    }
}

#[test]
fn zwj_emoji_4byte_combining_never_panic() {                  // C2 guard
    let cases = ["👨‍👩‍👧‍👦!", "Music 𝄞? Done 🎉.", "Wait.\u{0301} Next. e\u{0301}toile!", "。", "！", "...", "e.g."];
    for t in cases { for s in [1u32, 4, 100] { let _ = paginate_text(t, &cfg(s)); } }
}
```

And for `audio/wav.rs` tests (same file's `mod tests` is the natural home):

```rust
#[test]
fn concat_rejects_mismatched_sample_rates() {                 // M1 — red until fixed
    let a = make_wav(22050, 1, 16, &[0u8; 200]);
    let b = make_wav(44100, 1, 16, &[0u8; 200]);
    assert!(concat_wav_files(vec![a, b]).is_err(),
        "mismatched-rate concat must error, not stamp the first header over foreign PCM");
}

#[test]
fn duration_and_envelope_survive_truncated_stream() {         // C5 guard (passes today)
    let mut f = make_wav(22050, 1, 16, &[0u8; 100]);
    f.truncate(f.len() - 60);
    assert!(get_wav_duration(&f).is_ok());
    assert_eq!(extract_envelope(&f, 10).unwrap().values.len(), 10);
}
```

---

*End of review. The branch's thesis is proven — model residency works and the latest commits show real engineering discipline in responding to review. Phase 0 is a day; Phases 1–2 are the difference between "fast demo" and "fast product."*
