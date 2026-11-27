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
    std::thread::Builder::new()
        .name("discord-watch".into())
        .spawn(move || {
            // a panic here would silently stop the routing and both guards:
            // log it and start the loop again
            loop {
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| watch(&app)));
                if r.is_ok() {
                    break;
                }
                log::error!("the Discord watcher stopped after a panic; starting it again");
                std::thread::sleep(Duration::from_secs(2));
            }
        })
        .expect("spawn discord watcher");
}

/// Whether Discord records (is in a call), seen by any of the watcher's
/// checks: one place for what follows a change. A call that starts while
/// "Slå min mikrofon fra" is on (pressed in an earlier call, perhaps days
/// ago, with the window hidden) would be silent with nothing in Discord
/// showing why: the window comes forward and says so (D32).
#[cfg(windows)]
fn recording_seen(
    app: &AppHandle,
    state: &crate::state::Inner,
    mic: &ct_engine::win::sessions::DiscordMic,
) {
    use ct_engine::win::sessions;
    let recording = *mic != sessions::DiscordMic::Unknown;
    let muted_for_discord = state
        .config
        .lock()
        .map(|c| c.mic_muted && c.route_discord)
        .unwrap_or(false);
    // unmuted: the next muted call is told again; the notice goes
    if !muted_for_discord {
        state.muted_call_told.store(false, Ordering::Relaxed);
        if let Ok(mut t) = state.muted_call_at.lock() {
            *t = None;
        }
    }
    let mut since = state
        .not_recording_since
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if !recording && since.is_none() {
        *since = Some(Instant::now());
    }
    // a new call: Discord had not recorded for 5 s (a short gap is Discord
    // reopening its microphone, or a failed look, within the same call)
    let new_call = recording && since.is_none_or(|t| t.elapsed() >= Duration::from_secs(5));
    if recording {
        *since = None;
    }
    drop(since);
    if state.discord_recording.swap(recording, Ordering::Relaxed) != recording {
        crate::audio::refresh_mic(app);
    }
    // once per mute, and only when the mute really silences the user in
    // Discord (Discord records Claude Talk's microphone)
    if new_call
        && muted_for_discord
        && *mic == sessions::DiscordMic::Cable
        && !state.muted_call_told.swap(true, Ordering::Relaxed)
    {
        log::info!("a call started while the microphone is muted: showing the window");
        if let Ok(mut t) = state.muted_call_at.lock() {
            *t = Some(Instant::now());
        }
        // (its own thread: the first window can take seconds to build, and
        // the watcher keeps guarding meanwhile; shown without taking focus,
        // so a key meant for another program presses nothing here)
        let app = app.clone();
        std::thread::spawn(move || {
            crate::commands::show_main_quietly(&app);
            use tauri::Emitter;
            let _ = app.emit("muted-call", ());
        });
    }
}

#[cfg(windows)]
fn watch(app: &AppHandle) {
    use ct_engine::win::host::discord_executables;
    use ct_engine::win::{devices, policy, sessions, ComGuard};
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
    let mut guard_out = DefaultGuard::load(devices::Flow::Render);
    let mut guard_in = DefaultGuard::load(devices::Flow::Capture);
    let mut tick = 0u32;
    while !state.shutting_down.load(Ordering::Relaxed) {
        if !tick.is_multiple_of(4) {
            tick += 1;
            let cable = policy::cable_capture_id();
            let mic = sessions::discord_mic(&exes, cable.as_deref());
            recording_seen(app, &state, &mic);
            std::thread::sleep(Duration::from_millis(500));
            continue;
        }
        tick += 1;
        guard_default_output(&state, &mut guard_out);
        guard_default_input(&state, &mut guard_in);
        let enabled = state.config.lock().map(|c| c.route_discord).unwrap_or(true);
        let mut pids = policy::pids_of(&exes);
        pids.sort_unstable();
        let running = !pids.is_empty();
        // Not routing (after "Gendan"): Discord keeps the user's
        // microphone – also after it was closed when "Gendan" was
        // pressed, or updated itself (a new version has no setting)
        if !enabled && running && tick % 5 == 1 {
            let on_cable = policy::discord_routed_to_cable(&exes) == Some(true);
            let now = policy::discord_input_now(&exes).unwrap_or_default();
            let last_ours = policy::last_given_back().is_some_and(|l| l.eq_ignore_ascii_case(&now));
            let roots = discord_roots(&exes);
            let as_wanted = policy::discord_input_as_wanted(&exes);
            // an empty value: a new Discord (updated, restarted) has
            // none yet – filled until it takes, once per Discord start;
            // a user who clears it after that is not overridden
            let fresh_empty = now.is_empty() && !filled_for(&roots) && as_wanted == Some(false);
            // the value Claude Talk wrote last, no longer the one to
            // give (the user chose another microphone in Claude Talk
            // meanwhile); an older one the user picked again in
            // Windows is theirs
            let stale_ours = as_wanted == Some(false) && (last_ours || policy::is_forgotten(&now));
            if on_cable || fresh_empty || stale_ours {
                let n = restore();
                log::info!("Discord's own microphone given back now that it runs ({n})");
            } else if !now.is_empty()
                && !last_ours
                && !policy::is_forgotten(&now)
                && policy::discord_own_input().as_deref() != Some(&now)
                && is_a_real_microphone(&now)
            {
                // the user picked Discord's input in Windows: theirs
                log::info!("Discord's input was changed in Windows; kept as the user's own");
                policy::set_discord_own_input(Some(now.clone()));
                policy::note_users_own_value(&now);
                if let Ok(mut c) = state.config.lock() {
                    c.discord_own_input = Some(now);
                }
                state.save_config();
            }
        }
        if enabled && running {
            match policy::discord_routed_to_cable(&exes) {
                // (not while Claude Talk quits: shutdown has just
                // given Discord its own microphone back)
                Some(false)
                    if !state.shutting_down.load(Ordering::Relaxed)
                        && !GAVE_UP.load(Ordering::Relaxed) =>
                {
                    match policy::route_discord_to_cable(&exes) {
                        // Claude Talk began quitting, gave up after a
                        // panic or was set back with "Gendan"
                        // meanwhile: undo
                        Ok(n)
                            if n > 0
                                && (state.shutting_down.load(Ordering::Relaxed)
                                    || GAVE_UP.load(Ordering::Relaxed)
                                    || !state
                                        .config
                                        .lock()
                                        .map(|c| c.route_discord)
                                        .unwrap_or(true)) =>
                        {
                            restore();
                        }
                        Ok(n) if n > 0 => {
                            log::info!("Discord's microphone set to Claude Talk ({n} processes)");
                            // keep the user's own per-app choice to give back
                            let own = policy::discord_own_input();
                            let changed = state
                                .config
                                .lock()
                                .map(|mut c| {
                                    let changed = c.discord_own_input != own;
                                    c.discord_own_input = own.clone();
                                    changed
                                })
                                .unwrap_or(false);
                            if changed {
                                state.save_config();
                            }
                            // the microphone set for Discord in Windows
                            // is the user's voice too (D28)
                            if let Some(id) = own.as_deref().and_then(endpoint_of) {
                                adopt_discords_microphone(app, &state, &id);
                            }
                            routed_since = Some(Instant::now());
                            routed_pids = discord_roots(&exes);
                            restarted_since_route = false;
                        }
                        Ok(_) => {}
                        Err(e) => log::warn!("could not set Discord's microphone: {e}"),
                    }
                }
                Some(true) => {
                    routed_since.get_or_insert_with(Instant::now);
                    // Discord's main process: helper processes come
                    // and go without Discord restarting
                    let roots = discord_roots(&exes);
                    if routed_pids.is_empty() {
                        routed_pids = roots;
                    } else if routed_pids != roots {
                        routed_pids = roots;
                        restarted_since_route = true;
                        routed_since = Some(Instant::now());
                    }
                }
                _ => {}
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
        state
            .cable_capture_ok
            .store(cable.is_some(), Ordering::Relaxed);
        let mic = sessions::discord_mic(&exes, cable.as_deref());
        let uses = match &mic {
            sessions::DiscordMic::Cable => YES,
            sessions::DiscordMic::Other(_) => NO,
            sessions::DiscordMic::Unknown => UNKNOWN,
        };
        if uses == YES {
            // it worked: a later problem may need a restart again
            restarted_since_route = false;
        }
        // Not routing, Windows' value for Discord is not the cable,
        // yet Discord records it: Discord itself is set to "Claude
        // Talk – til Discord" (or still runs from before, K12). Once
        // Claude Talk is gone, nobody would hear the user.
        let on_cable_anyway =
            !enabled && uses == YES && policy::discord_routed_to_cable(&exes) == Some(false);
        // the same Discord as when its microphone was given back: it
        // still records what it opened before (K12) – a restart
        // fixes that; a Discord started since then chose the cable
        let same_discord = {
            let r = RESTORED_FOR.lock().unwrap_or_else(|p| p.into_inner());
            !r.is_empty() && *r == discord_roots(&exes)
        };
        state
            .discord_cable_chosen
            .store(on_cable_anyway && !same_discord, Ordering::Relaxed);
        state
            .discord_still_on_cable
            .store(on_cable_anyway && same_discord, Ordering::Relaxed);
        if let sessions::DiscordMic::Other(endpoint) = &mic {
            if enabled {
                adopt_discords_microphone(app, &state, endpoint);
            }
        }
        state.discord_uses_cable.store(uses, Ordering::Relaxed);
        recording_seen(app, &state, &mic);
        crate::audio::refresh_mic(app);
        follow_default_microphone(&state);
        // Windows says "cable" but Discord still records elsewhere for
        // a while: Discord picked a specific device or needs a restart.
        // If its device setting is "Default" (it records Windows'
        // default microphone), a restart fixes it. A microphone picked
        // in Discord itself stays after a restart: show the guide.
        let on_default = match &mic {
            sessions::DiscordMic::Other(endpoint) => {
                let e = endpoint.to_lowercase();
                devices::default_ids(devices::Flow::Capture)
                    .iter()
                    .flatten()
                    .any(|d| d.to_lowercase() == e)
                    || policy::discord_own_input()
                        .is_some_and(|own| own.to_lowercase().contains(&e))
                    // what Claude Talk gave Discord itself (D28)
                    || policy::discord_given_back()
                        .iter()
                        .any(|g| g.to_lowercase().contains(&e))
            }
            _ => false,
        };
        let restart = uses == NO
            && on_default
            && routed_since.is_some_and(|t| t.elapsed() > Duration::from_secs(6))
            && enabled
            && !restarted_since_route;
        state
            .discord_needs_restart
            .store(restart, Ordering::Relaxed);
        // Discord plays on its own device (chosen in Discord), the
        // Claude window on Windows' default: the user would hear
        // Claude somewhere else, or not at all (a muted monitor)
        let default_out =
            devices::resolve(&devices::DeviceSelector::Default(devices::Flow::Render))
                .ok()
                .map(|(_, e)| e);
        let discord_out = sessions::sessions(devices::Flow::Render)
            .unwrap_or_default()
            .into_iter()
            .find(|s| {
                s.active
                    && exes.iter().any(|x| x.eq_ignore_ascii_case(&s.exe))
                    && !ct_engine::win::host::is_own_device(&s.endpoint_name)
            });
        let mismatch = match (&discord_out, &default_out) {
            (Some(d), Some(def)) if !d.endpoint_id.eq_ignore_ascii_case(&def.id) => Some((
                d.endpoint_id.clone(),
                d.endpoint_name.clone(),
                def.name.clone(),
            )),
            _ => None,
        };
        if let Ok(mut m) = state.output_mismatch.lock() {
            *m = mismatch;
        }
        let speakers = devices::resolve(&devices::DeviceSelector::Default(devices::Flow::Render))
            .map(|(_, e)| {
                e.form_factor.is_open_air() && !ct_engine::win::host::is_own_device(&e.name)
            })
            .unwrap_or(false);
        // speakers added as an output in Advanced: Discord or Claude
        // played there reaches the microphone just the same
        let added: Vec<String> = state
            .config
            .lock()
            .map(|c| {
                c.mixer
                    .outputs
                    .iter()
                    .map(|o| o.device_id.clone())
                    .collect()
            })
            .unwrap_or_default();
        let added_speakers = !added.is_empty()
            && devices::list(devices::Flow::Render)
                .map(|l| {
                    l.iter()
                        .any(|e| added.contains(&e.id) && e.form_factor.is_open_air())
                })
                .unwrap_or(false);
        if state.added_speakers.swap(added_speakers, Ordering::Relaxed) != added_speakers {
            log::info!("speakers among the added outputs: {added_speakers}");
            crate::audio::apply_switches(app);
        }
        if state.speakers.swap(speakers, Ordering::Relaxed) != speakers {
            log::info!(
                "default output is now {}",
                if speakers {
                    "speakers"
                } else {
                    "headphones/headset"
                }
            );
            crate::audio::apply_switches(app);
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
}

/// Windows must never play into our cable (or, in Chrome/app mode, into the
/// extra cable): Discord would send everything the computer plays, its own
/// call included, to everyone, and the user would hear nothing. That
/// happens when someone picks "Claude Talk – intern" in the taskbar's sound
/// menu, or a driver install took a role. Put the last real device back,
/// role by role, and tell the user.
#[cfg(windows)]
fn guard_default_output(state: &crate::state::Inner, guard: &mut DefaultGuard) {
    guard_default(state, ct_engine::win::devices::Flow::Render, guard);
}

/// The same for recording: if Windows made "Claude Talk – til Discord" the
/// default microphone (no other microphone at install, a headset unplugged),
/// every program on "Default" – Discord too once Claude Talk lets go – would
/// record the cable (D31).
#[cfg(windows)]
fn guard_default_input(state: &crate::state::Inner, guard: &mut DefaultGuard) {
    guard_default(state, ct_engine::win::devices::Flow::Capture, guard);
}

/// Per role: the user's last real default device, and – after Claude Talk
/// had to pick another one because the cable had become the default while
/// the user's device was gone – the device it picked (the stand-in) and the
/// user's device to go back to when it returns. Kept in a file, so a restart
/// in between (overnight, an update) doesn't make the stand-in stick.
#[cfg(windows)]
#[derive(Default)]
pub struct DefaultGuard {
    last_good: [Option<String>; 3],
    stand_in: [Option<StandIn>; 3],
}

#[cfg(windows)]
#[derive(Clone, PartialEq)]
struct StandIn {
    id: String,
    wanted: String,
}

#[cfg(windows)]
pub fn stand_in_file() -> std::path::PathBuf {
    // device ids belong to this PC: not in the roaming profile
    std::env::var_os("LOCALAPPDATA")
        .map(|p| std::path::PathBuf::from(p).join("Claude Talk"))
        .unwrap_or_else(crate::paths::app_dir)
        .join("default-stand-ins.txt")
}

#[cfg(windows)]
fn flow_tag(flow: ct_engine::win::devices::Flow) -> &'static str {
    if flow == ct_engine::win::devices::Flow::Render {
        "render"
    } else {
        "capture"
    }
}

#[cfg(windows)]
impl DefaultGuard {
    /// A stand-in still the default at the first check goes back to the
    /// user's device if that is there (as Windows itself would have done
    /// without Claude Talk's choice); one the user replaced meanwhile is
    /// dropped then (and the file rewritten).
    fn load(flow: ct_engine::win::devices::Flow) -> Self {
        let mut g = Self::default();
        let text = std::fs::read_to_string(stand_in_file()).unwrap_or_default();
        for line in text.lines() {
            let f: Vec<&str> = line
                .trim_start_matches('\u{feff}')
                .trim()
                .split('\t')
                .collect();
            if f.len() != 4 || f[0] != flow_tag(flow) || f[2].is_empty() || f[3].is_empty() {
                continue;
            }
            let Ok(role) = f[1].parse::<usize>() else {
                continue;
            };
            if role > 2 {
                continue;
            }
            g.stand_in[role] = Some(StandIn {
                id: f[2].to_string(),
                wanted: f[3].to_string(),
            });
        }
        g
    }

    /// Rewrites this flow's lines, keeping the other flow's.
    fn save(&self, flow: ct_engine::win::devices::Flow) {
        if NO_FILES.load(Ordering::Relaxed) {
            return;
        }
        static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _one = WRITING.lock().unwrap_or_else(|p| p.into_inner());
        let file = stand_in_file();
        let tag = flow_tag(flow);
        let mut lines: Vec<String> = std::fs::read_to_string(&file)
            .unwrap_or_default()
            .lines()
            .map(|l| l.trim_start_matches('\u{feff}'))
            .filter(|l| !l.is_empty() && !l.starts_with(&format!("{tag}\t")))
            .map(String::from)
            .collect();
        for (role, si) in self.stand_in.iter().enumerate() {
            if let Some(si) = si {
                lines.push(format!("{tag}\t{role}\t{}\t{}", si.id, si.wanted));
            }
        }
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = file.with_extension(format!("{}.tmp", std::process::id()));
        // (written through to the disk before the rename, as the settings
        // are: a power cut must not leave an empty file)
        if write_durably(&tmp, lines.join("\n").as_bytes()) {
            if let Err(e) = std::fs::rename(&tmp, &file) {
                log::warn!("could not save {}: {e}", file.display());
                let _ = std::fs::remove_file(&tmp);
            }
        }
    }
}

#[cfg(windows)]
fn guard_default(
    state: &crate::state::Inner,
    flow: ct_engine::win::devices::Flow,
    guard: &mut DefaultGuard,
) {
    use ct_engine::win::devices::FormFactor;
    use ct_engine::win::{devices, host, policy};
    let (external, routing) = state
        .config
        .lock()
        .map(|c| (c.external_claude.is_some(), c.route_discord))
        .unwrap_or((false, true));
    // A VB-CABLE the user had before Claude Talk is theirs to use as a
    // default device (a common streaming setup); only while Claude Talk
    // sends Discord through it does the output matter (D31). A stand-in
    // already picked still goes back when the user's device returns.
    let guarding = policy::cable_is_ours() || (flow == devices::Flow::Render && routing);
    let extra = if external && flow == devices::Flow::Render {
        host::extra_cable().map(|c| c.render.id)
    } else {
        None
    };
    let Ok(devs) = devices::list(flow) else {
        return;
    };
    let bad = |id: &str| {
        extra.as_deref() == Some(id)
            || devs
                .iter()
                .any(|e| e.id.eq_ignore_ascii_case(id) && host::is_own_device(&e.name))
    };
    let present = |id: &str| devs.iter().any(|e| e.id.eq_ignore_ascii_case(id));
    // a device that can stand in: no virtual or streaming device, and for
    // recording no loopback input ("Stereo Mix" would send the computer's
    // sound); headsets, headphones, speakers and microphones before the rest
    // (a monitor's sound often goes nowhere)
    // (by name, so it also works for a device that isn't plugged in;
    // "virtual" alone would rule out headsets like "HyperX Virtual Surround")
    let fit_name = |n: &str| {
        let n = n.to_lowercase();
        ![
            "virtual audio",
            "virtual cable",
            "vb-audio",
            "cable input",
            "cable output",
            "voicemeeter",
            "steam streaming",
        ]
        .iter()
        .any(|p| n.contains(p))
            && (flow == devices::Flow::Render
                || !devices::NOT_A_MICROPHONE
                    .iter()
                    .any(|p| n.contains(&p.to_lowercase())))
    };
    let fit = |e: &devices::Endpoint| !bad(&e.id) && fit_name(&e.name);
    let fit_id = |id: &str| {
        devs.iter()
            .find(|e| e.id.eq_ignore_ascii_case(id))
            .is_none_or(&fit)
    };
    let set = |id: &str, role: usize| match policy::set_default_endpoint_role(id, role) {
        Ok(()) => {
            log::warn!("default {flow:?} (role {role}) set to {id}");
            true
        }
        Err(e) => {
            log::warn!("could not set the default {flow:?}: {e}");
            false
        }
    };
    let mut fixed: Option<String> = None;
    let ids = devices::default_ids(flow);
    let before = guard.stand_in.clone();
    for (role, id) in ids.iter().enumerate() {
        let Some(id) = id else { continue };
        if !bad(id) {
            match guard.stand_in[role].clone() {
                // still the stand-in Claude Talk picked: back to the user's
                // device as soon as it is there again
                // (no notice: Windows would have done the same by itself)
                Some(si) if id.eq_ignore_ascii_case(&si.id) => {
                    if present(&si.wanted) && !bad(&si.wanted) && set(&si.wanted, role) {
                        guard.stand_in[role] = None;
                        guard.last_good[role] = Some(si.wanted.clone());
                    }
                }
                // the user (or Windows) chose something else meanwhile
                // (remembered as the user's device to go back to only if it
                // is a real one: never a loopback input, "Steam Streaming"
                // or the recording side of the extra cable, which Windows or
                // another program may have made the default for a moment)
                _ => {
                    // a default not in this tick's list (it appeared between
                    // the two calls): look again next time
                    let Some(name) = devs
                        .iter()
                        .find(|e| e.id.eq_ignore_ascii_case(id))
                        .map(|e| e.name.to_lowercase())
                    else {
                        continue;
                    };
                    let real = !name.contains("steam streaming")
                        && (flow == devices::Flow::Render
                            || !devices::NOT_A_MICROPHONE
                                .iter()
                                .any(|p| name.contains(&p.to_lowercase())));
                    // (a stand-in waits through a moment on such a device)
                    if real {
                        guard.stand_in[role] = None;
                        guard.last_good[role] = Some(id.clone());
                    }
                }
            }
            continue;
        }
        if !guarding {
            continue;
        }
        // the user's device: the last real default seen, or – the cable was
        // the default already when Claude Talk started – the one Windows
        // itself remembers as the default before the cable
        let wanted = guard.last_good[role]
            .clone()
            .or_else(|| guard.stand_in[role].as_ref().map(|s| s.wanted.clone()))
            .or_else(|| {
                policy::last_default_by_windows(
                    flow == devices::Flow::Render,
                    role,
                    &|i: &str, name: &str| i != id && !bad(i) && fit_id(i) && fit_name(name),
                )
            })
            .filter(|g| !bad(g));
        // what stands in: the user's device if it is there; else what
        // Windows itself would fall back to – the present device it had as
        // the default most recently; else by type (a headset's hands-free
        // part only for calls, a monitor last)
        let rank =
            |e: &devices::Endpoint| match (flow == devices::Flow::Render, role, e.form_factor) {
                (true, 2, FormFactor::Headset) => 0,
                (true, _, FormFactor::Headphones) => 0,
                (true, _, FormFactor::Speakers) => 1,
                (true, _, FormFactor::Headset) => 2,
                (false, _, FormFactor::Microphone) => 0,
                (false, _, FormFactor::Headset) => 1,
                _ => 3,
            };
        let back = wanted
            .clone()
            .filter(|g| present(g))
            .or_else(|| {
                policy::last_default_by_windows(
                    flow == devices::Flow::Render,
                    role,
                    &|i: &str, _: &str| {
                        !i.eq_ignore_ascii_case(id)
                            && devs.iter().any(|e| e.id.eq_ignore_ascii_case(i) && fit(e))
                    },
                )
            })
            .or_else(|| {
                devs.iter()
                    .filter(|e| fit(e))
                    .min_by_key(|e| rank(e))
                    .map(|e| e.id.clone())
            });
        let Some(back) = back else { continue };
        if set(&back, role) {
            log::warn!("default {flow:?} (role {role}) was our cable; set back");
            guard.stand_in[role] = match wanted {
                Some(w) if w != back => Some(StandIn {
                    id: back.clone(),
                    wanted: w,
                }),
                _ => None,
            };
            fixed = Some(back);
        }
    }
    if guard.stand_in != before {
        guard.save(flow);
    }
    // tell the user, naming the device the sound or microphone is on now
    if let Some(id) = fixed {
        let name = devs
            .iter()
            .find(|e| e.id.eq_ignore_ascii_case(&id))
            .map(|e| e.name.clone());
        let slot = if flow == devices::Flow::Render {
            &state.output_fixed
        } else {
            &state.mic_fixed
        };
        if let Ok(mut n) = slot.lock() {
            *n = Some((Instant::now(), name));
        }
    }
}

/// The endpoint id in a device interface path
/// (`\\?\SWD#MMDEVAPI#{0.0.1.00000000}.{guid}#{interface}`).
#[cfg(windows)]
fn endpoint_of(path: &str) -> Option<String> {
    let rest = path.split("MMDEVAPI#").nth(1)?;
    let id = rest.split("#{").next()?;
    (!id.is_empty()).then(|| id.to_string())
}

/// Discord's main processes (helper processes are not a restart).
#[cfg(windows)]
fn discord_roots(exes: &[String]) -> Vec<u32> {
    let names: Vec<String> = exes.iter().map(|e| e.to_lowercase()).collect();
    ct_engine::win::process::find_roots(&ct_engine::win::process::snapshot(), &names)
}

/// Discord records from a specific microphone of the user's choice. Before
/// the user is asked to set Discord to "Default" (which then means Claude
/// Talk), Claude Talk takes over that microphone, so the others in the call
/// keep hearing the same microphone as before. Only when the user has not
/// chosen a microphone in Claude Talk.
#[cfg(windows)]
fn adopt_discords_microphone(app: &AppHandle, state: &crate::state::Inner, endpoint: &str) {
    use ct_engine::win::devices::{self, DeviceSelector, Flow, NOT_A_MICROPHONE};
    if state
        .config
        .lock()
        .map(|c| c.microphone.is_some() || c.microphone_chosen)
        .unwrap_or(true)
    {
        return;
    }
    let own = devices::patterns(NOT_A_MICROPHONE);
    let Ok((_, chosen)) = devices::resolve(&DeviceSelector::Id(endpoint.to_string())) else {
        return;
    };
    if chosen.flow != Flow::Capture
        || own
            .iter()
            .any(|p| chosen.name.to_lowercase().contains(&p.to_lowercase()))
    {
        return;
    }
    let default = devices::resolve(&DeviceSelector::DefaultExcluding(Flow::Capture, own))
        .map(|(_, e)| e.id)
        .unwrap_or_default();
    if default.eq_ignore_ascii_case(&chosen.id) {
        return; // the same microphone anyway
    }
    log::info!(
        "Discord records from '{}'; Claude Talk uses it as the microphone too",
        chosen.name
    );
    let cfg = state.config.lock().map(|mut c| {
        // (the user may have chosen one meanwhile)
        if c.microphone.is_some() || c.microphone_chosen {
            return None;
        }
        c.microphone = Some(chosen.id.clone());
        c.microphone_name = Some(chosen.name.clone());
        Some(c.clone())
    });
    let Ok(Some(cfg)) = cfg else { return };
    state.save_config();
    crate::audio::use_microphone(app, Some(chosen.id));
    use tauri::Emitter;
    let _ = app.emit("config-changed", cfg);
}

/// A new default microphone (headset plugged in, chosen in Windows), or the
/// chosen microphone plugged back in: the microphone stream moves to it.
#[cfg(windows)]
fn follow_default_microphone(state: &crate::state::Inner) {
    use ct_engine::graph::ids;
    use ct_engine::win::devices::{self, DeviceSelector, NOT_A_MICROPHONE};
    let g = state.audio.lock().unwrap_or_else(|p| p.into_inner());
    let Some(host) = g.as_ref() else { return };
    let current = host.source_device(ids::MIC);
    if current.is_empty() {
        return; // not running (paused, waiting) or not a device
    }
    let wanted = devices::resolve(&DeviceSelector::PreferredMicrophone(devices::patterns(
        NOT_A_MICROPHONE,
    )))
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
/// "Gendan": a Discord recording the cable right now keeps doing so until it
/// starts again (K12) – remember which one, so it isn't taken for a Discord
/// set to the cable itself (K37).
#[cfg(windows)]
pub fn note_gendan() {
    let exes = ct_engine::win::host::discord_executables();
    let cable = ct_engine::win::policy::cable_capture_id();
    let on_cable = matches!(
        ct_engine::win::sessions::discord_mic(&exes, cable.as_deref()),
        ct_engine::win::sessions::DiscordMic::Cable
    );
    *RESTORED_FOR.lock().unwrap_or_else(|p| p.into_inner()) = if on_cable {
        discord_roots(&exes)
    } else {
        Vec::new()
    };
}

#[cfg(windows)]
pub fn restore() -> usize {
    let _com = ct_engine::win::ComGuard::new();
    // also in the helper and uninstall modes, which never started the audio:
    // what to give back comes from the settings
    let cfg = crate::config::Config::peek(&crate::paths::config_file());
    if ct_engine::win::policy::discord_own_input().is_none() {
        ct_engine::win::policy::set_discord_own_input(cfg.discord_own_input.clone());
    }
    if ct_engine::win::devices::preferred_microphone().is_none() {
        ct_engine::win::devices::set_preferred_microphone(cfg.microphone.clone());
    }
    if ct_engine::win::policy::discord_given_back().is_empty() {
        load_given_back();
    }
    ct_engine::win::policy::set_cable_is_ours(crate::installer::cable_is_ours());
    let before = ct_engine::win::policy::discord_given_back();
    let exes = ct_engine::win::host::discord_executables();
    let n = ct_engine::win::policy::restore_discord(&exes);
    // remember what Claude Talk itself gave Discord (never the user's own
    // setting); not from the elevated uninstaller (D27)
    let after = ct_engine::win::policy::discord_given_back();
    if after != before {
        save_given_back();
    }
    log_restore_and_fill(n)
}

/// Writes the list of values Claude Talk gave Discord (one writer at a time,
/// whole file or nothing; never from the elevated uninstaller, D27).
#[cfg(windows)]
pub fn save_given_back() {
    if !NO_FILES.load(Ordering::Relaxed) {
        // one writer at a time, whole file or nothing
        static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _one = WRITING.lock().unwrap_or_else(|p| p.into_inner());
        let file = crate::paths::given_back_file();
        let tmp = file.with_extension(format!("{}.tmp", std::process::id()));
        let _ = std::fs::create_dir_all(crate::paths::app_dir());
        // ("-" marks a value the user replaced under "Din mikrofon")
        let mut lines = ct_engine::win::policy::discord_given_back();
        lines.extend(
            ct_engine::win::policy::forgotten()
                .into_iter()
                .map(|f| format!("-{f}")),
        );
        let list = lines.join("\n");
        // (a file per process: the helper and the app may write at once)
        // (written through to the disk before the rename, as the settings
        // are: a power cut must not leave an empty file)
        if write_durably(&tmp, list.as_bytes()) {
            if let Err(e) = std::fs::rename(&tmp, &file) {
                log::warn!("could not save {}: {e}", file.display());
                let _ = std::fs::remove_file(&tmp);
            }
        }
    }
}

#[cfg(windows)]
fn log_restore_and_fill(n: usize) -> usize {
    let exes = ct_engine::win::host::discord_executables();
    if ct_engine::win::policy::discord_input_as_wanted(&exes) == Some(true) {
        *FILLED_FOR.lock().unwrap_or_else(|p| p.into_inner()) = discord_roots(&exes);
    }
    log::info!("restored Discord's microphone ({n} settings cleared)");
    n
}

#[cfg(windows)]
fn tell_restart_discord(da: bool) {
    use windows::core::HSTRING;
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONWARNING, MB_OK, MB_SETFOREGROUND, MB_TOPMOST,
    };
    let (title, text) = if da {
        (
            "Genstart Discord",
            "Discord bruger stadig Claude Talk-mikrofonen, som er tavs, nu hvor Claude Talk er lukket. Luk Discord helt (højreklik på Discords ikon ved uret, og vælg »Afslut Discord« (på engelsk »Quit Discord«)), og start det igen. Ellers kan de andre ikke høre dig.",
        )
    } else {
        (
            "Restart Discord",
            "Discord still uses the Claude Talk microphone, which is silent now that Claude Talk is closed. Quit Discord completely (right-click Discord's icon by the clock and choose “Quit Discord”), and start it again. Otherwise the others can't hear you.",
        )
    };
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(text),
            &HSTRING::from(title),
            MB_OK | MB_ICONWARNING | MB_SETFOREGROUND | MB_TOPMOST,
        );
    }
}

/// The Discord (its main processes) that was recording the cable when
/// "Gendan" was pressed (empty: none was).
static RESTORED_FOR: std::sync::Mutex<Vec<u32>> = std::sync::Mutex::new(Vec::new());

/// The Discord start (its main processes) that has its microphone: an empty
/// value seen later in that same run is the user's "Default".
static FILLED_FOR: std::sync::Mutex<Vec<u32>> = std::sync::Mutex::new(Vec::new());

#[cfg(windows)]
fn filled_for(roots: &[u32]) -> bool {
    *FILLED_FOR.lock().unwrap_or_else(|p| p.into_inner()) == roots
}

/// A per-app input value that is a microphone the user can choose (never
/// Claude Talk's own devices, nor one that can't be resolved right now).
#[cfg(windows)]
fn is_a_real_microphone(path: &str) -> bool {
    use ct_engine::win::devices::{self, DeviceSelector, NOT_A_MICROPHONE};
    let Some(id) = endpoint_of(path) else {
        return false;
    };
    match devices::resolve(&DeviceSelector::Id(id)) {
        Ok((_, e)) => {
            let n = e.name.to_lowercase();
            !NOT_A_MICROPHONE
                .iter()
                .any(|p| n.contains(&p.to_lowercase()))
        }
        Err(_) => false,
    }
}

#[cfg(windows)]
fn write_durably(path: &std::path::Path, data: &[u8]) -> bool {
    use std::io::Write;
    let ok = std::fs::File::create(path)
        .and_then(|mut f| {
            f.write_all(data)?;
            f.sync_all()
        })
        .is_ok();
    if !ok {
        log::warn!("could not write {}", path.display());
        let _ = std::fs::remove_file(path);
    }
    ok
}

/// Set when the main thread panicked: the watcher must not point Discord at
/// the cable again while the program goes down.
#[cfg(windows)]
static GAVE_UP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Set in the running app (see `mark_running`).
#[cfg(windows)]
static RUNNING_APP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The program is going down after a panic: give Discord its microphone back
/// as on quit, with the helper and RunOnce when Discord is closed.
#[cfg(windows)]
pub fn give_up_after_panic() {
    // only the running app: not the elevated install/uninstall modes (D27)
    if !RUNNING_APP.load(Ordering::Relaxed) {
        return;
    }
    GAVE_UP.store(true, Ordering::Relaxed);
    restore();
    if ct_engine::win::policy::pids_of(&ct_engine::win::host::discord_executables()).is_empty() {
        crate::autostart::restore_at_next_sign_in(true);
        spawn_restore_helper();
    }
}

/// Set by the elevated uninstall cleanup: it writes nothing into the user's
/// folders.
pub static NO_FILES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What Claude Talk has given Discord itself (see `paths::given_back_file`).
#[cfg(windows)]
pub fn load_given_back() {
    let lines: Vec<String> = std::fs::read_to_string(crate::paths::given_back_file())
        .unwrap_or_default()
        .lines()
        .map(|s| s.trim_start_matches('\u{feff}').trim().to_string())
        .filter(|s| !s.is_empty() && s != "-")
        .collect();
    let (forgotten, given): (Vec<String>, Vec<String>) =
        lines.into_iter().partition(|l| l.starts_with('-'));
    ct_engine::win::policy::set_discord_given_back(given);
    ct_engine::win::policy::set_forgotten(
        forgotten
            .iter()
            .filter_map(|l| l.strip_prefix('-').map(|v| v.trim().to_string()))
            .filter(|v| !v.is_empty())
            .collect(),
    );
}

#[cfg(not(windows))]
pub fn restore() -> usize {
    0
}

#[cfg(not(windows))]
pub fn note_gendan() {}

const MAIN_MUTEX: &str = "Local\\ClaudeTalkMain";

/// Marks this process as the running Claude Talk (the restore helper below
/// steps aside while it exists). Kept for the life of the program.
#[cfg(windows)]
pub fn mark_running() {
    RUNNING_APP.store(true, Ordering::Relaxed);
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
/// aside if Claude Talk starts again, and gives up after 30 days.
#[cfg(windows)]
pub fn restore_when_discord_starts() -> i32 {
    use ct_engine::win::host::discord_executables;
    use ct_engine::win::{policy, ComGuard};
    let _com = ComGuard::new();
    // Discord isn't installed (any more): nothing to wait for
    let installed = std::env::var_os("LOCALAPPDATA").is_some_and(|l| {
        ["Discord", "DiscordPTB", "DiscordCanary"]
            .iter()
            .any(|d| std::path::Path::new(&l).join(d).join("Update.exe").exists())
    });
    if !installed {
        crate::autostart::restore_at_next_sign_in(false);
        return 0;
    }
    let parent = std::env::args()
        .skip_while(|a| a != "--restore-when-discord-starts")
        .nth(1)
        .and_then(|p| p.parse::<u32>().ok())
        .unwrap_or(0);
    if parent == 0 && main_running() {
        // started from RunOnce, and Claude Talk already runs: it does the work
        crate::autostart::restore_at_next_sign_in(false);
        return 0;
    }
    // Windows removes a RunOnce value before it runs it: arm it again, so a
    // session that ends before Discord runs still has a helper next time
    crate::autostart::restore_at_next_sign_in(true);
    // first wait until the Claude Talk that started us has really ended
    // (until then its "running" mark is still there)
    if parent != 0 {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{
            OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
        };
        if let Ok(h) = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, parent) } {
            unsafe {
                WaitForSingleObject(h, 60_000);
                let _ = CloseHandle(h);
            }
        }
    }
    let exes = discord_executables();
    let t0 = Instant::now();
    // (ends with the Windows session at the latest)
    while t0.elapsed() < Duration::from_secs(30 * 24 * 3600) {
        if main_running() {
            return 0;
        }
        if !policy::pids_of(&exes).is_empty() {
            // Discord starts its audio process a little later, and Windows
            // may only take the setting once Discord has used sound (K8):
            // try until it holds, while Discord runs (up to 30 minutes)
            let t1 = Instant::now();
            let mut wait = 3u64;
            while t1.elapsed() < Duration::from_secs(30 * 60) {
                std::thread::sleep(Duration::from_secs(wait));
                wait = (wait * 2).min(30);
                if main_running() {
                    return 0;
                }
                if policy::pids_of(&exes).is_empty() {
                    break; // Discord closed again: wait for its next start
                }
                let n = restore();
                log::info!("restore helper: Discord's own microphone back ({n})");
                // done only when confirmed (no cable any more: nothing points
                // Discord at it); then not again at the next sign-in
                let cable_gone = policy::cable_capture_id().is_none();
                let done = (cable_gone || policy::discord_routed_to_cable(&exes) == Some(false))
                    && policy::discord_input_as_wanted(&exes) == Some(true);
                if done {
                    crate::autostart::restore_at_next_sign_in(false);
                    // the setting is right, but a Discord already in a call
                    // may keep recording the (now silent) cable until it
                    // starts again (K12): say so, since Claude Talk is closed
                    let cable = policy::cable_capture_id();
                    if !cable_gone
                        && matches!(
                            ct_engine::win::sessions::discord_mic(&exes, cable.as_deref()),
                            ct_engine::win::sessions::DiscordMic::Cable
                        )
                    {
                        let da = crate::config::Config::peek(&crate::paths::config_file()).language
                            == "da";
                        tell_restart_discord(da);
                    }
                    return 0;
                }
            }
            if !policy::pids_of(&exes).is_empty() {
                return 0; // tried for 30 minutes; again at the next sign-in
            }
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
            .arg(std::process::id().to_string())
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
    // Discord's own launcher must be there before Discord is closed: without
    // it, Discord could not be started again and the user would be left
    // outside the call with no Discord at all
    let updater = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .map(|l| l.join(folder).join("Update.exe"))
        .filter(|p| p.exists())
        .ok_or("error.discordRestart")?;
    for pid in policy::pids_of(&[exe.to_string()]) {
        unsafe {
            if let Ok(h) = OpenProcess(PROCESS_TERMINATE, false, pid) {
                let _ = TerminateProcess(h, 0);
                let _ = CloseHandle(h);
            }
        }
    }
    std::thread::sleep(Duration::from_millis(1200));
    if !policy::pids_of(&[exe.to_string()]).is_empty() {
        // Discord runs as administrator (or refused): it is still running,
        // and starting it again would only bring the old one to the front
        log::warn!("restart Discord: Discord could not be closed");
        return Err("error.discordClose".into());
    }
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
