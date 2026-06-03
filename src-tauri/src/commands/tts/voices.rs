// ElevenLabs voice listing and output format commands.

use crate::config::AppConfig;
use std::sync::Mutex;
use tauri::State;

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
    let data_dir = crate::tts::cli::CliTtsBackend::data_dir();
    let path = std::path::Path::new(&data_dir);
    if !path.exists() {
        return Ok(Vec::new());
    }

    let mut voices = Vec::new();
    let entries =
        std::fs::read_dir(path).map_err(|e| format!("Failed to read voices dir: {}", e))?;
    for entry in entries.filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.is_file() && p.extension().is_some_and(|ext| ext == "onnx") {
            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                let label = parse_piper_voice_label(stem);
                voices.push(PiperVoiceOption {
                    value: stem.to_string(),
                    label,
                });
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
