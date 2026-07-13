// ElevenLabs voice listing and output format commands.

use crate::config::AppConfig;
use crate::config::TtsEngine;
use std::sync::Mutex;
use tauri::State;

#[tauri::command]
pub fn list_tts_engines() -> Vec<crate::tts::catalog::EngineCatalogEntry> {
    crate::tts::catalog::list_engines()
}

#[tauri::command]
pub fn list_tts_voices(
    engine: TtsEngine,
    config: State<'_, Mutex<AppConfig>>,
) -> Result<Vec<crate::tts::catalog::VoiceCatalogEntry>, String> {
    if engine == TtsEngine::ElevenLabs {
        let cfg = config.lock().unwrap();
        let backend = crate::tts::elevenlabs::ElevenLabsTtsBackend::new(cfg.tts.elevenlabs.clone());
        return backend
            .list_voices()
            .map(|voices| {
                voices
                    .into_iter()
                    .map(|voice| crate::tts::catalog::VoiceCatalogEntry {
                        id: voice.voice_id.clone(),
                        label: voice.name.unwrap_or(voice.voice_id),
                        language: voice.labels.as_ref().and_then(|labels| {
                            labels
                                .get("language")
                                .and_then(|language| language.as_str().map(str::to_string))
                        }),
                        description: voice.description,
                        gender: voice.labels.as_ref().and_then(|labels| {
                            labels
                                .get("gender")
                                .and_then(|gender| gender.as_str().map(str::to_string))
                        }),
                        preview_url: voice.preview_url,
                    })
                    .collect()
            })
            .map_err(|e| format!("Failed to fetch voices: {}", e));
    }

    if engine == TtsEngine::Cartesia {
        let cfg = config.lock().unwrap();
        let backend =
            crate::tts::cartesia::CartesiaTtsBackend::new(cfg.tts.cartesia.clone());
        return match backend.list_voices() {
            Ok(voices) => Ok(voices
                .into_iter()
                .map(|v| crate::tts::catalog::VoiceCatalogEntry {
                    id: v.id,
                    label: v.name.unwrap_or_else(|| "Unnamed voice".into()),
                    language: None,
                    description: v.description,
                    gender: None,
                    preview_url: None,
                })
                .collect()),
            Err(e) => {
                log::warn!("Cartesia voice refresh failed, using static list: {}", e);
                Ok(crate::tts::catalog::list_static_voices(&engine))
            }
        };
    }

    if engine == TtsEngine::Local {
        let local_voices = get_local_piper_voices().unwrap_or_default();
        if !local_voices.is_empty() {
            return Ok(local_voices
                .into_iter()
                .map(|v| crate::tts::catalog::VoiceCatalogEntry {
                    id: v.value,
                    label: v.label,
                    language: Some("en".to_string()),
                    description: Some("Local Piper voice model".to_string()),
                    gender: None,
                    preview_url: None,
                })
                .collect());
        }
    }

    Ok(crate::tts::catalog::list_static_voices(&engine))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PiperVoiceOption {
    pub value: String,
    pub label: String,
}

fn clean_name(name: &str) -> String {
    name.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

fn parse_piper_voice_label(filename: &str) -> String {
    let parts: Vec<&str> = filename.split('-').collect();
    if parts.len() >= 3 {
        let name = clean_name(parts[1]);
        let quality = parts[2];
        format!("{} ({})", name, quality)
    } else if parts.len() == 2 {
        clean_name(parts[1])
    } else {
        filename.to_string()
    }
}

#[tauri::command]
pub fn get_local_piper_voices() -> Result<Vec<PiperVoiceOption>, String> {
    let mut paths_to_scan = Vec::new();

    // 1. User's home piper-voices dir
    let data_dir = crate::tts::cli::CliTtsBackend::data_dir();
    paths_to_scan.push(std::path::PathBuf::from(&data_dir));

    // 2. Engine voices dir
    if let Some(local_dir) = dirs::data_local_dir() {
        paths_to_scan.push(local_dir.join("CopySpeak").join("engines").join("piper").join("voices"));
    }

    let mut voices = Vec::new();
    let mut seen_stems = std::collections::HashSet::new();

    for path in paths_to_scan {
        if !path.exists() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let p = entry.path();
                if p.is_file() && p.extension().is_some_and(|ext| ext == "onnx") {
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        let stem_string = stem.to_string();
                        if !seen_stems.contains(&stem_string) {
                            seen_stems.insert(stem_string.clone());
                            let label = parse_piper_voice_label(stem);
                            voices.push(PiperVoiceOption {
                                value: stem_string,
                                label,
                            });
                        }
                    }
                }
            }
        }
    }

    // Sort by label
    voices.sort_by(|a, b| a.label.cmp(&b.label));

    Ok(voices)
}

#[tauri::command]
pub fn unload_piper_model() -> Result<bool, String> {
    log::info!("[IPC] unload_piper_model called");
    Ok(crate::tts::cli::unload_piper_model_internal())
}

#[tauri::command]
pub fn get_piper_server_status() -> crate::tts::cli::PiperServerStatus {
    crate::tts::cli::get_piper_server_status()
}

/// List available ElevenLabs voices.
/// Requires valid API key in config.
#[tauri::command]
pub fn list_elevenlabs_voices(
    config: State<'_, Mutex<AppConfig>>,
) -> Result<Vec<crate::tts::elevenlabs::ElevenLabsVoice>, String> {
    if crate::logging::is_debug_mode() {
        log::debug!("[IPC] list_elevenlabs_voices called");
    }

    let cfg = config.lock().unwrap();
    let backend = crate::tts::elevenlabs::ElevenLabsTtsBackend::new(cfg.tts.elevenlabs.clone());

    match backend.list_voices() {
        Ok(voices) => Ok(voices),
        Err(e) => Err(format!("Failed to fetch voices: {}", e)),
    }
}

/// Get voice details by ID from ElevenLabs API.
/// Useful for validating manually entered voice IDs.
#[tauri::command]
pub fn get_elevenlabs_voice_by_id(
    voice_id: String,
    config: State<'_, Mutex<AppConfig>>,
) -> Result<crate::tts::elevenlabs::ElevenLabsVoice, String> {
    if crate::logging::is_debug_mode() {
        log::debug!("[IPC] get_elevenlabs_voice_by_id called for: {}", voice_id);
    }

    let cfg = config.lock().unwrap();
    let backend = crate::tts::elevenlabs::ElevenLabsTtsBackend::new(cfg.tts.elevenlabs.clone());

    match backend.get_voice_by_id(&voice_id) {
        Ok(voice) => Ok(voice),
        Err(e) => Err(format!("Failed to fetch voice: {}", e)),
    }
}

/// Get available ElevenLabs output formats for the frontend.
#[tauri::command]
pub fn get_elevenlabs_output_formats() -> Vec<(String, String)> {
    if crate::logging::is_debug_mode() {
        log::debug!("[IPC] get_elevenlabs_output_formats called");
    }
    use crate::tts::elevenlabs::ElevenLabsOutputFormat;

    ElevenLabsOutputFormat::all()
        .iter()
        .map(|fmt| (format!("{:?}", fmt), fmt.label().to_string()))
        .collect()
}
