//! Keeps Discord's microphone pointed at Claude Talk and checks that it
//! really is, outside any audio thread: whether Discord records is checked
//! every 0.5 s (the microphone opens for a call), the rest every 2 s.

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
            // Discord's processes when the routing took hold, and whether
            // Discord has restarted since: if it still records another mic
            // after a restart, its input is set to a specific device and a
            // further restart would not help (the guide is shown instead).
            let mut routed_pids: Vec<u32> = Vec::new();
            let mut restarted_since_route = false;
            let mut last_logged = (u8::MAX, false);
            let mut last_good: [Option<String>; 3] = [None, None, None];
            let mut tick = 0u32;
            while !state.shutting_down.load(Ordering::Relaxed) {
                if !tick.is_multiple_of(4) {
                    tick += 1;
                    let cable = policy::cable_capture_id();
                    let mic = sessions::discord_mic(&exes, cable.as_deref());
                    let recording = mic != sessions::DiscordMic::Unknown;
                    if state.discord_recording.swap(recording, Ordering::Relaxed) != recording {
                        crate::audio::refresh_mic(&app);
                    }
                    std::thread::sleep(Duration::from_millis(500));
                    continue;
                }
                tick += 1;
                guard_default_output(&state, &mut last_good);
                let enabled = state.config.lock().map(|c| c.route_discord).unwrap_or(true);
                let mut pids = policy::pids_of(&exes);
                pids.sort_unstable();
                let running = !pids.is_empty();
                if !enabled && running && policy::discord_routed_to_cable(&exes) == Some(true) {
                    // "Restore normal sound settings" was pressed while Discord was closed
                    let n = policy::restore_discord(&exes);
                    log::info!("Discord's own microphone restored now that it runs ({n})");
                }
                if enabled && running {
                    match policy::discord_routed_to_cable(&exes) {
                        Some(false) => match policy::route_discord_to_cable(&exes) {
                            Ok(n) if n > 0 => {
                                log::info!(
                                    "Discord's microphone set to Claude Talk ({n} processes)"
                                );
                                routed_since = Some(Instant::now());
                                routed_pids = pids.clone();
                                restarted_since_route = false;
                            }
                            Ok(_) => {}
                            Err(e) => log::warn!("could not set Discord's microphone: {e}"),
                        },
                        Some(true) => {
                            routed_since.get_or_insert_with(Instant::now);
                            if routed_pids.is_empty() {
                                routed_pids = pids.clone();
                            } else if routed_pids != pids {
                                routed_pids = pids.clone();
                                restarted_since_route = true;
                                routed_since = Some(Instant::now());
                            }
                        }
                        None => {}
                    }
                }
                if !running {
                    routed_since = None;
                }
                if !enabled {
                    routed_pids.clear();
                    restarted_since_route = false;
                }
                let cable = policy::cable_capture_id();
                let mic = sessions::discord_mic(&exes, cable.as_deref());
                let uses = match mic {
                    sessions::DiscordMic::Cable => YES,
                    sessions::DiscordMic::Other => NO,
                    sessions::DiscordMic::Unknown => UNKNOWN,
                };
                state.discord_uses_cable.store(uses, Ordering::Relaxed);
                state
                    .discord_recording
                    .store(mic != sessions::DiscordMic::Unknown, Ordering::Relaxed);
                crate::audio::refresh_mic(&app);
                follow_default_microphone(&state);
                // Windows says "cable" but Discord still records elsewhere for
                // a while: Discord picked a specific device or needs a restart.
                // If its device setting is "Default", a restart fixes it.
                let restart = uses == NO
                    && routed_since.is_some_and(|t| t.elapsed() > Duration::from_secs(6))
                    && enabled
                    && !restarted_since_route;
                state
                    .discord_needs_restart
                    .store(restart, Ordering::Relaxed);
                let speakers =
                    devices::resolve(&devices::DeviceSelector::Default(devices::Flow::Render))
                        .map(|(_, e)| {
                            e.form_factor.is_open_air()
                                && !ct_engine::win::host::is_own_device(&e.name)
                        })
                        .unwrap_or(false);
                if state.speakers.swap(speakers, Ordering::Relaxed) != speakers {
                    log::info!(
                        "default output is now {}",
                        if speakers {
                            "speakers"
                        } else {
                            "headphones/headset"
                        }
                    );
                    crate::audio::apply_switches(&app);
                }
                if (uses, restart) != last_logged {
                    log::info!(
                        "Discord microphone: {:?}{}",
                        mic,
                        if restart { " (restart suggested)" } else { "" }
                    );
                    last_logged = (uses, restart);
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        })
        .expect("spawn discord watcher");
}

/// Windows must never play into our cable (or, in Chrome/app mode, into the
/// extra cable): Discord would send everything the computer plays, its own
/// call included, to everyone, and the user would hear nothing. That
/// happens when someone picks "Claude Talk – intern" in the taskbar's sound
/// menu, or a driver install took a role. Put the last real device back,
/// role by role, and tell the user.
#[cfg(windows)]
fn guard_default_output(state: &crate::state::Inner, last_good: &mut [Option<String>; 3]) {
    use ct_engine::win::{devices, host, policy};
    let external = state
        .config
        .lock()
        .map(|c| c.external_claude.is_some())
        .unwrap_or(false);
    let extra = if external {
        host::extra_cable().map(|c| c.render.id)
    } else {
        None
    };
    let Ok(renders) = devices::list(devices::Flow::Render) else {
        return;
    };
    let bad = |id: &str| {
        extra.as_deref() == Some(id)
            || renders
                .iter()
                .any(|e| e.id == id && host::is_own_device(&e.name))
    };
    let virtual_name = |n: &str| {
        let n = n.to_lowercase();
        ["virtual", "cable", "voicemeeter"]
            .iter()
            .any(|p| n.contains(p))
    };
    let ids = devices::default_ids(devices::Flow::Render);
    for (role, id) in ids.iter().enumerate() {
        let Some(id) = id else { continue };
        if !bad(id) {
            last_good[role] = Some(id.clone());
            continue;
        }
        let back = last_good[role]
            .clone()
            .filter(|g| renders.iter().any(|e| &e.id == g) && !bad(g))
            .or_else(|| {
                renders
                    .iter()
                    .find(|e| !bad(&e.id) && !virtual_name(&e.name))
                    .map(|e| e.id.clone())
            });
        let Some(back) = back else { continue };
        match policy::set_default_endpoint_role(&back, role) {
            Ok(()) => {
                log::warn!("default output (role {role}) was our cable; set back to {back}");
                if let Ok(mut n) = state.output_fixed_at.lock() {
                    *n = Some(Instant::now());
                }
            }
            Err(e) => log::warn!("could not set the default output back: {e}"),
        }
    }
}

/// A new default microphone (headset plugged in, chosen in Windows): the
/// microphone stream moves to it.
#[cfg(windows)]
fn follow_default_microphone(state: &crate::state::Inner) {
    use ct_engine::graph::ids;
    use ct_engine::win::devices::{self, DeviceSelector, Flow, NOT_A_MICROPHONE};
    let g = state.audio.lock().unwrap_or_else(|p| p.into_inner());
    let Some(host) = g.as_ref() else { return };
    let current = host.source_device(ids::MIC);
    if current.is_empty() {
        return; // not running (paused, waiting) or not a device
    }
    let wanted = devices::resolve(&DeviceSelector::DefaultExcluding(
        Flow::Capture,
        devices::patterns(NOT_A_MICROPHONE),
    ))
    .map(|(_, e)| e.name)
    .unwrap_or_default();
    if !wanted.is_empty() && wanted != current {
        log::info!("microphone: default changed, moving from '{current}' to '{wanted}'");
        host.restart_source(ids::MIC);
    }
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

const MAIN_MUTEX: &str = "Local\\ClaudeTalkMain";

/// Marks this process as the running Claude Talk (the restore helper below
/// steps aside while it exists). Kept for the life of the program.
#[cfg(windows)]
pub fn mark_running() {
    use windows::core::HSTRING;
    use windows::Win32::System::Threading::CreateMutexW;
    // the handle is never closed: Windows closes it when the program ends
    let _ = unsafe { CreateMutexW(None, false, &HSTRING::from(MAIN_MUTEX)) };
}

#[cfg(windows)]
fn main_running() -> bool {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};
    match unsafe {
        OpenMutexW(
            SYNCHRONIZATION_SYNCHRONIZE,
            false,
            &HSTRING::from(MAIN_MUTEX),
        )
    } {
        Ok(h) => {
            let _ = unsafe { CloseHandle(h) };
            true
        }
        Err(_) => false,
    }
}

/// Claude Talk quit while Discord was closed: Windows' audio service keeps
/// Discord's microphone choice in memory and only lets it change while
/// Discord runs (K12). This small helper (`--restore-when-discord-starts`)
/// waits for Discord, gives it its own microphone back, and ends. It steps
/// aside if Claude Talk starts again, and gives up after 12 hours.
#[cfg(windows)]
pub fn restore_when_discord_starts() -> i32 {
    use ct_engine::win::host::discord_executables;
    use ct_engine::win::{policy, ComGuard};
    let _com = ComGuard::new();
    let exes = discord_executables();
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(12 * 3600) {
        if main_running() {
            return 0;
        }
        if !policy::pids_of(&exes).is_empty() {
            // Discord starts its audio process a little later
            for wait in [3u64, 10, 30] {
                std::thread::sleep(Duration::from_secs(wait));
                if main_running() {
                    return 0;
                }
                let n = policy::restore_discord(&exes);
                log::info!("restore helper: Discord's own microphone back ({n})");
            }
            return 0;
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    0
}

/// Starts the helper above, detached and without a window.
#[cfg(windows)]
pub fn spawn_restore_helper() {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    if let Ok(exe) = std::env::current_exe() {
        let r = std::process::Command::new(exe)
            .arg("--restore-when-discord-starts")
            .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
            .spawn();
        log::info!("restore helper started: {}", r.is_ok());
    }
}

/// Closes Discord and starts it again through its own launcher.
#[cfg(windows)]
pub fn restart() -> Result<(), String> {
    use ct_engine::win::{policy, process};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    let variants = [
        ("discord.exe", "Discord", "Discord.exe"),
        ("discordptb.exe", "DiscordPTB", "DiscordPTB.exe"),
        ("discordcanary.exe", "DiscordCanary", "DiscordCanary.exe"),
    ];
    let procs = process::snapshot();
    let Some((exe, folder, start)) = variants
        .iter()
        .find(|(e, _, _)| procs.iter().any(|p| p.exe == *e))
    else {
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
    let local = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .ok_or("error.internal")?;
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
