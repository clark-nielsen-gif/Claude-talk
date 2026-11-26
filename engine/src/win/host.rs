//! The running engine: sources, outputs and routes, kept consistent with the
//! [`RoutingGraph`] rules. Used by the app and by the CI test tool.

use super::devices::{self, DeviceSelector, Flow, OWN_VIRTUAL_DEVICES};
use super::stream::{spawn_capture, spawn_render, CaptureTarget, ProcessTarget, StreamHandle, StreamState};
use crate::graph::{ids, Output, OutputId, Party, Rejection, Route, RoutingGraph, Source, SourceId};
use crate::mixer::{connect, disconnect, FanoutHandle, GainParams, MixerHandle, OutputMixer, SourceFanout, TapStats};
use crate::{ENGINE_CHANNELS, ENGINE_RATE};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Buffer each route keeps against scheduling jitter. WASAPI shared mode
/// delivers 10 ms periods; 20 ms of margin holds through normal system load
/// while keeping the end-to-end latency inside the 40 ms budget.
pub const ROUTE_BUFFER_MS: f64 = 20.0;

/// Executables whose sound is "Discord".
pub fn discord_executables() -> Vec<String> {
    ["discord.exe", "discordptb.exe", "discordcanary.exe", "discorddevelopment.exe"].iter().map(|s| s.to_string()).collect()
}

/// Render endpoint names of the virtual cable, most preferred first.
pub fn cable_render_patterns() -> Vec<String> {
    ["claude talk – intern", "cable input", "vb-audio virtual cable"].iter().map(|s| s.to_string()).collect()
}

#[derive(Clone, Debug)]
pub enum SourceKind {
    Microphone(DeviceSelector),
    DeviceLoopback(DeviceSelector),
    Program(ProcessTarget),
}

#[derive(Clone, Debug)]
pub enum OutputKind {
    /// Played into a render device (the virtual cable, headphones…).
    Device(DeviceSelector),
    /// Pulled by the app (the Claude window bridge).
    Pulled { channels: usize },
}

pub struct HostConfig {
    pub mic: DeviceSelector,
    pub to_discord: DeviceSelector,
    pub discord: ProcessTarget,
    /// PID of the Claude window's WebView2 browser process (0 = not open).
    pub claude_pid: Arc<AtomicU32>,
}

impl HostConfig {
    pub fn standard(claude_pid: Arc<AtomicU32>) -> Self {
        let own: Vec<String> = OWN_VIRTUAL_DEVICES.iter().map(|s| s.to_string()).collect();
        HostConfig {
            mic: DeviceSelector::DefaultExcluding(Flow::Capture, own),
            to_discord: DeviceSelector::FirstMatching(Flow::Render, cable_render_patterns()),
            discord: ProcessTarget::ExeNames(discord_executables()),
            claude_pid,
        }
    }
}

struct SourceEntry {
    handle: FanoutHandle,
    stream: StreamHandle,
}

struct OutputEntry {
    handle: MixerHandle,
    stream: Option<StreamHandle>,
}

struct RouteEntry {
    params: Arc<GainParams>,
    stats: Arc<TapStats>,
}

#[derive(Clone, Debug)]
pub struct StreamInfo {
    pub state: StreamState,
    pub device: String,
    pub pid: u32,
    pub last_error: u32,
    pub restarts: u32,
}

#[derive(Clone, Debug)]
pub struct SourceSnapshot {
    pub id: SourceId,
    pub label: String,
    pub peak: f32,
    pub rms: f32,
    pub muted: bool,
    pub gain_db: f32,
    pub overflow_frames: u64,
    pub stream: StreamInfo,
}

#[derive(Clone, Debug)]
pub struct OutputSnapshot {
    pub id: OutputId,
    pub label: String,
    pub peak: f32,
    pub rms: f32,
    pub muted: bool,
    pub gain_db: f32,
    pub limiter_reduction: f32,
    pub stream: Option<StreamInfo>,
}

#[derive(Clone, Debug)]
pub struct RouteSnapshot {
    pub source: SourceId,
    pub output: OutputId,
    pub gain_db: f32,
    pub muted: bool,
    pub underrun_frames: u64,
    pub drift_ppm: f32,
    pub buffered_ms: f32,
}

#[derive(Clone, Debug, Default)]
pub struct HostSnapshot {
    pub sources: Vec<SourceSnapshot>,
    pub outputs: Vec<OutputSnapshot>,
    pub routes: Vec<RouteSnapshot>,
}

pub struct AudioHost {
    graph: RoutingGraph,
    sources: BTreeMap<SourceId, SourceEntry>,
    outputs: BTreeMap<OutputId, OutputEntry>,
    routes: BTreeMap<(SourceId, OutputId), RouteEntry>,
    pulled: BTreeMap<OutputId, OutputMixer>,
}

fn info(s: &StreamHandle) -> StreamInfo {
    StreamInfo {
        state: s.status.state(),
        device: s.status.device_name.lock().map(|n| n.clone()).unwrap_or_default(),
        pid: s.status.pid.load(Ordering::Relaxed),
        last_error: s.status.last_error.load(Ordering::Relaxed) as u32,
        restarts: s.status.restarts.load(Ordering::Relaxed),
    }
}

impl AudioHost {
    /// An empty host (for the advanced mixer and tests).
    pub fn empty() -> Self {
        AudioHost { graph: RoutingGraph::new(), sources: BTreeMap::new(), outputs: BTreeMap::new(), routes: BTreeMap::new(), pulled: BTreeMap::new() }
    }

    /// The standard Claude Talk setup (see `graph::RoutingGraph::claude_talk_default`).
    pub fn standard(cfg: HostConfig) -> Self {
        let mut h = AudioHost::empty();
        let set = |p: &[Party]| p.iter().cloned().collect::<BTreeSet<_>>();
        h.add_output(Output { id: ids::TO_DISCORD, label: "Til Discord".into(), heard_by: set(&[Party::Discord]) }, OutputKind::Device(cfg.to_discord));
        h.add_output(Output { id: ids::TO_CLAUDE, label: "Til Claude".into(), heard_by: set(&[Party::Claude]) }, OutputKind::Pulled { channels: 1 });
        let none = BTreeSet::new;
        let _ = h.add_source(Source { id: ids::MIC, label: "Mikrofon".into(), parties: set(&[Party::User]), contains_outputs: none() }, SourceKind::Microphone(cfg.mic));
        let _ = h.add_source(
            Source { id: ids::CLAUDE, label: "Claude".into(), parties: set(&[Party::Claude]), contains_outputs: none() },
            SourceKind::Program(ProcessTarget::Pid(cfg.claude_pid)),
        );
        let _ = h.add_source(
            Source { id: ids::DISCORD, label: "Discord".into(), parties: set(&[Party::Discord]), contains_outputs: none() },
            SourceKind::Program(cfg.discord),
        );
        for (s, o) in [(ids::MIC, ids::TO_DISCORD), (ids::CLAUDE, ids::TO_DISCORD), (ids::MIC, ids::TO_CLAUDE), (ids::DISCORD, ids::TO_CLAUDE)] {
            let _ = h.set_route(s, o, Route::default());
        }
        h
    }

    pub fn graph(&self) -> &RoutingGraph {
        &self.graph
    }

    pub fn add_output(&mut self, spec: Output, kind: OutputKind) {
        let id = spec.id;
        self.remove_output(id);
        self.graph.add_output(spec);
        match kind {
            OutputKind::Device(sel) => {
                let (mixer, handle) = OutputMixer::new(id, ENGINE_CHANNELS, 480);
                let stream = spawn_render("out", sel, mixer);
                self.outputs.insert(id, OutputEntry { handle, stream: Some(stream) });
            }
            OutputKind::Pulled { channels } => {
                let (mixer, handle) = OutputMixer::new(id, channels, 480);
                self.pulled.insert(id, mixer);
                self.outputs.insert(id, OutputEntry { handle, stream: None });
            }
        }
    }

    /// Hands a pulled output's mixer to whoever drives it (the Claude bridge).
    pub fn take_pulled(&mut self, id: OutputId) -> Option<OutputMixer> {
        self.pulled.remove(&id)
    }

    pub fn add_source(&mut self, spec: Source, kind: SourceKind) -> Result<(), Rejection> {
        let id = spec.id;
        self.graph.add_source(spec)?;
        if let Some(old) = self.sources.remove(&id) {
            drop(old);
        }
        let rate = ENGINE_RATE; // all capture streams deliver 48 kHz (WASAPI converts)
        let (fanout, handle) = SourceFanout::new(id, rate);
        let target = match kind {
            SourceKind::Microphone(sel) => CaptureTarget::Device(sel),
            SourceKind::DeviceLoopback(sel) => CaptureTarget::DeviceLoopback(sel),
            SourceKind::Program(p) => CaptureTarget::Process(p),
        };
        let stream = spawn_capture("src", target, fanout);
        self.sources.insert(id, SourceEntry { handle, stream });
        // re-wire existing routes of this source
        let routes: Vec<(OutputId, Route)> = self.graph.routes().filter(|(s, _, _)| *s == id).map(|(_, o, r)| (o, r)).collect();
        for (o, r) in routes {
            self.wire(id, o, r);
        }
        Ok(())
    }

    pub fn remove_source(&mut self, id: SourceId) {
        let outs: Vec<OutputId> = self.routes.keys().filter(|(s, _)| *s == id).map(|&(_, o)| o).collect();
        for o in outs {
            self.unwire(id, o);
        }
        self.graph.remove_source(id);
        self.sources.remove(&id);
    }

    pub fn remove_output(&mut self, id: OutputId) {
        let srcs: Vec<SourceId> = self.routes.keys().filter(|(_, o)| *o == id).map(|&(s, _)| s).collect();
        for s in srcs {
            self.unwire(s, id);
        }
        self.graph.remove_output(id);
        self.outputs.remove(&id);
        self.pulled.remove(&id);
    }

    fn wire(&mut self, s: SourceId, o: OutputId, r: Route) {
        let (Some(src), Some(out)) = (self.sources.get_mut(&s), self.outputs.get_mut(&o)) else { return };
        let params = GainParams::new(r.gain_db, r.muted);
        let stats = connect(&mut src.handle, &mut out.handle, params.clone(), ROUTE_BUFFER_MS);
        self.routes.insert((s, o), RouteEntry { params, stats });
    }

    fn unwire(&mut self, s: SourceId, o: OutputId) {
        if self.routes.remove(&(s, o)).is_some() {
            if let (Some(src), Some(out)) = (self.sources.get_mut(&s), self.outputs.get_mut(&o)) {
                disconnect(&mut src.handle, &mut out.handle);
            }
        }
    }

    /// Adds or updates a route (refused if it breaks the echo/feedback rules).
    pub fn set_route(&mut self, s: SourceId, o: OutputId, r: Route) -> Result<Option<Party>, Rejection> {
        let warn = self.graph.set_route(s, o, r)?;
        if let Some(e) = self.routes.get(&(s, o)) {
            e.params.gain_db.store(r.gain_db);
            e.params.muted.store(r.muted, Ordering::Relaxed);
        } else {
            self.wire(s, o, r);
        }
        Ok(warn)
    }

    pub fn remove_route(&mut self, s: SourceId, o: OutputId) {
        self.graph.remove_route(s, o);
        self.unwire(s, o);
    }

    pub fn set_route_muted(&mut self, s: SourceId, o: OutputId, muted: bool) {
        if let Some(mut r) = self.graph.route(s, o) {
            r.muted = muted;
            let _ = self.set_route(s, o, r);
        }
    }

    pub fn set_source_muted(&self, id: SourceId, muted: bool) {
        if let Some(s) = self.sources.get(&id) {
            s.handle.params.muted.store(muted, Ordering::Relaxed);
        }
    }

    pub fn set_source_gain(&self, id: SourceId, db: f32) {
        if let Some(s) = self.sources.get(&id) {
            s.handle.params.gain_db.store(db);
        }
    }

    pub fn set_output_muted(&self, id: OutputId, muted: bool) {
        if let Some(o) = self.outputs.get(&id) {
            o.handle.params.muted.store(muted, Ordering::Relaxed);
        }
    }

    pub fn set_output_gain(&self, id: OutputId, db: f32) {
        if let Some(o) = self.outputs.get(&id) {
            o.handle.params.gain_db.store(db);
        }
    }

    pub fn set_self_hearing_unlocked(&mut self, unlocked: bool) {
        let before: BTreeSet<_> = self.graph.routes().map(|(s, o, _)| (s, o)).collect();
        self.graph.set_self_hearing_unlocked(unlocked);
        let after: BTreeSet<_> = self.graph.routes().map(|(s, o, _)| (s, o)).collect();
        for (s, o) in before.difference(&after) {
            self.unwire(*s, *o);
        }
    }

    /// Makes a stream reopen its device (default device changed, etc.).
    pub fn restart_streams(&self) {
        for s in self.sources.values() {
            s.stream.request_restart();
        }
        for o in self.outputs.values() {
            if let Some(st) = &o.stream {
                st.request_restart();
            }
        }
    }

    /// Frees memory the audio threads have released. Call ~every 100 ms.
    pub fn maintain(&mut self) {
        for s in self.sources.values_mut() {
            s.handle.collect_garbage();
        }
        for o in self.outputs.values_mut() {
            o.handle.collect_garbage();
        }
    }

    /// Meters, states and route statistics (meters reset on each call).
    pub fn snapshot(&self) -> HostSnapshot {
        let mut snap = HostSnapshot::default();
        for (id, s) in &self.sources {
            let (peak, rms) = s.handle.stats.meter.read();
            snap.sources.push(SourceSnapshot {
                id: *id,
                label: self.graph.source(*id).map(|x| x.label.clone()).unwrap_or_default(),
                peak,
                rms,
                muted: s.handle.params.muted.load(Ordering::Relaxed),
                gain_db: s.handle.params.gain_db.load(),
                overflow_frames: s.handle.stats.overflow_frames.load(Ordering::Relaxed),
                stream: info(&s.stream),
            });
        }
        for (id, o) in &self.outputs {
            let (peak, rms) = o.handle.stats.meter.read();
            snap.outputs.push(OutputSnapshot {
                id: *id,
                label: self.graph.output(*id).map(|x| x.label.clone()).unwrap_or_default(),
                peak,
                rms,
                muted: o.handle.params.muted.load(Ordering::Relaxed),
                gain_db: o.handle.params.gain_db.load(),
                limiter_reduction: o.handle.stats.limiter_reduction.load(),
                stream: o.stream.as_ref().map(info),
            });
        }
        for (&(s, o), r) in &self.routes {
            snap.routes.push(RouteSnapshot {
                source: s,
                output: o,
                gain_db: r.params.gain_db.load(),
                muted: r.params.muted.load(Ordering::Relaxed),
                underrun_frames: r.stats.underrun_frames.load(Ordering::Relaxed),
                drift_ppm: r.stats.drift_ppm.load(),
                buffered_ms: r.stats.buffered_ms.load(),
            });
        }
        snap
    }
}

/// True if a capture endpoint with this name is one of ours.
pub fn is_own_device(name: &str) -> bool {
    let n = name.to_lowercase();
    OWN_VIRTUAL_DEVICES.iter().any(|p| n.contains(p))
}

/// Whether the virtual cable is installed (render side found).
pub fn cable_installed() -> bool {
    devices::resolve(&DeviceSelector::FirstMatching(Flow::Render, cable_render_patterns())).is_ok()
}
