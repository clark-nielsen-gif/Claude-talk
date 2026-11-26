//! Commands the UI can call. Each one does one plain thing; anything that can
//! fail returns a translation key for a plain-language message, never a raw
//! error code alone.

use crate::config::{Config, EchoMode, Theme};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub windows_build: u32,
    /// Process loopback (Windows 11 / build 20348+) is available.
    pub supported: bool,
    pub cable_installed: bool,
    pub test_mode: bool,
    /// An extra virtual cable the user installed: "Claude in Chrome or the
    /// app" can be offered. `output` is the microphone to choose there.
    pub extra_cable: Option<ExtraCableInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtraCableInfo {
    pub name: String,
    pub output: Option<String>,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    #[cfg(windows)]
    let (build, supported, cable, extra) = {
        let _com = ct_engine::win::ComGuard::new();
        (
            ct_engine::win::windows_build(),
            ct_engine::win::supports_process_loopback(),
            ct_engine::win::host::cable_installed(),
            ct_engine::win::host::extra_cable().map(|c| ExtraCableInfo {
                name: c.render.name,
                output: c.capture_name,
            }),
        )
    };
    #[cfg(not(windows))]
    let (build, supported, cable, extra) = (0, false, false, None);
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        windows_build: build,
        supported,
        cable_installed: cable,
        test_mode: cfg!(feature = "ci-testpage"),
        extra_cable: extra,
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
    pub instructions: Option<crate::config::Instructions>,
    pub claude_hears_discord: Option<bool>,
}

#[tauri::command]
pub fn set_config(
    app: AppHandle,
    state: State<AppState>,
    patch: ConfigPatch,
) -> Result<Config, String> {
    let (cfg, hotkeys_changed, autostart_changed) = {
        let mut c = state
            .config
            .lock()
            .map_err(|_| "error.internal".to_string())?;
        if let Some(v) = patch.language.filter(|l| l == "da" || l == "en") {
            if v != c.language {
                // "Tal dansk" follows the language the user picks
                c.instructions.danish = v == "da";
            }
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
        if let Some(mut v) = patch.instructions {
            v.wake_word = v.wake_word.trim().chars().take(40).collect();
            v.own = v.own.chars().take(2000).collect();
            c.instructions = v;
        }
        if let Some(v) = patch.claude_hears_discord {
            c.claude_hears_discord = v;
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
        crate::hotkeys::register(&app)?;
    }
    if autostart_changed {
        crate::autostart::apply(&app, cfg.autostart);
    }
    Ok(cfg)
}

#[derive(Serialize)]
pub struct Microphone {
    pub id: String,
    pub name: String,
}

/// The microphones the user can choose (never Claude Talk's own devices).
#[tauri::command]
pub fn list_microphones() -> Vec<Microphone> {
    #[cfg(windows)]
    {
        use ct_engine::win::devices::{self, Flow};
        let _com = ct_engine::win::ComGuard::new();
        let own = devices::patterns(devices::NOT_A_MICROPHONE);
        devices::list(Flow::Capture)
            .unwrap_or_default()
            .into_iter()
            .filter(|e| {
                let n = e.name.to_lowercase();
                !own.iter().any(|p| n.contains(&p.to_lowercase()))
            })
            .map(|e| Microphone {
                id: e.id,
                name: e.name,
            })
            .collect()
    }
    #[cfg(not(windows))]
    Vec::new()
}

/// Chooses the microphone (None: Windows' default microphone).
#[tauri::command]
pub fn set_microphone(
    app: AppHandle,
    state: State<AppState>,
    id: Option<String>,
) -> Result<Config, String> {
    let cfg = {
        let mut c = state
            .config
            .lock()
            .map_err(|_| "error.internal".to_string())?;
        c.microphone = id.filter(|s| !s.is_empty());
        c.clone()
    };
    state.save_config();
    crate::audio::use_microphone(&app, cfg.microphone.clone());
    Ok(cfg)
}

/// The exact message the instructions become (shown as a preview).
#[tauri::command]
pub fn instruction_text(
    instructions: crate::config::Instructions,
    language: String,
    hears_others: bool,
) -> String {
    instructions.text(&language, hears_others)
}

/// Writes the instructions into the Claude window and sends them. The result
/// arrives as the "claude-prompt" event ({ ok, reason }).
#[tauri::command]
pub fn send_instructions(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    let text = {
        let c = state
            .config
            .lock()
            .map_err(|_| "error.internal".to_string())?;
        c.instructions.text(&c.language, c.claude_hears_discord)
    };
    if text.is_empty() {
        return Err("error.noInstructions".into());
    }
    if !crate::claude_window::is_open(&app) {
        return Err("error.claudeNotOpen".into());
    }
    crate::claude_window::send_prompt(&app, &text);
    Ok(())
}

// `async`: on Windows, creating a window inside a synchronous command
// deadlocks (it runs in WebView2's IPC handler on the main thread).
#[tauri::command(async)]
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
        "support" => "https://github.com/clark-nielsen-gif/Claude-talk/issues/new",
        "download" => "https://github.com/clark-nielsen-gif/Claude-talk/releases/latest",
        "logs" => {
            return app
                .opener()
                .open_path(crate::paths::logs_dir().to_string_lossy(), None::<&str>)
                .map_err(|_| "error.open".to_string())
        }
        _ => return Err("error.open".into()),
    };
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|_| "error.open".to_string())
}

/// Saves logs, settings and a status snapshot as a zip on the Desktop and
/// returns its path.
// off the main thread: zipping the logs takes a moment, and the main thread
// carries the Claude window's audio
#[tauri::command(async)]
pub fn save_report(app: AppHandle) -> Result<String, String> {
    crate::report::save(&app).map_err(|e| {
        log::error!("save report: {e}");
        "error.report".to_string()
    })
}

// off the main thread: it may ask first (a Windows dialog)
#[tauri::command(async)]
pub fn quit(app: AppHandle) {
    crate::request_quit(&app);
}

#[tauri::command(async)]
pub fn show_main(app: AppHandle) {
    // started with Windows: the window (and its WebView2) is made only
    // when it is first opened
    if app.get_webview_window("main").is_none() {
        if let Err(e) = crate::create_main_window(&app) {
            log::warn!("main window: {e}");
        }
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    crate::audio::refresh_mic(&app);
}

#[tauri::command]
pub fn restore_audio_settings(app: AppHandle, state: State<AppState>) -> Config {
    // Claude leaves the call too, so the screen says what is true (switching
    // "Claude i Discord" on again sets everything up again)
    let cfg = {
        let mut c = state.config.lock().unwrap_or_else(|p| p.into_inner());
        c.route_discord = false;
        c.claude_in_discord = false;
        c.clone()
    };
    state.save_config();
    crate::discord::restore();
    crate::audio::apply_switches(&app);
    crate::tray::refresh(&app);
    let _ = app.emit("config-changed", &cfg);
    cfg
}

/// Installs Discord's microphone from the app (one Windows prompt).
/// Returns the installer's exit code (0 ok, 2 no internet, …); "cancelled"
/// if the user declined the prompt.
#[tauri::command]
pub async fn install_cable() -> Result<u32, String> {
    #[cfg(windows)]
    {
        let r = tauri::async_runtime::spawn_blocking(crate::installer::install_cable_elevated)
            .await
            .map_err(|_| "error.internal".to_string())??;
        log::info!("install Discord's microphone from the app: {r:?}");
        r.ok_or_else(|| "cancelled".to_string())
    }
    #[cfg(not(windows))]
    Err("unsupported".into())
}

#[tauri::command]
pub async fn restart_discord() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(crate::discord::restart)
        .await
        .map_err(|_| "error.internal".to_string())?
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
    /// Added by the user in the mixer (can be removed).
    pub removable: bool,
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
        let Some(host) = g.as_ref() else {
            return Matrix {
                sources: vec![],
                outputs: vec![],
                cells: vec![],
            };
        };
        let graph = host.graph();
        let who = |p: &Party| match p {
            Party::Claude => "Claude".to_string(),
            Party::Discord => "Discord".to_string(),
            Party::User => "Du".to_string(),
            Party::App(a) => a.clone(),
            Party::Other => "?".to_string(),
        };
        let sources: Vec<Labeled> = graph
            .sources()
            .filter(|s| s.id < 900)
            .map(|s| Labeled {
                id: s.id,
                label: s.label.clone(),
                removable: crate::mixer::is_added_source(s.id),
            })
            .collect();
        let outputs: Vec<Labeled> = graph
            .outputs()
            .map(|o| Labeled {
                id: o.id,
                label: o.label.clone(),
                removable: crate::mixer::is_added_output(o.id),
            })
            .collect();
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
                cells.push(MatrixCell {
                    source: s.id,
                    output: o.id,
                    state: st,
                    gain_db: gain,
                    who: w,
                });
            }
        }
        Matrix {
            sources,
            outputs,
            cells,
        }
    }
    #[cfg(not(windows))]
    {
        let _ = state;
        Matrix {
            sources: vec![],
            outputs: vec![],
            cells: vec![],
        }
    }
}

#[tauri::command]
pub fn set_route(
    app: AppHandle,
    state: State<AppState>,
    source: u32,
    output: u32,
    enabled: bool,
    gain_db: f32,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        use ct_engine::graph::Route;
        // the external-Claude routes follow their own switch only
        if crate::mixer::imp::is_external(source, output) {
            return Err("error.internal".into());
        }
        let mut g = state.audio.lock().unwrap_or_else(|p| p.into_inner());
        let Some(host) = g.as_mut() else {
            return Err("error.internal".into());
        };
        if enabled {
            host.set_route(
                source,
                output,
                Route {
                    gain_db: gain_db.clamp(-60.0, 12.0),
                    muted: crate::audio::starts_muted(source, output),
                },
            )
            .map_err(|e| format!("error.{}", e.key()))?;
        } else {
            host.remove_route(source, output);
        }
        drop(g);
        crate::audio::apply_switches(&app);
        crate::mixer::route_changed(&state);
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
    if let Some(h) = state
        .audio
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_mut()
    {
        h.set_self_hearing_unlocked(unlocked);
    }
    #[cfg(not(windows))]
    let _ = (state, unlocked);
}

#[tauri::command]
pub async fn self_test(app: AppHandle) -> Result<Vec<crate::selftest::RouteResult>, String> {
    tauri::async_runtime::spawn_blocking(move || crate::selftest::run(&app))
        .await
        .map_err(|_| "error.internal".to_string())?
}
