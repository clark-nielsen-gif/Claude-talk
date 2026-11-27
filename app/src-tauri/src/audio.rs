//! Runs the audio engine for the app: starts the standard setup, applies the
//! user's switches, and publishes meters and status to the UI ~30× a second.

use crate::state::AppState;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Level {
    pub peak: f32,
    pub rms: f32,
}

/// What the UI needs, in plain terms (no device jargon).
#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AudioStatus {
    pub engine_running: bool,
    pub mic: Level,
    pub claude: Level,
    pub discord: Level,
    pub to_discord: Level,
    pub to_claude: Level,
    /// "ok", "missing" (no microphone), "blocked" (Windows privacy), "starting"
    pub mic_state: String,
    pub mic_name: String,
    /// The Claude window is open and its page uses our microphone.
    pub claude_window_open: bool,
    pub claude_listening: bool,
    /// The Claude window is on claude.ai's login page.
    pub claude_login: bool,
    /// Discord is running (its sound is being captured).
    pub discord_running: bool,
    /// "yes": Discord records from our cable, "no": another mic, "": unknown.
    pub discord_uses_cable: String,
    pub discord_needs_restart: bool,
    pub discord_cable_chosen: bool,
    pub discord_still_on_cable: bool,
    /// Windows' default output was our cable and was set back (last 30 s).
    pub output_fixed: bool,
    pub mic_fixed: bool,
    /// the devices the output and microphone were set to
    pub fixed_output: Option<String>,
    pub fixed_mic: Option<String>,
    /// a Discord call started while the microphone was muted (shown while
    /// it still is, for 2 minutes)
    pub muted_call: bool,
    pub muted_call_id: Option<u64>,
    /// Shortcuts that could not be registered ("" = all fine).
    pub hotkey_problem: String,
    /// Claude in another program (Chrome, Edge, the app): its sound and
    /// whether it is captured right now.
    pub claude_external: Level,
    pub claude_external_running: bool,
    /// Discord plays on another device than Claude: [Discord's, Claude's].
    pub output_mismatch: Option<[String; 2]>,
    /// Default output is speakers (echo risk).
    pub speakers: bool,
    /// Echo reduction is running on the microphone.
    pub echo_active: bool,
    /// The virtual cable (Discord's microphone) is installed and playing.
    pub cable_ok: bool,
    pub latency_ms: f32,
    pub drift_ppm: f32,
    pub xruns: u64,
}

#[cfg(windows)]
mod imp {
    use super::*;
    use ct_engine::graph::ids;
    use ct_engine::win::host::{AudioHost, HostConfig};
    use ct_engine::win::stream::StreamState;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    const E_ACCESSDENIED: u32 = 0x8007_0005;

    pub fn start(app: &AppHandle) {
        // device lookups below (extra cable, mixer devices) need COM on
        // this thread; started minimized there is no window to set it up
        let _com = ct_engine::win::ComGuard::new();
        let state = app.state::<AppState>().inner().clone();
        if let Ok(c) = state.config.lock() {
            ct_engine::win::devices::set_preferred_microphone(c.microphone.clone());
            ct_engine::win::policy::set_discord_own_input(c.discord_own_input.clone());
        }
        crate::discord::load_given_back();
        ct_engine::win::policy::set_cable_is_ours(crate::installer::cable_is_ours());
        // Claude Talk runs: it gives Discord its microphone back itself
        crate::autostart::restore_at_next_sign_in(false);
        #[allow(unused_mut)]
        let mut cfg = HostConfig::standard(state.claude_pid.clone());
        // CI builds only: the "microphone" is a test program's sound.
        #[cfg(feature = "ci-testpage")]
        if let Ok(exe) = std::env::var("CLAUDE_TALK_TEST_MIC_EXE") {
            cfg.mic = ct_engine::win::host::SourceKind::Program(
                ct_engine::win::stream::ProcessTarget::ExeNames(vec![exe.to_lowercase()]),
            );
            log::warn!("TEST MODE: microphone replaced by program {exe}");
        }
        let mut host = AudioHost::standard(cfg);
        if let Some(mixer) = host.take_pulled(ids::TO_CLAUDE) {
            crate::claude_window::spawn_bridge(app.clone(), mixer);
        }
        // the user's advanced mixer (added sources/outputs, routing)
        let saved = state
            .config
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        crate::mixer::imp::apply(&mut host, &saved.mixer, saved.advanced_unlocked);
        if let Some(exe) = saved.external_claude.as_deref() {
            if let Err(e) = crate::mixer::imp::set_external_claude(&mut host, Some(exe)) {
                // the extra cable is gone: turn the mode off, or the mic would
                // stay open for good with no switch to turn it off
                log::warn!("external Claude not started ({e}); mode turned off");
                let cfg = state.config.lock().map(|mut c| {
                    c.external_claude = None;
                    c.clone()
                });
                state.save_config();
                if let Ok(cfg) = cfg {
                    use tauri::Emitter;
                    let _ = app.emit("config-changed", cfg);
                }
            }
        }
        *state.audio.lock().unwrap_or_else(|p| p.into_inner()) = Some(host);
        apply_switches(app);
        let app2 = app.clone();
        std::thread::Builder::new()
            .name("audio-status".into())
            .spawn(move || status_loop(app2))
            .expect("spawn status thread");
        log::info!("audio engine started");
    }

    /// Routes the switches decide (see `switch_mute`).
    const SWITCHED_ROUTES: [(u32, u32); 7] = {
        use crate::mixer::imp::{EXTERNAL_OUT, EXTERNAL_SRC};
        [
            (ids::CLAUDE, ids::TO_DISCORD),
            (ids::DISCORD, ids::TO_CLAUDE),
            (ids::MIC, ids::TO_CLAUDE),
            (ids::MIC, ids::TO_DISCORD),
            (EXTERNAL_SRC, ids::TO_DISCORD),
            (ids::DISCORD, EXTERNAL_OUT),
            (ids::MIC, EXTERNAL_OUT),
        ]
    };

    /// A route the switches decide starts muted; `apply_switches` (always
    /// called right after a route change) opens it if the switches allow.
    pub fn starts_muted(s: u32, o: u32) -> bool {
        SWITCHED_ROUTES.contains(&(s, o))
    }

    /// Whether the switches mute this route (`None`: not a switched route).
    /// Used when applying the switches and whenever a route is (re)created,
    /// so no route is ever live for a moment against a switch.
    pub fn switch_mute(
        cfg: &crate::config::Config,
        ptt_down: bool,
        s: u32,
        o: u32,
    ) -> Option<bool> {
        use crate::mixer::imp::{EXTERNAL_OUT, EXTERNAL_SRC};
        let on = cfg.claude_in_discord;
        // "Claude hears only you": Discord's voices never reach Claude
        let discord_to_claude = on && cfg.claude_hears_discord;
        // To Claude the microphone follows the big switch and, if
        // push-to-talk is on, the key.
        // After "Gendan" the mute keeps the microphone from Claude only:
        // Discord may still record the cable for a while (K12), and the
        // window says Discord keeps hearing you.
        let mic_to_claude = on && !(cfg.push_to_talk && !ptt_down) && !cfg.mic_muted;
        let claude_to_discord = on && !cfg.claude_to_discord_muted;
        Some(match (s, o) {
            (ids::CLAUDE, ids::TO_DISCORD) | (EXTERNAL_SRC, ids::TO_DISCORD) => !claude_to_discord,
            (ids::DISCORD, ids::TO_CLAUDE) | (ids::DISCORD, EXTERNAL_OUT) => !discord_to_claude,
            (ids::MIC, ids::TO_CLAUDE) | (ids::MIC, EXTERNAL_OUT) => !mic_to_claude,
            // Your microphone always reaches Discord (Discord uses our
            // cable as its microphone).
            (ids::MIC, ids::TO_DISCORD) => false,
            _ => return None,
        })
    }

    /// Applies config switches (big switch, mutes, push-to-talk) to the routes.
    pub fn apply_switches(app: &AppHandle) {
        let state = app.state::<AppState>();
        // The audio lock first, then the newest settings: concurrent callers
        // (watcher, hotkeys, tray, UI) apply one after another, each with the
        // settings as they are at that moment.
        let mut guard = state.audio.lock().unwrap_or_else(|p| p.into_inner());
        let cfg = state
            .config
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        let ptt_down = state.ptt_down.load(Ordering::Relaxed);
        let Some(host) = guard.as_mut() else { return };
        host.set_source_muted(ids::MIC, cfg.mic_muted && cfg.route_discord);
        for (s, o) in SWITCHED_ROUTES {
            if let Some(m) = switch_mute(&cfg, ptt_down, s, o) {
                host.set_route_muted(s, o, m);
            }
        }
        // echo reduction: always, never, or automatically with speakers
        let speakers =
            state.speakers.load(Ordering::Relaxed) || state.added_speakers.load(Ordering::Relaxed);
        let echo = match cfg.echo {
            crate::config::EchoMode::On => true,
            crate::config::EchoMode::Off => false,
            crate::config::EchoMode::Auto => speakers,
        };
        // "Claude hears only you" with speakers: the others' voices come out
        // of the speakers into the microphone; echo reduction takes them out
        // (its reference is everything that is not you).
        let echo = echo || (speakers && cfg.claude_in_discord && !cfg.claude_hears_discord);
        // CI builds only: force echo reduction on or off for the test run
        #[cfg(feature = "ci-testpage")]
        let echo = match std::env::var("CLAUDE_TALK_TEST_ECHO").as_deref() {
            Ok("on") => true,
            Ok("off") => false,
            _ => echo,
        };
        host.set_echo_reduction(echo);
    }

    /// Opens the microphone only while someone can hear it: Discord is
    /// recording, the Claude page uses it, Claude runs in another program,
    /// or Claude Talk's window is open (its "we hear you" meter). Otherwise
    /// the device stays closed – Windows' microphone indicator goes off and
    /// Bluetooth headsets keep their good sound.
    /// A fixed start for numbering moments in the status.
    static STATUS_EPOCH: std::sync::LazyLock<std::time::Instant> =
        std::sync::LazyLock::new(std::time::Instant::now);

    pub fn refresh_mic(app: &AppHandle) {
        let state = app.state::<AppState>();
        let window_open = window_shown(app);
        let external = state
            .config
            .lock()
            .map(|c| c.external_claude.is_some())
            .unwrap_or(false);
        let needed = window_open
            || external
            || state.discord_recording.load(Ordering::Relaxed)
            || state.claude_mic_active.load(Ordering::Relaxed);
        let guard = state.audio.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(host) = guard.as_ref() {
            host.set_source_paused(ids::MIC, !needed);
        }
    }

    /// Records from this microphone from now on (None: Windows' default).
    pub fn use_microphone(app: &AppHandle, id: Option<String>) {
        log::info!(
            "microphone chosen: {}",
            id.as_deref().unwrap_or("Windows' default")
        );
        ct_engine::win::devices::set_preferred_microphone(id);
        let state = app.state::<AppState>();
        let guard = state.audio.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(host) = guard.as_ref() {
            host.restart_source(ids::MIC);
        }
    }

    /// The main window is on screen: visible and not minimized.
    fn window_shown(app: &AppHandle) -> bool {
        app.get_webview_window("main")
            .map(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false))
            .unwrap_or(false)
    }

    fn level(peak: f32, rms: f32) -> Level {
        Level { peak, rms }
    }

    fn status_loop(app: AppHandle) {
        let state = app.state::<AppState>().inner().clone();
        let mut last_states: Vec<(u32, StreamState)> = Vec::new();
        // diagnostics for the log: loudest peak per source/output over ~5 s
        let mut peaks: std::collections::BTreeMap<String, f32> = Default::default();
        let mut last_diag = std::time::Instant::now();
        let mut was_shown = false;
        while !state.shutting_down.load(Ordering::Relaxed) {
            // 30 meter updates a second while the window is on screen; when
            // it is hidden or minimized only the bookkeeping runs.
            let visible = window_shown(&app);
            if visible != was_shown {
                was_shown = visible;
                refresh_mic(&app); // the meter needs the mic only on screen
            }
            std::thread::sleep(Duration::from_millis(if visible { 33 } else { 250 }));
            let (snap, echo_active) = {
                let mut g = state.audio.lock().unwrap_or_else(|p| p.into_inner());
                let Some(host) = g.as_mut() else { continue };
                host.maintain();
                (host.snapshot(), host.echo_reduction_active())
            };
            for s in &snap.sources {
                let e = peaks.entry(format!("src{}", s.id)).or_insert(0.0);
                *e = e.max(s.peak);
            }
            for o in &snap.outputs {
                let e = peaks.entry(format!("out{}", o.id)).or_insert(0.0);
                *e = e.max(o.peak);
            }
            if let Some((p, _)) = snap.echo {
                let e = peaks.entry("echo".into()).or_insert(0.0);
                *e = e.max(p);
            }
            if last_diag.elapsed() >= Duration::from_secs(30) {
                last_diag = std::time::Instant::now();
                let db = |v: f32| if v > 0.0 { 20.0 * v.log10() } else { -120.0 };
                let levels: Vec<String> = peaks
                    .iter()
                    .map(|(k, v)| format!("{k}={:.0}", db(*v)))
                    .collect();
                let routes: Vec<String> = snap
                    .routes
                    .iter()
                    .map(|r| {
                        format!(
                            "{}>{}{} buf={:.0}/{:.0}ms under={} drift={:.0}",
                            r.source,
                            r.output,
                            if r.muted { "(muted)" } else { "" },
                            r.buffered_ms,
                            r.target_ms,
                            r.underrun_frames,
                            r.drift_ppm
                        )
                    })
                    .collect();
                log::debug!(
                    "levels dBFS [{}] routes [{}] echo {:?}",
                    levels.join(" "),
                    routes.join("; "),
                    snap.echo.map(|e| e.1)
                );
                peaks.clear();
            }
            let mut st = AudioStatus {
                engine_running: true,
                ..Default::default()
            };
            for s in &snap.sources {
                let l = level(s.peak, s.rms);
                match s.id {
                    ids::MIC => {
                        st.mic = l;
                        st.mic_name = s.stream.device.clone();
                        st.mic_state = match s.stream.state {
                            StreamState::Running => "ok",
                            StreamState::Failed if s.stream.last_error == E_ACCESSDENIED => {
                                "blocked"
                            }
                            StreamState::WaitingForDevice | StreamState::Failed => "missing",
                            _ => "starting",
                        }
                        .into();
                    }
                    ids::CLAUDE => st.claude = l,
                    crate::mixer::imp::EXTERNAL_SRC => {
                        st.claude_external = l;
                        st.claude_external_running = s.stream.state == StreamState::Running;
                    }
                    ids::DISCORD => {
                        st.discord = l;
                        st.discord_running = s.stream.state == StreamState::Running;
                    }
                    _ => {}
                }
                // log state changes (never from the audio threads themselves)
                let key = (s.id, s.stream.state);
                if !last_states.contains(&key) {
                    last_states.retain(|(id, _)| *id != s.id);
                    last_states.push(key);
                    log::info!(
                        "source {} '{}' -> {:?} (device '{}', pid {}, hr 0x{:08X})",
                        s.id,
                        s.label,
                        s.stream.state,
                        s.stream.device,
                        s.stream.pid,
                        s.stream.last_error
                    );
                }
            }
            for o in &snap.outputs {
                let l = level(o.peak, o.rms);
                match o.id {
                    ids::TO_DISCORD => {
                        st.to_discord = l;
                        // both sides: Discord records the recording side
                        st.cable_ok = o
                            .stream
                            .as_ref()
                            .is_some_and(|s| s.state == StreamState::Running)
                            && state.cable_capture_ok.load(Ordering::Relaxed);
                    }
                    ids::TO_CLAUDE => st.to_claude = l,
                    _ => {}
                }
            }
            let mut worst_latency = 0f32;
            for r in &snap.routes {
                st.xruns += r.underrun_frames;
                worst_latency = worst_latency.max(r.buffered_ms);
                if r.source == ids::MIC && r.output == ids::TO_DISCORD {
                    st.drift_ppm = r.drift_ppm;
                }
            }
            // capture period (10 ms) + route buffer + render queue (1.5
            // periods, 15 ms) + limiter look-ahead (1.5 ms), plus one 10 ms
            // frame through echo reduction when it runs
            st.latency_ms = worst_latency + 28.0 + if echo_active { 10.0 } else { 0.0 };
            st.discord_uses_cable = match state.discord_uses_cable.load(Ordering::Relaxed) {
                crate::discord::YES => "yes",
                crate::discord::NO => "no",
                _ => "",
            }
            .into();
            st.discord_needs_restart = state.discord_needs_restart.load(Ordering::Relaxed);
            st.discord_cable_chosen = state.discord_cable_chosen.load(Ordering::Relaxed);
            st.discord_still_on_cable = state.discord_still_on_cable.load(Ordering::Relaxed);
            st.claude_login = state.claude_login.load(Ordering::Relaxed);
            // shown for 2 minutes: the window may be hidden when it happens
            let recent = |m: &std::sync::Mutex<Option<(std::time::Instant, Option<String>)>>| {
                m.lock()
                    .ok()
                    .and_then(|f| f.clone())
                    .filter(|(t, _)| t.elapsed() < Duration::from_secs(120))
            };
            if let Some((_, name)) = recent(&state.output_fixed) {
                st.output_fixed = true;
                st.fixed_output = name;
            }
            // while the call that started muted goes on (until 5 s without
            // recording; a short gap is the same call), and the user is still
            // muted (unmuting clears it); the id tells the page a new one
            let at = state.muted_call_at.lock().ok().and_then(|t| *t);
            st.muted_call = at.is_some()
                && state
                    .config
                    .lock()
                    .map(|c| c.mic_muted && c.route_discord)
                    .unwrap_or(false);
            st.muted_call_id =
                at.map(|t| t.saturating_duration_since(*STATUS_EPOCH).as_millis() as u64);
            if let Some((_, name)) = recent(&state.mic_fixed) {
                st.mic_fixed = true;
                st.fixed_mic = name;
            }
            st.hotkey_problem = state
                .hotkey_problem
                .lock()
                .map(|h| h.clone())
                .unwrap_or_default();
            st.output_mismatch = state
                .output_mismatch
                .lock()
                .ok()
                .and_then(|m| m.as_ref().map(|(_, d, c)| [d.clone(), c.clone()]));
            st.speakers = state.speakers.load(Ordering::Relaxed);
            st.echo_active = echo_active;
            st.claude_window_open = state.claude_pid.load(Ordering::Relaxed) != 0;
            st.claude_listening = state.claude_mic_active.load(Ordering::Relaxed);
            if visible {
                let _ = app.emit("audio-status", &st);
            }
        }
    }
}

#[cfg(windows)]
pub use imp::{apply_switches, refresh_mic, start, starts_muted, use_microphone};

#[cfg(not(windows))]
pub fn start(_app: &AppHandle) {}
#[cfg(not(windows))]
pub fn apply_switches(_app: &AppHandle) {}
#[cfg(not(windows))]
pub fn refresh_mic(_app: &AppHandle) {}
#[cfg(not(windows))]
pub fn use_microphone(_app: &AppHandle, _id: Option<String>) {}
