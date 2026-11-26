//! The real-time mixing core.
//!
//! ```text
//!  capture thread                      output thread (own clock)
//!  ┌──────────────┐  SPSC ring   ┌───────────────────────────────┐
//!  │ SourceFanout ├─────────────►│ SourceTap (resample+drift+gain)│─┐
//!  │  (per source)├──┐           └───────────────────────────────┘ │ sum → limiter
//!  └──────────────┘  │  ring     ┌───────────────────────────────┐ │ → master gain
//!                    └──────────►│ SourceTap (other output)       │ │ → meter → device
//!                                └───────────────────────────────┘─┘
//! ```
//!
//! * Every source→output route has its own lock-free ring and its own
//!   resampler, so each output can run on its own clock without any lock.
//! * Taps are added and removed through a command queue. Removed objects are
//!   sent back to the control thread to be dropped there, so the audio
//!   thread never frees memory.
//! * After construction, `OutputMixer::render` and `SourceFanout::push` do
//!   not allocate, lock, block or log (checked by `tests/realtime.rs`).

use crate::dsp::{db_to_lin, AtomicF32, MeterValues, SmoothedGain, TruePeakLimiter};
use crate::graph::{OutputId, SourceId};
use crate::resample::AdaptiveResampler;
use crate::{ENGINE_CHANNELS, ENGINE_RATE};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Maximum routes into one output and outputs fed by one source.
pub const MAX_TAPS: usize = 16;
/// Largest block an output may request at once; bigger requests are split.
pub const MAX_BLOCK: usize = 4096;
/// Gain changes glide over this many milliseconds (no clicks).
const RAMP_MS: f32 = 15.0;

/// Gain and mute, shared between UI and audio threads.
#[derive(Debug)]
pub struct GainParams {
    pub gain_db: AtomicF32,
    pub muted: AtomicBool,
}

impl GainParams {
    pub fn new(gain_db: f32, muted: bool) -> Arc<Self> {
        Arc::new(GainParams {
            gain_db: AtomicF32::new(gain_db),
            muted: AtomicBool::new(muted),
        })
    }
    #[inline]
    fn linear(&self) -> f32 {
        if self.muted.load(Ordering::Relaxed) {
            0.0
        } else {
            db_to_lin(self.gain_db.load())
        }
    }
}

/// Live statistics of one route (for the diagnostics page).
#[derive(Debug, Default)]
pub struct TapStats {
    pub underrun_frames: AtomicU64,
    pub skipped_frames: AtomicU64,
    pub drift_ppm: AtomicF32,
    pub buffered_ms: AtomicF32,
    /// Current jitter-buffer target of the route (adapts to the source).
    pub target_ms: AtomicF32,
}

/// Live statistics and meter of one output.
#[derive(Debug, Default)]
pub struct OutputStats {
    pub meter: MeterValues,
    pub limiter_reduction: AtomicF32,
    pub rendered_frames: AtomicU64,
    pub dropped_commands: AtomicU64,
}

/// Live statistics and meter of one source.
#[derive(Debug, Default)]
pub struct SourceStats {
    pub meter: MeterValues,
    pub overflow_frames: AtomicU64,
    pub captured_frames: AtomicU64,
}

pub struct SourceTap {
    pub source: SourceId,
    ring: rtrb::Consumer<f32>,
    resampler: AdaptiveResampler,
    gain: SmoothedGain,
    params: Arc<GainParams>,
    stats: Arc<TapStats>,
    in_rate: u32,
}

pub enum MixerCommand {
    AddTap(Box<SourceTap>),
    RemoveTap(SourceId),
}

pub enum FanoutCommand {
    AddOutput(OutputId, Box<rtrb::Producer<f32>>),
    RemoveOutput(OutputId),
}

pub struct OutputMixer {
    channels: usize,
    taps: [Option<Box<SourceTap>>; MAX_TAPS],
    scratch: Vec<f32>,
    mix: Vec<f32>,
    limiter: TruePeakLimiter,
    master: SmoothedGain,
    master_params: Arc<GainParams>,
    stats: Arc<OutputStats>,
    commands: rtrb::Consumer<MixerCommand>,
    garbage: rtrb::Producer<Box<SourceTap>>,
    blocks_per_second: f64,
}

/// Control-thread side of an output mixer.
pub struct MixerHandle {
    pub output: OutputId,
    pub params: Arc<GainParams>,
    pub stats: Arc<OutputStats>,
    commands: rtrb::Producer<MixerCommand>,
    /// Commands the audio thread had no room for yet (its device may be
    /// gone for a while); sent in order before anything new.
    pending: std::collections::VecDeque<MixerCommand>,
    garbage: rtrb::Consumer<Box<SourceTap>>,
    blocks_per_second: f64,
}

impl OutputMixer {
    /// `channels` is the device's channel count; `block_frames` its typical
    /// period (used to tune drift control and buffer targets).
    pub fn new(
        output: OutputId,
        channels: usize,
        block_frames: usize,
    ) -> (OutputMixer, MixerHandle) {
        TruePeakLimiter::warm_up();
        let (cmd_tx, cmd_rx) = rtrb::RingBuffer::new(256);
        let (gb_tx, gb_rx) = rtrb::RingBuffer::new(256);
        let params = GainParams::new(0.0, false);
        let stats = Arc::new(OutputStats::default());
        let bps = ENGINE_RATE as f64 / block_frames.max(1) as f64;
        let mixer = OutputMixer {
            channels,
            taps: std::array::from_fn(|_| None),
            scratch: vec![0.0; MAX_BLOCK * ENGINE_CHANNELS],
            mix: vec![0.0; MAX_BLOCK * ENGINE_CHANNELS],
            limiter: TruePeakLimiter::new(ENGINE_CHANNELS, ENGINE_RATE as f32, -1.0, 1.5, 80.0),
            master: SmoothedGain::new(1.0, ENGINE_RATE as f32, RAMP_MS),
            master_params: params.clone(),
            stats: stats.clone(),
            commands: cmd_rx,
            garbage: gb_tx,
            blocks_per_second: bps,
        };
        let handle = MixerHandle {
            output,
            params,
            stats,
            commands: cmd_tx,
            pending: Default::default(),
            garbage: gb_rx,
            blocks_per_second: bps,
        };
        (mixer, handle)
    }

    pub fn latency_frames(&self) -> usize {
        self.limiter.latency()
    }

    pub fn blocks_per_second(&self) -> f64 {
        self.blocks_per_second
    }

    fn apply_commands(&mut self) {
        while let Ok(cmd) = self.commands.pop() {
            match cmd {
                MixerCommand::AddTap(tap) => {
                    let src = tap.source;
                    // replace an existing tap for the same source
                    if let Some(slot) = self
                        .taps
                        .iter_mut()
                        .find(|t| t.as_ref().is_some_and(|t| t.source == src))
                    {
                        let old = slot.replace(tap);
                        self.discard(old);
                    } else if let Some(slot) = self.taps.iter_mut().find(|t| t.is_none()) {
                        *slot = Some(tap);
                    } else {
                        self.stats.dropped_commands.fetch_add(1, Ordering::Relaxed);
                        self.discard(Some(tap));
                    }
                }
                MixerCommand::RemoveTap(src) => {
                    if let Some(slot) = self
                        .taps
                        .iter_mut()
                        .find(|t| t.as_ref().is_some_and(|t| t.source == src))
                    {
                        let old = slot.take();
                        self.discard(old);
                    }
                }
            }
        }
    }

    fn discard(&mut self, tap: Option<Box<SourceTap>>) {
        if let Some(tap) = tap {
            if let Err(rtrb::PushError::Full(tap)) = self.garbage.push(tap) {
                // The control thread is not collecting; leaking one tap is
                // better than freeing memory on the audio thread.
                std::mem::forget(tap);
            }
        }
    }

    /// Renders `out` (interleaved, `channels` as given to `new`).
    pub fn render(&mut self, out: &mut [f32]) {
        self.apply_commands();
        let ch_out = self.channels.max(1);
        let total = out.len() / ch_out;
        let mut done = 0;
        while done < total {
            let frames = (total - done).min(MAX_BLOCK);
            self.render_block(frames);
            // engine stereo → device channels
            let dst = &mut out[done * ch_out..(done + frames) * ch_out];
            let src = &self.mix[..frames * ENGINE_CHANNELS];
            match ch_out {
                1 => {
                    for (d, s) in dst.iter_mut().zip(src.chunks_exact(2)) {
                        *d = 0.5 * (s[0] + s[1]);
                    }
                }
                2 => dst.copy_from_slice(src),
                n => {
                    for (d, s) in dst.chunks_exact_mut(n).zip(src.chunks_exact(2)) {
                        d[0] = s[0];
                        d[1] = s[1];
                        for x in &mut d[2..] {
                            *x = 0.0;
                        }
                    }
                }
            }
            done += frames;
        }
        self.stats
            .rendered_frames
            .fetch_add(total as u64, Ordering::Relaxed);
    }

    fn render_block(&mut self, frames: usize) {
        let n = frames * ENGINE_CHANNELS;
        let mix = &mut self.mix[..n];
        mix.fill(0.0);
        for tap in self.taps.iter_mut().flatten() {
            let scratch = &mut self.scratch[..n];
            let st = tap.resampler.pull(&mut tap.ring, scratch);
            if st.underrun_frames > 0 {
                tap.stats
                    .underrun_frames
                    .fetch_add(st.underrun_frames as u64, Ordering::Relaxed);
            }
            if st.skipped_frames > 0 {
                tap.stats
                    .skipped_frames
                    .fetch_add(st.skipped_frames as u64, Ordering::Relaxed);
            }
            tap.stats
                .drift_ppm
                .store(tap.resampler.drift().drift_ppm() as f32);
            tap.stats.target_ms.store(tap.resampler.target_ms());
            tap.stats.buffered_ms.store(
                tap.ring.slots() as f32 / ENGINE_CHANNELS as f32 / tap.in_rate as f32 * 1000.0,
            );
            tap.gain.set_target(tap.params.linear());
            tap.gain.mix_into(scratch, mix, ENGINE_CHANNELS);
        }
        self.master.set_target(self.master_params.linear());
        self.master.apply(mix, ENGINE_CHANNELS);
        self.limiter.process(mix);
        self.stats.limiter_reduction.store(self.limiter.reduction);
        self.stats.meter.feed(mix);
    }
}

impl MixerHandle {
    /// Drops objects the audio thread has released. Call regularly from the
    /// control thread (e.g. with the meter timer).
    pub fn collect_garbage(&mut self) {
        while self.garbage.pop().is_ok() {}
        self.flush();
    }

    /// Sends queued commands while there is room.
    fn flush(&mut self) {
        while let Some(c) = self.pending.pop_front() {
            if let Err(rtrb::PushError::Full(c)) = self.commands.push(c) {
                self.pending.push_front(c);
                break;
            }
        }
    }

    /// Never loses a command: what does not fit now is sent later, in order.
    fn send(&mut self, cmd: MixerCommand) {
        self.flush();
        if !self.pending.is_empty() {
            self.pending.push_back(cmd);
        } else if let Err(rtrb::PushError::Full(c)) = self.commands.push(cmd) {
            self.pending.push_back(c);
        }
    }

    pub fn remove_source(&mut self, source: SourceId) {
        self.send(MixerCommand::RemoveTap(source));
    }

    fn add_tap(&mut self, tap: Box<SourceTap>) {
        self.send(MixerCommand::AddTap(tap));
    }

    pub fn blocks_per_second(&self) -> f64 {
        self.blocks_per_second
    }
}

pub struct SourceFanout {
    source: SourceId,
    outputs: [Option<(OutputId, Box<rtrb::Producer<f32>>)>; MAX_TAPS],
    conv: Vec<f32>,
    gain: SmoothedGain,
    params: Arc<GainParams>,
    stats: Arc<SourceStats>,
    commands: rtrb::Consumer<FanoutCommand>,
    garbage: rtrb::Producer<Box<rtrb::Producer<f32>>>,
}

/// Control-thread side of a source.
pub struct FanoutHandle {
    pub source: SourceId,
    pub rate: u32,
    pub params: Arc<GainParams>,
    pub stats: Arc<SourceStats>,
    commands: rtrb::Producer<FanoutCommand>,
    pending: std::collections::VecDeque<FanoutCommand>,
    garbage: rtrb::Consumer<Box<rtrb::Producer<f32>>>,
}

impl SourceFanout {
    /// `rate` is the capture device's sample rate.
    pub fn new(source: SourceId, rate: u32) -> (SourceFanout, FanoutHandle) {
        let (cmd_tx, cmd_rx) = rtrb::RingBuffer::new(256);
        let (gb_tx, gb_rx) = rtrb::RingBuffer::new(256);
        let params = GainParams::new(0.0, false);
        let stats = Arc::new(SourceStats::default());
        (
            SourceFanout {
                source,
                outputs: std::array::from_fn(|_| None),
                conv: vec![0.0; MAX_BLOCK * ENGINE_CHANNELS],
                gain: SmoothedGain::new(1.0, rate as f32, RAMP_MS),
                params: params.clone(),
                stats: stats.clone(),
                commands: cmd_rx,
                garbage: gb_tx,
            },
            FanoutHandle {
                source,
                rate,
                params,
                stats,
                commands: cmd_tx,
                pending: Default::default(),
                garbage: gb_rx,
            },
        )
    }

    pub fn source(&self) -> SourceId {
        self.source
    }

    fn apply_commands(&mut self) {
        while let Ok(cmd) = self.commands.pop() {
            match cmd {
                FanoutCommand::AddOutput(o, prod) => {
                    if let Some(slot) = self
                        .outputs
                        .iter_mut()
                        .find(|s| s.as_ref().is_some_and(|(id, _)| *id == o))
                    {
                        let old = slot.replace((o, prod));
                        if let Some((_, p)) = old {
                            Self::discard(&mut self.garbage, p);
                        }
                    } else if let Some(slot) = self.outputs.iter_mut().find(|s| s.is_none()) {
                        *slot = Some((o, prod));
                    } else {
                        Self::discard(&mut self.garbage, prod);
                    }
                }
                FanoutCommand::RemoveOutput(o) => {
                    if let Some(slot) = self
                        .outputs
                        .iter_mut()
                        .find(|s| s.as_ref().is_some_and(|(id, _)| *id == o))
                    {
                        if let Some((_, p)) = slot.take() {
                            Self::discard(&mut self.garbage, p);
                        }
                    }
                }
            }
        }
    }

    fn discard(
        garbage: &mut rtrb::Producer<Box<rtrb::Producer<f32>>>,
        p: Box<rtrb::Producer<f32>>,
    ) {
        if let Err(rtrb::PushError::Full(p)) = garbage.push(p) {
            std::mem::forget(p);
        }
    }

    /// Pushes captured audio (interleaved, `in_channels`) to every output.
    /// `None` means the device reported silence for `frames` frames.
    pub fn push(&mut self, input: Option<&[f32]>, in_channels: usize, frames: usize) {
        self.apply_commands();
        let mut done = 0;
        while done < frames {
            let n = (frames - done).min(MAX_BLOCK);
            let conv = &mut self.conv[..n * ENGINE_CHANNELS];
            match input {
                None => conv.fill(0.0),
                Some(inp) => {
                    let inp = &inp[done * in_channels..(done + n) * in_channels];
                    match in_channels {
                        1 => {
                            for (d, &s) in conv.chunks_exact_mut(2).zip(inp) {
                                d[0] = s;
                                d[1] = s;
                            }
                        }
                        2 => conv.copy_from_slice(inp),
                        c => {
                            for (d, s) in conv.chunks_exact_mut(2).zip(inp.chunks_exact(c)) {
                                d[0] = s[0];
                                d[1] = s[1];
                            }
                        }
                    }
                }
            }
            // a broken driver or program must not put NaN/Inf into the mix
            for v in conv.iter_mut() {
                if !v.is_finite() {
                    *v = 0.0;
                }
            }
            self.gain.set_target(self.params.linear());
            self.gain.apply(conv, ENGINE_CHANNELS);
            self.stats.meter.feed(conv);
            for (_, prod) in self.outputs.iter_mut().flatten() {
                if prod.slots() >= conv.len() {
                    if let Ok(mut chunk) = prod.write_chunk(conv.len()) {
                        let (a, b) = chunk.as_mut_slices();
                        let k = a.len();
                        a.copy_from_slice(&conv[..k]);
                        b.copy_from_slice(&conv[k..]);
                        chunk.commit_all();
                    }
                } else {
                    self.stats
                        .overflow_frames
                        .fetch_add(n as u64, Ordering::Relaxed);
                }
            }
            done += n;
        }
        self.stats
            .captured_frames
            .fetch_add(frames as u64, Ordering::Relaxed);
    }
}

impl FanoutHandle {
    pub fn collect_garbage(&mut self) {
        while self.garbage.pop().is_ok() {}
        self.flush();
    }

    fn flush(&mut self) {
        while let Some(c) = self.pending.pop_front() {
            if let Err(rtrb::PushError::Full(c)) = self.commands.push(c) {
                self.pending.push_front(c);
                break;
            }
        }
    }

    /// Never loses a command: what does not fit now is sent later, in order.
    fn send(&mut self, cmd: FanoutCommand) {
        self.flush();
        if !self.pending.is_empty() {
            self.pending.push_back(cmd);
        } else if let Err(rtrb::PushError::Full(c)) = self.commands.push(cmd) {
            self.pending.push_back(c);
        }
    }

    pub fn remove_output(&mut self, output: OutputId) {
        self.send(FanoutCommand::RemoveOutput(output));
    }
}

/// Connects a source to an output: creates the ring and the tap and hands
/// both ends to the audio threads. Called on the control thread (allocates).
///
/// `target_ms` is the buffer each route keeps to absorb scheduling jitter;
/// it must cover the source's capture period plus the output's period.
pub fn connect(
    source: &mut FanoutHandle,
    output: &mut MixerHandle,
    route: Arc<GainParams>,
    target_ms: f64,
) -> Arc<TapStats> {
    // room for 500 ms: far more than ever buffered; overflow is counted
    let cap = (source.rate as usize / 2).max(MAX_BLOCK) * ENGINE_CHANNELS;
    let (prod, cons) = rtrb::RingBuffer::<f32>::new(cap);
    let stats = Arc::new(TapStats::default());
    let tap = Box::new(SourceTap {
        source: source.source,
        ring: cons,
        resampler: AdaptiveResampler::new(
            ENGINE_CHANNELS,
            source.rate,
            ENGINE_RATE,
            target_ms,
            output.blocks_per_second,
        ),
        gain: SmoothedGain::new(0.0, ENGINE_RATE as f32, RAMP_MS),
        params: route,
        stats: stats.clone(),
        in_rate: source.rate,
    });
    output.add_tap(tap);
    source.send(FanoutCommand::AddOutput(output.output, Box::new(prod)));
    stats
}

/// A plain ring from a source, without resampling or jitter buffer: for a
/// consumer that runs on the source's own clock (echo reduction reads the
/// microphone this way). Delivers interleaved stereo at the source's rate.
/// Called on the control thread (allocates).
pub fn connect_raw(source: &mut FanoutHandle, id: OutputId) -> rtrb::Consumer<f32> {
    let cap = (source.rate as usize / 2).max(MAX_BLOCK) * ENGINE_CHANNELS;
    let (prod, cons) = rtrb::RingBuffer::<f32>::new(cap);
    source.send(FanoutCommand::AddOutput(id, Box::new(prod)));
    cons
}

/// Disconnects a route on both sides.
pub fn disconnect(source: &mut FanoutHandle, output: &mut MixerHandle) {
    output.remove_source(source.source);
    source.remove_output(output.output);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_consumer_gets_the_source_unchanged() {
        let (mut fan, mut h) = SourceFanout::new(1, ENGINE_RATE);
        let mut raw = connect_raw(&mut h, 77);
        let input: Vec<f32> = (0..480).map(|i| i as f32 / 480.0).collect();
        fan.push(Some(&input), 1, 480);
        assert_eq!(raw.slots(), 480 * ENGINE_CHANNELS);
        let c = raw.read_chunk(raw.slots()).unwrap();
        let (a, b) = c.as_slices();
        let got: Vec<f32> = a.iter().chain(b).copied().collect();
        for k in (0..480).step_by(97) {
            assert!(
                (got[2 * k] - input[k]).abs() < 1e-6 && (got[2 * k + 1] - input[k]).abs() < 1e-6
            );
        }
        c.commit_all();
        h.remove_output(77);
        fan.push(Some(&input), 1, 480);
        assert_eq!(raw.slots(), 0);
    }

    fn tone(freq: f64, rate: f64, frames: usize, phase: &mut f64, ch: usize) -> Vec<f32> {
        let mut v = Vec::with_capacity(frames * ch);
        for _ in 0..frames {
            let s = (2.0 * std::f64::consts::PI * *phase).sin() as f32 * 0.3;
            *phase += freq / rate;
            for _ in 0..ch {
                v.push(s);
            }
        }
        v
    }

    pub(crate) fn level_db(x: &[f32], ch: usize, freq: f64) -> f64 {
        let mono: Vec<f32> = x.chunks_exact(ch).map(|f| f[0]).collect();
        let w = 2.0 * std::f64::consts::PI * freq / 48000.0;
        let c = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0f64, 0.0f64);
        // Hann window to keep leakage from other tones far below -60 dB
        let n = mono.len() as f64;
        for (i, &v) in mono.iter().enumerate() {
            let win = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n).cos();
            let s0 = v as f64 * win + c * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        let mag = (s1 * s1 + s2 * s2 - c * s1 * s2).max(0.0).sqrt() / (n / 4.0);
        20.0 * mag.max(1e-12).log10()
    }

    #[test]
    fn two_sources_at_different_rates_mix_cleanly_and_routes_isolate() {
        // mic 44.1 kHz mono +200 ppm, "claude" 48 kHz stereo -150 ppm;
        // output 1 gets both, output 2 gets only the mic.
        let (mut mic, mut mic_h) = SourceFanout::new(1, 44100);
        let (mut cl, mut cl_h) = SourceFanout::new(2, 48000);
        let (mut out1, mut out1_h) = OutputMixer::new(101, 2, 480);
        let (mut out2, mut out2_h) = OutputMixer::new(102, 1, 480);
        connect(&mut mic_h, &mut out1_h, GainParams::new(0.0, false), 20.0);
        let cl_route = GainParams::new(0.0, false);
        let cl_stats = connect(&mut cl_h, &mut out1_h, cl_route.clone(), 20.0);
        let mic2 = connect(&mut mic_h, &mut out2_h, GainParams::new(-6.0, false), 20.0);

        let (mut pm, mut pc) = (0.0, 0.0);
        let (mut fm, mut fc) = (0.0f64, 0.0f64);
        let mut o1 = vec![0f32; 480 * 2];
        let mut o2 = vec![0f32; 480];
        let mut tail1 = Vec::new();
        let mut tail2 = Vec::new();
        let blocks = 100 * 60; // one simulated minute
        for b in 0..blocks {
            fm += 441.0 * (1.0 + 200e-6);
            let nm = fm.floor() as usize;
            fm -= nm as f64;
            mic.push(Some(&tone(2500.0, 44100.0, nm, &mut pm, 1)), 1, nm);
            fc += 480.0 * (1.0 - 150e-6);
            let nc = fc.floor() as usize;
            fc -= nc as f64;
            cl.push(Some(&tone(440.0, 48000.0, nc, &mut pc, 2)), 2, nc);
            out1.render(&mut o1);
            out2.render(&mut o2);
            if b == 100 {
                // after the first second (buffers primed), count from zero
                cl_stats.underrun_frames.store(0, Ordering::Relaxed);
                mic2.underrun_frames.store(0, Ordering::Relaxed);
            }
            if b >= blocks - 50 {
                tail1.extend_from_slice(&o1);
                tail2.extend_from_slice(&o2);
            }
        }
        // out1 has both tones at the right level, out2 only the mic, 6 dB down
        let l = |x: &[f32], ch, f| level_db(x, ch, f);
        assert!((l(&tail1, 2, 2500.0) - l(&tail1, 2, 440.0)).abs() < 1.0);
        assert!(l(&tail1, 2, 2500.0) > -15.0);
        assert!(l(&tail2, 1, 2500.0) - l(&tail1, 2, 2500.0) < -5.0);
        assert!(
            l(&tail2, 1, 440.0) - l(&tail2, 1, 2500.0) < -60.0,
            "claude leaked into out2"
        );
        assert_eq!(cl_stats.underrun_frames.load(Ordering::Relaxed), 0);
        assert_eq!(mic2.underrun_frames.load(Ordering::Relaxed), 0);
        assert_eq!(mic_h.stats.overflow_frames.load(Ordering::Relaxed), 0);

        // mute the claude route → it disappears from out1 within a block or two
        cl_route.muted.store(true, Ordering::Relaxed);
        let mut tail = Vec::new();
        for b in 0..20 {
            mic.push(Some(&tone(2500.0, 44100.0, 441, &mut pm, 1)), 1, 441);
            cl.push(Some(&tone(440.0, 48000.0, 480, &mut pc, 2)), 2, 480);
            out1.render(&mut o1);
            if b >= 4 {
                tail.extend_from_slice(&o1);
            }
        }
        assert!(l(&tail, 2, 440.0) - l(&tail, 2, 2500.0) < -60.0);

        // removing the route at runtime frees it on the control thread
        disconnect(&mut cl_h, &mut out1_h);
        cl.push(Some(&tone(440.0, 48000.0, 480, &mut pc, 2)), 2, 480);
        out1.render(&mut o1);
        out1_h.collect_garbage();
        cl_h.collect_garbage();
    }

    #[test]
    fn limiter_keeps_loud_sum_below_ceiling() {
        let (mut a, mut ah) = SourceFanout::new(1, 48000);
        let (mut b, mut bh) = SourceFanout::new(2, 48000);
        let (mut o, mut oh) = OutputMixer::new(1, 2, 480);
        connect(&mut ah, &mut oh, GainParams::new(6.0, false), 20.0);
        connect(&mut bh, &mut oh, GainParams::new(6.0, false), 20.0);
        let full = vec![0.95f32; 480 * 2];
        let mut out = vec![0f32; 480 * 2];
        let mut peak = 0f32;
        for _ in 0..200 {
            a.push(Some(&full), 2, 480);
            b.push(Some(&full), 2, 480);
            o.render(&mut out);
            peak = out.iter().fold(peak, |m, v| m.max(v.abs()));
        }
        assert!(peak <= db_to_lin(-1.0) * 1.0001, "peak {peak}");
        assert!(peak >= db_to_lin(-2.0), "limiter too aggressive: {peak}");
        assert!(o.stats.limiter_reduction.load() < 0.5);
    }
}
