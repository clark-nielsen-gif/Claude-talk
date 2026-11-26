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
    /// Windows' default output was our cable and was set back (last 30 s).
    pub output_fixed: bool,
    /// Shortcuts that could not be registered ("" = all fine).
    pub hotkey_problem: String,
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
                if let Ok(mut c) = state.config.lock() {
                    c.external_claude = None;
                }
                state.save_config();
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
        let mic_to_claude = on && !(cfg.push_to_talk && !ptt_down);
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
        host.set_source_muted(ids::MIC, cfg.mic_muted);
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
            if last_diag.elapsed() >= Duration::from_secs(5) {
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
                        st.cable_ok = o
                            .stream
                            .as_ref()
                            .is_some_and(|s| s.state == StreamState::Running);
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
            st.latency_ms = worst_latency + 26.5 + if echo_active { 10.0 } else { 0.0 };
            st.discord_uses_cable = match state.discord_uses_cable.load(Ordering::Relaxed) {
                crate::discord::YES => "yes",
                crate::discord::NO => "no",
                _ => "",
            }
            .into();
            st.discord_needs_restart = state.discord_needs_restart.load(Ordering::Relaxed);
            st.claude_login = state.claude_login.load(Ordering::Relaxed);
            st.output_fixed = state
                .output_fixed_at
                .lock()
                .ok()
                .and_then(|t| *t)
                .is_some_and(|t| t.elapsed() < Duration::from_secs(30));
            st.hotkey_problem = state
                .hotkey_problem
                .lock()
                .map(|h| h.clone())
                .unwrap_or_default();
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
pub use imp::{apply_switches, refresh_mic, start, starts_muted};

#[cfg(not(windows))]
pub fn start(_app: &AppHandle) {}
#[cfg(not(windows))]
pub fn apply_switches(_app: &AppHandle) {}
#[cfg(not(windows))]
pub fn refresh_mic(_app: &AppHandle) {}
