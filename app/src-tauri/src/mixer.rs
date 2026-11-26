//! Advanced mixer: sources and outputs the user adds, presets, and keeping
//! the routing across restarts (`mixer` and `presets` in config.json).
//!
//! The standard sources (microphone, Claude, Discord) and outputs (to
//! Discord, to Claude) always exist and cannot be removed. Added sources
//! get ids from 200, added outputs from 300. The routing rules in
//! `ct_engine::graph` apply to everything: nobody hears themselves, and
//! loops are refused.

use crate::config::{Mixer, MixerOutput, MixerRoute, MixerSource, MixerSourceKind, Preset};
use crate::state::AppState;
use serde::Serialize;
use tauri::{AppHandle, State};

pub const FIRST_SOURCE: u32 = 200;
pub const FIRST_OUTPUT: u32 = 300;
/// Ids from here are internal (self-test tones).
const INTERNAL: u32 = 900;
/// Name of the built-in preset (the routing Claude Talk starts with).
pub const STANDARD: &str = "standard";

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub value: String,
    pub label: String,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Choices {
    /// Microphones and other input devices.
    pub inputs: Vec<Choice>,
    /// Programs making sound right now.
    pub programs: Vec<Choice>,
    /// Playback devices.
    pub outputs: Vec<Choice>,
}

pub fn is_added_source(id: u32) -> bool {
    (FIRST_SOURCE..INTERNAL).contains(&id)
}

pub fn is_added_output(id: u32) -> bool {
    id >= FIRST_OUTPUT
}

fn next_id(used: impl Iterator<Item = u32>, first: u32) -> u32 {
    used.filter(|&i| i >= first).max().map_or(first, |m| m + 1)
}

/// "spotify.exe" → "Spotify".
fn program_label(exe: &str) -> String {
    let stem = exe.strip_suffix(".exe").unwrap_or(exe);
    let mut c = stem.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

#[cfg(windows)]
pub mod imp {
    use super::*;
    use ct_engine::graph::{ids, Output, Party, Route, Source};
    use ct_engine::win::devices::{self, DeviceSelector, Flow};
    use ct_engine::win::host::{discord_executables, AudioHost, OutputKind, SourceKind};
    use ct_engine::win::stream::ProcessTarget;
    use std::collections::BTreeSet;

    /// The routes Claude Talk starts with.
    pub const STANDARD_ROUTES: [(u32, u32); 4] = [
        (ids::MIC, ids::TO_DISCORD),
        (ids::CLAUDE, ids::TO_DISCORD),
        (ids::MIC, ids::TO_CLAUDE),
        (ids::DISCORD, ids::TO_CLAUDE),
    ];

    /// Who a program's sound belongs to: Discord and the Claude window keep
    /// their own identity, so the echo locks still protect them.
    fn party_of_program(exe: &str) -> Party {
        let e = exe.to_lowercase();
        if discord_executables().contains(&e) {
            Party::Discord
        } else if e == "msedgewebview2.exe" || e == "claude.exe" {
            // the Claude window, and the Claude app: Claude's voice
            Party::Claude
        } else {
            Party::App(e)
        }
    }

    /// Claude Talk's own program name (the installed `claude-talk.exe`, the
    /// portable `Claude Talk.exe`, or a renamed copy): never a mixer source –
    /// it plays into the cable and hosts the Claude window, so capturing it
    /// would feed our own outputs back into themselves.
    fn own_exe() -> Vec<String> {
        let mut v = vec!["claude-talk.exe".to_string(), "claude talk.exe".to_string()];
        if let Some(n) = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|f| f.to_string_lossy().to_lowercase()))
        {
            v.push(n);
        }
        v
    }

    /// `new`: added by the user just now (the device must be there to be
    /// checked). A saved source was checked when it was added; it waits for
    /// its device like any other stream.
    pub fn add_source(host: &mut AudioHost, s: &MixerSource, new: bool) -> Result<(), String> {
        if let MixerSourceKind::Program { exe } = &s.kind {
            let e = exe.to_lowercase();
            if own_exe().contains(&e) || e == "msedgewebview2.exe" {
                return Err("error.ownProgram".into());
            }
        }
        if let (MixerSourceKind::Input { device_id }, true) = (&s.kind, new) {
            // never one of Claude Talk's own devices (a loop the graph can't see)
            // (a device that isn't there now can't be checked: refused)
            let name = ct_engine::win::devices::resolve(&DeviceSelector::Id(device_id.clone()))
                .map(|(_, e)| e.name.to_lowercase())
                .map_err(|_| "error.generic".to_string())?;
            if ct_engine::win::devices::NOT_A_MICROPHONE
                .iter()
                .any(|p| name.contains(&p.to_lowercase()))
            {
                return Err("error.ownProgram".into());
            }
        }
        let (party, kind) = match &s.kind {
            MixerSourceKind::Input { device_id } => (
                Party::User,
                SourceKind::Microphone(DeviceSelector::Id(device_id.clone())),
            ),
            MixerSourceKind::Program { exe } => (
                party_of_program(exe),
                SourceKind::Program(ProcessTarget::ExeNames(vec![exe.to_lowercase()])),
            ),
        };
        let spec = Source {
            id: s.id,
            label: s.label.clone(),
            parties: BTreeSet::from([party]),
            contains_outputs: BTreeSet::new(),
        };
        host.add_source(spec, kind)
            .map_err(|e| format!("error.{}", e.key()))
    }

    pub fn add_output(host: &mut AudioHost, o: &MixerOutput) {
        let spec = Output {
            id: o.id,
            label: o.label.clone(),
            heard_by: BTreeSet::from([Party::User]),
        };
        host.add_output(
            spec,
            OutputKind::Device(DeviceSelector::Id(o.device_id.clone())),
        );
    }

    /// Makes the routing equal to `routes` (`None`: the standard routing).
    fn set_routes(host: &mut AudioHost, routes: Option<&Vec<MixerRoute>>) {
        let wanted: Vec<(u32, u32, f32)> = match routes {
            Some(r) => r.iter().map(|r| (r.source, r.output, r.gain_db)).collect(),
            None => STANDARD_ROUTES.iter().map(|&(s, o)| (s, o, 0.0)).collect(),
        };
        let current: Vec<(u32, u32)> = host
            .graph()
            .routes()
            .filter(|(s, o, _)| *s < INTERNAL && !is_external(*s, *o))
            .map(|(s, o, _)| (s, o))
            .collect();
        for (s, o) in current {
            if !wanted.iter().any(|w| w.0 == s && w.1 == o) {
                host.remove_route(s, o);
            }
        }
        for (s, o, gain_db) in wanted {
            if let Err(e) = host.set_route(
                s,
                o,
                Route {
                    gain_db,
                    muted: crate::audio::starts_muted(s, o),
                },
            ) {
                log::info!("mixer: route {s}->{o} not restored ({})", e.key());
            }
        }
    }

    /// Builds the saved mixer on a freshly started host.
    pub fn apply(host: &mut AudioHost, m: &Mixer, unlocked: bool) {
        host.set_self_hearing_unlocked(unlocked);
        for s in &m.sources {
            if let Err(e) = add_source(host, s, false) {
                log::warn!("mixer: source '{}' not added ({e})", s.label);
            }
        }
        for o in &m.outputs {
            add_output(host, o);
        }
        if m.routes.is_some() {
            set_routes(host, m.routes.as_ref());
        }
    }

    /// Switches from one mixer to another while running (presets).
    pub fn switch(host: &mut AudioHost, from: &Mixer, to: &Mixer) {
        for s in &from.sources {
            host.remove_source(s.id);
        }
        for o in &from.outputs {
            host.remove_output(o.id);
        }
        for s in &to.sources {
            if let Err(e) = add_source(host, s, false) {
                log::warn!("mixer: source '{}' not added ({e})", s.label);
            }
        }
        for o in &to.outputs {
            add_output(host, o);
        }
        set_routes(host, to.routes.as_ref());
    }

    /// The routing as it is now, for saving.
    pub fn routes_of(host: &AudioHost) -> Vec<MixerRoute> {
        host.graph()
            .routes()
            .filter(|(s, o, _)| *s < INTERNAL && !is_external(*s, *o))
            .map(|(source, output, r)| MixerRoute {
                source,
                output,
                gain_db: r.gain_db,
            })
            .collect()
    }

    /// Routes of "Claude in another program": owned by that setting, never
    /// by presets or the saved routing.
    pub fn is_external(s: u32, o: u32) -> bool {
        s == EXTERNAL_SRC || o == EXTERNAL_OUT
    }

    /// Output to the extra cable (Claude in another program hears it) and
    /// the source that is that program's sound.
    pub const EXTERNAL_OUT: u32 = 190;
    pub const EXTERNAL_SRC: u32 = 191;

    pub fn set_external_claude(host: &mut AudioHost, exe: Option<&str>) -> Result<(), String> {
        host.remove_source(EXTERNAL_SRC);
        host.remove_output(EXTERNAL_OUT);
        let Some(exe) = exe else { return Ok(()) };
        let Some(cable) = ct_engine::win::host::extra_cable() else {
            log::warn!("external Claude: no extra virtual cable");
            return Err("error.noExtraCable".into());
        };
        host.add_output(
            Output {
                id: EXTERNAL_OUT,
                label: "Til Claude (kabel)".into(),
                heard_by: BTreeSet::from([Party::Claude]),
            },
            OutputKind::Device(DeviceSelector::Id(cable.render.id.clone())),
        );
        let label = match exe {
            "claude.exe" => "Claude-appen",
            "chrome.exe" => "Claude i Chrome",
            _ => "Claude i Edge",
        };
        host.add_source(
            Source {
                id: EXTERNAL_SRC,
                label: label.into(),
                parties: BTreeSet::from([Party::Claude]),
                contains_outputs: BTreeSet::new(),
            },
            SourceKind::Program(ProcessTarget::ExeNames(vec![exe.to_string()])),
        )
        .map_err(|e| format!("error.{}", e.key()))?;
        for (s, o) in [
            (ids::MIC, EXTERNAL_OUT),
            (ids::DISCORD, EXTERNAL_OUT),
            (EXTERNAL_SRC, ids::TO_DISCORD),
        ] {
            let route = Route {
                muted: true, // opened by apply_switches
                ..Route::default()
            };
            host.set_route(s, o, route)
                .map_err(|e| format!("error.{}", e.key()))?;
        }
        log::info!("external Claude: {exe} via '{}'", cable.render.name);
        Ok(())
    }

    pub fn choices() -> Choices {
        // Inputs: real microphones only – "Stereo Mix" and similar carry what
        // the computer plays (including Claude and Discord) and would let
        // people hear themselves. Outputs: anything but our own cable.
        let pick = |flow, avoid: &[&str]| {
            let avoid = ct_engine::win::devices::patterns(avoid);
            devices::list(flow)
                .unwrap_or_default()
                .into_iter()
                .filter(|e| {
                    let n = e.name.to_lowercase();
                    !avoid.iter().any(|p| n.contains(p.as_str()))
                })
                .map(|e| Choice {
                    value: e.id,
                    label: e.name,
                })
                .collect::<Vec<_>>()
        };
        let mut skip: Vec<String> = discord_executables();
        skip.extend(["msedgewebview2.exe".to_string()]);
        skip.extend(own_exe());
        let mut programs: Vec<Choice> = Vec::new();
        for s in ct_engine::win::sessions::sessions(Flow::Render).unwrap_or_default() {
            let exe = s.exe.to_lowercase();
            if exe.is_empty() || skip.contains(&exe) || programs.iter().any(|p| p.value == exe) {
                continue;
            }
            programs.push(Choice {
                label: program_label(&exe),
                value: exe,
            });
        }
        programs.sort_by_key(|a| a.label.to_lowercase());
        Choices {
            inputs: pick(Flow::Capture, ct_engine::win::devices::NOT_A_MICROPHONE),
            programs,
            // our cable, and the extra cables (Claude's microphone in
            // Chrome/app mode): never an output "you" listen to
            outputs: pick(
                Flow::Render,
                &[
                    ct_engine::win::devices::OWN_VIRTUAL_DEVICES,
                    &ct_engine::win::host::EXTRA_CABLE_PATTERNS[..],
                ]
                .concat(),
            ),
        }
    }
}

/// Programs that can be "Claude" in the external mode.
pub const EXTERNAL_PROGRAMS: [&str; 3] = ["claude.exe", "chrome.exe", "msedge.exe"];

/// Turns "Use Claude in Chrome or the app" on (with that program) or off.
fn set_external_claude_blocking(
    app: &AppHandle,
    state: &AppState,
    exe: Option<String>,
) -> Result<(), String> {
    let exe = exe.map(|e| e.to_lowercase());
    if exe
        .as_deref()
        .is_some_and(|e| !EXTERNAL_PROGRAMS.contains(&e))
    {
        return Err("error.internal".into());
    }
    #[cfg(windows)]
    {
        if let Some(h) = state
            .audio
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_mut()
        {
            if let Err(e) = imp::set_external_claude(h, exe.as_deref()) {
                // the old routes are gone already: the mode is off now, and
                // the settings must say so (or the mic stays open for good)
                state
                    .config
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .external_claude = None;
                state.save_config();
                return Err(e);
            }
        }
        crate::audio::apply_switches(app);
    }
    #[cfg(not(windows))]
    let _ = &app;
    state
        .config
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .external_claude = exe;
    state.save_config();
    remember_routes(state);
    Ok(())
}

/// Saves the current routing into the config (after any change in the mixer).
fn remember_routes(state: &AppState) {
    #[cfg(windows)]
    {
        let routes = state
            .audio
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .map(imp::routes_of);
        if let Some(r) = routes {
            state
                .config
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .mixer
                .routes = Some(r);
            state.save_config();
        }
    }
    #[cfg(not(windows))]
    let _ = state;
}

/// Called by `set_route` after a cell changed.
pub fn route_changed(state: &AppState) {
    remember_routes(state);
}

#[tauri::command]
pub fn mixer_choices() -> Choices {
    #[cfg(windows)]
    return imp::choices();
    #[cfg(not(windows))]
    Choices::default()
}

/// `kind`: "input" (value = device id) or "program" (value = exe name).
fn add_mixer_source_blocking(
    state: &AppState,
    kind: String,
    value: String,
    label: String,
) -> Result<u32, String> {
    let kind = match kind.as_str() {
        "input" => MixerSourceKind::Input { device_id: value },
        "program" => MixerSourceKind::Program {
            exe: value.to_lowercase(),
        },
        _ => return Err("error.internal".into()),
    };
    // one add at a time: a double click would give two entries one id
    let _one = ADDING.lock().unwrap_or_else(|p| p.into_inner());
    let id = {
        let c = state.config.lock().unwrap_or_else(|p| p.into_inner());
        next_id(c.mixer.sources.iter().map(|s| s.id), FIRST_SOURCE)
    };
    let label = if label.trim().is_empty() {
        match &kind {
            MixerSourceKind::Program { exe } => program_label(exe),
            MixerSourceKind::Input { .. } => format!("Kilde {}", id - FIRST_SOURCE + 1),
        }
    } else {
        label.trim().chars().take(40).collect()
    };
    let src = MixerSource { id, label, kind };
    #[cfg(windows)]
    {
        let mut g = state.audio.lock().unwrap_or_else(|p| p.into_inner());
        let host = g.as_mut().ok_or("error.internal")?;
        imp::add_source(host, &src, true)?;
    }
    log::info!("mixer: added source {id} '{}'", src.label);
    state
        .config
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .mixer
        .sources
        .push(src);
    remember_routes(state);
    Ok(id)
}

fn remove_mixer_source_blocking(state: &AppState, id: u32) -> Result<(), String> {
    if !is_added_source(id) {
        return Err("error.internal".into());
    }
    #[cfg(windows)]
    if let Some(h) = state
        .audio
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_mut()
    {
        h.remove_source(id);
    }
    state
        .config
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .mixer
        .sources
        .retain(|s| s.id != id);
    remember_routes(state);
    Ok(())
}

/// Held while a source or output is added (ids are handed out one at a time).
static ADDING: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn add_mixer_output_blocking(
    state: &AppState,
    device_id: String,
    label: String,
) -> Result<u32, String> {
    let _one = ADDING.lock().unwrap_or_else(|p| p.into_inner());
    let id = {
        let c = state.config.lock().unwrap_or_else(|p| p.into_inner());
        next_id(c.mixer.outputs.iter().map(|o| o.id), FIRST_OUTPUT)
    };
    let label = if label.trim().is_empty() {
        format!("Udgang {}", id - FIRST_OUTPUT + 1)
    } else {
        label.trim().chars().take(40).collect()
    };
    let out = MixerOutput {
        id,
        label,
        device_id,
    };
    #[cfg(windows)]
    {
        let mut g = state.audio.lock().unwrap_or_else(|p| p.into_inner());
        let host = g.as_mut().ok_or("error.internal")?;
        imp::add_output(host, &out);
    }
    log::info!("mixer: added output {id} '{}'", out.label);
    state
        .config
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .mixer
        .outputs
        .push(out);
    remember_routes(state);
    Ok(id)
}

fn remove_mixer_output_blocking(state: &AppState, id: u32) -> Result<(), String> {
    if !is_added_output(id) {
        return Err("error.internal".into());
    }
    #[cfg(windows)]
    if let Some(h) = state
        .audio
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_mut()
    {
        h.remove_output(id);
    }
    state
        .config
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .mixer
        .outputs
        .retain(|o| o.id != id);
    remember_routes(state);
    Ok(())
}

#[tauri::command]
pub fn save_preset(state: State<AppState>, name: String) -> Result<(), String> {
    let name: String = name.trim().chars().take(40).collect();
    if name.is_empty() || name.eq_ignore_ascii_case(STANDARD) {
        return Err("error.presetName".into());
    }
    remember_routes(&state);
    let mut c = state.config.lock().unwrap_or_else(|p| p.into_inner());
    let mixer = c.mixer.clone();
    match c.presets.iter_mut().find(|p| p.name == name) {
        Some(p) => p.mixer = mixer,
        None => c.presets.push(Preset { name, mixer }),
    }
    drop(c);
    state.save_config();
    Ok(())
}

/// Switches to a saved preset, or to the standard routing ("standard").
fn load_preset_blocking(app: &AppHandle, state: &AppState, name: String) -> Result<(), String> {
    let (from, to) = {
        let c = state.config.lock().unwrap_or_else(|p| p.into_inner());
        let to = if name == STANDARD {
            Mixer::default()
        } else {
            c.presets
                .iter()
                .find(|p| p.name == name)
                .map(|p| p.mixer.clone())
                .ok_or("error.internal")?
        };
        (c.mixer.clone(), to)
    };
    #[cfg(windows)]
    {
        if let Some(h) = state
            .audio
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_mut()
        {
            imp::switch(h, &from, &to);
        }
        crate::audio::apply_switches(app);
    }
    #[cfg(not(windows))]
    let _ = (&app, &from);
    log::info!("mixer: preset '{name}'");
    state.config.lock().unwrap_or_else(|p| p.into_inner()).mixer = to;
    state.save_config();
    Ok(())
}

#[tauri::command]
pub fn delete_preset(state: State<AppState>, name: String) {
    state
        .config
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .presets
        .retain(|p| p.name != name);
    state.save_config();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_labels() {
        assert_eq!(next_id([].into_iter(), FIRST_SOURCE), 200);
        assert_eq!(next_id([1, 2, 200, 204].into_iter(), FIRST_SOURCE), 205);
        assert_eq!(next_id([101, 102].into_iter(), FIRST_OUTPUT), 300);
        assert_eq!(program_label("spotify.exe"), "Spotify");
        assert_eq!(program_label("vlc"), "Vlc");
        assert!(is_added_source(200) && !is_added_source(3) && !is_added_source(900));
        assert!(is_added_output(300) && !is_added_output(101));
    }
}

// The commands below run on a worker thread: closing audio streams can take
// a moment, and the window must not freeze meanwhile.
async fn off_main<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| "error.internal".to_string())?
}

#[tauri::command]
pub async fn set_external_claude(
    app: AppHandle,
    state: State<'_, AppState>,
    exe: Option<String>,
) -> Result<(), String> {
    let st = state.inner().clone();
    let app2 = app.clone();
    let st2 = st.clone();
    let r = off_main(move || set_external_claude_blocking(&app, &st, exe)).await;
    // every page shows the mode that is really in force (also after an error)
    if let Ok(cfg) = st2.config.lock().map(|c| c.clone()) {
        use tauri::Emitter;
        let _ = app2.emit("config-changed", cfg);
    }
    r
}

#[tauri::command]
pub async fn add_mixer_source(
    state: State<'_, AppState>,
    kind: String,
    value: String,
    label: String,
) -> Result<u32, String> {
    let st = state.inner().clone();
    off_main(move || add_mixer_source_blocking(&st, kind, value, label)).await
}

#[tauri::command]
pub async fn remove_mixer_source(state: State<'_, AppState>, id: u32) -> Result<(), String> {
    let st = state.inner().clone();
    off_main(move || remove_mixer_source_blocking(&st, id)).await
}

#[tauri::command]
pub async fn add_mixer_output(
    state: State<'_, AppState>,
    device_id: String,
    label: String,
) -> Result<u32, String> {
    let st = state.inner().clone();
    off_main(move || add_mixer_output_blocking(&st, device_id, label)).await
}

#[tauri::command]
pub async fn remove_mixer_output(state: State<'_, AppState>, id: u32) -> Result<(), String> {
    let st = state.inner().clone();
    off_main(move || remove_mixer_output_blocking(&st, id)).await
}

#[tauri::command]
pub async fn load_preset(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    let st = state.inner().clone();
    off_main(move || load_preset_blocking(&app, &st, name)).await
}
