# CopySpeak TTS — Review of `tts-perf-v2` vs upstream `ilyaizen/copyspeak-tts@main`

**Scope:** every commit on `NairoDorian/copyspeak-tts@tts-perf-v2` that sits on top of upstream `main`
(20 commits, 63 files, **+2,680 / −3,134**). Your branch was cut from the latest upstream `main`, so
the diff is exactly "your changes."

**How this was produced:** static review of the full diff, plus simulation of Rust's UTF‑8 slicing
rules to produce concrete crashing inputs. The Rust side could **not** be compiled in the review
environment (no toolchain), so treat "won't panic / will compile" claims as best‑effort reasoning,
not a green CI run. **Before shipping, run `cargo clippy`, `cargo test`, and `bun run test`.**

Bottom line: the refactor is mostly sound and contains several genuine improvements, but the AI agent
introduced **two everyday‑input bugs** (playback speed, non‑ASCII pagination), **one robustness
regression** (WAV bounds check), and **deleted ~28 real tests**. Fix those four and the branch is in
good shape.

---

## Severity legend
- 🔴 **Critical** — wrong behavior or crash on normal, everyday input. Fix before merge.
- 🟠 **High** — real regression, narrower trigger, or lost safety net.
- 🟡 **Medium** — quality/robustness; fix soon.
- 🟢 **Minor / observation** — optional polish.
- ✅ **Verified good** — change is correct; keep it (listed so you don't "fix" it by mistake).

---

## 🔴 1. Playback speed is applied twice (and inverted for Piper)

**Files:** `src-tauri/src/commands/tts/synthesis.rs:66`, `src-tauri/src/tts/cli.rs:812`,
`src/lib/stores/playback-store.svelte.ts:268,338`

**What upstream did:** the backend was always called with a hard‑coded neutral speed —
`backend.synthesize(&text, &voice, 1.0)` — so synthesis produced **normal‑speed** audio, and the
user's speed setting was applied **once**, at playback, via `audioEl.playbackRate`.

**What the fork does now:** it threads the real `playback_speed` into the backend:

```rust
// synthesis.rs:66  (now)
tokio::task::spawn_blocking(move || backend.synthesize(&text, &voice, speed))
```

…while the frontend **still** applies it again:

```ts
// playback-store.svelte.ts:268 (unchanged)
this._audioEl.playbackRate = this.speed;
```

So speed is applied **twice**:
- **OpenAI** sends `"speed": speed` to the API → sped‑up audio → then `playbackRate` speeds it again.
  At 1.5× the user effectively hears ~2.25×.
- **Piper** is worse. `cli.rs:812` sends `"length_scale": speed`. In Piper, `length_scale` is the
  **duration** multiplier — it is the *inverse* of speed (1.0 = normal, 2.0 = **half speed**, 0.5 =
  2× speed). So at 1.5× the synthesis is *slowed* to ~0.67×, then `playbackRate` speeds it back up,
  giving roughly normal duration but a **shifted pitch** and wasted work.
- ElevenLabs/Cartesia ignore the param, so they're only "single‑applied" (still fine because the
  frontend handles them), which makes the bug **engine‑dependent and confusing**.

I grep‑confirmed there is **no** `1.0/speed` compensation anywhere in the tree.

**Recommended fix (smallest, matches upstream):** stop passing user speed into the backend; let
playback own it.

```rust
// synthesis.rs:66
tokio::task::spawn_blocking(move || backend.synthesize(&text, &voice, 1.0))
```

(You can drop the now-unused `speed` plumbing through `synthesize_async`, `synthesize_queued_*`, etc.,
or just pass `1.0` at the call site.)

**Alternative (only if you deliberately want speed baked into saved files):** keep `speed` for OpenAI,
send `length_scale: 1.0 / speed` for Piper (fixes the inversion), **and** set
`audioEl.playbackRate = 1.0` on the frontend. This is more invasive and changes the semantics of
exported `.wav` files, so prefer the first option unless you have a reason.

---

## 🔴 2. Byte‑offset pagination panics on non‑ASCII text

**File:** `src-tauri/src/pagination.rs` — `is_abbreviation_at` (lines ~73–135) and the fragment
builders (`boundary.position + 1` at **lines 178 and 311**).

**What upstream did:** built `let chars: Vec<char> = text.chars().collect()` once and indexed by
**character** (`chars[pos-3..=pos]`). Indexing a `Vec<char>` can never split a UTF‑8 sequence, so it
was panic‑safe for any input.

**What the fork does now:** to avoid that allocation, it slices the `&str` by **raw byte offset**:

```rust
let slice  = &text[pos - 3..=pos];   // :82
let slice  = &text[pos - 2..=pos];   // :89
let window = &text[pos..=pos + 2];   // :99
let window = &text[pos..=pos + 1];   // :106
&text[pos + 2..pos + 3]              // :116
let sentence_end = boundary.position + 1;  // :178 and :311
```

The `pos >= 3` / `pos+2 < len` guards prevent **out‑of‑bounds**, but **not** the real failure mode:
slicing a `&str` at an index that is **not a UTF‑8 char boundary panics** (`byte index N is not a
char boundary`). Two ways this fires on normal input:

1. **Multi‑byte char near an ASCII delimiter** (accents, “curly quotes”, em‑dashes).
2. **CJK sentence delimiters.** `is_sentence_end` matches `。！？` (line 68), which are **3 bytes
   each**, so `pos` lands on a multi‑byte char *and* `sentence_end = position + 1` lands **inside** it
   (`+ 1` is wrong; it should be `+ delimiter.len_utf8()`).

I simulated Rust's exact slicing rules. Concrete inputs that **panic today**:

| Input | Where it panics |
|---|---|
| `"Según el Sr. García."` (Spanish) | `&text[pos-2..=pos]` on the final `.` after `í` |
| `"He said “hi”. Bye."` (curly quotes from a web copy) | `&text[pos-2..=pos]` after the `”` |
| `"éab."`, `".aé"`, `"x.éy"` | various windows |
| `"これはテストです。終わり。"` (any CJK) | both `is_abbreviation_at` **and** `&text[..position+1]` |

For a clipboard‑driven TTS app that explicitly supports Spanish and reads arbitrary web text, these
are **everyday** inputs, not edge cases. The panic occurs while splitting text for synthesis. In the
async fragment paths it is swallowed by `spawn_blocking`, but in the boundary‑detection path it
aborts synthesis (no audio), and with `strip = true` (see §8) the backtrace is symbol‑less.

**Recommended fix — option A (simplest, restores upstream safety):** go back to `Vec<char>` indexing
in `is_abbreviation_at` and the fragment builders. The `Vec<char>` cost is negligible for the
abbreviation window; if you want to avoid the whole‑text allocation, only options B/C below need byte
offsets.

**Option B (keep byte offsets, make them safe):** never slice without a boundary check.

```rust
// instead of: let slice = &text[pos - 3..=pos];
if let Some(slice) = text.get(pos - 3..=pos) {
    if matches!(slice.to_lowercase().as_str(), "e.g." | "i.e." | "n.b." | "etc.") {
        return true;
    }
}
```

`str::get(range)` returns `None` instead of panicking on a bad boundary, so every window becomes a
safe `if let`.

**Also fix the delimiter width (both sites):**

```rust
// :178 and :311 — was: let sentence_end = boundary.position + 1;
let sentence_end = boundary.position + boundary.delimiter.len_utf8();
```

(`force_split` at ~231–248 already walks chars correctly via `c.len_utf8()`, so it's fine — only
`is_abbreviation_at` and the `+ 1` sites need changes.)

**Add a regression test** with `"Según el Sr. García."`, `"He said “hi”. Bye."`, and a CJK string,
asserting `paginate_text` returns without panicking and the joined fragments equal the input.

---

## 🟠 3. WAV envelope dropped the data‑chunk bounds check → panic on truncated/streamed WAVs

**File:** `src-tauri/src/audio/wav.rs:164` (new single‑pass `extract_envelope`) and `parse_wav_header`
(the `data` chunk, ~lines 77–79).

**What upstream did:** `read_pcm_samples` guarded the data slice:

```rust
let data_end = info.data_offset + info.data_size;
if data_end > bytes.len() {
    return Err("Corrupted audio file: data chunk extends beyond file ...".into());
}
```

A bad/oversized `data_size` produced a graceful `Err`, and callers fell back to a default envelope.

**What the fork does now:** the rewritten single‑pass `extract_envelope` slices directly with **no**
guard:

```rust
// wav.rs:164
let data = &audio_bytes[wav_info.data_offset..wav_info.data_offset + wav_info.data_size];
```

If a WAV header over‑reports `data_size` — common with **truncated files** and **streaming WAV
writers that emit a placeholder size** — this slice **panics** (`range end index out of bounds`).
`parse_wav_header` validates the `fmt ` chunk against the file length (line 55) but **not** the
`data` chunk, so nothing catches it upstream of the slice.

**Impact / nuance:**
- The async sites (`:581, :918, :1034, :1175`) call through `extract_envelope_async`, which runs in
  `spawn_blocking`, so the panic is caught → default envelope (flat bar, wrong 2000 ms duration). Bad
  UX + a scary logged panic, but not a crash.
- **`synthesis.rs:543` calls `extract_envelope(wav_bytes, 1)` synchronously** (file‑save path). Here
  the panic is **not** caught — `.unwrap_or_else(...)` only handles `Err`, not a panic — so it
  propagates on the synthesis task.

**Recommended fix:** restore the guard in `parse_wav_header` (best — one place protects everyone):

```rust
} else if chunk_id == b"data" {
    if offset + 8 + chunk_size > bytes.len() {
        return Err("Corrupted audio file: data chunk extends beyond file".into());
    }
    data_offset = offset + 8;
    data_size = chunk_size;
}
```

…or clamp at the slice as a belt‑and‑braces measure:

```rust
let end = (wav_info.data_offset + wav_info.data_size).min(audio_bytes.len());
let data = &audio_bytes[wav_info.data_offset..end];
```

(`num_bars` is the constant `40`/`1` at the two call sites, so there's no divide‑by‑zero — only the
data slice needs guarding.)

---

## 🟠 4. The OpenAI & ElevenLabs test suites were gutted (~28 cases deleted)

**Files:** `src/lib/components/engine/elevenlabs-engine.test.ts`,
`src/lib/components/engine/openai-engine.test.ts`,
`src/lib/components/engine/engine-page.test.ts`.

| File | Upstream | Fork |
|---|---|---|
| `elevenlabs-engine.test.ts` | **15** cases (326 lines) | **1** case (25 lines) |
| `openai-engine.test.ts` | **15** cases (326 lines) | **1** case (17 lines) |
| `engine-page.test.ts` | 4 cases (263 lines) | 4 cases (144 lines) |

The deleted ElevenLabs/OpenAI cases were the **error‑handling** tests — they asserted that backend
failures (`auth_failed`, `rate_limit`, `http_error`, `not_found`, `permission_denied`, `unavailable`,
`io_error`, `unknown`), loading state, and success/failure alerts are surfaced correctly in the UI.
They were replaced by a single "renders a couple of selects" smoke test.

This is the classic refactor shortcut: the mock infrastructure was reworked (new
`src/lib/mocks/app-state.ts`, `app-navigation.ts`, `ui/mock-info-tooltip.svelte`; deleted
`vitest-plugin-mock-state.ts`; new `src/test-setup.ts`), the old tests broke, and they were **deleted
instead of ported**. You've lost exactly the coverage that catches regressions in user‑facing error
messages.

**Recommended fix:** port the 28 cases onto the new mock setup. Upstream still has them — recover with:

```bash
git show upstream/main:src/lib/components/engine/elevenlabs-engine.test.ts > /tmp/el.old.ts
git show upstream/main:src/lib/components/engine/openai-engine.test.ts   > /tmp/oa.old.ts
```

Then rewrite their `vi.mock(...)` blocks to use `src/lib/mocks/app-state.ts` etc. If a few are
genuinely obsolete, delete those **individually with a comment** rather than nuking the file.

---

## 🟡 5. Parallel cloud synthesis: silent fragment drops, no abort, delayed first audio

**File:** `src-tauri/src/commands/tts/synthesis.rs` — `synthesize_queued_parallel` (969–1094), used
for OpenAI/ElevenLabs/Cartesia with >1 fragment.

Three smaller issues:
1. **Silent failure.** A fragment whose synthesis errors leaves `per_fragment_wavs[i] = None`, which
   the emit loop `continue`s past (line ~1031). The user gets audio with a **silent gap** and **no
   error surfaced**. Consider surfacing a `pagination:fragment-error` or aborting the batch.
2. **No abort check.** Unlike the sequential path (which checks `queue.should_stop()` between
   fragments, line 879), the parallel path never checks — hitting **Stop** won't cancel in‑flight or
   pending cloud fragments.
3. **First‑audio latency.** It awaits **all** fragments before emitting **any** (collect‑then‑emit).
   The sequential path streams each fragment as it's ready. For a long paste on a cloud engine this
   delays the first sound — a little ironic on a perf branch. Consider emitting in order **as** each
   index becomes contiguously available.

None breaks correctness (ordering is preserved — results are indexed and emitted `0..N`), so this is
Medium, not Critical.

---

## 🟢 6. Telemetry: up to 9 samples lost on exit

**File:** `src-tauri/src/telemetry.rs` — `record_sample` now persists only every 10th sample via a
global `AtomicU32` (a reasonable I/O optimization). But if the app exits before hitting a multiple of
10, the trailing `<10` timing samples are **never written**. Telemetry only feeds ETA estimates, so
impact is small. Optional: flush telemetry on shutdown (you already added a Piper cleanup hook in
`main.rs` — flush there too). Also note `count.is_multiple_of(...)` requires a recent Rust toolchain.

## 🟢 7. `audio-utils.ts` uses native‑endian `Int16Array`

**File:** `src/lib/stores/playback/audio-utils.ts` — the WAV encoder switched from
`view.setInt16(off, v, /*LE*/ true)` to writing an `Int16Array`, which uses the **platform's** native
endianness rather than guaranteed little‑endian. Correct on little‑endian Windows (your only target),
so this is fine in practice — just flagging that it's technically less portable than the explicit LE
write it replaced.

## 🟢 8. `strip = true` hides production backtraces

**File:** `src-tauri/Cargo.toml` — the new `[profile.release]` (`opt-level=3`, `lto=true`,
`codegen-units=1`, `strip=true`) is a sensible perf/size win, but `strip = true` removes symbols, so
the panics in §2/§3 will produce **symbol‑less** backtraces in shipped builds. Consider
`strip = "debuginfo"` (keeps the symbol table) or `split-debuginfo`, at least until §2/§3 are fixed.

## 🟢 9. HUD timing‑delay removal (verified safe — noted for future debugging)

**File:** `src-tauri/src/hud.rs` — the old `thread::spawn` + `50 ms` sleep before each `hud:*` emit was
removed in favor of a direct `app.emit`. Because the HUD window is **persistent** (created hidden at
startup, `main.rs:511`; comment at `hud.rs:3`), its listeners are already registered, so this is a
fine optimization. *If* you ever see the HUD fail to render on the very first synthesis immediately
after launch, this removed warm‑up is the first place to look.

---

## ✅ Verified correct — keep these (don't "fix" them)

- **`AudioPlayer` / `audio/stream.rs` removal.** The rodio `play()` / `play_streaming()` path was
  **already dead** upstream (no caller; playback is frontend‑driven via the `audio-ready` event).
  Removing `stream.rs` and the `Play`/`PlayStreaming` commands is legitimate. *Note:* the remaining
  `AudioPlayer` is now a no‑op shell (its `sink` is never populated, so `is_playing()` is always
  `false`) — but that was true upstream too, so it's not a regression. You could delete it entirely
  later for clarity.
- **`HistoryManager` removal + history helper pruning** (`create_entry`, `add_entry`,
  `add_entry_complete`, `update_file_format`, `get_file_path`, …). These were only ever used by the
  deleted `HistoryManager`; no live references remain.
- **`FragmentQueue` slim‑down** (removed `next`/`previous`/`pause`/`resume`/`start`/`get_audio`/…).
  `commands/queue.rs` (unchanged) only calls methods that still exist (`status`, `current_index`,
  `len`, `fragments`, `skip_to`, `stop`, `clear`). No compile break.
- **`autostart.rs`** — only the `#[allow(dead_code)]` `is_autostart_enabled` (and its test) was
  removed; the live functions remain.
- **Piper persistent server (`cli.rs`)** — the locking is correct: `prewarm_piper_server` stores its
  child into the global `PIPER_SERVER` **before** the `ClearWarming` guard clears the flag, and the
  guard drops the lock before clearing it, so there's **no double‑start**; the lock is released before
  the HTTP synthesis request. (The "minimal lock hold time" comment is a little misleading — the lock
  *is* held across the full cold start + warm‑up — but that's correct serialization, not a bug.)
- **`config/*` changes** — mechanical `#[derive(Default)]` conversions with identical default values;
  the new `cuda: bool` field is `#[serde(default)]` (backward‑compatible with old config files);
  removed `AudioFormat::from_extension` was dead‑coded.
- **`commands/config.rs`** — Piper auto‑restart on config change is sound, **and** `set_listening`
  now persisting `listen_enabled` + emitting `config-changed` is a real **bug fix** (the listening
  state previously didn't survive restart / didn't sync the tray).
- **`control_server.rs`** — clean addition of `GET /piper-status`.
- **`clipboard.rs`** — single‑lock refactor; logic identical to upstream.
- **Frontend perf**: `analyser.ts` (reuse one `Uint8Array` instead of per‑frame alloc),
  `fragment-queue.ts` (`decodedBuffer` field), `local-engine.svelte` (CUDA toggle + dynamic Piper
  voice discovery with proper error handling) — all good.
- **Dependency bumps** — patch/minor for the JS side; `dirs` 5→6 and `winreg` 0.52→0.56 are
  API‑stable for the calls used. Low risk.

---

## Suggested order of work
1. **§1 speed** — one‑line revert; fixes a wrong‑sounding result on every engine.
2. **§2 pagination** — guard the slices / restore `Vec<char>` + fix `+ delimiter.len_utf8()`; add the
   Spanish/curly‑quote/CJK regression test.
3. **§3 WAV** — restore the `data` bounds check in `parse_wav_header`.
4. **§4 tests** — port the 28 OpenAI/ElevenLabs cases onto the new mocks.
5. Then §5 (parallel robustness) and the §6–§9 polish.

## Pre‑merge checklist
- [ ] `cd src-tauri && cargo clippy --all-targets -- -D warnings`
- [ ] `cd src-tauri && cargo test`
- [ ] `bun run test` (or `npm run test`) — confirm the restored engine tests pass
- [ ] Manual: set speed to 1.5× on **Piper** and **OpenAI**, confirm pitch/tempo sound correct
- [ ] Manual: paste a long **Spanish** passage and a long **Chinese/Japanese** passage; confirm no crash
- [ ] Manual: feed a deliberately truncated `.wav` to the file‑save path; confirm graceful handling
