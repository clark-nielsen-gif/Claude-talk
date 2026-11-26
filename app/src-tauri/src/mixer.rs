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
    use ct_engine::win::host::{
        discord_executables, is_own_device, AudioHost, OutputKind, SourceKind,
    };
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
        } else if e == "msedgewebview2.exe" {
            Party::Claude
        } else {
            Party::App(e)
        }
    }

    pub fn add_source(host: &mut AudioHost, s: &MixerSource) -> Result<(), String> {
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
            .filter(|(s, _, _)| *s < INTERNAL)
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
                    muted: false,
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
            if let Err(e) = add_source(host, s) {
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
            if let Err(e) = add_source(host, s) {
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
            .filter(|(s, _, _)| *s < INTERNAL)
            .map(|(source, output, r)| MixerRoute {
                source,
                output,
                gain_db: r.gain_db,
            })
            .collect()
    }

    pub fn choices() -> Choices {
        let devs = |flow| {
            devices::list(flow)
                .unwrap_or_default()
                .into_iter()
                .filter(|e| !is_own_device(&e.name))
                .map(|e| Choice {
                    value: e.id,
                    label: e.name,
                })
                .collect::<Vec<_>>()
        };
        let mut skip: Vec<String> = discord_executables();
        skip.extend([
            "claude-talk.exe".to_string(),
            "msedgewebview2.exe".to_string(),
        ]);
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
            inputs: devs(Flow::Capture),
            programs,
            outputs: devs(Flow::Render),
        }
    }
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
#[tauri::command]
pub fn add_mixer_source(
    state: State<AppState>,
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
        imp::add_source(host, &src)?;
    }
    log::info!("mixer: added source {id} '{}'", src.label);
    state
        .config
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .mixer
        .sources
        .push(src);
    remember_routes(&state);
    Ok(id)
}

#[tauri::command]
pub fn remove_mixer_source(state: State<AppState>, id: u32) -> Result<(), String> {
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
    remember_routes(&state);
    Ok(())
}

#[tauri::command]
pub fn add_mixer_output(
    state: State<AppState>,
    device_id: String,
    label: String,
) -> Result<u32, String> {
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
    remember_routes(&state);
    Ok(id)
}

#[tauri::command]
pub fn remove_mixer_output(state: State<AppState>, id: u32) -> Result<(), String> {
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
    remember_routes(&state);
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
#[tauri::command]
pub fn load_preset(app: AppHandle, state: State<AppState>, name: String) -> Result<(), String> {
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
        crate::audio::apply_switches(&app);
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
