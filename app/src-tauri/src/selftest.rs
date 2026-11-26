//! "Run self-test": a distinct short tone is sent along each route (from a
//! test source with the same rules as the real one) and we check it arrives
//! where it should – and nowhere else. "Til Discord" is checked by recording
//! the virtual cable, exactly what Discord hears; "Til Claude" by reading what
//! the bridge sends to the Claude window.

use serde::Serialize;
use tauri::AppHandle;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RouteResult {
    pub route: String,
    pub passed: bool,
    pub level_db: f64,
    pub leak_db: f64,
}

#[cfg(windows)]
pub fn run(app: &AppHandle) -> Result<Vec<RouteResult>, String> {
    use crate::state::AppState;
    use ct_engine::analysis::{mono, tone_db_near, tone_tolerance, Sine};
    use ct_engine::graph::{ids, Party, Route, Source};
    use ct_engine::mixer::{connect, GainParams, OutputMixer, SourceFanout};
    use ct_engine::win::devices::{DeviceSelector, Flow};
    use ct_engine::win::stream::{spawn_capture, CaptureTarget};
    use ct_engine::win::ComGuard;
    use ct_engine::{ENGINE_CHANNELS, ENGINE_RATE};
    use std::collections::BTreeSet;
    use std::time::Duration;
    use tauri::Manager;

    let _com = ComGuard::new();
    let state = app.state::<AppState>().inner().clone();
    // The tones would be heard in a call, or by Claude in a voice
    // conversation: not while Discord records, Discord's state is not known
    // yet, or the Claude page uses its microphone.
    {
        use std::sync::atomic::Ordering::Relaxed;
        let busy = state.discord_recording.load(Relaxed)
            || state.discord_uses_cable.load(Relaxed) == crate::discord::YES
            || state.claude_mic_active.load(Relaxed);
        if busy {
            return Err("error.selftestInCall".into());
        }
    }
    // one at a time (the test sources share their ids)
    static RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if RUNNING.swap(true, std::sync::atomic::Ordering::AcqRel) {
        return Err("error.selftestRunning".into());
    }
    struct Done;
    impl Drop for Done {
        fn drop(&mut self) {
            RUNNING.store(false, std::sync::atomic::Ordering::Release);
        }
    }
    let _done = Done;
    // the test sources go again whatever happens below (error, panic)
    struct Cleanup(AppState, usize);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            if let Some(p) = self.0.bridge_probe.lock().ok().as_mut() {
                **p = None;
            }
            let mut g = self.0.audio.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(host) = g.as_mut() {
                for i in 0..self.1 {
                    host.remove_source(900 + i as u32);
                }
            }
        }
    }
    // (route name, party, output, tone)
    let tests = [
        ("micToDiscord", Party::User, ids::TO_DISCORD, 700.0),
        ("claudeToDiscord", Party::Claude, ids::TO_DISCORD, 1100.0),
        ("micToClaude", Party::User, ids::TO_CLAUDE, 1700.0),
        ("discordToClaude", Party::Discord, ids::TO_CLAUDE, 2300.0),
    ];
    let cleanup = Cleanup(state.clone(), tests.len());
    {
        let mut g = state.audio.lock().map_err(|_| "error.internal")?;
        let host = g.as_mut().ok_or("error.internal")?;
        for (i, (name, party, out, f)) in tests.iter().enumerate() {
            let id = 900 + i as u32;
            let mut sine = Sine::new(*f, ENGINE_RATE as f64, 0.25);
            host.add_generated_source(
                Source {
                    id,
                    label: format!("selftest-{name}"),
                    parties: BTreeSet::from([party.clone()]),
                    contains_outputs: BTreeSet::new(),
                },
                move |b| sine.fill(b, ENGINE_CHANNELS),
            )
            .map_err(|_| "error.internal")?;
            host.set_route(id, *out, Route::default())
                .map_err(|_| "error.internal")?;
        }
    }
    // record the cable (what Discord hears) and the bridge (what Claude hears)
    *state.bridge_probe.lock().map_err(|_| "error.internal")? = Some(Vec::new());
    let (fan, mut fan_h) = SourceFanout::new(1, ENGINE_RATE);
    let (mut mix, mut mix_h) = OutputMixer::new(2, ENGINE_CHANNELS, 480);
    connect(&mut fan_h, &mut mix_h, GainParams::new(0.0, false), 30.0);
    let patterns = ct_engine::win::host::cable_capture_patterns();
    let cap = spawn_capture(
        "selftest",
        CaptureTarget::Device(DeviceSelector::FirstMatching(Flow::Capture, patterns)),
        fan,
    );
    let mut cable = Vec::new();
    let mut buf = vec![0f32; 480 * ENGINE_CHANNELS];
    let t0 = std::time::Instant::now();
    let mut pulled = 0u64;
    while t0.elapsed() < Duration::from_millis(2200) {
        let due = (t0.elapsed().as_secs_f64() * ENGINE_RATE as f64) as u64;
        while pulled + 480 <= due {
            mix.render(&mut buf);
            cable.extend_from_slice(&buf);
            pulled += 480;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    drop(cap);
    drop(fan_h);
    let claude = state
        .bridge_probe
        .lock()
        .map_err(|_| "error.internal")?
        .take()
        .unwrap_or_default();
    drop(cleanup);
    // analyse the last second
    let tail = |v: &[f32], ch: usize| {
        let m = mono(v, ch);
        let n = m.len().min(ENGINE_RATE as usize);
        m[m.len() - n..].to_vec()
    };
    let to_discord = tail(&cable, ENGINE_CHANNELS);
    let to_claude = tail(&claude, 1);
    let rate = ENGINE_RATE as f64;
    let mut out = Vec::new();
    for (name, _, output, f) in tests {
        let (here, there) = if output == ids::TO_DISCORD {
            (&to_discord, &to_claude)
        } else {
            (&to_claude, &to_discord)
        };
        let level = tone_db_near(here, f, tone_tolerance(f), rate);
        let leak = tone_db_near(there, f, tone_tolerance(f), rate);
        let passed = level > -30.0 && leak < level - 60.0;
        out.push(RouteResult {
            route: name.into(),
            passed,
            level_db: level,
            leak_db: leak,
        });
    }
    log::info!("self-test: {out:?}");
    Ok(out)
}

#[cfg(not(windows))]
pub fn run(_app: &AppHandle) -> Result<Vec<RouteResult>, String> {
    Err("error.internal".into())
}
