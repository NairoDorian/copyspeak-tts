// @see docs_internal/event-system.md

import type { UnlistenFn } from "@tauri-apps/api/event";
import { historyStore } from "$lib/stores/history-store.svelte";

let unlistenHistory: UnlistenFn | null = null;
let isListening = false;

export async function startHistoryEventListeners(): Promise<void> {
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) {
    return;
  }

  if (isListening) {
    return;
  }

  try {
    const { listen } = await import("@tauri-apps/api/event");

    // Every history write emits "history-updated" — refreshing on
    // speak-request as well was a wasted full-history IPC dump inside the
    // copy-to-first-audio window.
    unlistenHistory = await listen<void>("history-updated", async () => {
      await historyStore.refresh();
    });

    isListening = true;
  } catch (error) {
    console.error("[history-events] Failed to start event listeners:", error);
  }
}

export async function stopHistoryEventListeners(): Promise<void> {
  if (unlistenHistory) {
    unlistenHistory();
    unlistenHistory = null;
  }
  isListening = false;
}

export function isListeningForHistoryEvents(): boolean {
  return isListening;
}

export async function refreshHistory(): Promise<void> {
  await historyStore.refresh();
}
