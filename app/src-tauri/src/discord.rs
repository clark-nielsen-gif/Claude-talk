//! Keeps Discord's microphone pointed at Claude Talk and checks that it
//! really is: every 2 s, outside any audio thread.

use crate::state::AppState;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// `uses_cable` values shared with the status loop.
pub const UNKNOWN: u8 = 0;
pub const YES: u8 = 1;
pub const NO: u8 = 2;

#[cfg(windows)]
pub fn spawn_watcher(app: AppHandle) {
    use ct_engine::win::host::discord_executables;
    use ct_engine::win::{devices, policy, sessions, ComGuard};
    std::thread::Builder::new()
        .name("discord-watch".into())
        .spawn(move || {
            let _com = ComGuard::new();
            let exes = discord_executables();
            let state = app.state::<AppState>().inner().clone();
            let mut routed_since: Option<Instant> = None;
            let mut last_logged = (u8::MAX, false);
            while !state.shutting_down.load(Ordering::Relaxed) {
                let enabled = state.config.lock().map(|c| c.route_discord).unwrap_or(true);
                let running = !policy::pids_of(&exes).is_empty();
                if enabled && running {
                    match policy::discord_routed_to_cable(&exes) {
                        Some(false) => match policy::route_discord_to_cable(&exes) {
                            Ok(n) if n > 0 => {
                                log::info!("Discord's microphone set to Claude Talk ({n} processes)");
                                routed_since = Some(Instant::now());
                            }
                            Ok(_) => {}
                            Err(e) => log::warn!("could not set Discord's microphone: {e}"),
                        },
                        Some(true) => {
                            routed_since.get_or_insert_with(Instant::now);
                        }
                        None => {}
                    }
                }
                if !running {
                    routed_since = None;
                }
                let cable = policy::cable_capture_id();
                let mic = sessions::discord_mic(&exes, cable.as_deref());
                let uses = match mic {
                    sessions::DiscordMic::Cable => YES,
                    sessions::DiscordMic::Other => NO,
                    sessions::DiscordMic::Unknown => UNKNOWN,
                };
                state.discord_uses_cable.store(uses, Ordering::Relaxed);
                // Windows says "cable" but Discord still records elsewhere for
                // a while: Discord picked a specific device or needs a restart.
                // If its device setting is "Default", a restart fixes it.
                let restart = uses == NO && routed_since.is_some_and(|t| t.elapsed() > Duration::from_secs(6)) && enabled;
                state.discord_needs_restart.store(restart, Ordering::Relaxed);
                let speakers = devices::resolve(&devices::DeviceSelector::Default(devices::Flow::Render))
                    .map(|(_, e)| e.form_factor.is_open_air() && !ct_engine::win::host::is_own_device(&e.name))
                    .unwrap_or(false);
                if state.speakers.swap(speakers, Ordering::Relaxed) != speakers {
                    log::info!("default output is now {}", if speakers { "speakers" } else { "headphones/headset" });
                    crate::audio::apply_switches(&app);
                }
                if (uses, restart) != last_logged {
                    log::info!("Discord microphone: {:?}{}", mic, if restart { " (restart suggested)" } else { "" });
                    last_logged = (uses, restart);
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        })
        .expect("spawn discord watcher");
}

#[cfg(not(windows))]
pub fn spawn_watcher(_app: AppHandle) {}

/// Restores Discord's own microphone (button, and the uninstaller via
/// `--restore-audio`). Returns how many settings were cleared.
#[cfg(windows)]
pub fn restore() -> usize {
    let _com = ct_engine::win::ComGuard::new();
    let n = ct_engine::win::policy::restore_discord(&ct_engine::win::host::discord_executables());
    log::info!("restored Discord's microphone ({n} settings cleared)");
    n
}

#[cfg(not(windows))]
pub fn restore() -> usize {
    0
}

/// Closes Discord and starts it again through its own launcher.
#[cfg(windows)]
pub fn restart() -> Result<(), String> {
    use ct_engine::win::{policy, process};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    let variants = [("discord.exe", "Discord", "Discord.exe"), ("discordptb.exe", "DiscordPTB", "DiscordPTB.exe"), ("discordcanary.exe", "DiscordCanary", "DiscordCanary.exe")];
    let procs = process::snapshot();
    let Some((exe, folder, start)) = variants.iter().find(|(e, _, _)| procs.iter().any(|p| p.exe == *e)) else {
        return Err("error.discordNotRunning".into());
    };
    for pid in policy::pids_of(&[exe.to_string()]) {
        unsafe {
            if let Ok(h) = OpenProcess(PROCESS_TERMINATE, false, pid) {
                let _ = TerminateProcess(h, 0);
                let _ = CloseHandle(h);
            }
        }
    }
    std::thread::sleep(Duration::from_millis(1200));
    let local = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from).ok_or("error.internal")?;
    let updater = local.join(folder).join("Update.exe");
    std::process::Command::new(updater)
        .args(["--processStart", start])
        .spawn()
        .map(|_| ())
        .map_err(|e| {
            log::warn!("restart Discord: {e}");
            "error.discordRestart".to_string()
        })
}

#[cfg(not(windows))]
pub fn restart() -> Result<(), String> {
    Err("error.internal".into())
}
