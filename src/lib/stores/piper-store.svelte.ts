/**
 * Piper Model Status Store
 *
 * Tracks the lifecycle of the persistent Piper HTTP server:
 * loading -> warming_up -> ready / error / stopped
 *
 * Receives real-time events from the Rust backend via piper-status-changed.
 * Falls back to polling get_piper_server_status for initial state sync.
 */

import { isTauri } from "$lib/services/tauri.js";
import type { PiperStatusPhase, PiperStatusPayload, PiperServerStatus } from "$lib/types";

let invoke: typeof import("@tauri-apps/api/core").invoke | null = null;
let listenFn: typeof import("@tauri-apps/api/event").listen | null = null;

class PiperStore {
  phase = $state<PiperStatusPhase>("stopped");
  model = $state<string | null>(null);
  cuda = $state(false);
  error = $state<string | null>(null);
  port = $state<number | null>(null);
  ready = $state(false);

  get isLoading(): boolean {
    return this.phase === "loading";
  }

  get isWarmingUp(): boolean {
    return this.phase === "warming_up";
  }

  get isReady(): boolean {
    return this.phase === "ready";
  }

  get isError(): boolean {
    return this.phase === "error";
  }

  get isStopped(): boolean {
    return this.phase === "stopped";
  }

  get isLoadingOrWarming(): boolean {
    return this.phase === "loading" || this.phase === "warming_up";
  }

  get statusLabel(): string {
    switch (this.phase) {
      case "loading":
        return "Loading model...";
      case "warming_up":
        return this.cuda ? "Loading in VRAM..." : "Warming up...";
      case "ready":
        return this.model ?? "Ready";
      case "error":
        return this.error ?? "Error";
      case "stopped":
      default:
        return "";
    }
  }

  applyPayload(payload: PiperStatusPayload) {
    this.phase = payload.phase;
    this.model = payload.model;
    this.cuda = payload.cuda;
    this.error = payload.error;
  }

  async loadFromBackend(): Promise<void> {
    if (!isTauri || !invoke) return;
    try {
      const status = await invoke<PiperServerStatus>("get_piper_server_status");
      if (status.ready) {
        this.phase = "ready";
        this.model = status.model;
        this.cuda = status.cuda;
        this.ready = true;
        this.port = status.port;
        this.error = null;
      } else if (status.running) {
        this.phase = "loading";
        this.cuda = status.cuda;
        this.port = status.port;
      } else {
        this.phase = "stopped";
        this.model = null;
        this.ready = false;
        this.port = null;
        this.error = null;
      }
    } catch {
      // Silently fail - store remains at default
    }
  }

  async unloadModel(): Promise<void> {
    if (!isTauri || !invoke) return;
    try {
      await invoke<boolean>("unload_piper_model");
    } catch (e) {
      console.error("Failed to unload Piper model:", e);
    }
  }
}

export const piperStore = new PiperStore();

if (isTauri) {
  Promise.all([
    import("@tauri-apps/api/core").then((core) => {
      invoke = core.invoke;
    }),
    import("@tauri-apps/api/event").then((event) => {
      listenFn = event.listen;
    })
  ])
    .then(() => {
      piperStore.loadFromBackend();
      if (listenFn) {
        listenFn<PiperStatusPayload>("piper-status-changed", (event) => {
          piperStore.applyPayload(event.payload);
        });
      }
    })
    .catch(() => {});
}
