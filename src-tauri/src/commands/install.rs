// Engine installer launcher.
//
// Spawns the PowerShell installer for a local TTS engine (or the uv bootstrap)
// in a detached window. The installer script owns the window and any prompts;
// this command returns immediately. No output is streamed back in v1.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

/// Map a CopySpeak engine id to its installer script filename under `scripts/`.
fn installer_script_for(engine: &str) -> Result<&'static str, String> {
    match engine {
        "uv" => Ok("install-uv.ps1"),
        "chatterbox" => Ok("install-chatterbox.ps1"),
        "kitten" | "kittentts" | "kitten-tts" => Ok("install-kittentts.ps1"),
        "piper" => Ok("install-piper.ps1"),
        "kokoro" | "kokoro-tts" => Ok("install-kokoro.ps1"),
        "pocket" | "pocket-tts" => Ok("install-pocket.ps1"),
        "edge" | "edge-tts" => Ok("install-edge-tts.ps1"),
        other => Err(format!("unknown engine installer: {other}")),
    }
}

/// Resolve `scripts/<name>` from dev (CARGO_MANIFEST_DIR) or exe-relative
/// candidates. Returns the first existing path.
fn resolve_script(filename: &str) -> Result<PathBuf, String> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    // Dev: <repo>/scripts — CARGO_MANIFEST_DIR points at src-tauri.
    // ponytail: baked at compile time; fine for dev/alpha, not for packaged
    // installs. Bundle scripts as Tauri resources when distribution matters.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    candidates.push(manifest.join("..").join("scripts").join(filename));

    // Packaged: alongside the exe, or one dir up (resource dir layouts).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidates.push(exe_dir.join("scripts").join(filename));
            candidates.push(exe_dir.join("..").join("scripts").join(filename));
        }
    }

    candidates
        .into_iter()
        .find(|p| p.exists())
        .ok_or_else(|| format!("installer script not found: {filename}"))
}

/// Launch an engine installer by id in a new PowerShell window.
///
/// Returns once the window is spawned; the script runs to completion and
/// waits for a keypress before closing.
#[tauri::command]
pub fn install_engine(engine: String) -> Result<(), String> {
    let filename = installer_script_for(&engine)?;
    let script_path = resolve_script(filename)?;
    let script_str = script_path.display().to_string();

    log::info!("Launching installer for '{engine}': {script_str}");

    #[cfg(target_os = "windows")]
    {
        // Wrapper runs the script in try/catch so a terminating error inside
        // the installer (e.g. `throw`) is captured instead of escaping past the
        // pause — without this, failed installers auto-close the window before
        // the user can read what went wrong. The "Press any key" prompt always
        // runs whether the installer threw or exited non-zero.
        let wrapper = format!(
            r#"$ErrorActionPreference = 'Continue'; $code = 0; try {{ & '{script}' }} catch {{ $code = -1; Write-Host ''; Write-Host "ERROR: $_" -ForegroundColor Red }}; if ($LASTEXITCODE -ne 0 -and $code -eq 0) {{ $code = $LASTEXITCODE }}; Write-Host ''; if ($code -eq 0) {{ Write-Host 'Installer finished successfully.' -ForegroundColor Green }} else {{ Write-Host 'Installer finished with exit code:' $code -ForegroundColor Red }}; Write-Host ''; Write-Host 'Press any key to close...' -ForegroundColor Cyan; $null = $Host.UI.RawUI.ReadKey('NoEcho,IncludeKeyDown'); exit $code"#,
            script = script_str
        );

        // ponytail: CREATE_NEW_CONSOLE gives the installer its own window so
        // it doesn't share the parent's (dev terminal) stdout. The wrapper
        // above pauses for a keypress on both success and failure.
        let spawn = |exe: &str| {
            Command::new(exe)
                .args([
                    "-ExecutionPolicy",
                    "Bypass",
                    "-WindowStyle",
                    "Normal",
                    "-Command",
                    &wrapper,
                ])
                .creation_flags(CREATE_NEW_CONSOLE)
                .spawn()
        };

        if spawn("pwsh.exe").is_err() {
            log::warn!("pwsh.exe unavailable, falling back to powershell.exe");
            spawn("powershell.exe").map_err(|e| format!("Failed to launch installer: {e}"))?;
        }
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = script_str;
        Err("Engine installers are Windows-only.".into())
    }
}

// ── KittenTTS installer (user commands) ──────────────────────────────────────

const INSTALLER_SCRIPT: &str = include_str!("../../../scripts/install-kittentts.ps1");
const CLI_SCRIPT: &str = "";

fn write_to_temp(content: &str, temp_dir: &std::path::Path, filename: &str) -> io::Result<PathBuf> {
    let dest = temp_dir.join(filename);
    fs::write(&dest, content)?;
    Ok(dest)
}

#[tauri::command]
pub fn get_installer_script_path() -> String {
    let temp_dir = std::env::temp_dir().join("copyspeak-installer");
    temp_dir.join("install-kittentts.ps1").display().to_string()
}

#[tauri::command]
pub fn run_kittentts_installer() -> Result<(), String> {
    let temp_dir = std::env::temp_dir().join("copyspeak-installer");
    fs::create_dir_all(&temp_dir).map_err(|e| format!("Failed to create temp directory: {}", e))?;

    let script_path = write_to_temp(INSTALLER_SCRIPT, &temp_dir, "install-kittentts.ps1")
        .map_err(|e| format!("Failed to write installer script: {}", e))?;

    let _ = write_to_temp(CLI_SCRIPT, &temp_dir, "kittentts-cli.py");

    let script_path_str = script_path.display().to_string();

    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("pwsh.exe");
        let script_wrapper = format!(
            r#"$ErrorActionPreference = 'Continue'; & '{script}'; $exitCode = $LASTEXITCODE; Write-Host ''; if ($exitCode -eq 0) {{ Write-Host 'Installation successful!' -ForegroundColor Green }} else {{ Write-Host 'Installation failed with exit code:' $exitCode -ForegroundColor Red }}; Write-Host ''; Write-Host 'Press any key to close...' -ForegroundColor Cyan; $null = $Host.UI.RawUI.ReadKey('NoEcho,IncludeKeyDown'); exit $exitCode"#,
            script = script_path_str
        );

        cmd.args([
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Normal",
            "-Command",
            &script_wrapper,
        ]);

        match cmd.spawn() {
            Ok(_) => {
                log::info!("Opened KittenTTS installer in new PowerShell 7 window");
            }
            Err(_) => {
                log::warn!("pwsh.exe not found, falling back to powershell.exe");
                let mut cmd_fallback = Command::new("powershell.exe");
                cmd_fallback.args([
                    "-ExecutionPolicy",
                    "Bypass",
                    "-WindowStyle",
                    "Normal",
                    "-Command",
                    &script_wrapper,
                ]);

                cmd_fallback
                    .spawn()
                    .map_err(|e| format!("Failed to launch installer: {}", e))?;
                log::info!("Opened KittenTTS installer in new Windows PowerShell window");
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        return Err("Installer only available on Windows".into());
    }

    Ok(())
}
