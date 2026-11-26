//! The running engine: sources, outputs and routes, kept consistent with the
//! [`RoutingGraph`] rules. Used by the app and by the CI test tool.

use super::devices::{self, DeviceSelector, Flow, OWN_VIRTUAL_DEVICES};
use super::stream::{
    spawn_capture, spawn_render, CaptureTarget, ProcessTarget, StreamHandle, StreamState,
};
use crate::graph::{
    ids, Output, OutputId, Party, Rejection, Route, RoutingGraph, Source, SourceId,
};
use crate::mixer::{
    connect, connect_fixed, connect_raw, disconnect, FanoutHandle, GainParams, MixerHandle,
    OutputMixer, SourceFanout, TapStats,
};
use crate::{ENGINE_CHANNELS, ENGINE_RATE};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Buffer each route starts with against scheduling jitter. WASAPI shared
/// mode delivers 10 ms periods; when the capture and render clocks' phases
/// cross, one period plus jitter (±2 ms each side) plus the resampler chunk
/// is needed. A two-hour simulation (resample.rs tests) gave no dropouts at
/// 20 ms and repeated dropouts below it; the buffer grows by itself (max
/// 40 ms) when a device is burstier than that, and never shrinks back below
/// the level that ran dry.
pub const ROUTE_BUFFER_MS: f64 = 20.0;

/// Executables whose sound is "Discord".
pub fn discord_executables() -> Vec<String> {
    [
        "discord.exe",
        "discordptb.exe",
        "discordcanary.exe",
        "discorddevelopment.exe",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Render endpoint names of the virtual cable, most preferred first.
pub fn cable_render_patterns() -> Vec<String> {
    devices::patterns(devices::CABLE_RENDER_PATTERNS)
}

/// Capture endpoint names of the virtual cable (Discord's microphone).
pub fn cable_capture_patterns() -> Vec<String> {
    devices::patterns(devices::CABLE_CAPTURE_PATTERNS)
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
    /// Normally the user's microphone; CI tests replace it with a program.
    pub mic: SourceKind,
    pub to_discord: DeviceSelector,
    pub discord: ProcessTarget,
    /// PID of the Claude window's WebView2 browser process (0 = not open).
    pub claude_pid: Arc<AtomicU32>,
}

impl HostConfig {
    pub fn standard(claude_pid: Arc<AtomicU32>) -> Self {
        let own: Vec<String> = devices::patterns(devices::NOT_A_MICROPHONE);
        HostConfig {
            mic: SourceKind::Microphone(DeviceSelector::PreferredMicrophone(own)),
            to_discord: DeviceSelector::FirstMatching(Flow::Render, cable_render_patterns()),
            discord: ProcessTarget::ExeNames(discord_executables()),
            claude_pid,
        }
    }
}

struct SourceEntry {
    handle: FanoutHandle,
    stream: Option<StreamHandle>,
    _generator: Option<crate::generator::Generator>,
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
    /// the route's current jitter-buffer target (grows after underruns)
    pub target_ms: f32,
}

#[derive(Clone, Debug, Default)]
pub struct HostSnapshot {
    pub sources: Vec<SourceSnapshot>,
    pub outputs: Vec<OutputSnapshot>,
    pub routes: Vec<RouteSnapshot>,
    /// Echo reduction's output (peak since last call, frames produced).
    pub echo: Option<(f32, u64)>,
}

pub struct AudioHost {
    graph: RoutingGraph,
    sources: BTreeMap<SourceId, SourceEntry>,
    outputs: BTreeMap<OutputId, OutputEntry>,
    routes: BTreeMap<(SourceId, OutputId), RouteEntry>,
    pulled: BTreeMap<OutputId, OutputMixer>,
    aec: Option<AecStage>,
}

/// Internal ids of the echo-reduction stage (never in the routing graph).
const AEC_MIC_IN: OutputId = 9001;
const AEC_REF_IN: OutputId = 9002;
/// Source ids from here are internal (self-test tones): never echo references.
const INTERNAL_SOURCES: SourceId = 900;

/// Echo reduction between the raw microphone and the outputs. The
/// microphone's routes are moved from the raw capture to the processed
/// stream while it is active.
struct AecStage {
    processed: FanoutHandle,
    ref_in: MixerHandle,
    /// sources feeding the reference (everything that is not the user)
    reference: Vec<SourceId>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for AecStage {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn info(s: &StreamHandle) -> StreamInfo {
    StreamInfo {
        state: s.status.state(),
        device: s
            .status
            .device_name
            .lock()
            .map(|n| n.clone())
            .unwrap_or_default(),
        pid: s.status.pid.load(Ordering::Relaxed),
        last_error: s.status.last_error.load(Ordering::Relaxed) as u32,
        restarts: s.status.restarts.load(Ordering::Relaxed),
    }
}

impl AudioHost {
    /// An empty host (for the advanced mixer and tests).
    pub fn empty() -> Self {
        AudioHost {
            graph: RoutingGraph::new(),
            sources: BTreeMap::new(),
            outputs: BTreeMap::new(),
            routes: BTreeMap::new(),
            pulled: BTreeMap::new(),
            aec: None,
        }
    }

    /// The standard Claude Talk setup (see `graph::RoutingGraph::claude_talk_default`).
    pub fn standard(cfg: HostConfig) -> Self {
        let mut h = AudioHost::empty();
        let set = |p: &[Party]| p.iter().cloned().collect::<BTreeSet<_>>();
        h.add_output(
            Output {
                id: ids::TO_DISCORD,
                label: "Til Discord".into(),
                heard_by: set(&[Party::Discord]),
            },
            OutputKind::Device(cfg.to_discord),
        );
        h.add_output(
            Output {
                id: ids::TO_CLAUDE,
                label: "Til Claude".into(),
                heard_by: set(&[Party::Claude]),
            },
            OutputKind::Pulled { channels: 1 },
        );
        let none = BTreeSet::new;
        let _ = h.add_source(
            Source {
                id: ids::MIC,
                label: "Mikrofon".into(),
                parties: set(&[Party::User]),
                contains_outputs: none(),
            },
            cfg.mic,
        );
        let _ = h.add_source(
            Source {
                id: ids::CLAUDE,
                label: "Claude".into(),
                parties: set(&[Party::Claude]),
                contains_outputs: none(),
            },
            SourceKind::Program(ProcessTarget::Pid(cfg.claude_pid)),
        );
        let _ = h.add_source(
            Source {
                id: ids::DISCORD,
                label: "Discord".into(),
                parties: set(&[Party::Discord]),
                contains_outputs: none(),
            },
            SourceKind::Program(cfg.discord),
        );
        for (s, o) in [
            (ids::MIC, ids::TO_DISCORD),
            (ids::CLAUDE, ids::TO_DISCORD),
            (ids::MIC, ids::TO_CLAUDE),
            (ids::DISCORD, ids::TO_CLAUDE),
        ] {
            // muted until the app applies its switches ("Claude hears only
            // you", the big switch): never live for a moment by default
            let _ = h.set_route(
                s,
                o,
                Route {
                    muted: true,
                    ..Route::default()
                },
            );
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
                self.outputs.insert(
                    id,
                    OutputEntry {
                        handle,
                        stream: Some(stream),
                    },
                );
            }
            OutputKind::Pulled { channels } => {
                let (mixer, handle) = OutputMixer::new(id, channels, 480);
                self.pulled.insert(id, mixer);
                self.outputs.insert(
                    id,
                    OutputEntry {
                        handle,
                        stream: None,
                    },
                );
            }
        }
    }

    /// Hands a pulled output's mixer to whoever drives it (the Claude bridge).
    pub fn take_pulled(&mut self, id: OutputId) -> Option<OutputMixer> {
        self.pulled.remove(&id)
    }

    pub fn add_source(&mut self, spec: Source, kind: SourceKind) -> Result<(), Rejection> {
        let id = spec.id;
        // echo reduction holds the old streams of these: rebuild it around the new one
        // echo reduction holds the microphone and every reference source:
        // rebuild it around the new set
        let aec_again = self.aec.is_some() && id < INTERNAL_SOURCES;
        if aec_again {
            self.set_echo_reduction(false);
        }
        let r = self.add_source_inner(spec, kind);
        if aec_again {
            self.set_echo_reduction(true);
        }
        r
    }

    fn add_source_inner(&mut self, spec: Source, kind: SourceKind) -> Result<(), Rejection> {
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
        self.sources.insert(
            id,
            SourceEntry {
                handle,
                stream: Some(stream),
                _generator: None,
            },
        );
        // re-wire existing routes of this source
        let routes: Vec<(OutputId, Route)> = self
            .graph
            .routes()
            .filter(|(s, _, _)| *s == id)
            .map(|(_, o, r)| (o, r))
            .collect();
        for (o, r) in routes {
            self.wire(id, o, r);
        }
        Ok(())
    }

    /// A source that generates audio itself (self-test tones). `gen` fills
    /// interleaved stereo 48 kHz.
    pub fn add_generated_source(
        &mut self,
        spec: Source,
        gen: impl FnMut(&mut [f32]) + Send + 'static,
    ) -> Result<(), Rejection> {
        let id = spec.id;
        self.graph.add_source(spec)?;
        self.sources.remove(&id);
        let (fanout, handle) = SourceFanout::new(id, ENGINE_RATE);
        let g = crate::generator::Generator::spawn(fanout, gen);
        self.sources.insert(
            id,
            SourceEntry {
                handle,
                stream: None,
                _generator: Some(g),
            },
        );
        Ok(())
    }

    pub fn remove_source(&mut self, id: SourceId) {
        let aec_again = self.aec.is_some() && id < INTERNAL_SOURCES;
        if aec_again {
            self.set_echo_reduction(false);
        }
        let outs: Vec<OutputId> = self
            .routes
            .keys()
            .filter(|(s, _)| *s == id)
            .map(|&(_, o)| o)
            .collect();
        for o in outs {
            self.unwire(id, o);
        }
        self.graph.remove_source(id);
        self.sources.remove(&id);
        if aec_again {
            self.set_echo_reduction(true);
        }
    }

    pub fn remove_output(&mut self, id: OutputId) {
        let srcs: Vec<SourceId> = self
            .routes
            .keys()
            .filter(|(_, o)| *o == id)
            .map(|&(s, _)| s)
            .collect();
        for s in srcs {
            self.unwire(s, id);
        }
        self.graph.remove_output(id);
        self.outputs.remove(&id);
        self.pulled.remove(&id);
    }

    /// The fanout a source's routes hang off (the processed microphone
    /// while echo reduction is on).
    fn route_source(&mut self, s: SourceId) -> Option<&mut FanoutHandle> {
        if s == ids::MIC {
            if let Some(a) = self.aec.as_mut() {
                return Some(&mut a.processed);
            }
        }
        self.sources.get_mut(&s).map(|e| &mut e.handle)
    }

    fn wire(&mut self, s: SourceId, o: OutputId, r: Route) {
        let params = GainParams::new(r.gain_db, r.muted);
        let Some(mut out) = self.outputs.remove(&o) else {
            return;
        };
        let stats = self
            .route_source(s)
            .map(|src| connect(src, &mut out.handle, params.clone(), ROUTE_BUFFER_MS));
        self.outputs.insert(o, out);
        if let Some(stats) = stats {
            self.routes.insert((s, o), RouteEntry { params, stats });
        }
    }

    fn unwire(&mut self, s: SourceId, o: OutputId) {
        if self.routes.remove(&(s, o)).is_some() {
            let Some(mut out) = self.outputs.remove(&o) else {
                return;
            };
            if let Some(src) = self.route_source(s) {
                disconnect(src, &mut out.handle);
            }
            self.outputs.insert(o, out);
        }
    }

    pub fn echo_reduction_active(&self) -> bool {
        self.aec.is_some()
    }

    /// Turns echo reduction on or off while running.
    pub fn set_echo_reduction(&mut self, on: bool) {
        if on == self.aec.is_some() {
            return;
        }
        // the microphone's current routes move between raw and processed
        let mic_routes: Vec<(OutputId, Route)> = self
            .graph
            .routes()
            .filter(|(s, _, _)| *s == ids::MIC)
            .map(|(_, o, r)| (o, r))
            .collect();
        for (o, _) in &mic_routes {
            self.unwire(ids::MIC, *o);
        }
        if on {
            match self.start_aec() {
                Ok(stage) => self.aec = Some(stage),
                Err(e) => log::warn!("echo reduction unavailable: {e}"),
            }
        } else if let Some(mut stage) = self.aec.take() {
            if let Some(mic) = self.sources.get_mut(&ids::MIC) {
                mic.handle.remove_output(AEC_MIC_IN);
            }
            for s in stage.reference.clone() {
                if let Some(src) = self.sources.get_mut(&s) {
                    disconnect(&mut src.handle, &mut stage.ref_in);
                }
            }
            drop(stage);
        }
        for (o, r) in mic_routes {
            self.wire(ids::MIC, o, r);
        }
    }

    fn start_aec(&mut self) -> Result<AecStage, String> {
        use crate::aec::{EchoReducer, FRAME};
        // The microphone is read raw, on its own clock (no extra jitter
        // buffer); the reference (Claude + Discord) is pulled once per
        // microphone frame with a short buffer, so it stays ahead of any
        // echo the microphone picks up.
        let Some(mic) = self.sources.get_mut(&ids::MIC) else {
            return Err("no microphone".into());
        };
        let mut raw = connect_raw(&mut mic.handle, AEC_MIC_IN);
        // the microphone's (re)starts, passed on to the processed microphone:
        // the gap around a reopen must not grow the routes behind it
        let mic_stats = mic.handle.stats.clone();
        let (ref_mix, mut ref_in) = OutputMixer::new(AEC_REF_IN, 1, FRAME);
        let mut ref_mix = ref_mix.without_limiter();
        let (mut fanout, processed) = SourceFanout::new(ids::MIC, ENGINE_RATE);
        // Everything that may come out of the speakers and is not the user:
        // Claude, Discord, Claude in another program, added programs.
        let reference: Vec<SourceId> = self
            .graph
            .sources()
            .filter(|s| {
                s.id != ids::MIC && s.id < INTERNAL_SOURCES && !s.parties.contains(&Party::User)
            })
            .map(|s| s.id)
            .collect();
        for s in &reference {
            if let Some(src) = self.sources.get_mut(s) {
                connect_fixed(
                    &mut src.handle,
                    &mut ref_in,
                    GainParams::new(0.0, false),
                    crate::aec::REF_BUFFER_MS,
                );
            }
        }
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop2 = stop.clone();
        let thread = std::thread::Builder::new()
            .name("echo-reduction".into())
            .spawn(move || {
                super::high_res_timer();
                let _mmcss = super::stream::Mmcss::enter();
                crate::flush_denormals();
                // created here: the AEC3 graph is bound to its thread
                let mut reducer = match EchoReducer::new() {
                    Ok(r) => Some(r),
                    Err(e) => {
                        log::warn!(
                            "echo reduction could not start ({e}); microphone passes through"
                        );
                        None
                    }
                };
                let ch = ENGINE_CHANNELS;
                let mut m = vec![0f32; FRAME];
                let mut r = vec![0f32; FRAME];
                let mut mic_restarts = mic_stats.restarts.load(Ordering::Relaxed);
                while !stop2.load(Ordering::Relaxed) {
                    let now = mic_stats.restarts.load(Ordering::Relaxed);
                    if now != mic_restarts {
                        mic_restarts = now;
                        fanout.mark_restart();
                    }
                    // The microphone delivers 10 ms packets (some USB and
                    // Bluetooth mics several at once) and this thread reads
                    // them as they come; more than 60 ms waiting is a stall
                    // (or start-up) backlog that would otherwise add its
                    // delay for good.
                    let backlog = raw.slots() / ch;
                    if backlog > 6 * FRAME {
                        let drop_frames = backlog - FRAME;
                        if let Ok(c) = raw.read_chunk(drop_frames * ch) {
                            c.commit_all();
                        }
                    }
                    if raw.slots() < FRAME * ch {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                        continue;
                    }
                    if let Ok(c) = raw.read_chunk(FRAME * ch) {
                        let (a, b) = c.as_slices();
                        for (k, v) in m.iter_mut().enumerate() {
                            let i = k * ch;
                            let at = |j: usize| if j < a.len() { a[j] } else { b[j - a.len()] };
                            *v = 0.5 * (at(i) + at(i + 1));
                        }
                        c.commit_all();
                    }
                    // The reference for the moment this frame was recorded,
                    // not the moment it is processed: a microphone that
                    // delivers several frames at once would otherwise get
                    // a reference that runs dry and moves (round-6 review).
                    // read both as one: a push between the two reads would
                    // date this frame a frame too late
                    let (rest, newest) = loop {
                        let seq = mic_stats.push_seq.load(Ordering::Acquire);
                        if seq % 2 == 1 {
                            std::thread::yield_now(); // a push is half done
                            continue;
                        }
                        let slots = raw.slots();
                        let newest = mic_stats.last_push_ns.load(Ordering::Acquire);
                        if mic_stats.push_seq.load(Ordering::Acquire) == seq {
                            break ((slots / ch) as u64, newest);
                        }
                    };
                    let recorded = newest.saturating_sub(rest * 1_000_000_000 / ENGINE_RATE as u64);
                    ref_mix.render_at(&mut r, recorded);
                    match reducer.as_mut() {
                        Some(red) => fanout.push(Some(red.process(&r, &m)), 1, FRAME),
                        None => fanout.push(Some(&m), 1, FRAME),
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        log::info!("echo reduction on");
        Ok(AecStage {
            processed,
            ref_in,
            reference,
            stop,
            thread: Some(thread),
        })
    }

    /// Adds or updates a route (refused if it breaks the echo/feedback rules).
    pub fn set_route(
        &mut self,
        s: SourceId,
        o: OutputId,
        r: Route,
    ) -> Result<Option<Party>, Rejection> {
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

    /// Closes a source's device while nobody needs it (true), or opens it
    /// again (false). Its routes keep running with silence.
    pub fn set_source_paused(&self, id: SourceId, paused: bool) {
        if let Some(st) = self.sources.get(&id).and_then(|s| s.stream.as_ref()) {
            st.set_paused(paused);
        }
    }

    /// Makes one source reopen its device (e.g. a new default microphone).
    pub fn restart_source(&self, id: SourceId) {
        if let Some(st) = self.sources.get(&id).and_then(|s| s.stream.as_ref()) {
            st.request_restart();
        }
    }

    /// The device a source records from right now ("" if none).
    pub fn source_device(&self, id: SourceId) -> String {
        self.sources
            .get(&id)
            .and_then(|s| s.stream.as_ref())
            // a paused or reopening stream has no current device
            .filter(|st| st.status.state() == super::stream::StreamState::Running)
            .map(|st| {
                st.status
                    .device_name
                    .lock()
                    .map(|n| n.clone())
                    .unwrap_or_default()
            })
            .unwrap_or_default()
    }

    /// Makes a stream reopen its device (default device changed, etc.).
    pub fn restart_streams(&self) {
        for s in self.sources.values() {
            if let Some(st) = &s.stream {
                st.request_restart();
            }
        }
        for o in self.outputs.values() {
            if let Some(st) = &o.stream {
                st.request_restart();
            }
        }
    }

    /// Frees memory the audio threads have released. Call ~every 100 ms.
    pub fn maintain(&mut self) {
        if let Some(a) = self.aec.as_mut() {
            a.processed.collect_garbage();
            a.ref_in.collect_garbage();
        }
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
                label: self
                    .graph
                    .source(*id)
                    .map(|x| x.label.clone())
                    .unwrap_or_default(),
                peak,
                rms,
                muted: s.handle.params.muted.load(Ordering::Relaxed),
                gain_db: s.handle.params.gain_db.load(),
                overflow_frames: s.handle.stats.overflow_frames.load(Ordering::Relaxed),
                stream: s.stream.as_ref().map(info).unwrap_or(StreamInfo {
                    state: StreamState::Running,
                    device: String::new(),
                    pid: 0,
                    last_error: 0,
                    restarts: 0,
                }),
            });
        }
        for (id, o) in &self.outputs {
            let (peak, rms) = o.handle.stats.meter.read();
            snap.outputs.push(OutputSnapshot {
                id: *id,
                label: self
                    .graph
                    .output(*id)
                    .map(|x| x.label.clone())
                    .unwrap_or_default(),
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
                target_ms: r.stats.target_ms.load(),
            });
        }
        snap.echo = self.aec.as_ref().map(|a| {
            (
                a.processed.stats.meter.read().0,
                a.processed.stats.captured_frames.load(Ordering::Relaxed),
            )
        });
        snap
    }
}

/// True if a capture endpoint with this name is one of ours.
pub fn is_own_device(name: &str) -> bool {
    let n = name.to_lowercase();
    OWN_VIRTUAL_DEVICES.iter().any(|p| n.contains(p))
}

/// Name fragments of other virtual cables a user may have installed
/// themselves (VB-CABLE A/B/C/D, Hi-Fi Cable). With one of these,
/// Claude can also be used in another program ("Claude in Chrome or the app").
/// VoiceMeeter is not one of them: it is a mixer whose own buses can send
/// what we play straight back into a microphone, which we cannot see.
pub const EXTRA_CABLE_PATTERNS: [&str; 5] =
    ["cable-a", "cable-b", "cable-c", "cable-d", "hi-fi cable"];

/// An extra virtual cable: where we play to, and the microphone the other
/// program must choose.
#[derive(Clone, Debug)]
pub struct ExtraCable {
    pub render: devices::Endpoint,
    pub capture_name: Option<String>,
}

pub fn extra_cable() -> Option<ExtraCable> {
    let renders = devices::list(Flow::Render).ok()?;
    let render = EXTRA_CABLE_PATTERNS.iter().find_map(|p| {
        renders.iter().find(|e| {
            let n = e.name.to_lowercase();
            n.contains(p) && !n.contains("claude talk")
        })
    })?;
    let lower = render.name.to_lowercase();
    let prefix = lower
        .split(" input")
        .next()
        .unwrap_or(&lower)
        .trim()
        .to_string();
    let capture_name = devices::list(Flow::Capture)
        .ok()?
        .into_iter()
        .find(|e| {
            e.name
                .to_lowercase()
                .starts_with(&format!("{prefix} output"))
        })
        .map(|e| e.name);
    Some(ExtraCable {
        render: render.clone(),
        capture_name,
    })
}

/// Whether the virtual cable is installed (render side found, also when it
/// is disabled in Windows' sound settings: then it needs turning on, not
/// installing).
pub fn cable_installed() -> bool {
    devices::exists_enabled_or_disabled(Flow::Render, &cable_render_patterns())
}
