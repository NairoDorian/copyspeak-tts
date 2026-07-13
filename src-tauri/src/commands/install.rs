// Engine installer launcher.
//
// Spawns the PowerShell installer for a local TTS engine (or the uv bootstrap)
// in a detached window. The installer script owns the window and any prompts;
// this command returns immediately. No output is streamed back in v1.

use std::path::PathBuf;
use std::process::Command;


/// Map a CopySpeak engine id to its installer script filename under `scripts/`.
fn installer_script_for(engine: &str) -> Result<&'static str, String> {
    match engine {
        "uv" => Ok("install-uv.ps1"),
        "chatterbox" => Ok("install-chatterbox.ps1"),
        "kitten" | "kittentts" | "kitten-tts" => Ok("install-kittentts.ps1"),
        "piper" => Ok("install-piper.ps1"),
        "kokoro" | "kokoro-tts" => Ok("install-kokoro.ps1"),
        "edge" | "edge-tts" => Ok("install-edge-tts.ps1"),
        other => Err(format!("unknown engine installer: {other}")),
    }
}

/// Resolve `scripts/<name>` from dev (CARGO_MANIFEST_DIR) or exe-relative
/// candidates. Returns the first existing path.
fn resolve_script(filename: &str) -> Result<PathBuf, String> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    // Dev: <repo>/scripts — CARGO_MANIFEST_DIR points at src-tauri.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(repo_root) = manifest.parent() {
        candidates.push(repo_root.join("scripts").join(filename));
    }

    // Packaged: alongside the exe, or one dir up (resource dir layouts).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidates.push(exe_dir.join("scripts").join(filename));
            if let Some(parent_dir) = exe_dir.parent() {
                candidates.push(parent_dir.join("scripts").join(filename));
            }
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
            r#"$ErrorActionPreference = 'Continue'; $code = 0; try {{ & '{script}' }} catch {{ $code = -1; Write-Host ''; Write-Host "ERROR: $_" -ForegroundColor Red }}; if ($LASTEXITCODE -ne 0 -and $code -eq 0) {{ $code = $LASTEXITCODE }}; Write-Host ''; if ($code -eq 0) {{ Write-Host 'Installer finished successfully.' -ForegroundColor Green }} else {{ Write-Host 'Installer finished with exit code:' $code -ForegroundColor Red }}; Write-Host ''; if ($Host.UI.RawUI) {{ Write-Host 'Press any key to close...' -ForegroundColor Cyan; $null = $Host.UI.RawUI.ReadKey('NoEcho,IncludeKeyDown') }} else {{ Write-Host 'Press Enter to close...' -ForegroundColor Cyan; $null = Read-Host }}; exit $code"#,
            script = script_str
        );

        // ponytail: launch via cmd /c start so that the console handles are
        // completely detached. This avoids inheriting standard stream pipes
        // redirected by terminal dev runners, ensuring a visible, interactive
        // console window is created.
        let spawn = |exe: &str| {
            Command::new("cmd")
                .args([
                    "/c",
                    "start",
                    "",
                    exe,
                    "-ExecutionPolicy",
                    "Bypass",
                    "-WindowStyle",
                    "Normal",
                    "-Command",
                    &wrapper,
                ])
                .spawn()
        };

        let shell = if has_working_pwsh() {
            "pwsh.exe"
        } else {
            "powershell.exe"
        };

        log::info!("Using shell '{shell}' to launch installer");
        spawn(shell).map_err(|e| format!("Failed to launch installer: {e}"))?;
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = script_str;
        Err("Engine installers are Windows-only.".into())
    }
}

#[cfg(target_os = "windows")]
fn has_working_pwsh() -> bool {
    if let Ok(output) = Command::new("pwsh.exe").arg("--version").output() {
        output.status.success()
    } else {
        false
    }
}
