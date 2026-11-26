//! Claude Talk – bring Claude's voice into your Discord calls, without echo.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod autostart;
mod claude_window;
mod commands;
mod config;
mod discord;
mod hotkeys;
mod logging;
mod paths;
mod report;
mod selftest;
mod state;
mod tray;

use state::{AppState, Inner};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, WindowEvent};

/// Stops audio and exits (tray "Quit" and the quit command).
pub fn shutdown(app: &AppHandle) {
    let st = app.state::<AppState>();
    st.shutting_down.store(true, Ordering::Relaxed);
    #[cfg(windows)]
    {
        // dropping the host stops and joins every audio thread
        let host = st.audio.lock().ok().and_then(|mut g| g.take());
        drop(host);
    }
    log::info!("Claude Talk quit");
    log::logger().flush();
    app.exit(0);
}

fn main() {
    logging::init();
    // Used by the uninstaller: give Discord its own microphone back, then exit.
    if std::env::args().any(|a| a == "--restore-audio") {
        discord::restore();
        log::logger().flush();
        return;
    }
    let config = config::Config::load(&paths::config_file());
    let started_minimized = std::env::args().any(|a| a == "--minimized");
    let state = Inner::new(config);

    let builder = tauri::Builder::default()
        // a second start just shows the running instance
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            commands::show_main(app.clone());
        }))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .manage(state);
    #[cfg(feature = "updater")]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    builder
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_config,
            commands::set_config,
            commands::open_claude,
            commands::open_place,
            commands::save_report,
            commands::quit,
            commands::show_main,
            commands::restore_audio_settings,
            commands::restart_discord,
            commands::get_matrix,
            commands::set_route,
            commands::set_unlocked,
            commands::self_test,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            tray::create(&handle)?;
            let info = commands::app_info();
            if info.supported {
                audio::start(&handle);
                discord::spawn_watcher(handle.clone());
            } else {
                log::warn!("Windows build {} lacks process loopback; showing the 'needs Windows 11' screen", info.windows_build);
            }
            if let Err(e) = hotkeys::register(&handle) {
                log::warn!("hotkeys: {e}");
            }
            let autostart = handle.state::<AppState>().config.lock().map(|c| c.autostart).unwrap_or(true);
            autostart::apply(&handle, autostart);
            if let Some(w) = app.get_webview_window("main") {
                if !started_minimized {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the main window keeps Claude Talk running in the tray
            // (Discord's microphone depends on it).
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Claude Talk");
}
