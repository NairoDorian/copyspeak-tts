// TTS engine/voice catalog IPC commands.
//
// Thin wrappers over `crate::tts::catalog` so the frontend can populate the
// Voice Profiles UI without re-encoding the static catalog in TypeScript.

use crate::config::TtsEngine;
use crate::tts::catalog::{EngineCatalogEntry, VoiceCatalogEntry};
use tauri::command;

/// Return the full static engine catalog (engines, options, static voices).
#[command]
pub fn list_tts_engines() -> Vec<EngineCatalogEntry> {
    crate::tts::catalog::list_engines()
}

/// Return the static voice list for a given engine (refresh hits the live API).
#[command]
pub fn list_tts_voices(engine: TtsEngine) -> Vec<VoiceCatalogEntry> {
    crate::tts::catalog::list_static_voices(&engine)
}
