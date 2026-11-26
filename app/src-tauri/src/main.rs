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

/// The first time the window is closed: say that Claude Talk keeps running.
fn first_close_hint(app: &AppHandle) {
    let st = app.state::<AppState>();
    let da = {
        let Ok(mut c) = st.config.lock() else { return };
        if c.close_hint_shown {
            return;
        }
        c.close_hint_shown = true;
        c.language != "en"
    };
    st.save_config();
    #[cfg(windows)]
    std::thread::spawn(move || {
        use windows::core::HSTRING;
        use windows::Win32::UI::WindowsAndMessaging::{
            MessageBoxW, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND,
        };
        let (title, text) = if da {
            (
                "Claude Talk kører videre",
                "Claude Talk kører videre nede ved uret, så Discord stadig kan høre dig og Claude.\n\nKlik på ikonet ved uret for at åbne vinduet igen. Vil du lukke helt, så højreklik på ikonet og vælg »Luk Claude Talk«.",
            )
        } else {
            (
                "Claude Talk keeps running",
                "Claude Talk keeps running by the clock, so Discord can still hear you and Claude.\n\nClick the icon by the clock to open the window again. To quit, right-click the icon and choose “Quit Claude Talk”.",
            )
        };
        unsafe {
            MessageBoxW(
                None,
                &HSTRING::from(text),
                &HSTRING::from(title),
                MB_OK | MB_ICONINFORMATION | MB_SETFOREGROUND,
            )
        };
    });
    #[cfg(not(windows))]
    let _ = da;
}

/// Creates the (hidden) main window from its configuration.
pub fn create_main_window(app: &AppHandle) -> tauri::Result<()> {
    if let Some(cfg) = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .cloned()
    {
        tauri::WebviewWindowBuilder::from_config(app, &cfg)?.build()?;
    }
    Ok(())
}

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
    // one line per string: no source indentation inside the dialog text
    let (title, text) = if da {
        (
            "Luk Claude Talk",
            concat!(
                "Du er i et opkald i Discord, og Discord bruger Claude Talk som mikrofon.\n\n",
                "Lukker du Claude Talk nu, kan de andre ikke høre dig, før Discord starter igen.\n\n",
                "Ja: Luk, og genstart Discord. Du skal selv gå ind i opkaldet igen bagefter.\n\n",
                "Nej: Luk uden at genstarte Discord. De andre kan ikke høre dig, før du selv genstarter Discord.\n\n",
                "Annuller: Luk ikke. Bliv i opkaldet med Claude Talk.",
            ),
        )
    } else {
        (
            "Quit Claude Talk",
            concat!(
                "You're in a Discord call, and Discord uses Claude Talk as its microphone.\n\n",
                "If you quit Claude Talk now, nobody can hear you until Discord starts again.\n\n",
                "Yes: Quit and restart Discord. Join the call again yourself afterwards.\n\n",
                "No: Quit without restarting Discord. Nobody can hear you until you restart Discord yourself.\n\n",
                "Cancel: Don't quit. Stay in the call with Claude Talk.",
            ),
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
        // Discord closed: its choice can only be put back once it runs again
        let routed = st.config.lock().map(|c| c.route_discord).unwrap_or(true);
        let discord_running =
            !ct_engine::win::policy::pids_of(&ct_engine::win::host::discord_executables())
                .is_empty();
        if routed && !discord_running {
            discord::spawn_restore_helper();
        }
    }
    log::info!("Claude Talk quit");
    log::logger().flush();
}

fn finish(app: &AppHandle) {
    app.exit(0);
}

fn main() {
    // Run elevated by the NSIS installer / uninstaller (logs in an
    // admin-only folder, never the user's %APPDATA%).
    #[cfg(windows)]
    for (flag, f) in [
        ("--install-cable", installer::install_cable as fn() -> i32),
        ("--uninstall-cleanup", installer::uninstall_cleanup),
        (
            "--restore-when-discord-starts",
            discord::restore_when_discord_starts,
        ),
    ] {
        if std::env::args().any(|a| a == flag) {
            // (the uninstaller removes that folder, so it logs nowhere)
            if flag == "--install-cable" {
                logging::init_in(installer::elevated_log_dir());
            } else if flag == "--restore-when-discord-starts" {
                logging::init(); // runs as the user
            }
            let code = f();
            log::logger().flush();
            std::process::exit(code);
        }
    }
    logging::init();
    let config = config::Config::load(&paths::config_file());
    let started_minimized = std::env::args().any(|a| a == "--minimized");
    let state = Inner::new(config);

    let builder = tauri::Builder::default()
        // a second start just shows the running instance
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            commands::show_main(app.clone());
        }))
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
            #[cfg(windows)]
            discord::mark_running();
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
            // microphone should not wait for it. Started with Windows
            // (in the tray), it is not created at all until it is opened:
            // no WebView2 processes, memory or CPU while nobody looks.
            if !started_minimized {
                commands::show_main(handle.clone());
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
                    first_close_hint(window.app_handle());
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
