/**
 * Global Playback Store
 *
 * Single source of truth for audio playback state across all routes.
 * The GlobalPlayer component mounts the <audio> element and calls
 * setAudioElement() + setupListeners() on mount.
 *
 * Supports streaming pagination playback: fragments are queued and played
 * sequentially as they arrive from the backend.
 */

import { isTauri } from "$lib/services/tauri.js";
import type { EffectId } from "$lib/types";
import {
  audioBufferToWavBlob,
  detectAudioMimeType,
  prependLowLevelPreroll
} from "./playback/audio-utils.js";
import { AudioAnalyser } from "./playback/analyser.js";
import { getEffect } from "./playback/effects/registry.js";
import { FragmentQueue, type QueuedFragment } from "./playback/fragment-queue.js";
import { hudStore } from "./hud-store.svelte.js";

const WINDOWS_AUDIO_PREROLL_MS = 200;

class PlaybackStore {
  isPlaying = $state(false);
  isPaused = $state(false);
  isSynthesizing = $state(false);
  error = $state<string | null>(null);
  hasCachedAudio = $state(false);

  // Pagination state for HUD display
  currentFragmentIndex = $state<number | null>(null);
  totalFragments = $state<number | null>(null);

  // Synced from config by whoever has it loaded (synthesize-page or global-player)
  pitch = $state(1.0);
  volume = $state(100);
  speed = $state(1.0);
  activeEffect = $state<EffectId>("none");
  // Synced from config.hud.enabled (+layout); gates the 60fps amplitude loop
  hudEnabled = $state(true);

  private _audioEl: HTMLAudioElement | null = null;
  private _audioCtx: AudioContext | null = null;
  private _decodedBuffer: AudioBuffer | null = null;
  private _originalBytes: ArrayBuffer | null = null;
  private _cachedPitchUrl: {
    ratio: number;
    effectId: EffectId;
    preroll: boolean;
    url: string;
  } | null = null;
  private _unlistenFns: Array<() => void> = [];
  private _emit: ((name: string, payload: unknown) => Promise<void>) | null = null;
  private _emitTo: ((target: string, name: string, payload: unknown) => Promise<void>) | null =
    null;
  private _invoke: ((cmd: string, args?: Record<string, unknown>) => Promise<unknown>) | null =
    null;
  private _stopping = false;
  // Incremented by handleStop so an in-flight decode/render can't start
  // playback after the user stopped it
  private _playGeneration = 0;

  // Modular components
  private _analyser = new AudioAnalyser();
  private _fragmentQueue: FragmentQueue;

  constructor() {
    // Initialize fragment queue with handlers
    this._fragmentQueue = new FragmentQueue({
      onFragmentPlay: async (fragment: QueuedFragment) => {
        this.currentFragmentIndex = fragment.index;
        this.totalFragments = fragment.total;
        await this.handleAudioReady(fragment);
      },
      onQueueComplete: () => {
        this._analyser.stop();
        this.isPlaying = false;
        this.isPaused = false;
        this.currentFragmentIndex = null;
        this.totalFragments = null;
        void this._emit?.("hud:stop", null);
        this.reportPlaybackState(false, false);
      }
    });
  }

  // Playback happens in this webview; the backend needs the state for the
  // tray busy icon, tray click behavior, and HUD auto-hide.
  private reportPlaybackState(playing: boolean, paused: boolean): void {
    if (!this._invoke) return;
    this._invoke("set_playback_state", { playing, paused }).catch(() => {});
  }

  setAudioElement(el: HTMLAudioElement | null) {
    this._audioEl = el;
    if (el) {
      el.onplay = () => {
        this.isPlaying = true;
        this.isPaused = false;
      };
      el.onpause = () => {
        if (this._stopping) return;
        this.isPaused = !el.ended;
        this.isPlaying = !el.ended;
      };
      el.onended = () => {
        this._fragmentQueue.handleFragmentEnded();
      };
    }
  }

  async buildPlaybackUrl(pitchRatio: number, applyPreroll?: boolean): Promise<string> {
    const effectId = this.activeEffect;
    const preroll = applyPreroll ?? this.shouldApplyWindowsPreroll();
    if (
      this._cachedPitchUrl &&
      this._cachedPitchUrl.ratio === pitchRatio &&
      this._cachedPitchUrl.effectId === effectId &&
      this._cachedPitchUrl.preroll === preroll
    ) {
      return this._cachedPitchUrl.url;
    }
    if (this._cachedPitchUrl) {
      URL.revokeObjectURL(this._cachedPitchUrl.url);
      this._cachedPitchUrl = null;
    }
    const effect = getEffect(effectId);
    let blob: Blob;
    if (!preroll && pitchRatio === 1.0 && !effect && this._originalBytes) {
      const mimeType = detectAudioMimeType(this._originalBytes);
      blob = new Blob([this._originalBytes], { type: mimeType });
    } else if (this._decodedBuffer && this._audioCtx) {
      let buffer: AudioBuffer;
      if (pitchRatio === 1.0) {
        buffer = this._decodedBuffer;
      } else {
        const outputLen = Math.max(1, Math.round(this._decodedBuffer.length / pitchRatio));
        const offline = new OfflineAudioContext(
          this._decodedBuffer.numberOfChannels,
          outputLen,
          this._decodedBuffer.sampleRate
        );
        const src = offline.createBufferSource();
        src.buffer = this._decodedBuffer;
        src.playbackRate.value = pitchRatio;
        src.connect(offline.destination);
        src.start(0);
        buffer = await offline.startRendering();
      }
      if (effect) {
        buffer = await effect.process(buffer, this._audioCtx);
      }
      if (preroll) {
        buffer = prependLowLevelPreroll(buffer, WINDOWS_AUDIO_PREROLL_MS);
      }
      blob = audioBufferToWavBlob(buffer);
    } else {
      return "";
    }
    const url = URL.createObjectURL(blob);
    this._cachedPitchUrl = { ratio: pitchRatio, effectId, preroll, url };
    return url;
  }

  private shouldApplyWindowsPreroll(): boolean {
    return isTauri && navigator.userAgent.includes("Windows");
  }

  async handleAudioReady(fragment: QueuedFragment): Promise<void> {
    const base64 = fragment.audioBase64;
    const gen = this._playGeneration;

    // Invalidate caches for new audio fragment
    if (this._cachedPitchUrl) {
      URL.revokeObjectURL(this._cachedPitchUrl.url);
      this._cachedPitchUrl = null;
    }
    this._originalBytes = null;

    // Use pre-decoded buffer if available (from background pre-decode)
    if (fragment.decodedBuffer) {
      this._decodedBuffer = fragment.decodedBuffer;
      fragment.decodedBuffer = undefined; // Free memory — PCM buffer no longer needed
    } else {
      const binary = atob(base64);
      const len = binary.length;
      const bytes = new Uint8Array(len);
      for (let i = 0; i < len; i++) bytes[i] = binary.charCodeAt(i);

      // decodeAudioData detaches the buffer it is given — hand it a copy and
      // keep the original for the raw-bytes fast path in buildPlaybackUrl.
      this._originalBytes = bytes.buffer;

      // Wire AnalyserNode once per audio element (guard prevents double-wiring)
      if (this._audioEl && this._audioCtx && !this._analyser.getAnalyser()) {
        this._analyser.setup(this._audioEl, this._audioCtx, {
          emitTo: this._emitTo
        });
      }

      try {
        this._decodedBuffer = await this._audioCtx!.decodeAudioData(bytes.buffer.slice(0));
      } catch (e) {
        this.error = `Audio decode error: ${e}`;
        // Advance the queue — leaving isProcessing stuck would block every
        // subsequent fragment until restart.
        this._fragmentQueue.handleFragmentEnded();
        return;
      }
    }

    // The user stopped playback while we were decoding — don't start it.
    if (gen !== this._playGeneration) return;

    if (this._decodedBuffer) {
      const accurateDurationMs = Math.round(this._decodedBuffer.duration * 1000);
      hudStore.setAccurateDurationMs(accurateDurationMs);
      this._emit?.("hud:audio-duration", accurateDurationMs);
    }
    // The preroll masks the Windows audio-device wake-up clip; only the first
    // fragment of a playback needs it. Continuation fragments arrive while the
    // device is already active — skipping the preroll removes a 200ms gap and
    // a decode/re-encode per fragment.
    const isContinuation = fragment.total > 1 && fragment.index > 0;
    const url = await this.buildPlaybackUrl(
      this.pitch,
      this.shouldApplyWindowsPreroll() && !isContinuation
    );
    if (gen !== this._playGeneration) return;
    if (this._audioEl && url) {
      this._audioEl.src = url;
      if (this.hudEnabled) {
        this._analyser.start();
      }
      this.playAudio();
    }
    this.hasCachedAudio = true;

    // Pre-decode the next fragment in the background while this one plays
    this.predecodeNextFragment();
  }

  private async predecodeNextFragment(): Promise<void> {
    const fragments = this._fragmentQueue.getQueue();
    if (fragments.length < 2) return;
    const nextFragment = fragments[1]; // [0] is current, [1] is next
    if (nextFragment.decodedBuffer || !this._audioCtx) return;

    const binary = atob(nextFragment.audioBase64);
    const len = binary.length;
    const bytes = new Uint8Array(len);
    for (let i = 0; i < len; i++) bytes[i] = binary.charCodeAt(i);
    try {
      nextFragment.decodedBuffer = await this._audioCtx.decodeAudioData(bytes.buffer);
    } catch (e) {
      console.debug("[PlaybackStore] Pre-decode failed:", e);
    }
  }

  /**
   * Handle incoming audio fragment from backend.
   * Queues the fragment and starts processing if not already.
   */
  async handleFragmentReady(payload: {
    audio_base64: string;
    fragment_index: number;
    fragment_total: number;
    is_final: boolean;
    text: string;
  }): Promise<void> {
    // Add to queue
    this._fragmentQueue.enqueue({
      audioBase64: payload.audio_base64,
      index: payload.fragment_index,
      total: payload.fragment_total,
      text: payload.text,
      isFinal: payload.is_final
    });
    // Start processing if not already
    if (!this._fragmentQueue.isProcessing()) {
      await this._fragmentQueue.startProcessing();
    }
  }

  playAudio() {
    if (!this._audioEl) {
      console.error("[PlaybackStore] playAudio: no audio element");
      return;
    }
    this._audioEl.volume = this.volume / 100;
    this._audioEl.playbackRate = this.speed;
    this._audioEl.play().catch((err) => {
      console.error("[PlaybackStore] play() failed:", err);
    });
    this.reportPlaybackState(true, false);
  }

  async handleReplay(): Promise<void> {
    if (!this._audioEl) return;
    const url = await this.buildPlaybackUrl(this.pitch);
    if (url) {
      this._audioEl.src = url;
      this._audioEl.currentTime = 0;
      this.playAudio();
    }
  }

  handleStop() {
    this._analyser.stop();
    this._stopping = true;
    // Invalidate any in-flight decode/render so it can't start playback
    // after the user stopped it.
    this._playGeneration++;

    // Clear the fragment queue
    this._fragmentQueue.clear();
    this.currentFragmentIndex = null;
    this.totalFragments = null;

    if (this._audioEl) {
      this._audioEl.pause();
      this._audioEl.currentTime = 0;
    }
    this.isPlaying = false;
    this.isPaused = false;
    void this._emit?.("hud:stop", null);
    this.reportPlaybackState(false, false);
    setTimeout(() => {
      this._stopping = false;
    }, 0);
  }

  handleTogglePause() {
    if (!this._audioEl) return;
    if (this._audioEl.paused) {
      this._audioEl.play().catch(() => {});
      this.isPaused = false;
    } else {
      this._audioEl.pause();
      this.isPaused = true;
    }
    this.reportPlaybackState(true, this.isPaused);
  }

  // Keep volume/speed in sync with config (called by synthesize-page via $effect)
  syncPlaybackConfig(volume: number, speed: number, pitch: number, effect: EffectId = "none") {
    // Config may omit playback_speed/pitch (legacy fields migrated to profiles),
    // so coerce to finite defaults to avoid NaN assignments that throw inside effects.
    const vol = Number.isFinite(volume) ? volume : 100;
    const spd = Number.isFinite(speed) ? speed : 1.0;
    const pit = Number.isFinite(pitch) ? pitch : 1.0;
    this.volume = vol;
    this.speed = spd;
    this.pitch = pit;
    if (this.activeEffect !== effect) {
      this.activeEffect = effect;
      if (this._cachedPitchUrl) {
        URL.revokeObjectURL(this._cachedPitchUrl.url);
        this._cachedPitchUrl = null;
      }
    }
    if (this._audioEl) {
      this._audioEl.volume = vol / 100;
      this._audioEl.playbackRate = spd;
    }
    // Sync pitch and speed to HUD store for progress bar timing
    hudStore.setPitch(pit);
    hudStore.setSpeed(spd);
  }

  async setupListeners(): Promise<void> {
    if (!isTauri) return;
    try {
      const { listen, emit, emitTo } = await import("@tauri-apps/api/event");
      const { invoke } = await import("@tauri-apps/api/core");
      this._emit = emit;
      this._emitTo = emitTo;
      this._invoke = invoke;

      // Pre-warm AudioContext at startup to avoid cold-start delay on first playback
      this._audioCtx = new AudioContext();
      if (this._audioCtx.state === "suspended") {
        await this._audioCtx.resume();
      }

      // Legacy single audio-ready event (for non-paginated playback)
      const unAudioReady = await listen<string>("audio-ready", async (e) => {
        this._fragmentQueue.enqueue({
          audioBase64: e.payload,
          index: 1,
          total: 1,
          text: "",
          isFinal: true
        });
        if (!this._fragmentQueue.isProcessing()) {
          await this._fragmentQueue.startProcessing();
        }
      });

      // New streaming fragment-ready event
      const unFragmentReady = await listen<{
        audio_base64: string;
        fragment_index: number;
        fragment_total: number;
        is_final: boolean;
        text: string;
      }>("audio-fragment-ready", async (e) => {
        await this.handleFragmentReady(e.payload);
      });

      const unPlaybackStop = await listen("playback-stop", () => {
        this.handleStop();
      });

      const unTogglePause = await listen("playback-toggle-pause", () => {
        this.handleTogglePause();
      });

      const unSynthesis = await listen<boolean>("synthesis-state-change", (e) => {
        this.isSynthesizing = e.payload;
      });

      const unAbort = await listen("synthesis-aborted", () => {
        // Handle abort event from backend - clear queue and stop
        this.handleStop();
      });

      // Mid-stream synthesis failure: the final fragment will never arrive,
      // so stop instead of waiting forever with the HUD visible.
      const unFailed = await listen("pagination:failed", () => {
        this.handleStop();
      });

      this._unlistenFns = [
        unAudioReady,
        unFragmentReady,
        unPlaybackStop,
        unTogglePause,
        unSynthesis,
        unAbort,
        unFailed
      ];
    } catch (e) {
      console.error("Failed to setup playback listeners:", e);
    }
  }

  teardownListeners() {
    this._analyser.stop();
    this._analyser.destroy();
    for (const fn of this._unlistenFns) fn();
    this._unlistenFns = [];
    if (this._cachedPitchUrl) {
      URL.revokeObjectURL(this._cachedPitchUrl.url);
      this._cachedPitchUrl = null;
    }
    if (this._audioCtx) {
      this._audioCtx.close();
      this._audioCtx = null;
    }
    this._emit = null;
    this._emitTo = null;
    this._invoke = null;
    this._decodedBuffer = null;
    this._originalBytes = null;
    this.hasCachedAudio = false;
    this._fragmentQueue.clear();
    this.currentFragmentIndex = null;
    this.totalFragments = null;
  }
}

export const playbackStore = new PlaybackStore();
