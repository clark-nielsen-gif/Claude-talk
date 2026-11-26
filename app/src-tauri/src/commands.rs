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
    pub advanced_unlocked: Option<bool>,
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
            if v {
                c.route_discord = true; // switching Claude on re-enables Discord routing
            }
        }
        if let Some(v) = patch.advanced_unlocked {
            c.advanced_unlocked = v;
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
        "windowsUpdate" => "ms-settings:windowsupdate",
        "download" => "https://github.com/clark-nielsen-gif/Claude-talk/releases/latest",
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

#[tauri::command]
pub fn restore_audio_settings(state: State<AppState>) {
    if let Ok(mut c) = state.config.lock() {
        c.route_discord = false;
    }
    state.save_config();
    crate::discord::restore();
}

#[tauri::command]
pub async fn restart_discord() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(crate::discord::restart).await.map_err(|_| "error.internal".to_string())?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatrixCell {
    pub source: u32,
    pub output: u32,
    pub state: &'static str,
    pub gain_db: f32,
    pub who: Option<String>,
}

#[derive(Serialize)]
pub struct Labeled {
    pub id: u32,
    pub label: String,
}

#[derive(Serialize)]
pub struct Matrix {
    pub sources: Vec<Labeled>,
    pub outputs: Vec<Labeled>,
    pub cells: Vec<MatrixCell>,
}

/// Sources × outputs with each cell's state as the routing rules see it.
#[tauri::command]
pub fn get_matrix(state: State<AppState>) -> Matrix {
    #[cfg(windows)]
    {
        use ct_engine::graph::{Party, Rejection};
        let g = state.audio.lock().unwrap_or_else(|p| p.into_inner());
        let Some(host) = g.as_ref() else { return Matrix { sources: vec![], outputs: vec![], cells: vec![] } };
        let graph = host.graph();
        let who = |p: &Party| match p {
            Party::Claude => "Claude".to_string(),
            Party::Discord => "Discord".to_string(),
            Party::User => "Du".to_string(),
            Party::App(a) => a.clone(),
            Party::Other => "?".to_string(),
        };
        let sources: Vec<Labeled> = graph.sources().filter(|s| s.id < 900).map(|s| Labeled { id: s.id, label: s.label.clone() }).collect();
        let outputs: Vec<Labeled> = graph.outputs().map(|o| Labeled { id: o.id, label: o.label.clone() }).collect();
        let mut cells = Vec::new();
        for s in &sources {
            for o in &outputs {
                let (st, w) = match graph.route(s.id, o.id) {
                    Some(r) => (if r.muted { "off" } else { "on" }, None),
                    None => match graph.check_route(s.id, o.id) {
                        Ok(_) => ("off", None),
                        Err(Rejection::SelfHearing { party, .. }) => ("locked", Some(who(&party))),
                        Err(Rejection::Feedback { .. }) => ("loop", None),
                        Err(_) => ("off", None),
                    },
                };
                let gain = graph.route(s.id, o.id).map(|r| r.gain_db).unwrap_or(0.0);
                cells.push(MatrixCell { source: s.id, output: o.id, state: st, gain_db: gain, who: w });
            }
        }
        Matrix { sources, outputs, cells }
    }
    #[cfg(not(windows))]
    {
        let _ = state;
        Matrix { sources: vec![], outputs: vec![], cells: vec![] }
    }
}

#[tauri::command]
pub fn set_route(app: AppHandle, state: State<AppState>, source: u32, output: u32, enabled: bool, gain_db: f32) -> Result<(), String> {
    #[cfg(windows)]
    {
        use ct_engine::graph::Route;
        let mut g = state.audio.lock().unwrap_or_else(|p| p.into_inner());
        let Some(host) = g.as_mut() else { return Err("error.internal".into()) };
        if enabled {
            host.set_route(source, output, Route { gain_db: gain_db.clamp(-60.0, 12.0), muted: false }).map_err(|e| format!("error.{}", e.key()))?;
        } else {
            host.remove_route(source, output);
        }
        drop(g);
        crate::audio::apply_switches(&app);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (app, state, source, output, enabled, gain_db);
        Err("error.internal".into())
    }
}

#[tauri::command]
pub fn set_unlocked(state: State<AppState>, unlocked: bool) {
    #[cfg(windows)]
    if let Some(h) = state.audio.lock().unwrap_or_else(|p| p.into_inner()).as_mut() {
        h.set_self_hearing_unlocked(unlocked);
    }
    #[cfg(not(windows))]
    let _ = (state, unlocked);
}

#[tauri::command]
pub async fn self_test(app: AppHandle) -> Result<Vec<crate::selftest::RouteResult>, String> {
    tauri::async_runtime::spawn_blocking(move || crate::selftest::run(&app)).await.map_err(|_| "error.internal".to_string())?
}
