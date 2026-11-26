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
    /// Discord is running (its sound is being captured).
    pub discord_running: bool,
    /// "yes": Discord records from our cable, "no": another mic, "": unknown.
    pub discord_uses_cable: String,
    pub discord_needs_restart: bool,
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
        *state.audio.lock().unwrap_or_else(|p| p.into_inner()) = Some(host);
        apply_switches(app);
        let app2 = app.clone();
        std::thread::Builder::new()
            .name("audio-status".into())
            .spawn(move || status_loop(app2))
            .expect("spawn status thread");
        log::info!("audio engine started");
    }

    /// Applies config switches (big switch, mutes, push-to-talk) to the routes.
    pub fn apply_switches(app: &AppHandle) {
        let state = app.state::<AppState>();
        let cfg = state
            .config
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        let ptt_down = state.ptt_down.load(Ordering::Relaxed);
        let mut guard = state.audio.lock().unwrap_or_else(|p| p.into_inner());
        let Some(host) = guard.as_mut() else { return };
        let on = cfg.claude_in_discord;
        host.set_source_muted(ids::MIC, cfg.mic_muted);
        host.set_route_muted(
            ids::CLAUDE,
            ids::TO_DISCORD,
            !on || cfg.claude_to_discord_muted,
        );
        host.set_route_muted(ids::DISCORD, ids::TO_CLAUDE, !on);
        // Your microphone always reaches Discord (Discord uses our cable as
        // its microphone). To Claude it follows the big switch and, if
        // push-to-talk is on, the key.
        host.set_route_muted(
            ids::MIC,
            ids::TO_CLAUDE,
            !on || (cfg.push_to_talk && !ptt_down),
        );
        host.set_route_muted(ids::MIC, ids::TO_DISCORD, false);
        // echo reduction: always, never, or automatically with speakers
        let echo = match cfg.echo {
            crate::config::EchoMode::On => true,
            crate::config::EchoMode::Off => false,
            crate::config::EchoMode::Auto => state.speakers.load(Ordering::Relaxed),
        };
        // CI builds only: force echo reduction on or off for the test run
        #[cfg(feature = "ci-testpage")]
        let echo = match std::env::var("CLAUDE_TALK_TEST_ECHO").as_deref() {
            Ok("on") => true,
            Ok("off") => false,
            _ => echo,
        };
        host.set_echo_reduction(echo);
    }

    fn level(peak: f32, rms: f32) -> Level {
        Level { peak, rms }
    }

    fn status_loop(app: AppHandle) {
        let state = app.state::<AppState>().inner().clone();
        let mut last_states: Vec<(u32, StreamState)> = Vec::new();
        // diagnostics for the log: loudest peak per source/output over ~5 s
        let mut peaks: std::collections::BTreeMap<String, f32> = Default::default();
        let mut tick: u32 = 0;
        while !state.shutting_down.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(33));
            let (snap, echo_active) = {
                let mut g = state.audio.lock().unwrap_or_else(|p| p.into_inner());
                let Some(host) = g.as_mut() else { continue };
                host.maintain();
                (host.snapshot(), host.echo_reduction_active())
            };
            tick += 1;
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
            if tick.is_multiple_of(150) {
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
                            "{}>{}{} buf={:.0}ms under={} drift={:.0}",
                            r.source,
                            r.output,
                            if r.muted { "(muted)" } else { "" },
                            r.buffered_ms,
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
            // capture period (10 ms) + route buffer + render period (10 ms)
            st.latency_ms = worst_latency + 20.0;
            st.discord_uses_cable = match state.discord_uses_cable.load(Ordering::Relaxed) {
                crate::discord::YES => "yes",
                crate::discord::NO => "no",
                _ => "",
            }
            .into();
            st.discord_needs_restart = state.discord_needs_restart.load(Ordering::Relaxed);
            st.speakers = state.speakers.load(Ordering::Relaxed);
            st.echo_active = echo_active;
            st.claude_window_open = state.claude_pid.load(Ordering::Relaxed) != 0;
            st.claude_listening = state.claude_mic_active.load(Ordering::Relaxed);
            let _ = app.emit("audio-status", &st);
        }
    }
}

#[cfg(windows)]
pub use imp::{apply_switches, start};

#[cfg(not(windows))]
pub fn start(_app: &AppHandle) {}
#[cfg(not(windows))]
pub fn apply_switches(_app: &AppHandle) {}
