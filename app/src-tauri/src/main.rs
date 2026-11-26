//! Claude Talk – bring Claude's voice into your Discord calls, without echo.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod autostart;
mod claude_window;
mod commands;
mod config;
mod discord;
mod hotkeys;
mod installer;
mod logging;
mod mixer;
mod paths;
mod report;
mod selftest;
mod state;
mod tray;
mod updates;

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
        // Nothing feeds Discord's microphone any more: give Discord its own
        // microphone back (the next start points it at Claude Talk again).
        discord::restore();
    }
    log::info!("Claude Talk quit");
    log::logger().flush();
    app.exit(0);
}

fn main() {
    logging::init();
    // Run elevated by the NSIS installer / uninstaller.
    #[cfg(windows)]
    for (flag, f) in [
        ("--install-cable", installer::install_cable as fn() -> i32),
        ("--uninstall-cleanup", installer::uninstall_cleanup),
    ] {
        if std::env::args().any(|a| a == flag) {
            let code = f();
            log::logger().flush();
            std::process::exit(code);
        }
    }
    let config = config::Config::load(&paths::config_file());
    let started_minimized = std::env::args().any(|a| a == "--minimized");
    let state = Inner::new(config);

    let builder = tauri::Builder::default()
        // a second start just shows the running instance
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            commands::show_main(app.clone());
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .manage(state);
    #[allow(unused_mut)]
    let mut context = tauri::generate_context!();
    // The updater only exists in builds made with an update-signing key; its
    // configuration (key + manifest URL) is set here, not in tauri.conf.json,
    // because the plugin refuses to start without one.
    #[cfg(feature = "updater")]
    let builder = match updates::pubkey() {
        Some(key) => {
            context.config_mut().plugins.0.insert(
                "updater".into(),
                serde_json::json!({ "pubkey": key, "endpoints": [updates::ENDPOINT] }),
            );
            builder.plugin(tauri_plugin_updater::Builder::new().build())
        }
        None => builder,
    };
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
            commands::install_cable,
            commands::send_instructions,
            commands::instruction_text,
            mixer::mixer_choices,
            mixer::add_mixer_source,
            mixer::remove_mixer_source,
            mixer::add_mixer_output,
            mixer::remove_mixer_output,
            mixer::save_preset,
            mixer::load_preset,
            mixer::delete_preset,
            mixer::set_external_claude,
            commands::get_matrix,
            commands::set_route,
            commands::set_unlocked,
            commands::self_test,
            updates::install_update,
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
            updates::check_in_background(handle.clone());
            let autostart = handle.state::<AppState>().config.lock().map(|c| c.autostart).unwrap_or(true);
            autostart::apply(&handle, autostart);
            #[cfg(feature = "ci-testpage")]
            if let Ok(v) = std::env::var("CLAUDE_TALK_TEST_OPEN_CLAUDE") {
                // the value is a delay in ms ("1": right away)
                let delay = v.parse::<u64>().unwrap_or(1);
                let h = handle.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(delay));
                    let h2 = h.clone();
                    let _ = h.run_on_main_thread(move || {
                        let _ = claude_window::open(&h2);
                    });
                });
            }
            // The main window is created here, after the sound is running:
            // its WebView2 can take seconds on a first start, and Discord's
            // microphone should not wait for it.
            if let Some(cfg) = app.config().app.windows.iter().find(|w| w.label == "main").cloned() {
                let w = tauri::WebviewWindowBuilder::from_config(&handle, &cfg)?.build()?;
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
        .run(context)
        .expect("error while running Claude Talk");
}
