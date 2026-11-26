//! Global hotkeys: mute "Claude → Discord", mute my microphone, and hold-to-
//! talk to Claude. Changeable in Settings.
use crate::state::AppState;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub fn register(app: &AppHandle) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let hk = app.state::<AppState>().config.lock().map(|c| c.hotkeys.clone()).unwrap_or_default();
    let mut failed = false;
    for (accel, action) in [(hk.mute_claude_to_discord, "muteClaude"), (hk.mute_mic, "muteMic"), (hk.push_to_talk_claude, "ptt")] {
        let Ok(sc) = accel.parse::<Shortcut>() else {
            failed = true;
            continue;
        };
        let action = action.to_string();
        if gs
            .on_shortcut(sc, move |app, _sc, ev| handle(app, &action, ev.state()))
            .is_err()
        {
            log::warn!("hotkey {accel} is in use by another program");
            failed = true;
        }
    }
    if failed {
        Err("error.hotkeyTaken".into())
    } else {
        Ok(())
    }
}

fn handle(app: &AppHandle, action: &str, st: ShortcutState) {
    let state = app.state::<AppState>();
    match (action, st) {
        ("muteClaude", ShortcutState::Pressed) => {
            if let Ok(mut c) = state.config.lock() {
                c.claude_to_discord_muted = !c.claude_to_discord_muted;
            }
        }
        ("muteMic", ShortcutState::Pressed) => {
            if let Ok(mut c) = state.config.lock() {
                c.mic_muted = !c.mic_muted;
            }
        }
        ("ptt", s) => state.ptt_down.store(s == ShortcutState::Pressed, Ordering::Relaxed),
        _ => return,
    }
    if action != "ptt" {
        state.save_config();
        crate::tray::refresh(app);
    }
    crate::audio::apply_switches(app);
    let cfg = state.config.lock().map(|c| c.clone()).unwrap_or_default();
    let _ = app.emit("config-changed", cfg);
}
