use std::path::PathBuf;
use winreg::enums::*;
use winreg::RegKey;

const REGISTRY_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APP_NAME: &str = "CopySpeak TTS";

fn get_current_exe_path() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| format!("Failed to get executable path: {}", e))
}

fn get_run_key() -> Result<RegKey, String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey_with_flags(REGISTRY_KEY, KEY_SET_VALUE | KEY_READ)
        .map_err(|e| format!("Failed to open registry key: {}", e))
}

pub fn enable_autostart() -> Result<(), String> {
    let exe_path = get_current_exe_path()?;
    let exe_path_str = exe_path.to_string_lossy();

    let quoted_path = format!("\"{}\"", exe_path_str);

    let run_key = get_run_key()?;
    run_key
        .set_value(APP_NAME, &quoted_path)
        .map_err(|e| format!("Failed to set registry value: {}", e))?;

    log::info!("Enabled auto-start: {}", quoted_path);
    Ok(())
}

pub fn disable_autostart() -> Result<(), String> {
    let run_key = get_run_key()?;

    match run_key.delete_value(APP_NAME) {
        Ok(()) => {
            log::info!("Disabled auto-start");
            Ok(())
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                log::info!("Auto-start was already disabled (registry value not found)");
                Ok(())
            } else {
                Err(format!("Failed to remove registry value: {}", e))
            }
        }
    }
}

pub fn sync_autostart_with_config(enabled: bool) -> Result<(), String> {
    if enabled {
        enable_autostart()
    } else {
        disable_autostart()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_current_exe_path() {
        let result = get_current_exe_path();
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(
            path.to_string_lossy().ends_with(".exe")
                || path.to_string_lossy().contains("copyspeak")
        );
    }
}
