//! Commands the UI can call. Each one does one plain thing; anything that can
//! fail returns a translation key for a plain-language message, never a raw
//! error code alone.

use crate::config::{Config, EchoMode, Theme};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub windows_build: u32,
    /// Process loopback (Windows 11 / build 20348+) is available.
    pub supported: bool,
    pub cable_installed: bool,
    pub test_mode: bool,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    #[cfg(windows)]
    let (build, supported, cable) = (
        ct_engine::win::windows_build(),
        ct_engine::win::supports_process_loopback(),
        {
            let _com = ct_engine::win::ComGuard::new();
            ct_engine::win::host::cable_installed()
        },
    );
    #[cfg(not(windows))]
    let (build, supported, cable) = (0, false, false);
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        windows_build: build,
        supported,
        cable_installed: cable,
        test_mode: cfg!(feature = "ci-testpage"),
    }
}

#[tauri::command]
pub fn get_config(state: State<AppState>) -> Config {
    state.config.lock().map(|c| c.clone()).unwrap_or_default()
}

/// Partial update from the UI; only the given fields change.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConfigPatch {
    pub language: Option<String>,
    pub theme: Option<Theme>,
    pub autostart: Option<bool>,
    pub onboarding_done: Option<bool>,
    pub claude_in_discord: Option<bool>,
    pub mic_muted: Option<bool>,
    pub claude_to_discord_muted: Option<bool>,
    pub push_to_talk: Option<bool>,
    pub echo: Option<EchoMode>,
    pub hotkeys: Option<crate::config::Hotkeys>,
}

#[tauri::command]
pub fn set_config(app: AppHandle, state: State<AppState>, patch: ConfigPatch) -> Result<Config, String> {
    let (cfg, hotkeys_changed, autostart_changed) = {
        let mut c = state.config.lock().map_err(|_| "error.internal".to_string())?;
        if let Some(v) = patch.language.filter(|l| l == "da" || l == "en") {
            c.language = v;
        }
        if let Some(v) = patch.theme {
            c.theme = v;
        }
        let autostart_changed = patch.autostart.is_some_and(|v| v != c.autostart);
        if let Some(v) = patch.autostart {
            c.autostart = v;
        }
        if let Some(v) = patch.onboarding_done {
            c.onboarding_done = v;
        }
        if let Some(v) = patch.claude_in_discord {
            c.claude_in_discord = v;
        }
        if let Some(v) = patch.mic_muted {
            c.mic_muted = v;
        }
        if let Some(v) = patch.claude_to_discord_muted {
            c.claude_to_discord_muted = v;
        }
        if let Some(v) = patch.push_to_talk {
            c.push_to_talk = v;
        }
        if let Some(v) = patch.echo {
            c.echo = v;
        }
        let hotkeys_changed = patch.hotkeys.as_ref().is_some_and(|h| *h != c.hotkeys);
        if let Some(v) = patch.hotkeys {
            c.hotkeys = v;
        }
        (c.clone(), hotkeys_changed, autostart_changed)
    };
    state.save_config();
    crate::audio::apply_switches(&app);
    crate::tray::refresh(&app);
    if hotkeys_changed {
        crate::hotkeys::register(&app).map_err(|_| "error.hotkeyTaken".to_string())?;
    }
    if autostart_changed {
        crate::autostart::apply(&app, cfg.autostart);
    }
    Ok(cfg)
}

#[tauri::command]
pub fn open_claude(app: AppHandle) -> Result<(), String> {
    crate::claude_window::open(&app).map_err(|e| {
        log::error!("open Claude window: {e}");
        "error.claudeWindow".to_string()
    })
}

/// Opens one of a fixed set of places. The UI cannot open arbitrary URLs.
#[tauri::command]
pub fn open_place(app: AppHandle, place: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let url = match place.as_str() {
        "discordVoice" => "discord://-/settings/voice",
        "micPrivacy" => "ms-settings:privacy-microphone",
        "soundSettings" => "ms-settings:sound",
        "vbAudio" => "https://vb-audio.com/Cable/",
        "vbAudioDonate" => "https://shop.vb-audio.com/en/win-apps/11-vb-cable.html",
        "logs" => {
            return app
                .opener()
                .open_path(crate::paths::logs_dir().to_string_lossy(), None::<&str>)
                .map_err(|_| "error.open".to_string())
        }
        _ => return Err("error.open".into()),
    };
    app.opener().open_url(url, None::<&str>).map_err(|_| "error.open".to_string())
}

/// Saves logs, settings and a status snapshot as a zip on the Desktop and
/// returns its path.
#[tauri::command]
pub fn save_report(app: AppHandle) -> Result<String, String> {
    crate::report::save(&app).map_err(|e| {
        log::error!("save report: {e}");
        "error.report".to_string()
    })
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    crate::shutdown(&app);
}

#[tauri::command]
pub fn show_main(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}
