use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::Emitter;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

pub fn set_piper_app_handle(handle: tauri::AppHandle) {
    let _ = APP_HANDLE.set(handle);
}

fn emit_model_status(phase: &str, model: Option<&str>, cuda: bool, error: Option<&str>) {
    if let Some(app) = APP_HANDLE.get() {
        let payload = serde_json::json!({
            "phase": phase,
            "model": model,
            "cuda": cuda,
            "error": error,
        });
        let _ = app.emit("piper-status-changed", payload);
    }
}

#[derive(Clone)]
pub struct ServerHandle {
    pub port: u16,
    pub client: reqwest::blocking::Client,
}

pub struct ActiveServer {
    pub child: Mutex<std::process::Child>,
    pub port: u16,
    pub model_name: String,
    pub cuda: bool,
    pub client: reqwest::blocking::Client,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StartingConfig {
    pub command: String,
    pub data_dir: String,
    pub cuda: bool,
}

pub enum ServerState {
    Stopped,
    Starting {
        _generation: u64,
        config: StartingConfig,
        stderr_tail: Arc<Mutex<Vec<String>>>,
    },
    Ready(Arc<ActiveServer>),
}

static CURRENT_GENERATION: AtomicU64 = AtomicU64::new(0);
static SERVER_STATE: OnceLock<Mutex<ServerState>> = OnceLock::new();

fn get_server_state() -> &'static Mutex<ServerState> {
    SERVER_STATE.get_or_init(|| Mutex::new(ServerState::Stopped))
}

fn get_piper_client() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        // H2: Build synthesis client with connect_timeout(2s) and standard pool
        reqwest::blocking::Client::builder()
            .tcp_nodelay(true)
            .connect_timeout(std::time::Duration::from_secs(2))
            .pool_max_idle_per_host(2)
            .build()
            .expect("Failed to build Piper HTTP client")
    })
}

fn get_free_port() -> Option<u16> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|addr| addr.port())
        .ok()
}

#[cfg(windows)]
pub(crate) fn get_nvidia_dll_paths(python_executable: &str) -> Option<String> {
    static NVIDIA_PATHS: OnceLock<Option<String>> = OnceLock::new();
    NVIDIA_PATHS
        .get_or_init(|| {
            // Enumerate every sub-directory of the `nvidia` namespace package that
            // contains a `bin` folder.  Forward-compatible with both the CUDA 12
            // wheel layout (nvidia/<pkg>/bin/*.dll) and the CUDA 13 layout where
            // all DLLs are consolidated under nvidia/cu13/bin/x86_64/*.dll.
            // We collect both `bin` dirs and their immediate subdirectories so
            // both layouts resolve correctly.
            let output = Command::new(python_executable)
                .args([
                    "-c",
                    "import os, glob, nvidia; \
                     nvidia_dir = list(nvidia.__path__)[0]; \
                     bin_dirs = glob.glob(os.path.join(nvidia_dir, '*', 'bin')); \
                     sub_dirs = glob.glob(os.path.join(nvidia_dir, '*', 'bin', '*')); \
                     all_dirs = [p for p in bin_dirs + sub_dirs if os.path.isdir(p)]; \
                     print(';'.join(all_dirs))"
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .ok()?;

            if output.status.success() {
                let paths_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !paths_str.is_empty() {
                    return Some(paths_str);
                }
            }
            None
        })
        .clone()
}

fn resolve_python_path() -> std::path::PathBuf {
    let base = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("CopySpeak")
        .join("engines")
        .join("piper");
    
    #[cfg(target_os = "windows")]
    {
        base.join(".venv").join("Scripts").join("python.exe")
    }
    #[cfg(not(target_os = "windows"))]
    {
        base.join(".venv").join("bin").join("python")
    }
}

/// Restart/Start the server in background.
fn spawn_start_thread(
    generation: u64,
    command: String,
    voice: String,
    data_dir: String,
    cuda: bool,
    stderr_tail: Arc<Mutex<Vec<String>>>,
) {
    std::thread::spawn(move || {
        let voice_file = if voice.ends_with(".onnx") {
            voice.clone()
        } else {
            format!("{}.onnx", voice)
        };
        let mut model_path = std::path::PathBuf::from(&data_dir).join(&voice_file);
        if !model_path.exists() {
            let engine_voice_path = dirs::data_local_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("CopySpeak")
                .join("engines")
                .join("piper")
                .join("voices")
                .join(&voice_file);

            if engine_voice_path.exists() {
                model_path = engine_voice_path;
            } else {
                let alt_path = std::path::PathBuf::from(&voice);
                if alt_path.exists() {
                    model_path = alt_path;
                } else {
                    log::warn!("[Piper] Start failed: model file not found at {}", model_path.display());
                    emit_model_status("error", Some(&voice), cuda, Some("Model file not found"));
                    return;
                }
            }
        }

        let port = match get_free_port() {
            Some(p) => p,
            None => {
                log::warn!("[Piper] Start failed: no free port available");
                emit_model_status("error", Some(&voice), cuda, Some("No free port available"));
                return;
            }
        };

        log::info!("[Piper] Starting HTTP server on port {} — model: {}, cuda: {}", port, model_path.display(), cuda);

        let python_exe = if command == "uv" {
            let venv_python = resolve_python_path();
            if venv_python.exists() {
                venv_python.to_string_lossy().to_string()
            } else {
                "python".to_string()
            }
        } else {
            command.clone()
        };

        let mut cmd = std::process::Command::new(&python_exe);
        let mut args = vec![
            "-m".to_string(),
            "piper.http_server".to_string(),
            "-m".to_string(),
            model_path.to_string_lossy().to_string(),
            "--port".to_string(),
            port.to_string(),
            "--host".to_string(),
            "127.0.0.1".to_string(),
        ];

        if !data_dir.is_empty() {
            args.push("--data-dir".to_string());
            args.push(data_dir.clone());
        }

        if cuda {
            args.push("--cuda".to_string());
        }

        cmd.args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        #[cfg(windows)]
        {
            cmd.creation_flags(CREATE_NO_WINDOW);
            if cuda {
                if let Some(nvidia_paths) = get_nvidia_dll_paths(&python_exe) {
                    let current_path = std::env::var("PATH").unwrap_or_default();
                    let new_path = format!("{};{}", nvidia_paths, current_path);
                    cmd.env("PATH", new_path);
                }
            }
        }

        emit_model_status("loading", Some(&voice), cuda, None);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                log::warn!("[Piper] Start failed: spawn error — {}", e);
                emit_model_status("error", Some(&voice), cuda, Some(&format!("Spawn error: {}", e)));
                return;
            }
        };

        // Drain stdout in background
        if let Some(stdout) = child.stdout.take() {
            std::thread::spawn(move || {
                use std::io::BufRead;
                let reader = std::io::BufReader::new(stdout);
                for line in reader.lines() {
                    match line {
                        Ok(line) => log::debug!("[piper-server] {}", line),
                        Err(_) => break,
                    }
                }
            });
        }

        // Drain stderr to tail buffer and debug log
        let stderr_tail_clone = stderr_tail.clone();
        if let Some(stderr) = child.stderr.take() {
            std::thread::spawn(move || {
                use std::io::BufRead;
                let reader = std::io::BufReader::new(stderr);
                for line in reader.lines() {
                    if let Ok(line) = line {
                        log::debug!("[piper-server] {}", line);
                        let mut buffer = stderr_tail_clone.lock().unwrap_or_else(|p| p.into_inner());
                        buffer.push(line);
                        if buffer.len() > 30 {
                            buffer.remove(0);
                        }
                    } else {
                        break;
                    }
                }
            });
        }

        // H5: Health check withconnect/timeout settings, scaled budget for CUDA
        let health_client = match reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_millis(1000))
            .connect_timeout(std::time::Duration::from_millis(500))
            .build()
        {
            Ok(c) => c,
            Err(_) => {
                let _ = child.kill();
                log::warn!("[Piper] Start failed: could not build health-check client");
                emit_model_status("error", Some(&voice), cuda, Some("Health client build failed"));
                return;
            }
        };

        let url = format!("http://127.0.0.1:{}/voices", port);
        let poll_start = std::time::Instant::now();
        let max_secs = if cuda { 60 } else { 15 };
        let mut ready = false;
        let mut poll_delay_ms = 100u64;
        let max_poll_delay_ms = 1600u64;

        while poll_start.elapsed() < std::time::Duration::from_secs(max_secs) {
            // Check if generation superseded
            if CURRENT_GENERATION.load(Ordering::SeqCst) != generation {
                log::info!("[Piper] Generation {} superseded. Killing child.", generation);
                let _ = child.kill();
                return;
            }

            if let Ok(Some(status)) = child.try_wait() {
                let err_tail = {
                    let buffer = stderr_tail.lock().unwrap_or_else(|p| p.into_inner());
                    buffer.join("\n")
                };
                log::warn!(
                    "[Piper] Server exited prematurely with code {:?}. Stderr tail:\n{}",
                    status.code(),
                    err_tail
                );
                emit_model_status("error", Some(&voice), cuda, Some("Server exited prematurely"));
                return;
            }

            if let Ok(resp) = health_client.get(&url).send() {
                if resp.status().is_success() {
                    ready = true;
                    break;
                }
            }

            std::thread::sleep(std::time::Duration::from_millis(poll_delay_ms));
            poll_delay_ms = (poll_delay_ms * 2).min(max_poll_delay_ms);
        }

        if !ready {
            let _ = child.kill();
            let err_tail = {
                let buffer = stderr_tail.lock().unwrap_or_else(|p| p.into_inner());
                buffer.join("\n")
            };
            let err_msg = format!("Start timed out after {}s", max_secs);
            log::warn!(
                "[Piper] Server start timed out after {}s. Stderr tail:\n{}",
                max_secs,
                err_tail
            );
            emit_model_status("error", Some(&voice), cuda, Some(&err_msg));
            return;
        }

        log::info!("[Piper] Server ready on port {} (generation {})", port, generation);

        // P3: Substantial CUDA warmup sentence to compile JIT kernels
        emit_model_status("warming_up", Some(&voice), cuda, None);
        let warmup_text = if cuda {
            "This is a warm-up sentence to compile the CUDA kernels and initialize the models."
        } else {
            "Hello"
        };

        let warmup_client = get_piper_client();
        let warmup_url = format!("http://127.0.0.1:{}/", port);
        let warmup_body = serde_json::json!({ "text": warmup_text, "length_scale": 1.0 });
        let warmup_start = std::time::Instant::now();
        match warmup_client.post(&warmup_url).json(&warmup_body).send() {
            Ok(resp) => {
                let _ = resp.bytes();
                log::info!("[Piper] Warmup completed in {:.1}s", warmup_start.elapsed().as_secs_f64());
            }
            Err(e) => {
                log::warn!("[Piper] Warmup failed: {}. First synthesis will be slower.", e);
            }
        }

        // Lock state and store ready server
        let mut state = get_server_state().lock().unwrap_or_else(|p| p.into_inner());
        if CURRENT_GENERATION.load(Ordering::SeqCst) == generation {
            *state = ServerState::Ready(Arc::new(ActiveServer {
                child: Mutex::new(child),
                port,
                model_name: voice.clone(),
                cuda,
                client: get_piper_client().clone(),
            }));
            emit_model_status("ready", Some(&voice), cuda, None);
        } else {
            log::info!("[Piper] Server on port {} was superseded during warmup. Killing.", port);
            let _ = child.kill();
        }
    });
}

pub fn ensure_running(
    command: String,
    voice: String,
    data_dir: String,
    cuda: bool,
) -> Result<ServerHandle, String> {
    let start_wait = std::time::Instant::now();
    loop {
        let mut state = get_server_state().lock().unwrap_or_else(|p| p.into_inner());
        match &mut *state {
            ServerState::Ready(server) => {
                let active = server.clone();
                drop(state);

                let is_alive = matches!(
                    active.child.lock().unwrap_or_else(|p| p.into_inner()).try_wait(),
                    Ok(None)
                );
                if is_alive && active.cuda == cuda {
                    return Ok(ServerHandle {
                        port: active.port,
                        client: active.client.clone(),
                    });
                } else {
                    let mut state = get_server_state().lock().unwrap_or_else(|p| p.into_inner());
                    // Re-verify under lock in case someone changed it
                    if let ServerState::Ready(curr) = &*state {
                        if Arc::ptr_eq(curr, &active) {
                            log::info!(
                                "[Piper] Killing dead/mismatched server on port {}",
                                active.port
                            );
                            // Kill through the Mutex — no try_unwrap, no dummy process.
                            // This works even if other threads still hold Arc clones.
                            {
                                let mut child = active
                                    .child
                                    .lock()
                                    .unwrap_or_else(|p| p.into_inner());
                                let _ = child.kill();
                                let _ = child.wait(); // reap to avoid zombies on Unix
                            }
                            // Bump the generation: any Starting thread racing in
                            // cannot resurrect the same configuration unobserved.
                            CURRENT_GENERATION.fetch_add(1, Ordering::SeqCst);
                            *state = ServerState::Stopped;
                        }
                    }
                }
            }
            ServerState::Starting { _generation: _, config: starting_config, stderr_tail } => {
                if starting_config.command == command
                    && starting_config.data_dir == data_dir
                    && starting_config.cuda == cuda
                {
                    // Wait for it
                    if start_wait.elapsed() > std::time::Duration::from_secs(65) {
                        let err_msg = {
                            let buffer = stderr_tail.lock().unwrap_or_else(|p| p.into_inner());
                            buffer.join("\n")
                        };
                        return Err(format!("Timeout waiting for Piper server to start. Stderr tail:\n{}", err_msg));
                    }
                    drop(state);
                    std::thread::sleep(std::time::Duration::from_millis(200));
                } else {
                    // Mismatched configuration! Trigger new start
                    let new_gen = CURRENT_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
                    let tail = Arc::new(Mutex::new(Vec::new()));
                    *state = ServerState::Starting {
                        _generation: new_gen,
                        config: StartingConfig {
                            command: command.clone(),
                            data_dir: data_dir.clone(),
                            cuda,
                        },
                        stderr_tail: tail.clone(),
                    };
                    drop(state);
                    spawn_start_thread(new_gen, command.clone(), voice.clone(), data_dir.clone(), cuda, tail);
                }
            }
            ServerState::Stopped => {
                let new_gen = CURRENT_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
                let tail = Arc::new(Mutex::new(Vec::new()));
                *state = ServerState::Starting {
                    _generation: new_gen,
                    config: StartingConfig {
                        command: command.clone(),
                        data_dir: data_dir.clone(),
                        cuda,
                    },
                    stderr_tail: tail.clone(),
                };
                drop(state);
                spawn_start_thread(new_gen, command.clone(), voice.clone(), data_dir.clone(), cuda, tail);
            }
        }
    }
}

pub fn unload_piper_model() -> bool {
    let mut state = get_server_state().lock().unwrap_or_else(|p| p.into_inner());
    match &*state {
        ServerState::Ready(server) => {
            log::info!("[Piper] Unloading model on port {}", server.port);
            {
                let mut child = server.child.lock().unwrap_or_else(|p| p.into_inner());
                let _ = child.kill();
                let _ = child.wait();
            }
            *state = ServerState::Stopped;
            emit_model_status("stopped", None, false, None);
            true
        }
        ServerState::Starting { .. } => {
            log::info!("[Piper] Cancelling in-flight start via generation bump");
            CURRENT_GENERATION.fetch_add(1, Ordering::SeqCst);
            *state = ServerState::Stopped;
            emit_model_status("stopped", None, false, None);
            true
        }
        ServerState::Stopped => false,
    }
}

pub fn get_piper_server_status() -> crate::tts::cli::PiperServerStatus {
    let state = get_server_state().lock().unwrap_or_else(|p| p.into_inner());
    match &*state {
        ServerState::Ready(server) => crate::tts::cli::PiperServerStatus {
            running: true,
            model: Some(server.model_name.clone()),
            port: Some(server.port),
            cuda: server.cuda,
            ready: true,
        },
        ServerState::Starting { config, .. } => crate::tts::cli::PiperServerStatus {
            running: true,
            model: None,
            port: None,
            cuda: config.cuda,
            ready: false,
        },
        ServerState::Stopped => crate::tts::cli::PiperServerStatus {
            running: false,
            model: None,
            port: None,
            cuda: false,
            ready: false,
        },
    }
}
