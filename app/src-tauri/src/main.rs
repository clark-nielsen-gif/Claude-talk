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

/// Tray "Quit" and the quit command. In a call, Discord would keep
/// recording our (now silent) cable until it restarts, so ask first.
pub fn request_quit(app: &AppHandle) {
    #[cfg(windows)]
    {
        let st = app.state::<AppState>();
        let in_call = st.discord_recording.load(Ordering::Relaxed)
            && st.discord_uses_cable.load(Ordering::Relaxed) == discord::YES;
        if in_call {
            let da = st.config.lock().map(|c| c.language != "en").unwrap_or(true);
            match ask_restart_discord(da) {
                QuitChoice::Cancel => return,
                QuitChoice::RestartDiscord => {
                    shutdown_inner(app);
                    if let Err(e) = discord::restart() {
                        log::warn!("could not restart Discord on quit: {e}");
                    }
                    finish(app);
                    return;
                }
                QuitChoice::Quit => {}
            }
        }
    }
    shutdown(app);
}

#[cfg(windows)]
enum QuitChoice {
    RestartDiscord,
    Quit,
    Cancel,
}

#[cfg(windows)]
fn ask_restart_discord(da: bool) -> QuitChoice {
    use windows::core::HSTRING;
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDCANCEL, IDYES, MB_ICONWARNING, MB_SETFOREGROUND, MB_TOPMOST, MB_YESNOCANCEL,
    };
    let (title, text) = if da {
        (
            "Luk Claude Talk",
            "Du er i et opkald i Discord, og Discord bruger Claude Talk som mikrofon.

             Hvis du lukker Claude Talk nu, kan de andre ikke høre dig, før Discord              starter igen.

Ja: Luk, og genstart Discord (opkaldet afbrydes kort).
             Nej: Luk uden at genstarte Discord.
Annuller: Bliv i opkaldet med Claude Talk.",
        )
    } else {
        (
            "Quit Claude Talk",
            "You're in a Discord call, and Discord uses Claude Talk as its microphone.

             If you quit Claude Talk now, nobody can hear you until Discord restarts.

             Yes: Quit and restart Discord (the call drops for a moment).
             No: Quit without restarting Discord.
Cancel: Stay in the call with Claude Talk.",
        )
    };
    let r = unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(text),
            &HSTRING::from(title),
            MB_YESNOCANCEL | MB_ICONWARNING | MB_TOPMOST | MB_SETFOREGROUND,
        )
    };
    if r == IDYES {
        QuitChoice::RestartDiscord
    } else if r == IDCANCEL {
        QuitChoice::Cancel
    } else {
        QuitChoice::Quit
    }
}

/// Stops audio and exits.
pub fn shutdown(app: &AppHandle) {
    shutdown_inner(app);
    finish(app);
}

/// Stops audio and gives Discord its own microphone back. Safe to call more
/// than once (also runs when Windows ends the session).
fn shutdown_inner(app: &AppHandle) {
    let st = app.state::<AppState>();
    if st.shutting_down.swap(true, Ordering::Relaxed) {
        return;
    }
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
}

fn finish(app: &AppHandle) {
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
        .build(context)
        .expect("error while building Claude Talk")
        .run(|app, event| {
            // Windows ends the session (sign-out, shutdown) or the app exits
            // any other way: Discord gets its own microphone back.
            if let tauri::RunEvent::Exit = event {
                shutdown_inner(app);
            }
        });
}
