//! Shared application state.
use crate::config::Config;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::{Arc, Mutex};

pub struct Inner {
    pub config: Mutex<Config>,
    /// PID of the Claude window's WebView2 browser process (0 = closed).
    pub claude_pid: Arc<AtomicU32>,
    /// The Claude page currently has our microphone open.
    pub claude_mic_active: AtomicBool,
    pub shutting_down: AtomicBool,
    pub ptt_down: AtomicBool,
    #[cfg(windows)]
    pub audio: Mutex<Option<ct_engine::win::host::AudioHost>>,
}

pub type AppState = Arc<Inner>;

impl Inner {
    pub fn new(config: Config) -> AppState {
        Arc::new(Inner {
            config: Mutex::new(config),
            claude_pid: Arc::new(AtomicU32::new(0)),
            claude_mic_active: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
            ptt_down: AtomicBool::new(false),
            #[cfg(windows)]
            audio: Mutex::new(None),
        })
    }

    pub fn save_config(&self) {
        if let Ok(c) = self.config.lock() {
            if let Err(e) = c.save(&crate::paths::config_file()) {
                log::warn!("could not save settings: {e}");
            }
        }
    }
}
