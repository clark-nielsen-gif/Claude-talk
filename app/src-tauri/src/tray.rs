//! System tray: the most important switches without opening the window.
use crate::state::AppState;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

const ID: &str = "main-tray";

fn t(lang: &str, key: &str) -> &'static str {
    let da = lang != "en";
    match key {
        "open" => {
            if da {
                "Åbn Claude Talk"
            } else {
                "Open Claude Talk"
            }
        }
        "claude" => {
            if da {
                "Åbn Claude-vinduet"
            } else {
                "Open the Claude window"
            }
        }
        "switch" => {
            if da {
                "Claude i Discord"
            } else {
                "Claude in Discord"
            }
        }
        "mute" => {
            if da {
                "Min mikrofon er slået fra"
            } else {
                "My microphone is off"
            }
        }
        "mute_claude" => {
            if da {
                "Claude hører ikke min mikrofon"
            } else {
                "Claude can't hear my microphone"
            }
        }
        "quit" => {
            if da {
                "Luk Claude Talk"
            } else {
                "Quit Claude Talk"
            }
        }
        _ => "",
    }
}

fn menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let cfg = app
        .state::<AppState>()
        .config
        .lock()
        .map(|c| c.clone())
        .unwrap_or_default();
    let l = cfg.language.as_str();
    Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open", t(l, "open"), true, None::<&str>)?,
            &MenuItem::with_id(app, "claude", t(l, "claude"), true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &CheckMenuItem::with_id(
                app,
                "switch",
                t(l, "switch"),
                true,
                cfg.claude_in_discord,
                None::<&str>,
            )?,
            &CheckMenuItem::with_id(
                app,
                "mute",
                // after "Gendan" Discord records the microphone itself: the
                // mute only keeps it from Claude (as the main window says)
                t(
                    l,
                    if cfg.route_discord {
                        "mute"
                    } else {
                        "mute_claude"
                    },
                ),
                true,
                cfg.mic_muted,
                None::<&str>,
            )?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", t(l, "quit"), true, None::<&str>)?,
        ],
    )
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let icon = app.default_window_icon().cloned();
    let mut b = TrayIconBuilder::with_id(ID)
        .menu(&menu(app)?)
        .tooltip("Claude Talk")
        .show_menu_on_left_click(false);
    if let Some(i) = icon {
        b = b.icon(i);
    }
    b.on_menu_event(|app, e| {
        let state = app.state::<AppState>();
        match e.id().as_ref() {
            "open" => crate::commands::show_main(app.clone()),
            "claude" => {
                let _ = crate::claude_window::open(app);
            }
            "switch" => {
                if let Ok(mut c) = state.config.lock() {
                    c.claude_in_discord = !c.claude_in_discord;
                    if c.claude_in_discord {
                        if !c.route_discord {
                            c.mic_muted = false; // (see set_config)
                        }
                        c.route_discord = true; // as in the main window
                    }
                }
                after_change(app);
            }
            "mute" => {
                if let Ok(mut c) = state.config.lock() {
                    c.mic_muted = !c.mic_muted;
                }
                after_change(app);
            }
            // off the main thread: the question dialog must not stop the
            // Claude window's audio, which the main thread carries
            "quit" => {
                let app = app.clone();
                std::thread::spawn(move || crate::request_quit(&app));
            }
            _ => {}
        }
    })
    .on_tray_icon_event(|tray, e| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = e
        {
            crate::commands::show_main(tray.app_handle().clone());
        }
    })
    .build(app)?;
    Ok(())
}

fn after_change(app: &AppHandle) {
    app.state::<AppState>().save_config();
    crate::audio::apply_switches(app);
    refresh(app);
    let cfg = app
        .state::<AppState>()
        .config
        .lock()
        .map(|c| c.clone())
        .unwrap_or_default();
    use tauri::Emitter;
    let _ = app.emit("config-changed", cfg);
}

/// Rebuilds the menu so check marks and language follow the settings.
pub fn refresh(app: &AppHandle) {
    if let (Some(tray), Ok(m)) = (app.tray_by_id(ID), menu(app)) {
        let _ = tray.set_menu(Some(m));
    }
}
