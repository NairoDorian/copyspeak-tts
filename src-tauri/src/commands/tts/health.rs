// TTS engine health check commands.

use crate::config::{AppConfig, TtsEngine};
use crate::tts::cli::CliTtsBackend;
use crate::tts::TtsError;
use std::sync::Mutex;
use tauri::State;

use super::helpers::create_backend;

/// Result of a TTS engine health check.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TtsHealthResult {
    pub success: bool,
    pub message: String,
    pub error_type: Option<String>,
}

/// Result of checking if a command exists in PATH.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CommandExistsResult {
    pub available: bool,
}

/// Check if a command exists in the system PATH.
/// This is used to check if local TTS engines are installed without fully testing them.
#[tauri::command]
pub fn check_command_exists(command: String) -> Result<CommandExistsResult, String> {
    if crate::logging::is_debug_mode() {
        log::debug!("[IPC] check_command_exists called for: {}", command);
    }

    // Try to find the command in PATH using `which` on Unix or `where` on Windows
    let result = if cfg!(target_os = "windows") {
        std::process::Command::new("where").arg(&command).output()
    } else {
        std::process::Command::new("which").arg(&command).output()
    };

    match result {
        Ok(output) => {
            let available = output.status.success();
            if crate::logging::is_debug_mode() {
                log::debug!("[IPC] check_command_exists({}): {}", command, available);
            }
            Ok(CommandExistsResult { available })
        }
        Err(e) => {
            log::warn!("[IPC] check_command_exists failed for {}: {}", command, e);
            Ok(CommandExistsResult { available: false })
        }
    }
}

/// Async + spawn_blocking: local-engine health checks can spawn a full CLI
/// test synthesis (seconds) — that must never run on the main thread.
#[tauri::command]
pub async fn test_tts_engine(config: State<'_, Mutex<AppConfig>>) -> Result<TtsHealthResult, String> {
    if crate::logging::is_debug_mode() {
        log::debug!("[IPC] test_tts_engine called");
    }

    let (active_backend, tts_config) = {
        let cfg = crate::lock_or_recover!(config);
        (cfg.tts.active_backend.clone(), cfg.tts.clone())
    };

    tokio::task::spawn_blocking(move || {
    let backend: Box<dyn crate::tts::TtsBackend> = create_backend(&active_backend, &tts_config);

    let backend_name = match active_backend {
        crate::config::TtsEngine::Local => tts_config.command.clone(),
        crate::config::TtsEngine::OpenAI => format!("OpenAI ({})", tts_config.openai.model),
        crate::config::TtsEngine::ElevenLabs => {
            format!("ElevenLabs ({})", tts_config.elevenlabs.model_id)
        }
        crate::config::TtsEngine::Cartesia => {
            format!("Cartesia ({})", tts_config.cartesia.model_id)
        }
        crate::config::TtsEngine::Http => format!("HTTP ({})", tts_config.http.url_template),
        crate::config::TtsEngine::Google => format!("Google ({})", tts_config.google.model),
        crate::config::TtsEngine::Microsoft => {
            format!("Microsoft ({})", tts_config.microsoft.model)
        }
        crate::config::TtsEngine::Edge => format!("Edge ({})", tts_config.edge.voice),
    };

    match backend.health_check() {
        Ok(()) => {
            log::info!("TTS engine health check passed: {}", backend_name);
            Ok(TtsHealthResult {
                success: true,
                message: format!("{} is available and configured correctly", backend_name),
                error_type: None,
            })
        }
        Err(e) => {
            log::warn!("TTS engine health check failed: {}", e);
            let (message, error_type) = match &e {
                TtsError::Unavailable(msg) => {
                    if msg.contains("API key") {
                        (
                            format!("{} - API key is missing or invalid", backend_name),
                            "api_key_missing",
                        )
                    } else if msg.contains("not found") || msg.contains("The system cannot find") {
                        (format!("Command '{}' not found. Please ensure the TTS engine is installed and in PATH.", backend_name), "not_found")
                    } else if msg.contains("Access is denied") || msg.contains("permission") {
                        (
                            format!(
                                "Permission denied accessing '{}'. Check permissions.",
                                backend_name
                            ),
                            "permission_denied",
                        )
                    } else {
                        (
                            format!("{} unavailable: {}", backend_name, msg),
                            "unavailable",
                        )
                    }
                }
                TtsError::Http(msg) => {
                    if msg.contains("401") || msg.contains("403") {
                        (
                            format!(
                                "{} - Authentication failed. Check your API key.",
                                backend_name
                            ),
                            "auth_failed",
                        )
                    } else if msg.contains("429") {
                        (
                            format!(
                                "{} - Rate limit exceeded. Please try again later.",
                                backend_name
                            ),
                            "rate_limit",
                        )
                    } else {
                        (
                            format!("{} - Network error: {}", backend_name, msg),
                            "http_error",
                        )
                    }
                }
                TtsError::Io(e) => {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        (format!("Command '{}' not found. Please ensure the TTS engine is installed.", backend_name), "not_found")
                    } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                        (
                            format!(
                                "Permission denied running '{}'. Check file permissions.",
                                backend_name
                            ),
                            "permission_denied",
                        )
                    } else {
                        (format!("IO error: {}", e), "io_error")
                    }
                }
                _ => (format!("TTS engine check failed: {}", e), "unknown"),
            };
            Ok(TtsHealthResult {
                success: false,
                message,
                error_type: Some(error_type.to_string()),
            })
        }
    }
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}

fn parse_engine(engine: &str) -> Result<TtsEngine, String> {
    match engine {
        "local" => Ok(TtsEngine::Local),
        "http" => Ok(TtsEngine::Http),
        "openai" => Ok(TtsEngine::OpenAI),
        "elevenlabs" => Ok(TtsEngine::ElevenLabs),
        "cartesia" => Ok(TtsEngine::Cartesia),
        "google" => Ok(TtsEngine::Google),
        "microsoft" => Ok(TtsEngine::Microsoft),
        "edge" => Ok(TtsEngine::Edge),
        _ => Err(format!("unknown engine: {}", engine)),
    }
}

/// Test a specific engine config (cloud or local) without making it active.
/// The Engines-page Test button calls this with `engine: entry.id`.
#[tauri::command]
pub async fn test_tts_engine_config(
    config: State<'_, Mutex<AppConfig>>,
    engine: String,
    preset: Option<String>,
) -> Result<TtsHealthResult, String> {
    let engine_enum = parse_engine(&engine)?;

    let result = tokio::task::spawn_blocking(move || {
        let mut tts_config = {
            let cfg = crate::lock_or_recover!(config);
            cfg.tts.clone()
        };
        if let Some(p) = preset {
            tts_config.preset = p;
        }
        let backend: Box<dyn crate::tts::TtsBackend> = create_backend(&engine_enum, &tts_config);
        let backend_name = match &engine_enum {
            TtsEngine::Local => tts_config.command.clone(),
            TtsEngine::OpenAI => format!("OpenAI ({})", tts_config.openai.model),
            TtsEngine::ElevenLabs => format!("ElevenLabs ({})", tts_config.elevenlabs.model_id),
            TtsEngine::Cartesia => format!("Cartesia ({})", tts_config.cartesia.model_id),
            TtsEngine::Http => format!("HTTP ({})", tts_config.http.url_template),
            TtsEngine::Google => format!("Google ({})", tts_config.google.model),
            TtsEngine::Microsoft => format!("Microsoft ({})", tts_config.microsoft.model),
            TtsEngine::Edge => format!("Edge ({})", tts_config.edge.voice),
        };
        match backend.health_check() {
            Ok(()) => Ok(TtsHealthResult {
                success: true,
                message: format!("{} is available and configured correctly", backend_name),
                error_type: None,
            }),
            Err(e) => {
                log::warn!("TTS engine config check failed ({}): {}", backend_name, e);
                Ok(TtsHealthResult {
                    success: false,
                    message: format!("{} test failed: {}", backend_name, e),
                    error_type: Some("unknown".into()),
                })
            }
        }
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?;

    Ok(result)
}

/// Test a local (uv-installed) engine by running a real synthesis through its
/// stable CLI/server wrapper — the same path `cli.rs` uses at runtime. Returns a
/// `TtsHealthResult` so the Engines-page Test button reuses the cloud-test UI.
/// Unlike `health_check` (which only probes binary/path presence for local
/// engines), this proves the engine actually produces audio.
///
/// `engine` is the preset id from the Engines page: piper | kitten | chatterbox |
/// kokoro | pocket.
#[tauri::command]
pub async fn test_local_engine(engine: String) -> Result<TtsHealthResult, String> {
    let spec = match local_engine_spec(&engine) {
        Some(s) => s,
        None => {
            return Ok(TtsHealthResult {
                success: false,
                message: format!("Unknown local engine: {engine}"),
                error_type: Some("unknown".into()),
            });
        }
    };
    let engine_label = engine.clone();
    let backend_name = format!("Local ({})", engine_label);
    let backend =
        CliTtsBackend::new(spec.command.clone(), spec.args_template.clone(), false, spec.preset.clone());

    let synth_result = tokio::task::spawn_blocking(move || {
        log::info!(
            "[IPC] test_local_engine '{}' — synthesizing test clip (voice: {})",
            engine,
            spec.voice
        );
        backend.synthesize("Hello.", &spec.voice, 1.0)
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?;

    match synth_result {
        Ok(bytes) => {
            // Validate the bytes look like a real audio file: non-empty and
            // either a WAV (RIFF....) or any non-trivial blob for mp3 engines.
            let looks_ok = bytes.len() > 44
                && (bytes.starts_with(b"RIFF")
                    || bytes.starts_with(&[0x49, 0x44, 0x33]) // ID3 (mp3)
                    || bytes.starts_with(&[0xFF, 0xFB]) // mp3 frame
                    || bytes.starts_with(&[0xFF, 0xF3])
                    || bytes.starts_with(&[0xFF, 0xF2]));
            if looks_ok {
                log::info!(
                    "[IPC] test_local_engine '{}' produced {} bytes — OK",
                    engine,
                    bytes.len()
                );
                Ok(TtsHealthResult {
                    success: true,
                    message: format!(
                        "{} synthesized a test clip successfully ({} bytes).",
                        backend_name,
                        bytes.len()
                    ),
                    error_type: None,
                })
            } else {
                log::warn!(
                    "[IPC] test_local_engine '{}' produced {} bytes — too small or unrecognized",
                    engine,
                    bytes.len()
                );
                Ok(TtsHealthResult {
                    success: false,
                    message: format!(
                        "{} produced no audio ({} bytes). The engine ran but did not generate valid output.",
                        backend_name,
                        bytes.len()
                    ),
                    error_type: Some("unknown".into()),
                })
            }
        }
        Err(e) => {
            log::warn!("[IPC] test_local_engine '{}' failed: {}", engine, e);
            synthesize_health_failure(&backend_name, &e)
        }
    }
}

/// Stable per-engine CLI spec, mirroring the profile snippet each installer
/// emits. Kept here (not in catalog.rs) because this is a *test* fixture, not a
/// runtime catalog entry — it only needs to drive one short synthesis.
struct LocalEngineSpec {
    command: String,
    args_template: Vec<String>,
    voice: String,
    preset: String,
}

fn local_engine_spec(engine: &str) -> Option<LocalEngineSpec> {
    // Voice is the engine's English default from its installer menu. The
    // {engine_dir} placeholder is resolved by CliTtsBackend::build_args at
    // run time.
    let uv_run = |project: &str, wrapper: &str| {
        vec![
            "run".into(),
            "--project".into(),
            format!("{{engine_dir}}/{project}"),
            "python".into(),
            format!("{{engine_dir}}/{project}/scripts/{wrapper}"),
            "--text-file".into(),
            "{input}".into(),
            "--voice".into(),
            "{voice}".into(),
            "--output".into(),
            "{output}".into(),
        ]
    };
    let spec = match engine {
        "piper" => LocalEngineSpec {
            command: "uv".into(),
            args_template: uv_run("piper", "copyspeak-piper.py"),
            voice: "en_US-amy-medium".into(),
            preset: "piper".into(),
        },
        "kitten" => LocalEngineSpec {
            command: "uv".into(),
            args_template: uv_run("kitten", "copyspeak-kitten.py"),
            voice: "Rosie".into(),
            preset: "kitten-tts".into(),
        },
        "chatterbox" => LocalEngineSpec {
            command: "uv".into(),
            args_template: uv_run("chatterbox", "copyspeak-chatterbox.py"),
            voice: "default".into(),
            preset: "chatterbox".into(),
        },
        "kokoro" => LocalEngineSpec {
            command: "kokoro-tts".into(),
            args_template: vec![
                "{input}".into(),
                "{output}".into(),
                "--voice".into(),
                "{voice}".into(),
                "--model".into(),
                "{engine_dir}/kokoro/models/kokoro-v1.0.onnx".into(),
                "--voices".into(),
                "{engine_dir}/kokoro/models/voices-v1.0.bin".into(),
            ],
            voice: "af_heart".into(),
            preset: "kokoro-tts".into(),
        },
        // Pocket TTS: runs via the persistent local HTTP server (pocket_server.py)
        // using the pocket-tts binary; no model files to stage.
        "pocket" => LocalEngineSpec {
            command: "pocket-tts".into(),
            args_template: vec![],
            voice: "alba".into(),
            preset: "pocket-tts".into(),
        },
        _ => return None,
    };
    Some(spec)
}

// Map a synthesis error to a TtsHealthResult. Localized for local engines:
// surfaces "run the installer" guidance instead of API-key chatter.
fn synthesize_health_failure(backend_name: &str, e: &TtsError) -> Result<TtsHealthResult, String> {
    let (message, error_type) = match e {
        TtsError::Unavailable(msg) => {
            if msg.contains("not found") || msg.contains("not recognized") {
                (
                    format!(
                        "{} not found. Run its installer from the Engines page first.",
                        backend_name
                    ),
                    "not_found",
                )
            } else {
                (format!("{} unavailable: {}", backend_name, msg), "unavailable")
            }
        }
        TtsError::Io(io_err) => {
            if io_err.kind() == std::io::ErrorKind::NotFound {
                (
                    format!(
                        "{} not found. Run its installer from the Engines page first.",
                        backend_name
                    ),
                    "not_found",
                )
            } else {
                (format!("IO error: {}", io_err), "io_error")
            }
        }
        _ => (format!("{} test failed: {}", backend_name, e), "unknown"),
    };
    Ok(TtsHealthResult {
        success: false,
        message,
        error_type: Some(error_type.to_string()),
    })
}
