//! Shared application state.
use crate::config::Config;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8};
use std::sync::{Arc, Mutex};

pub struct Inner {
    pub config: Mutex<Config>,
    /// PID of the Claude window's WebView2 browser process (0 = closed).
    pub claude_pid: Arc<AtomicU32>,
    /// The Claude page currently has our microphone open.
    pub claude_mic_active: AtomicBool,
    /// The Claude window shows claude.ai's login page.
    pub claude_login: AtomicBool,
    pub shutting_down: AtomicBool,
    pub ptt_down: AtomicBool,
    /// Discord records from our cable: 0 unknown, 1 yes, 2 no (see discord.rs).
    pub discord_uses_cable: AtomicU8,
    pub discord_needs_restart: AtomicBool,
    /// Discord records the cable although Claude Talk doesn't route it: set
    /// to "Claude Talk – til Discord" in Discord itself.
    pub discord_cable_chosen: AtomicBool,
    /// Discord still records the cable it opened before "Gendan" (K12).
    pub discord_still_on_cable: AtomicBool,
    /// The recording side of the cable ("Claude Talk – til Discord") is
    /// there and enabled (checked by the Discord watcher every 2 s).
    pub cable_capture_ok: AtomicBool,
    /// Discord is recording right now (in a call or a voice test).
    pub discord_recording: AtomicBool,
    /// The default output is speakers (sound can reach the microphone).
    pub speakers: AtomicBool,
    /// An output added in Advanced is speakers (same risk).
    pub added_speakers: AtomicBool,
    /// When the default output was last set back from our cable.
    /// When the default output / microphone was set back from the cable,
    /// and the device's name.
    pub output_fixed: Mutex<Option<(std::time::Instant, Option<String>)>>,
    pub mic_fixed: Mutex<Option<(std::time::Instant, Option<String>)>>,
    /// When a Discord call started while the microphone was muted (D32).
    pub muted_call_at: Mutex<Option<std::time::Instant>>,
    /// Shortcuts that could not be registered (shown in the app).
    pub hotkey_problem: Mutex<String>,
    /// Discord plays on another device than Windows' default (where the
    /// Claude window plays): (Discord's device id, its name, the default's name).
    pub output_mismatch: Mutex<Option<(String, String, String)>>,
    /// While Some, the Claude bridge appends what it sends (self-test).
    pub bridge_probe: Mutex<Option<Vec<f32>>>,
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
            claude_login: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
            ptt_down: AtomicBool::new(false),
            discord_uses_cable: AtomicU8::new(0),
            discord_needs_restart: AtomicBool::new(false),
            discord_cable_chosen: AtomicBool::new(false),
            discord_still_on_cable: AtomicBool::new(false),
            cable_capture_ok: AtomicBool::new(true),
            discord_recording: AtomicBool::new(false),
            speakers: AtomicBool::new(false),
            added_speakers: AtomicBool::new(false),
            output_fixed: Mutex::new(None),
            mic_fixed: Mutex::new(None),
            muted_call_at: Mutex::new(None),
            hotkey_problem: Mutex::new(String::new()),
            output_mismatch: Mutex::new(None),
            bridge_probe: Mutex::new(None),
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
