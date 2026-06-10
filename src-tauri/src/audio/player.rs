// Audio player: AudioPlayerInner (thread-bound) and AudioPlayer (thread-safe handle).
// Handles playback with interrupt/queue modes via a dedicated audio thread.

use crate::config::RetriggerMode;
use rodio::{OutputStream, OutputStreamHandle, Sink};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::thread;

/// Commands sent to the audio thread
pub(super) enum AudioCommand {
    Stop,
    TogglePause,
    SetMode(RetriggerMode),
    SetVolume(u8),
    SeekRelative(i32),
}

/// Internal AudioPlayer that runs on a dedicated thread (not Send+Sync)
struct AudioPlayerInner {
    _stream: Option<OutputStream>,
    stream_handle: Option<OutputStreamHandle>,
    sink: Option<Sink>,
    mode: RetriggerMode,
    volume: u8,
    playback_start: Option<std::time::Instant>,
    paused_duration: std::time::Duration,
    pause_start: Option<std::time::Instant>,
    current_position: std::time::Duration,
}

impl AudioPlayerInner {
    fn new() -> Self {
        Self {
            _stream: None,
            stream_handle: None,
            sink: None,
            mode: RetriggerMode::Interrupt,
            volume: 100,
            playback_start: None,
            paused_duration: std::time::Duration::ZERO,
            pause_start: None,
            current_position: std::time::Duration::ZERO,
        }
    }

    fn set_mode(&mut self, mode: RetriggerMode) {
        log::info!("Audio retrigger mode changed to: {:?}", mode);
        self.mode = mode;
    }

    fn set_volume(&mut self, volume: u8) {
        log::debug!("Volume changed to: {}%", volume);
        self.volume = volume;
        if let Some(ref sink) = self.sink {
            sink.set_volume(volume as f32 / 100.0);
        }
    }

    fn stop(&mut self) {
        if let Some(sink) = self.sink.take() {
            log::info!("Audio playback stopped");
            sink.stop();
        } else {
            log::debug!("stop() called but no active playback");
        }
        self._stream = None;
        self.stream_handle = None;
        self.playback_start = None;
        self.paused_duration = std::time::Duration::ZERO;
        self.pause_start = None;
        self.current_position = std::time::Duration::ZERO;
    }

    fn toggle_pause(&mut self) {
        if let Some(ref sink) = self.sink {
            if sink.is_paused() {
                log::info!("Audio playback resumed (via toggle)");
                sink.play();
                if let Some(pause_start) = self.pause_start {
                    self.paused_duration += pause_start.elapsed();
                    self.pause_start = None;
                }
            } else {
                log::info!("Audio playback paused (via toggle)");
                sink.pause();
                self.pause_start = Some(std::time::Instant::now());
            }
        } else {
            log::debug!("toggle_pause() called but no active playback");
        }
    }

    fn get_current_position(&self) -> std::time::Duration {
        if let Some(start) = self.playback_start {
            let elapsed = start.elapsed();
            let total_paused = self.paused_duration
                + self
                    .pause_start
                    .map(|p| p.elapsed())
                    .unwrap_or(std::time::Duration::ZERO);
            self.current_position + elapsed.saturating_sub(total_paused)
        } else {
            std::time::Duration::ZERO
        }
    }

    fn seek_relative(&mut self, delta: std::time::Duration) {
        if let Some(ref sink) = self.sink {
            let current = self.get_current_position();
            let new_pos = if delta.as_nanos() > 0 {
                current.saturating_add(delta)
            } else {
                current.saturating_sub(std::time::Duration::from_nanos(delta.as_nanos() as u64))
            };
            log::debug!(
                "Seeking from {:?} to {:?} (delta: {:?})",
                current,
                new_pos,
                delta
            );
            if let Err(e) = sink.try_seek(new_pos) {
                log::warn!("Seek failed: {:?}", e);
            } else {
                log::info!("Seeked to position: {:?}", new_pos);
                self.current_position = new_pos;
                self.playback_start = Some(std::time::Instant::now());
                self.paused_duration = std::time::Duration::ZERO;
                self.pause_start = None;
            }
        } else {
            log::debug!("seek_relative() called but no active playback");
        }
    }
}

/// Playback state information for the frontend.
#[derive(Clone, serde::Serialize)]
pub struct PlaybackState {
    pub is_playing: bool,
    pub is_paused: bool,
}

/// Thread-safe handle to the AudioPlayer.
/// Communicates with the audio thread via channels.
pub struct AudioPlayer {
    tx: Sender<AudioCommand>,
    is_playing: Arc<AtomicBool>,
    is_paused: Arc<AtomicBool>,
    playback_finished: Arc<AtomicBool>,
    currently_playing_entry_id: Arc<std::sync::Mutex<Option<String>>>,
}

impl AudioPlayer {
    pub fn new() -> Self {
        let (tx, rx) = channel::<AudioCommand>();
        let is_playing = Arc::new(AtomicBool::new(false));
        let is_paused = Arc::new(AtomicBool::new(false));
        let playback_finished = Arc::new(AtomicBool::new(false));
        let currently_playing_entry_id = Arc::new(std::sync::Mutex::new(None));

        // Playback happens in the webview's <audio> element; the frontend
        // reports state via set_playback_state. This thread only services
        // rodio safety-net commands, so it can block on recv().
        thread::spawn(move || {
            log::info!("Audio thread started");
            let mut player = AudioPlayerInner::new();

            while let Ok(cmd) = rx.recv() {
                match cmd {
                    AudioCommand::Stop => {
                        player.stop();
                    }
                    AudioCommand::TogglePause => {
                        player.toggle_pause();
                    }
                    AudioCommand::SetMode(mode) => {
                        player.set_mode(mode);
                    }
                    AudioCommand::SetVolume(volume) => {
                        player.set_volume(volume);
                    }
                    AudioCommand::SeekRelative(seconds) => {
                        if seconds >= 0 {
                            player.seek_relative(std::time::Duration::from_secs(seconds as u64));
                        } else {
                            player.seek_relative(std::time::Duration::from_secs(
                                (-seconds) as u64,
                            ));
                        }
                    }
                }
            }
        });

        Self {
            tx,
            is_playing,
            is_paused,
            playback_finished,
            currently_playing_entry_id,
        }
    }

    /// Update playback state as reported by the frontend audio element.
    /// Drives the tray busy icon, tray click behavior, and HUD auto-hide,
    /// which were dead while the state only tracked the unused rodio sink.
    pub fn set_playback_state_reported(&self, playing: bool, paused: bool) {
        let was_playing = self.is_playing.swap(playing, Ordering::Relaxed);
        self.is_paused.store(paused, Ordering::Relaxed);
        if was_playing && !playing {
            log::info!("Playback finished (frontend report)");
            self.playback_finished.store(true, Ordering::Relaxed);
            if let Ok(mut entry_id) = self.currently_playing_entry_id.lock() {
                *entry_id = None;
            }
        }
    }

    pub fn set_mode(&mut self, mode: RetriggerMode) {
        let _ = self.tx.send(AudioCommand::SetMode(mode));
    }

    /// Set the playback volume (0-100).
    pub fn set_volume(&mut self, volume: u8) {
        let _ = self.tx.send(AudioCommand::SetVolume(volume));
    }

    pub fn stop(&mut self) {
        let _ = self.tx.send(AudioCommand::Stop);
    }

    pub fn toggle_pause(&mut self) {
        let _ = self.tx.send(AudioCommand::TogglePause);
    }

    pub fn skip_forward(&mut self, seconds: u64) {
        let _ = self.tx.send(AudioCommand::SeekRelative(seconds as i32));
    }

    pub fn skip_backward(&mut self, seconds: u64) {
        let _ = self.tx.send(AudioCommand::SeekRelative(-(seconds as i32)));
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing.load(Ordering::Relaxed)
    }

    pub fn get_state(&self) -> PlaybackState {
        PlaybackState {
            is_playing: self.is_playing.load(Ordering::Relaxed),
            is_paused: self.is_paused.load(Ordering::Relaxed),
        }
    }

    /// Check and clear the playback finished flag.
    /// Returns true if playback finished since the last call, false otherwise.
    pub fn take_playback_finished(&self) -> bool {
        self.playback_finished.swap(false, Ordering::Relaxed)
    }

    /// Set the currently playing history entry ID.
    pub fn set_playing_entry_id(&self, entry_id: Option<String>) {
        if let Ok(mut id) = self.currently_playing_entry_id.lock() {
            *id = entry_id;
        }
    }

    /// Get the currently playing history entry ID.
    pub fn get_playing_entry_id(&self) -> Option<String> {
        self.currently_playing_entry_id.lock().ok()?.clone()
    }
}
