# CopySpeak TTS — `tts-perf-v2` Fork Review & Improvement Plan

> **Reviewed:** `NairoDorian/copyspeak-tts` branch `tts-perf-v2` (HEAD `7b26827`, 20 commits ahead of upstream)
> **Against:** `ilyaizen/copyspeak-tts` `main` (`f466314`) — the fork's exact merge base, 0 commits behind
> **Diff size:** 63 files, +2,680 / −3,134 lines
> **Review date:** 2026-06-09

---

## 0. How to read this document

Findings are grouped by severity and tagged with stable IDs (`C` = critical, `H` = high, `A` = architecture, `P` = performance, `S` = security, `Q` = quality, `T` = testing). Each finding has: **what**, **where** (file:line on the fork's HEAD), **why it matters**, **evidence**, and a **concrete fix**. Section 12 is a suggested execution order — if you only do one thing, do **C1–C6** before shipping anything.

Two findings (**C2**, partially **C1**) were *verified by execution*, not just by reading: I extracted your pagination code into a standalone Rust binary and reproduced the panics, and I confirmed Piper's `length_scale` semantics against the official `piper1-gpl` documentation. Evidence is in the Appendix (§13).

---

## 1. Review scope & method

1. Cloned both repos; confirmed the fork merge base equals upstream HEAD (no missed upstream commits — good).
2. Read the full backend diff (`src-tauri/`) and the full frontend diff (`src/`), plus the new scripts and docs.
3. Cross-checked every "dead code removal" against actual call sites in the *original* tree to confirm nothing live was deleted.
4. Cross-checked the Piper HTTP server integration against the official `piper1-gpl` docs (`docs/API_HTTP.md`, `docs/API_PYTHON.md`).
5. Compiled and ran a standalone reproduction of the rewritten pagination logic (rustc 1.75) to confirm suspected UTF-8 panics.
6. Audited the fork's `CHANGELOG.md` claims against the code as it exists at HEAD (several claims have drifted — see Q1).

**What I could not do in this environment:** a full `cargo check`/`cargo test` of `src-tauri` (Tauri's Windows-targeted crate graph + system deps), and running the app itself. Items that depend on runtime behavior are marked **[verify on Windows]**.

---

## 2. What the fork gets right (keep these)

Credit first — several changes are genuinely good and should be kept:

* **The core idea is correct.** Replacing per-utterance `piper` process spawns (model load every call) with a persistent `piper.http_server` is exactly the right architecture for low-latency local TTS. The pre-warm-at-startup + warm-up synthesis (ONNX JIT/first-inference priming) is a real, measurable win.
* **Legitimate dead-code removal.** I verified each deletion against the original: `history_manager.rs` was instantiated in `main.rs` but never called by any command; `audio/stream.rs` / `PlayStreaming` had zero external callers; the removed `AudioPlayer::play/pause/resume`, `FragmentQueue` navigation methods, telemetry `TimingSample`, `debug_log!` macro, etc. were all genuinely dead. Nothing live was destroyed by the deletions themselves.
* **Real upstream bug fixes** you made along the way: removing the *duplicate* `tauri_plugin_global_shortcut` plugin registration (upstream registered two handlers — likely double-triggering), persisting `listen_enabled` and syncing the tray label via `config-changed`, passing the real `pagination_config` instead of `PaginationConfig::default()` into `synthesize_paginated`, and the Piper health-check no longer hard-coding the KittenTTS `"Rosie"` voice.
* **Sensible micro-optimizations:** per-backend pooled `reqwest::Client`s, precomputed `Bearer` header, `OnceLock`-cached NVIDIA DLL discovery, base64 encoding moved to `spawn_blocking`, batched `history-updated` emission, single-pass envelope extraction with decimation, `Int16Array` bulk WAV writing, reusable analyser buffer, deferred telemetry saves, and `[profile.release] lto/codegen-units=1/strip`.
* **Good operational additions:** `RunEvent::Exit` server teardown, tray "Unload Model", `/piper-status` endpoint, `test-piper-perf.ps1`, dynamic voice discovery, the CPU/CUDA setup scripts, and the rewritten Piper docs.

The problems below are mostly in the *details* of how the good ideas were executed — and in a handful of places where an optimization changed observable behavior.

---
## 3. CRITICAL — fix before anything else

### C1. Piper speed is inverted *and* double-applied; the speed slider is effectively dead for Piper

**Where:** `src-tauri/src/tts/cli.rs:812` (`"length_scale": speed`), `src-tauri/src/commands/tts/synthesis.rs:268/342/356/...` (passes `cfg.playback.playback_speed` into `backend.synthesize`), `src/lib/stores/playback-store.svelte.ts:268/338` (`audioEl.playbackRate = speed`).

**What's wrong (two stacked bugs):**

1. **Inverted semantics.** Piper's `length_scale` is *phoneme duration*: per the official piper1-gpl docs, `length_scale=2.0` means **"twice as slow"**. Your code sends the user's speed multiplier directly: speed `1.35` → `length_scale: 1.35` → Piper speaks **1.35× slower**. The correct mapping is `length_scale = 1.0 / speed`.
2. **Double application.** The original passed a hard-coded `1.0` to every backend (`synthesize(&text, &voice, 1.0)`) and applied speed *once*, at playback, via `playbackRate`. Your fork now bakes (inverted) speed into the synthesized audio **and** the frontend still applies `playbackRate = speed` to the same audio.

**Net effect at the default 1.35×:** Piper generates ~35% *more* audio frames (slower and more expensive synthesis — directly against the perf goal), then the browser time-stretches it back ~1.35× faster. Perceived speed ≈ 1.0× no matter what the slider says, with time-stretch artifacts on top. At the slider max (`speed.clamp(0.25, 4.0)` in `commands/playback.rs:118`), Piper does **4× the synthesis work** for audio that plays back at roughly normal speed. The HUD progress timing (`hudStore.setSpeed`) is computed from the same value, so its ETA is wrong for Piper too.

**Knock-on bug — stale cache hits:** `speak_now`'s history-cache lookup (`synthesis.rs:~283`) matches on `(text, voice, engine)` only. Now that speed is baked into the audio, a cached entry synthesized at one speed is replayed verbatim at another speed setting. Same for the telemetry samples: `synthesis_ms` for Piper now varies with the speed slider, polluting `chars_per_ms` stats (interacts with H4).

**Fix (pick one policy and apply it everywhere):**

* **Recommended — synthesize at target speed, play at 1.0:** send `length_scale: (1.0 / speed).clamp(0.25, 4.0)` and make the frontend set `playbackRate = 1.0` for engine-side-speed audio (e.g., add a `speed_baked: bool` to the `audio-ready` / `audio-fragment-ready` payloads). This is the *better* end state: native-rate synthesis has correct prosody (no time-stretch artifacts) and at speeds > 1 Piper generates **fewer** frames → genuinely faster synthesis. Then add `speed` to the history-cache key (or store `synthesis_speed` in entry metadata and compare).
* **Minimal/safe revert:** restore the original contract — pass `1.0` to backends, keep `playbackRate` as the single speed mechanism. Zero risk, loses the potential synthesis-speedup win.

**Acceptance test:** with speed = 2.0, a fixed sentence must (a) *sound* ~2× faster, (b) have Piper synthesis wall-time *lower* than at speed 1.0 (log line `[Piper] Synth — total:…`), (c) produce a WAV whose duration ≈ half the 1.0× duration. Add a Rust unit test asserting the `length_scale` value placed in the JSON body for a given speed.

---

### C2. Pagination rewrite panics on real-world Unicode text — **reproduced**

**Where:** `src-tauri/src/pagination.rs:178` and `:311` (`boundary.position + 1 // byte after the delimiter char`), and the byte-window slices inside `is_abbreviation_at` (`pagination.rs:73-…`, e.g. `&text[pos - 3..=pos]`, `&text[pos..=pos + 2]`, `&text[pos + 2..pos + 3]`).

**What's wrong:** the rewrite from `Vec<char>` indexing to raw byte offsets (a good perf idea) assumes **1-byte delimiters and 1-byte neighbors**. `is_sentence_end` explicitly matches the 3-byte CJK delimiters `。！？`, so `position + 1` lands *inside* the delimiter; and the abbreviation windows slice fixed byte distances around `pos` regardless of char boundaries. Slicing a `&str` off a char boundary **panics** in Rust. Since `paginate_text` runs inside the synthesis commands, any user who double-copies Japanese/Chinese text — or even Latin text with a multibyte char adjacent to a period — crashes the synthesis task (and with `speak_now` holding the global `tokio::sync::Mutex<()>`, depending on panic propagation, can wedge the speak pipeline).

**Evidence (standalone repro of your exact code, rustc 1.75):**

```text
OK    [ASCII baseline] -> 8 fragments
PANIC [Japanese CJK delimiters]
        byte index 25 is not a char boundary; it is inside '。' (bytes 24..27)
        of `これはテストです。次の文です。さらにもう一文あります。`   ← paginate_text, line `boundary.position + 1`
OK    [Accented word before period] -> 6 fragments
PANIC [CJK char right after period]
        byte index 6 is not a char boundary; it is inside '日' (bytes 4..7)
        of `End.日本語 continues here. More text.`                  ← is_abbreviation_at window check
```

(The original char-vector implementation handles all four inputs without panicking.)

**Additional logic defects introduced by the same rewrite:**

* `pagination.rs` — the `"tc"` block inside `is_abbreviation_at` is unreachable nonsense: the window starts *at the delimiter byte*, so `&text[pos..=pos+1]` always begins with `.`/`!`/`?` and can never equal `"tc"`. Delete it.
* The forward windows (`".g."`, `".e."`, `".b."`) mark the *first* dot of `e.g.` as an abbreviation but will also misfire on any text where a period is followed by `g.`/`e.`/`b.` patterns; the original handled multi-dot abbreviations by looking *backwards* only. Reconsider whether this forward pass is needed at all (the original test suite passed without it).
* The "no boundaries found" fallback sets `position: text.len().saturating_sub(1)` with `delimiter: '.'` — if the last char is multibyte this byte offset is mid-char (harmless today only because `+1` lands exactly at `text.len()`, but it's a landmine for any future use of `position`).

**Fix:**

1. Keep byte offsets (the perf goal is fine) but make every step boundary-safe:
   * `let sentence_end = boundary.position + boundary.delimiter.len_utf8();` — you already store the delimiter, use it (both in `paginate_text` and `estimate_fragment_count`).
   * Rewrite `is_abbreviation_at` to walk chars, not bytes: use `text[..pos].chars().rev()` for the preceding word and `text[pos..].chars()` for lookahead, or guard every slice with `text.is_char_boundary(i)`.
2. Add the failing inputs above as `#[test]`s, plus a mini property test: for random Unicode strings, `paginate_text` must not panic and the concatenation of fragments (modulo trimming) must reproduce the input's non-whitespace content.
3. While you're there: `force_split` trims each fragment *after* counting chars, so fragment lengths can silently undershoot `max_size` — fine, but document it; and `paginate_text` computes `text[fragment_start..sentence_end].chars().count()` per boundary, making the whole pass O(n²) on pathological many-sentence inputs — track a running char count instead.

---

### C3. Pre-decoded fragments replay the **previous** fragment's audio (stale blob-URL cache)

**Where:** `src/lib/stores/playback-store.svelte.ts` — `handleAudioReady` (`:158`), pre-decoded branch at `:162`, cache invalidation only in the *else* branch (`:172-174`), `buildPlaybackUrl` cache check (`:104-110`), `predecodeNextFragment` (`:~205`).

**What's wrong:** `buildPlaybackUrl` caches the generated object URL keyed on `(pitchRatio, effectId)` only — *not on which audio it was built from*. The non-pre-decoded path in `handleAudioReady` revokes `_cachedPitchUrl` before rebuilding, so the cache never goes stale there. But the new fast path (`if (fragment.decodedBuffer) { … }`) **skips that invalidation** (and skips updating `_originalBytes`). During multi-fragment queued playback, pitch and effect don't change between fragments, so `buildPlaybackUrl` takes the cache hit and returns **the previous fragment's URL**. Result: from fragment 2 onward, every fragment whose background pre-decode succeeded *replays the prior fragment's audio*. The bug fires precisely when the optimization works.

(Secondary: on non-Windows builds where the `_originalBytes` fast path is reachable — `!shouldApplyWindowsPreroll() && pitch==1 && !effect` — the stale `_originalBytes` causes the same wrong-audio symptom.)

**Fix:** make audio identity part of the cache, and centralize invalidation:

```ts
// in handleAudioReady, BEFORE branching on fragment.decodedBuffer:
if (this._cachedPitchUrl) { URL.revokeObjectURL(this._cachedPitchUrl.url); this._cachedPitchUrl = null; }
this._originalBytes = null;            // will be set only by the decode branch
// optionally: key the cache on a monotonically increasing audio generation id
```

Also clear `decodedBuffer` from a fragment once consumed (it currently pins the full PCM `AudioBuffer` of every played fragment until the queue object is GC'd).

**Acceptance test:** vitest with a fake 3-fragment queue where each fragment's audio is distinguishable; assert `audioEl.src` changes between fragments and that `predecodeNextFragment` ran (the current code base has *no* test covering this path — see T1).

---

### C4. Long-lived Piper server's stdout/stderr are piped but never drained → server (and synthesis) eventually freezes

**Where:** `src-tauri/src/tts/cli.rs` — both spawn sites: prewarm (`cmd.args(&args).stdout(Stdio::piped()).stderr(Stdio::piped())`, ~`:187-190`) and on-demand start inside `synthesize_via_server` (~`:715`).

**What's wrong:** for the *one-shot* CLI path the original used `wait_with_output()`, which drains the pipes. The new **persistent** server child keeps the same `Stdio::piped()` setup, but nothing ever reads those pipe handles for the lifetime of the process. `piper.http_server` is a Flask/Werkzeug-style server that logs **every request** to stderr. Once the OS pipe buffer fills (Windows anonymous pipes are small — KBs), the Python process **blocks on write** inside its logging call and stops answering HTTP. Symptom in production: "Piper works great for a while, then synthesis times out / hangs forever" — and because your synthesis client has no timeout (H2), it hangs the app's speak pipeline with it. This is a classic spawned-daemon bug and it *will* happen; the number of requests until freeze just depends on Piper's log verbosity.

**Fix (choose one):**

* Simplest: `Stdio::null()` for both (you lose Piper's logs).
* Better: keep `piped()` and spawn two drain threads per server that forward lines to `log::debug!("[piper-server] {line}")` — this also gives you the server-side error visibility you currently lack (e.g., CUDA provider fallback warnings, voice-load errors). On kill/restart the threads end naturally on EOF.

**Acceptance test:** scripted loop of 500 short syntheses against a dev build (extend `test-piper-perf.ps1` with a `-Runs 500` soak mode); must complete with stable latency.

---

### C5. Envelope extraction now panics on truncated/corrupt WAVs (validation was deleted in the rewrite)

**Where:** `src-tauri/src/audio/wav.rs:164`:

```rust
let data = &audio_bytes[wav_info.data_offset..wav_info.data_offset + wav_info.data_size];
```

**What's wrong:** the old `read_pcm_samples` explicitly checked `data_offset + data_size <= bytes.len()` and `data_size != 0` and returned a descriptive error. The single-pass rewrite dropped both checks. `parse_wav_header` takes `data_size` verbatim from the chunk header without bounds-checking it against the buffer, so any truncated download, partially-written history file, or server response with a placeholder/oversized `data` size makes this slice **panic** instead of degrading gracefully. The fork now feeds this function *more* untrusted inputs than before (HTTP responses from the Piper server, cached files from disk, concatenated fragments). `extract_envelope` is called via `spawn_blocking` from `extract_envelope_async`, whose `unwrap_or_else` handles the *join error* from the panicked task — so playback limps on with a default envelope, but you're normalizing panics-as-control-flow and losing the real error.

**Fix:** restore the two checks (clamp instead of erroring is even nicer for streamed WAVs):

```rust
let end = wav_info.data_offset.saturating_add(wav_info.data_size).min(audio_bytes.len());
if end <= wav_info.data_offset { return Err("WAV has no audio data".into()); }
let data = &audio_bytes[wav_info.data_offset..end];
```

Also: `decode_frame_mono`'s `_ => 0.0` silently flattens unsupported bit depths where the old code returned an error — at least `log::warn!` once. And `concat_wav_files` copies `first[first_data_offset..]` *to end of buffer*, i.e. any chunk *after* `data` (LIST/INFO, etc.) is concatenated as PCM noise; use the declared (clamped) `data` size per fragment instead of "rest of file". **Add a corrupt-WAV unit-test corpus** (truncated header, truncated data, data_size = u32::MAX, trailing LIST chunk).

---

### C6. "Parallel" queued synthesis waits for **all** fragments before playing anything — and drops failures silently

**Where:** `src-tauri/src/commands/tts/synthesis.rs` — `synthesize_queued_parallel` (`:~969-1100`); selection at `:776-784`.

**What's wrong (four distinct problems):**

1. **First-audio latency regression.** The collection loop only exits when the `JoinSet` is fully drained; *then* fragments are emitted in order. The original sequential loop emitted fragment 0 right after it was synthesized. For a 10-fragment cloud text, time-to-first-audio went from `t(frag0)` to `≈ total_batch_time` — for the user this *feels slower* than upstream even though throughput improved. The whole point of the queue is streaming.
2. **Silent gaps.** A failed fragment is logged and skipped (`None => continue` in the emit loop) — the listener gets a hole in the middle of their text with no error surfaced, no retry. (The commit message advertises "exponential backoff"; no request retry/backoff exists anywhere in the cloud backends — only the server-start *poll* backs off. See Q1.)
3. **No abort.** The sequential path checks `q.should_stop()` per fragment; the parallel path never checks `ABORT_REQUESTED` or the queue stop flag — a user cannot cancel a long, billable cloud batch.
4. **Telemetry/history corruption.** `_telemetry_state` is unused (no `record_telemetry`) and history entries are written with `synthesis_ms: 0`. So precisely the engines routed through this path stop feeding the stats that H4's adaptive sizing depends on.

**Fix — keep bounded concurrency, add an ordered streaming emitter:** maintain `emit_cursor`; every time a result lands in `per_fragment_wavs[idx]`, flush while `per_fragment_wavs[emit_cursor].is_some()` (emit + history + telemetry per fragment, exactly like the sequential body), checking `ABORT_REQUESTED`/`should_stop` in the same loop and calling `join_set.abort_all()` on cancel. On fragment error: retry once (this is where real exponential backoff belongs), then surface a `pagination:fragment-failed` event so the UI can show it. This preserves the 3-way concurrency win *and* restores `t(frag0)` first-audio.

**Acceptance test:** integration test with a mock backend whose fragment N has injected delay/failure; assert (a) fragment 0 is emitted before fragment N completes, (b) a failure produces an error event rather than a silent skip, (c) abort stops within one fragment.

---
## 4. HIGH priority — correctness, robustness, resource safety

### H1. Server lifecycle races: duplicate/orphaned Piper processes and a misleading "minimal lock" comment

**Where:** `src-tauri/src/tts/cli.rs` — `prewarm_piper_server` (`:131`), `restart_piper_server` (`:68`), `synthesize_via_server` (`:629`), `PIPER_WARMING` (`:43`).

**Problems:**

1. **Orphan-on-overwrite.** The prewarm thread's final step is `*server = Some(PiperServerState{…})` — it *replaces* whatever is in the mutex without killing it. `std::process::Child` does **not** kill on drop. Interleaving that produces a leaked Piper process (≈ model-size RAM each):
   `synthesize_via_server` passes the `PIPER_WARMING` check (false) → user changes config → `restart_piper_server` kills old server, sets WARMING, spawns prewarm → meanwhile the synthesis call (already past the flag check) sees `server == None`, starts **server A** and stores it → prewarm thread finishes **server B** and overwrites A → A is orphaned, still resident, never reaped.
2. **The TOCTOU window is structural.** `PIPER_WARMING` is checked *before* taking the mutex; warming can begin after the check. Conversely `restart_piper_server` doesn't care whether a synthesis-initiated start is mid-flight.
3. **Comment/code mismatch:** the block is annotated "Acquire lock briefly … released before the HTTP synthesis request", but the `need_start` branch holds the global mutex through process spawn + up to **15 s** of readiness polling + a warm-up synthesis. During that window `get_piper_server_status()` (the `/piper-status` endpoint) and `unload_piper_model_internal()` (tray "Unload Model") block. Holding it is arguably the *intent* (it's your duplicate-start guard), but then the comment is wrong and the status endpoint should not share that lock.
4. `lock().unwrap()` everywhere on `PIPER_SERVER` — one panic while holding it (e.g., C2/C5 in a caller's thread is fine, but any future panic inside the critical section) poisons the mutex and every subsequent Piper call panics.

**Fix — make a real `PiperServerManager` (also solves Q2's 120-line duplication):** one module owning a small state machine `Stopped | Starting{generation} | Ready(state)`, a **single** `ensure_server(cfg) -> Result<Handle>` used by prewarm, on-demand start, and restart; a generation counter so a finishing starter that has been superseded kills *its own* child instead of storing it; `parking_lot::Mutex` or `lock().unwrap_or_else(|p| p.into_inner())` to neutralize poisoning; status read from an `ArcSwap`/`RwLock` snapshot so `/piper-status` and "Unload Model" never wait behind a 15 s start.

### H2. No timeout on Piper synthesis HTTP requests; abort cannot touch the server path; a hang wedges `speak_now` forever

**Where:** `get_piper_client` (`cli.rs:20-29` — `tcp_nodelay + pool_max_idle_per_host(2)`, **no `.timeout()`**); request at `cli.rs:815-819`; `ACTIVE_CLI_PID` set only in the CLI fallback (`cli.rs:913`); global speak lock at `synthesis.rs:251-252`.

**Problem chain:** a stuck server (see C4 — you've built one) → `client.post(...).send()` blocks indefinitely inside `spawn_blocking` → `speak_now` never returns → it still holds the app-wide `tokio::sync::Mutex<()>` → **every** subsequent speak waits forever; abort can't help because the abort path only kills `ACTIVE_CLI_PID`, which is never set on the server path.

**Fix:** (a) build the synthesis client with a generous-but-finite timeout — e.g. `connect_timeout(2s)` and a total timeout scaled to text length (`max(30s, chars/expected_cps * 3)`), or use per-request `.timeout(...)`; (b) on timeout, mark the server unhealthy → kill + restart it (don't just fall back to one-shot CLI while the zombie keeps the port and RAM); (c) make abort effective for Piper: since the blocking request can't be cancelled mid-flight, have `abort_synthesis` set a generation/abort flag the synthesis task checks immediately after `send()` returns and *before* emitting/saving, and optionally kill+restart the server when the user aborts a very long synthesis.

### H3. Any TTS sub-config change restarts the Piper server — including totally unrelated fields

**Where:** `src-tauri/src/commands/config.rs:~100` — `let tts_changed = old_tts_config != new_config.tts;` gating `restart_piper_server`.

**Problem:** `TtsConfig` includes the OpenAI/ElevenLabs/Cartesia sub-configs. While Local+piper is active, editing an *ElevenLabs API key* (or any cloud field, or `args_template` whitespace) kills the warmed server and pays the full model reload + warm-up again. Also, the restart fires even when only fields Piper doesn't consume changed.

**Fix:** compare only the fields that parameterize the server: `(command, voice, cuda, preset, active_backend == Local)`. Bonus: when `active_backend` switches *away* from Local/piper, decide a policy explicitly — either keep the server warm (document it; the tray "Unload Model" exists for reclaiming RAM) or stop it after an idle TTL (see P6).

### H4. `adaptive_fragment_size` thresholds are physically unreachable — the feature is dead code in practice

**Where:** `src-tauri/src/pagination.rs:268-…` (`chars_per_ms > 20.0` / `> 5.0`), fed by `telemetry.rs:60-66` (`chars_per_ms = chars / duration_ms`).

**Problem:** units. 20 chars/ms = **20,000 chars/second** of synthesis. Real numbers: Piper-GPU warm ≈ 1–3 chars/ms, Piper-CPU ≈ 0.1–0.5, cloud APIs ≈ 0.1–0.4 (network-bound). Neither branch can ever trigger, so every engine silently keeps the default size, and the CHANGELOG advertises behavior that cannot occur. (Compounded by C6.4: the parallel path records no telemetry at all.)

**Fix:** recalibrate from your own telemetry logs (suggested: `> 1.0` chars/ms → 3×, `> 0.3` → 2×), or express thresholds as "synthesis is N× faster than realtime playback" which is self-documenting. Add a unit test pinning the mapping. Then actually wire telemetry recording into the parallel path so the feature has data.

### H5. 15 s readiness window is too tight for CUDA cold starts; "ready" accepts any TCP response

**Where:** both readiness loops (`cli.rs:~230` and `:~738`): 15 s budget; `health_client.get(&url).send().is_ok()` counts *any* HTTP response (or even a 404) as ready; prewarm's health client has a 500 ms per-attempt timeout, the on-demand loop reuses the *no-timeout* synthesis client for polling (a slow-accepting socket can eat the whole budget in one call).

**Fix:** raise the budget for `cuda == true` (first CUDA init + cuDNN load on a cold driver can exceed 15 s) — e.g. 60 s with the same backoff; require `resp.status().is_success()` *and* ideally parse `/voices` JSON (also lets you cache the voice list — see A1); give the polling client its own short timeout in both paths. On failure, capture the child's stderr tail (you'll have it once C4's drain threads exist) into the error message instead of a bare "Timeout waiting for Piper server to start".

### H6. Switching engines now wipes the user's chosen voice every time

**Where:** `src/lib/components/layout/app-footer.svelte:349-352` — `// Always set default voice when switching to a local engine`.

**Problem vs original:** upstream kept `tts.voice` unless empty (which had its own bug: a Piper voice string leaked into Kokoro). Your fix overcorrects: Piper → OpenAI → Piper now resets a carefully chosen `en_US-libritts_r-medium` back to the default. **Fix:** remember the last voice *per preset* (`lastVoiceByPreset[preset]` persisted in config) and restore it on switch; fall back to the default only when unset. This fixes both the upstream bug and your regression.

### H7. `FragmentQueue::set_audio` is now a write-only RAM sink

**Where:** `synthesis.rs:~915` (`q.set_audio(index, wav_bytes.clone())`), `fragment_queue.rs` (the matching `get_audio`/`has_audio` were correctly deleted as dead).

**Problem:** after your dead-code pass, *nothing can ever read* the stored audio — but the sequential path still clones every fragment's full WAV into the queue, holding **all** fragments' audio simultaneously until the post-loop `q.clear()`. A 30-fragment article ≈ tens of MB held for no reason, plus one redundant full-buffer clone per fragment on the hot path. **Fix:** delete `set_audio` and the `audio` field from `QueuedFragment` (Rust side); the frontend already receives audio via events.

### H8. Replay/cache correctness beyond speed

**Where:** `speak_now` cache probe (`synthesis.rs:283-300`), `cache_audio` (`:227`).

The history-cache match `(text, voice, engine, success)` predates several knobs that now alter output for identical text: playback-speed-baked audio (C1), the `cuda` flag (different numerical output is fine, but after fixing C1 the *speed* must be in the key), and LLM post-processing (`post_process::try_process` rewrites `text` *before* the lookup — good — but sanitization settings changing between runs aren't reflected). Decide the cache identity explicitly — suggested key: `(processed_text, engine_id, voice, synthesis_speed)` — and write it into entry metadata so old entries can be invalidated. Also note the probe is an O(n) reverse scan of all history under the lock on *every* speak; fine at hundreds of entries, but an index (HashMap of key → entry id, rebuilt on load) removes it from the hot path.

### H9. `speak_now` vs `speak_queued` concurrency rules diverge

`speak_now` serializes through the managed `tokio::sync::Mutex<()>` (`synthesis.rs:251`); `speak_queued` (and `speak_history_entry`) never acquire it. Double-copy (→ queued path in the UI flow) racing a hotkey `speak_now` can interleave two pipelines: two `SynthesisGuard`s toggling the tray state, interleaved HUD events, and concurrent Piper requests serializing only at the Python server. Pick one rule — most likely "all speak entry points take the same lock, abort cancels the holder" — and apply it to all three commands.

---
## 5. ARCHITECTURE — simplifications that remove whole bug classes

### A1. Stop restarting the server on voice change — the Piper HTTP API already does multi-voice

**Why this is the biggest lever:** the official `piper.http_server` accepts an optional **`"voice"` field in the POST body** ("name of voice to use; defaults to `-m <VOICE>`") and serves `GET /voices`; with `--data-dir` pointing at your `~/piper-voices`, one server can synthesize any installed voice, loading it on demand and keeping it cached. Your current design treats *voice* as part of server identity (`state.model_name == voice` → kill + respawn + re-warm on every voice switch), which is exactly where the H1 races, the H3 over-restarts, and ~½ of the duplicated startup code come from.

**Proposed end state:**

* Server identity = `(command, data_dir, cuda)` only. Voice goes in each request body: `{"text": …, "voice": voice, "length_scale": 1.0/speed}`.
* Voice switch = zero downtime (first request with a new voice pays its load once, inside the server).
* `GET /voices` replaces both the readiness probe payload **and** the filesystem scan in `get_local_piper_voices` (`commands/tts/voices.rs`) — one source of truth, and the dropdown reflects what the *server* can actually load. Keep the fs scan only as a fallback when the server is down.
* Optionally warm a newly-selected voice with the existing hidden "Hello" request in the background on config change — a cheap request instead of a process restart.

**[verify on Windows]:** confirm your installed piper version's `http_server` honors per-request `voice` with `--data-dir` (documented in current `piper1-gpl`; pin the version in the setup scripts — see S4 — so this stays true).

### A2. Extract `piper_server.rs` and delete the duplication

`prewarm_piper_server` and the `need_start` branch of `synthesize_via_server` are ~120 lines of near-identical spawn/poll/warm-up code that have *already drifted* (different poll clients, different log text, comments describing older revisions). All lifecycle logic — `ensure_running`, `restart_if(cfg_changed)`, `unload`, `status`, the warming guard, exit cleanup — belongs in one `tts/piper_server.rs` with the H1 state machine. `cli.rs` then shrinks back to "CLI backends + thin HTTP call", and `CliTtsBackend::is_piper()`'s string-sniffing heuristic (`command contains "piper"` — which also misfires if a kokoro script path merely contains "piper") can be replaced by passing the preset explicitly from config into `create_backend`.

### A3. Ordered streaming emitter for parallel synthesis (the C6 fix, as architecture)

Generalize: a small `OrderedEmitter { next: usize, buf: BTreeMap<usize, Wav> }` that any producer (sequential, parallel, future streaming APIs) feeds, and which owns "emit + envelope + history + telemetry per fragment". Sequential and parallel paths currently duplicate that ~40-line block with subtle differences (telemetry present/absent, `synthesis_ms` real/zero) — one consumer ends the divergence.

### A4. Share one HTTP client per cloud engine across utterances

`create_backend` runs **per command invocation** (`commands/tts/helpers.rs:50`), so each ElevenLabs/OpenAI/Cartesia backend gets a *fresh* `reqwest::Client` → fresh pool → the advertised "eliminating TLS handshake per synthesis request" only holds *within* one paginated batch, not between utterances — which is the common case. Fix: `static OnceLock<Client>` per engine module (you already did exactly this for Groq in `post_process/mod.rs` — copy that pattern), with the per-instance config still owning keys/voices. `reqwest::Client` is internally `Arc`'d; cloning the static is the intended usage.

### A5. (Larger, optional) Replace base64-over-IPC audio transport

Every fragment is WAV → `base64` (+33% bytes) → Tauri event JSON → `atob` → copy → `decodeAudioData`. For long texts this is the dominant non-synthesis cost and memory churn. Tauri v2 options: serve the audio over a custom protocol / `convertFileSrc` from a temp file (you already write history files anyway), or `tauri::ipc::Response` raw bytes. Defer until C/H items land; measure first with the timing logs you already added (`emit:Xms`).

---

## 6. PERFORMANCE — locking in the wins, with measurement

> **Method first:** extend `test-piper-perf.ps1` into the project's benchmark harness: cold-start, warm single-shot, 20-fragment article, soak (C4 test), each reporting p50/p95 from the `[Piper] Synth — total/lock/req/read` and `[TTS] Pipeline — synth/env/hist/emit` log lines you added. Every item below should land with a before/after from this harness, on CPU and CUDA.

* **P1 — C1 done right is itself a perf win:** at user speeds > 1×, `length_scale = 1/speed` means *fewer frames to infer* — synthesis time drops roughly proportionally. Today you pay the inverse penalty.
* **P2 — True streaming for local long texts:** after A3, route Local/piper through the bounded-concurrency path with `MAX_CONCURRENT = 1..2` *and* ordered streaming emit. Even at concurrency 1 this gives "synthesize fragment N+1 while N plays" (the CHANGELOG already claims Piper goes through the parallel path; the code at `synthesis.rs:776-781` says otherwise — see Q1). Watch CPU contention between ONNX inference and playback on weak machines; make concurrency 1 the Piper default.
* **P3 — Warm-up honesty:** the prewarm comment says "send a substantial sentence … longer text warms more model layers" while the body sends `"Hello"` (`cli.rs:263`). For CUDA, a ~1–2 sentence warm-up measurably finishes kernel autotuning better than a single word; for CPU, "Hello" is fine. Make the warm-up text a const per mode and fix the comment.
* **P4 — Preroll 1200 ms → 200 ms (`playback-store.svelte.ts:24`) needs a device matrix [verify on Windows]:** that preroll exists to swallow sink wake-up; Bluetooth headsets and some USB DACs take 300–800 ms to open a stream, and with 200 ms the first syllables get clipped. Test wired/BT/HDMI; if any clip, make it a setting (`playback.preroll_ms`, default 200, "increase if speech start is cut off") rather than re-pessimizing everyone.
* **P5 — Envelope work is duplicated:** `handle_file_output` runs `extract_envelope(wav,1)` just for duration, and the playback path extracts a 40-bar envelope the HUD then largely supersedes with live analyser data. Cheap wins: compute duration from `WavInfo` directly (no PCM pass), and skip envelope extraction entirely when the HUD is disabled/hidden.
* **P6 — Idle RAM policy:** the model now lives in RAM forever (that's the feature) — add an optional idle TTL ("unload after N min unused", default off) using the manager from H1, so laptop users aren't forced to find the tray item. Also flush telemetry on exit: the new save-every-10-samples batching (`telemetry.rs`) loses up to 9 samples per session because `RunEvent::Exit` only tears down Piper.
* **P7 — Micro-notes:** audio thread poll 50 → 200 ms saves wakeups but adds up to 200 ms latency to `playback_finished` detection and tray icon updates — acceptable, but document it next to the constant; the `cleanup_artifacts` "optimization" still executes both regex passes unconditionally and only skips the final *assignment* (`sanitize/cleanup.rs`) — either early-exit after a no-change first pass or revert to the simple loop; `get_expanded_path`'s hard-coded Python 3.10–3.14 directory list keeps growing — prefer `where`/`py -0p` discovery once, cached.

---

## 7. SECURITY & PRIVACY

* **S1 — Control server is unauthenticated (pre-existing, surface grew):** `127.0.0.1:43117` accepts `POST /speak` from *any local process or browser tab* (a web page can fire `fetch("http://127.0.0.1:43117/speak", {method:"POST", mode:"no-cors", body:…})` — making your machine speak arbitrary text, switch the active engine via the request's `engine` field, and burn cloud-API credits). Your fork adds `/piper-status` (minor info disclosure: model name, port, CUDA). Fix: generate a per-install token at first run, require `Authorization: Bearer` (loopback CSRF can't set it cross-origin without a preflight your parser will reject), and have `test-piper-perf.ps1` read the token from the config file. Cheap and closes a real hole.
* **S2 — Arbitrary command execution is the configured design** (`tts.command` + `args_template` run as the user) — acceptable for a power-user tool, but: the new `--cuda`/server args are appended to a user-controlled command line, and `build_args` strips surrounding quotes per-arg. Document clearly in settings UI that this field executes programs; never let a future "import settings" feature populate it from untrusted JSON without a confirmation dialog.
* **S3 — Clipboard text written to world-default-permission temp files:** the CLI fallback writes the spoken text to a *fixed, predictable* path `%TEMP%\copyspeak_tts_input.txt` (`cli.rs:595`) and leaves it there on some error paths; the HTTP path avoids this (good — note it as another A1 benefit). Use `tempfile::NamedTempFile` (random name, auto-delete) for the fallback, and audit that history-off mode never persists text to disk.
* **S4 — Supply-chain hygiene in the setup scripts:** `setup-piper-*.ps1` and the in-app NVIDIA path discovery install/import **unpinned** `piper-tts[http]`, `onnxruntime(-gpu)` and eight `nvidia-*-cu12` wheels. Pin known-good versions (`piper-tts[http]==1.4.x`, `onnxruntime-gpu==1.x.y`) — this is both a security and a compatibility guarantee (A1's per-request `voice` support, `--cuda` flag presence). Also `get_nvidia_dll_paths` executes the configured Python with `-c import nvidia…` — fine, but it runs whatever `tts.command` points at; combine with the S2 warning.
* **S5 — Housekeeping:** the new `/piper-status` JSON is fine, but don't grow this server without auth (S1). `export-zip.ps1` shells out to 7-Zip from fixed paths — harmless, but quote `$OutputPath`.

---

## 8. CODE QUALITY — drift, duplication, consistency

### Q1. CHANGELOG/comments vs code: reconcile or readers (and future-you) will be misled

Verified mismatches at HEAD:

| Claim (CHANGELOG.md / comment) | Code reality |
|---|---|
| "Piper parallel paginated synthesis — speak_queued now routes the Piper preset through synthesize_queued_parallel" | `synthesis.rs:776-781` matches **only** OpenAI/ElevenLabs/Cartesia; commit `ed4084b` deliberately removed Piper from the comment but the CHANGELOG entry stayed |
| "exponential backoff" (commit `c37ad27`, CHANGELOG) | No request retry/backoff exists in any backend; only the server-start *poll* delay backs off |
| "Health-check poll … every 50ms for the first 2 seconds, then 200ms" | Both loops: 100 ms doubling to 1600 ms cap |
| "Replaced custom builder with bare `reqwest::blocking::Client::new()`" | `get_piper_client` uses a builder with `tcp_nodelay` + `pool_max_idle_per_host(2)` |
| "single cohesive critical section … eliminating race windows for duplicate server processes" | H1.1 race demonstrably remains |
| "Acquire lock briefly … released before the HTTP request" (`cli.rs` comment) | Lock held across spawn + ≤15 s poll + warm-up in the start branch |
| "send a substantial sentence" warm-up comment | Body is `"Hello"` |
| "Cleanup pass … only re-runs if the text actually changed" | Second pass always runs; only the assignment is conditional |

**Action:** one cleanup commit that rewrites the CHANGELOG's fork section to match HEAD, and fixes the in-code comments. Going forward, write CHANGELOG entries from the *final* diff, not per-iteration.

### Q2–Q9 (grouped)

* **Q2 Duplication:** the two server-start blocks (→ A2); the two emit helpers `spawn_fragment_emit` / `emit_audio_fragment` (same payload, different scheduling) — keep one with a `pipelined: bool`; sequential vs parallel post-fragment blocks (→ A3).
* **Q3 Mutex hygiene:** `lock().unwrap()` on `PIPER_SERVER`, config, history, telemetry, queue throughout new code, while `set_listening` uses `map_err` — pick one policy (`parking_lot`, or a `lock_or_recover!` helper) crate-wide.
* **Q4 Error typing:** `synthesize_via_server` returns `Result<_, String>` inside a `TtsBackend` world of `TtsError` — wrap server errors as `TtsError::CommandFailed`/new `TtsError::Server` so the UI's error mapping stays uniform, and so the fallback log can distinguish "server down" (worth restarting) from "bad request" (don't bother).
* **Q5 Heuristics → config:** `is_piper()` string sniffing and the `preset == "piper"` checks in `main.rs`/`config.rs` are two different detectors for the same concept; thread the preset enum through `create_backend` (see A2) and delete the sniffing.
* **Q6 Dead/odd logic from the rewrites:** the unreachable `"tc"` branch and questionable forward-window checks in `is_abbreviation_at` (C2); `decode_frame_mono`'s silent `_ => 0.0`; `extract_envelope`'s `bar_start/bar_end` stride-rounding can make the last bar disproportionately wide — fine visually, but comment it.
* **Q7 Frontend nits:** `predecodeNextFragment` errors are swallowed with an empty `catch {}` — at least `console.debug`; the single `try/catch` in `local-engine.svelte` resets `dataDir/homeDir` to placeholders if only the *voices* invoke fails — split the calls; `Int16Array` WAV writing assumes little-endian (true on all real targets — add a one-line comment); hoist `buffer.getChannelData(c)` out of the per-sample loop.
* **Q8 Event-payload growth:** `AudioFragmentEvent` carries the full fragment `text` alongside the audio for every fragment — the frontend HUD only shows a preview; truncate at the source.
* **Q9 Progress math:** `total_chars = text.len()` and `processed_chars` sum byte lengths while `fragment_size` is in chars — mixed units make the HUD % drift on non-ASCII; standardize on `chars().count()` (cache it).

---
## 9. TESTING & CI — the fork currently moves backwards here; reverse that first

### T1. Restore the deleted frontend test coverage (the single biggest quality regression)

Commit `7578c19` ("chore: upgrade frontend and backend dependencies…") describes itself as fixing "stale assertions", but it actually **deleted ~28 of 34 engine-settings tests**: `openai-engine.test.ts` 15 → 1, `elevenlabs-engine.test.ts` 15 → 1 (each now a single "renders two selects" smoke test), and `engine-page.test.ts` was reduced to 4 shallow tests with a stripped mock config. The deleted tests covered the interactions you've since been changing by hand: voice/model selection writing through to `localConfig`, API-key field behavior, format options, validation messaging. Whatever motivated the purge (Svelte 5.56 / vitest 4 friction, the bits-ui tooltip context errors you mocked around), the fix is to *port* the tests to the new mocking setup (`src/lib/mocks/*`, `mock-info-tooltip`), not delete them. **Action:** resurrect them from upstream (`git show upstream/main -- src/lib/components/engine/openai-engine.test.ts`), adapt to the new `test-setup.ts`, and add the missing tests for everything new: `local-engine` CUDA toggle + dynamic voice list, `recent-history` bulk select/delete/export flows, and the C3 fragment-playback scenario. Separately: the changed *expected output* in `sanitize/markdown.rs`'s `test_strip_lists_legend_example` (joined-prose → newline-preserving) altered the documented behavior contract inside a "dependency upgrade" commit — confirm which output you actually want spoken and write the rationale into the test name/comment.

### T2. Rust tests to add (each maps to a finding)

* Pagination Unicode property/regression tests (C2 — the four repro inputs are ready-made fixtures) and a `cargo fuzz`/`proptest` target for `paginate_text` + `is_abbreviation_at`.
* WAV corpus tests for `extract_envelope`/`parse_wav_header`/`concat_wav_files` (C5): truncated header, truncated data, `data_size` overflow, trailing LIST chunk, 8/24/32-bit, stereo.
* `length_scale` mapping test (C1) and a serialization test of the exact Piper request body.
* `adaptive_fragment_size` threshold tests with realistic `chars_per_ms` fixtures (H4).
* A fake-Piper integration test: a 30-line Rust (or Python) stub HTTP server with controllable latency/failure lets you test the whole manager — readiness timeout, voice switch, C4 soak, H2 timeout-then-restart — in CI without models.

### T3. CI pipeline (the fork has `.github/` but no enforcement of the new code)

GitHub Actions matrix: `cargo fmt --check`, `cargo clippy -- -D warnings` (you already got to 0 warnings in `2726c69` — pin that), `cargo test` (Linux runner is fine for the pure-logic modules; gate `#[cfg(windows)]` code behind unit-testable seams), `bun run check`, `bun run test`, plus a Windows job for `tauri build`. Add the soak/perf script as a manually-triggered workflow that uploads its timing report as an artifact, so perf claims in future CHANGELOG entries come with numbers.

### T4. Runtime guardrails

`std::panic::set_hook` logging panics to the flexi_logger file (today a panic inside `spawn_blocking` synthesis vanishes into a generic "Task join error"), and a debug-build assertion that no `Stdio::piped()` child outlives its drain (C4 class).

---

## 10. UPSTREAM — extract the clean fixes into PRs

These are independent of the Piper-server work, valuable to `ilyaizen/copyspeak-tts`, and shrinking your divergence makes future rebases cheaper: (1) duplicate global-shortcut plugin registration removal + `spawn_speak` dedup; (2) `set_listening` persistence + tray-label sync + startup label from config; (3) `synthesize_paginated` honoring the real `pagination_config`; (4) the CLI health-check using a discovered local voice instead of hard-coded `"Rosie"`; (5) the genuinely-dead-code removals (`history_manager.rs`, `audio/stream.rs`, dead methods) as a separate "chore" PR; (6) once C5 is fixed, the faster envelope extraction *with* its restored validation and tests. Keep the Piper server, CUDA, and UI work on your branch until §3–§4 are done — then it's a strong feature PR.

---

## 11. HOUSEKEEPING

* `.planning/`, `docs_internal/`, `agent-harness/`, `.pi/`, `skills/` ship in the repo root (inherited from upstream, but the fork keeps growing `docs_internal/` and `CHANGELOG.md` is now 42 KB) — decide what's user-facing; at minimum exclude internal dirs from release archives (`export-zip.ps1` / `.vercelignore` already gesture at this).
* New i18n keys (`history.clearAll*`) exist only in `en.json`; the repo's note says non-English locales live externally — make sure those keys enter that external flow or ES users see raw keys.
* `Cargo.toml` release profile additions are good; consider `panic = "abort"` only *after* T4's panic hook and the C2/C5 fixes (today aborting on those panics would crash the whole app instead of one task).
* Version pinning for the JS toolchain: the dep-upgrade commit moved to Svelte 5.56/Vite 8/Vitest 4 — capture the exact bun lockfile in the repo if it isn't already, so the (restored) tests run reproducibly.

---

## 12. SUGGESTED EXECUTION ORDER

**Milestone 1 — "Correct again" (small diffs, ship together):**
C1 (decide policy; minimal revert is one line + cache-key note) → C2 (boundary-safe pagination + tests) → C3 (cache invalidation, ~5 lines) → C5 (restore WAV bounds checks) → C4 (drain or null stdio) → H7 (delete `set_audio`). *Exit criteria:* repro binary's 4 cases pass inside `cargo test`; speed slider audibly works on Piper; 500-synthesis soak passes; multi-fragment queued playback plays distinct audio per fragment.

**Milestone 2 — "Robust server":**
H1 + A2 (the `PiperServerManager` rewrite subsumes both) → H2 (timeouts + abort) → H5 (CUDA-aware readiness) → H3 (precise restart predicate). *Exit criteria:* config-change storm test (toggle CUDA/voice 10× rapidly) ends with exactly one `piper` process; kill -9 of the server mid-synthesis recovers within one request.

**Milestone 3 — "Fast where it counts":**
C6/A3 (ordered streaming emitter) → A1 (per-request voice, server identity = command+data_dir+cuda) → P2 (Piper through bounded pipeline) → A4 (shared cloud clients) → H4 (recalibrated adaptive sizing, now with real telemetry) → P4 preroll matrix. *Exit criteria:* harness shows first-audio for a 10-fragment cloud text ≈ single-fragment latency; voice switch < 200 ms perceived.

**Milestone 4 — "Trust the codebase":**
T1 test restoration → T2 Rust tests → T3 CI → Q1 CHANGELOG/comment reconciliation → S1 control-server token → S3/S4 → §10 upstream PRs → P5/P6/Q-nits.

---

## 13. APPENDIX

### 13.1 Pagination panic reproduction

Extracted verbatim from `fork/src-tauri/src/pagination.rs` (structs + `detect_sentence_boundaries` + `is_abbreviation_at` + `paginate_text` + `force_split`), `PaginationConfig{enabled:true, fragment_size:10}`, wrapped in `catch_unwind`:

```text
OK    [ASCII baseline] -> 8 fragments
PANIC [Japanese CJK delimiters]
  byte index 25 is not a char boundary; it is inside '。' (bytes 24..27)
  of `これはテストです。次の文です。さらにもう一文あります。`
OK    [Accented word before period] -> 6 fragments
PANIC [CJK char right after period]
  byte index 6 is not a char boundary; it is inside '日' (bytes 4..7)
  of `End.日本語 continues here. More text.`
```

Panic #1 originates at the `boundary.position + 1` slice in `paginate_text`; panic #2 in `is_abbreviation_at`'s forward window. The upstream char-vector implementation processes all four inputs.

### 13.2 External references checked

* `piper1-gpl/docs/API_HTTP.md` — POST `/` fields: `text` (required), **`voice` (optional, defaults to `-m`)**, `speaker`, `speaker_id`, `length_scale` ("speaking speed; defaults to 1"), `noise_scale`, `noise_w_scale`; `GET /voices`; `--host/--port/--data-dir`; default port 5000.
* `piper1-gpl/docs/API_PYTHON.md` — `length_scale=2.0  # twice as slow` (the C1 semantics), GPU via `onnxruntime-gpu`.

### 13.3 Finding → location quick index

| ID | Primary location |
|---|---|
| C1 | `tts/cli.rs:812`, `commands/tts/synthesis.rs:268+`, `playback-store.svelte.ts:268` |
| C2 | `pagination.rs:73-145, 178, 311` |
| C3 | `playback-store.svelte.ts:104-114, 158-176, ~205` |
| C4 | `tts/cli.rs:~188, ~715` (both `Stdio::piped()` spawn sites) |
| C5 | `audio/wav.rs:164` (+ `parse_wav_header`, `concat_wav_files`) |
| C6 | `commands/tts/synthesis.rs:776-784, ~969-1100` |
| H1/A2 | `tts/cli.rs:39-145, 629-810` |
| H2 | `tts/cli.rs:20-29, 815`, `main.rs:250`, `synthesis.rs:251` |
| H3 | `commands/config.rs:~100-130, 182-196` |
| H4 | `pagination.rs:268+`, `telemetry.rs:60-66`, `synthesis.rs:662-691` |
| H5 | `tts/cli.rs:~225-250, ~735-760` |
| H6 | `app-footer.svelte:349` |
| H7 | `synthesis.rs:~915`, `fragment_queue.rs` |
| H8 | `synthesis.rs:283-300` |
| H9 | `synthesis.rs:251 vs 636+` |
| T1 | commit `7578c19`; `src/lib/components/engine/*.test.ts` |

*End of plan. Suggested branch names per milestone: `fix/correctness-pass`, `refactor/piper-server-manager`, `perf/streaming-pipeline`, `chore/tests-ci-docs`.*
