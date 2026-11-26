//! Pull-based adaptive resampler: reads a source's ring buffer at the
//! source's own rate and delivers exactly the number of frames an output
//! device asks for, at 48 kHz, while the drift controller keeps the ring
//! buffer at its target fill level.

use crate::drift::DriftController;
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Adjustable, Async, FixedAsync, PolynomialDegree, Resampler};

/// Size of the internal resampling chunk, in output frames (1 ms at 48 kHz).
/// Small chunks keep latency low; the polynomial resampler is cheap enough.
const CHUNK: usize = 48;

/// Fade length around gaps (1 ms at 48 kHz): no clicks when sound stops or
/// resumes after an underrun.
const FADE: usize = 48;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PullStats {
    /// Output frames that had to be filled with silence because the source
    /// did not deliver in time.
    pub underrun_frames: u32,
    /// Input frames discarded because far more was buffered than the target
    /// (start-up backlog, or the output stalled). Keeps latency bounded.
    pub skipped_frames: u32,
}

pub struct AdaptiveResampler {
    channels: usize,
    inner: Async<f32>,
    drift: DriftController,
    in_buf: Vec<f32>,
    out_buf: Vec<f32>,
    out_len: usize,
    out_pos: usize,
    nominal: f64,
    /// While true the source is (re)filling up to the target before audio
    /// is played, so a stall becomes one clean gap instead of crackle.
    priming: bool,
    target_frames: usize,
    min_target: usize,
    max_target: usize,
    step: usize,
    in_rate: u32,
    blocks_since_underrun: u64,
    blocks_per_second: f64,
    /// least room left after a pull since the last adaptation (frames)
    min_slack: usize,
    /// never shrink below the target that last ran dry
    floor: usize,
    /// fixed delay (echo reference): no growth, no re-prime, gaps are silence
    fixed: bool,
    /// the source restarted: the next underrun is that gap, not jitter
    source_epoch: u32,
    restart_grace: bool,
    /// frames of fade-in still to apply after a gap (no click on resume)
    fade_in: usize,
}

impl AdaptiveResampler {
    /// * `in_rate` – the source's sample rate.
    /// * `out_rate` – the output's sample rate (48 000 inside the engine).
    /// * `target_ms` – how much audio to keep buffered (latency vs. safety).
    /// * `blocks_per_second` – how often `pull` is called.
    pub fn new(
        channels: usize,
        in_rate: u32,
        out_rate: u32,
        target_ms: f64,
        blocks_per_second: f64,
    ) -> Self {
        let nominal = out_rate as f64 / in_rate as f64;
        let inner = Async::<f32>::new_poly(
            nominal,
            1.0 + 0.01,
            PolynomialDegree::Cubic,
            CHUNK,
            channels,
            FixedAsync::Output,
        )
        .expect("valid resampler parameters");
        let target_frames = ((in_rate as f64) * target_ms / 1000.0).round().max(1.0) as usize;
        let in_max = inner.input_frames_max();
        AdaptiveResampler {
            channels,
            drift: DriftController::new(target_frames as f64, blocks_per_second),
            in_buf: vec![0.0; in_max * channels],
            out_buf: vec![0.0; inner.output_frames_max() * channels],
            out_len: 0,
            out_pos: 0,
            inner,
            nominal,
            priming: true,
            target_frames,
            min_target: target_frames,
            // grow in small steps, never past 40 ms (latency budget)
            max_target: (in_rate as usize * 40 / 1000).max(target_frames),
            step: (in_rate as usize * 5 / 1000).max(1),
            in_rate,
            blocks_since_underrun: 0,
            blocks_per_second,
            min_slack: usize::MAX,
            floor: 0,
            fixed: false,
            source_epoch: 0,
            restart_grace: false,
            fade_in: 0,
        }
    }

    /// Current buffer target in milliseconds (adapts to the source's jitter).
    pub fn target_ms(&self) -> f32 {
        self.target_frames as f32 * 1000.0 / self.in_rate as f32
    }

    /// Keeps the delay constant (echo reference): no growth, no re-prime.
    pub fn set_fixed(&mut self) {
        self.fixed = true;
    }

    /// Called before each pull with the source's restart counter.
    pub fn note_source_epoch(&mut self, epoch: u32) {
        if epoch != self.source_epoch {
            self.source_epoch = epoch;
            self.restart_grace = true;
        }
    }

    fn adapt_after_underrun(&mut self) {
        // A source that delivers in bigger bursts than expected: give it more
        // room (5 ms at a time, up to 40 ms) instead of crackling forever.
        self.target_frames = (self.target_frames + self.step).min(self.max_target);
        self.floor = self.floor.max(self.target_frames);
        self.drift.set_target(self.target_frames as f64);
        self.blocks_since_underrun = 0;
        self.min_slack = usize::MAX;
    }

    fn adapt_when_stable(&mut self) {
        // After 15 s without trouble, win back 1 ms of latency.
        // ... but only if the buffer never ran low in that time (at least
        // 4 ms left after every pull), and never below the level that last
        // ran dry: the source's bursts need it.
        self.blocks_since_underrun += 1;
        if self.blocks_since_underrun as f64 > 15.0 * self.blocks_per_second {
            let spare = self.in_rate as usize * 4 / 1000 + CHUNK;
            if self.target_frames > self.min_target.max(self.floor)
                && self.min_slack != usize::MAX
                && self.min_slack > spare
            {
                self.target_frames =
                    (self.target_frames - (self.in_rate as usize / 1000)).max(self.min_target);
                self.drift.set_target(self.target_frames as f64);
            }
            self.blocks_since_underrun = 0;
            self.min_slack = usize::MAX;
        }
    }

    pub fn drift(&self) -> &DriftController {
        &self.drift
    }

    /// Frames of latency this stage holds on average (buffer target plus the
    /// resampler's own delay), in input frames.
    pub fn latency_frames(&self) -> usize {
        self.target_frames + self.inner.output_delay()
    }

    /// Fills `out` (interleaved, `channels`) completely. Never allocates.
    pub fn pull(&mut self, ring: &mut rtrb::Consumer<f32>, out: &mut [f32]) -> PullStats {
        let ch = self.channels;
        let mut stats = PullStats::default();
        let available = ring.slots() / ch;

        if self.priming {
            if available >= self.target_frames {
                self.priming = false;
                self.drift.reset();
                self.fade_in = FADE;
            } else {
                out.fill(0.0);
                stats.underrun_frames = (out.len() / ch) as u32;
                return stats;
            }
        }

        // Latency guard: drift control only corrects slowly (≤0.5 %), so a
        // big backlog (output started late, or stalled) is dropped at once
        // instead of being played late for a minute.
        let block_in = (out.len() / ch) as f64 / self.nominal;
        let limit = self.target_frames as f64 + (self.target_frames as f64).max(2.0 * block_in);
        let mut available = available;
        if available as f64 > limit {
            // keep the target, but never less than one block plus margin
            let keep = (self.target_frames as f64).max(1.5 * block_in + CHUNK as f64) as usize;
            let skip = available - keep;
            if let Ok(c) = ring.read_chunk(skip * ch) {
                c.commit_all();
            }
            stats.skipped_frames = skip as u32;
            available -= skip;
            self.drift.reset();
        }

        self.min_slack = self
            .min_slack
            .min(available.saturating_sub(block_in.ceil() as usize));
        if !self.fixed {
            self.adapt_when_stable();
        }
        // One controller update per pull (per output block).
        let rel = self.drift.update(available as f64);
        let _ = self.inner.set_resample_ratio(self.nominal * rel, true);

        let mut written = 0;
        let wanted = out.len();
        while written < wanted {
            if self.out_pos == self.out_len {
                let need = self.inner.input_frames_next();
                if ring.slots() < need * ch {
                    out[written..].fill(0.0);
                    stats.underrun_frames += ((wanted - written) / ch) as u32;
                    // the sound stops: ramp the last 1 ms down instead of a click
                    let tail = (written / ch).min(FADE);
                    for k in 0..tail {
                        let g = k as f32 / tail as f32; // 0 at the gap
                        let f = written / ch - 1 - k;
                        for c in 0..ch {
                            out[f * ch + c] *= g;
                        }
                    }
                    self.apply_fade_in(&mut out[..written]);
                    if self.fixed {
                        // keep the timeline: the gap is silence, nothing moves
                        self.fade_in = FADE;
                        return stats;
                    }
                    // Underrun: emit silence for the rest, re-prime, and
                    // enlarge the buffer – unless the source just restarted.
                    self.priming = true;
                    if std::mem::take(&mut self.restart_grace) {
                        self.min_slack = usize::MAX;
                    } else {
                        self.adapt_after_underrun();
                    }
                    return stats;
                }
                let chunk = ring.read_chunk(need * ch).expect("checked slots");
                let (a, b) = chunk.as_slices();
                self.in_buf[..a.len()].copy_from_slice(a);
                self.in_buf[a.len()..a.len() + b.len()].copy_from_slice(b);
                chunk.commit_all();
                let out_frames = self.out_buf.len() / ch;
                let input =
                    InterleavedSlice::new(&self.in_buf[..need * ch], ch, need).expect("in adapter");
                let mut output = InterleavedSlice::new_mut(&mut self.out_buf, ch, out_frames)
                    .expect("out adapter");
                let (_, produced) = self
                    .inner
                    .process_into_buffer(&input, &mut output, None)
                    .unwrap_or_default();
                self.out_len = produced * ch;
                self.out_pos = 0;
                if produced == 0 {
                    out[written..].fill(0.0);
                    return stats;
                }
            }
            let n = (self.out_len - self.out_pos).min(wanted - written);
            out[written..written + n]
                .copy_from_slice(&self.out_buf[self.out_pos..self.out_pos + n]);
            self.out_pos += n;
            written += n;
        }
        self.apply_fade_in(out);
        stats
    }

    fn apply_fade_in(&mut self, out: &mut [f32]) {
        let ch = self.channels;
        let frames = out.len() / ch;
        let n = self.fade_in.min(frames);
        for f in 0..n {
            let g = (FADE - self.fade_in + f + 1) as f32 / FADE as f32;
            for c in 0..ch {
                out[f * ch + c] *= g;
            }
        }
        self.fade_in -= n;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, rate: f64, n: usize, phase: &mut f64) -> Vec<f32> {
        (0..n)
            .map(|_| {
                let v = (2.0 * std::f64::consts::PI * *phase).sin() as f32 * 0.5;
                *phase += freq / rate;
                v
            })
            .collect()
    }

    /// Goertzel power of `freq` in `x`.
    fn tone_level(x: &[f32], freq: f64, rate: f64) -> f64 {
        let w = 2.0 * std::f64::consts::PI * freq / rate;
        let c = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0f64, 0.0f64);
        for &v in x {
            let s0 = v as f64 + c * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        ((s1 * s1 + s2 * s2 - c * s1 * s2).sqrt()) / (x.len() as f64 / 2.0)
    }

    /// 44.1 kHz source running 300 ppm fast into a 48 kHz sink for 10
    /// simulated minutes: no underruns after priming, tone preserved.
    #[test]
    fn converts_rate_and_tracks_drift_without_dropouts() {
        let in_rate = 44100.0 * (1.0 + 300e-6);
        let (mut prod, mut cons) = rtrb::RingBuffer::<f32>::new(44100);
        let mut r = AdaptiveResampler::new(1, 44100, 48000, 15.0, 100.0);
        let mut phase = 0.0;
        let mut frac = 0.0f64;
        let mut out = vec![0f32; 480];
        let mut underruns_after_start = 0u32;
        let mut last = Vec::new();
        for block in 0..(600 * 100) {
            frac += in_rate / 100.0;
            let n = frac.floor() as usize;
            frac -= n as f64;
            for v in sine(1000.0, 44100.0, n, &mut phase) {
                let _ = prod.push(v);
            }
            let st = r.pull(&mut cons, &mut out);
            if block > 10 {
                underruns_after_start += st.underrun_frames;
            }
            if block >= 600 * 100 - 20 {
                last.extend_from_slice(&out);
            }
        }
        assert_eq!(underruns_after_start, 0);
        // after a 10 ms pull, about (15 - 10) ms of margin remains buffered
        let fill_ms = cons.slots() as f64 / 44.1;
        assert!(fill_ms > 1.0 && fill_ms < 30.0, "fill {fill_ms} ms");
        assert!(
            (r.drift().drift_ppm() - 300.0).abs() < 30.0,
            "ppm {}",
            r.drift().drift_ppm()
        );
        // the tone is still a clean 1 kHz (other bins far below)
        let main = tone_level(&last, 1000.0 * (1.0 + 300e-6), 48000.0);
        let off = tone_level(&last, 1500.0, 48000.0);
        assert!(main > 0.4, "main {main}");
        assert!(
            20.0 * (off / main).log10() < -60.0,
            "spur {} dB",
            20.0 * (off / main).log10()
        );
    }

    /// The CI failure scenario: a device that delivers 22 ms packets and
    /// runs 557 ppm slow, read in 10 ms blocks. The tone must stay a clean,
    /// correctly pitched 1 kHz.
    #[test]
    fn bursty_slow_device_keeps_tone_clean() {
        let (mut prod, mut cons) = rtrb::RingBuffer::<f32>::new(48000);
        let mut r = AdaptiveResampler::new(1, 48000, 48000, 20.0, 100.0);
        let mut phase = 0.0;
        let mut acc = 0.0f64;
        let mut out = vec![0f32; 480];
        let mut tail = Vec::new();
        let blocks = 100 * 120;
        let mut late_underruns = 0;
        let mut skipped = 0;
        for b in 0..blocks {
            acc += 480.0 * (1.0 - 557e-6);
            while acc >= 1056.0 {
                for v in sine(1000.0, 48000.0, 1056, &mut phase) {
                    let _ = prod.push(v);
                }
                acc -= 1056.0;
            }
            let st = r.pull(&mut cons, &mut out);
            // the buffer may grow during the first seconds; after that, none
            if b > 100 * 10 {
                late_underruns += st.underrun_frames;
                skipped += st.skipped_frames;
            }
            if b >= blocks - 300 {
                tail.extend_from_slice(&out);
            }
        }
        assert_eq!(late_underruns, 0);
        assert!(r.target_ms() <= 45.0, "buffer grew to {} ms", r.target_ms());
        // the slow device's 1000 Hz plays at 1000·(1-557 ppm) in real time
        let f = 1000.0 * (1.0 - 557e-6);
        assert_eq!(skipped, 0);
        let main = 20.0 * tone_level(&tail, f, 48000.0).log10();
        assert!(
            main > -6.5 && main < -5.5,
            "level {main} dB (expected -6.0)"
        );
    }

    /// Two hours of 10 ms packets in and 10 ms pulls out, both with timing
    /// jitter and a clock difference, at several phase offsets (the worst
    /// case is when the two clocks' phases cross). Returns (dropouts after
    /// the first minute, final target ms, max target ms).
    fn simulate(start_ms: f64, phase_us: i64, seed: u64) -> (u32, f32, f32) {
        let (mut prod, mut cons) = rtrb::RingBuffer::<f32>::new(48000);
        let mut r = AdaptiveResampler::new(1, 48000, 48000, start_ms, 100.0);
        let mut rng = seed | 1;
        let mut rand = move || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            (rng % 2001) as i64 - 1000 // -1000..=1000
        };
        let mut out = vec![0.0f32; 480];
        let packet = vec![0.25f32; 480];
        let mut k_in: i64 = 0;
        let mut k_out: i64 = 0;
        let end: i64 = 2 * 3600 * 1_000_000;
        let mut next_in = 0i64;
        let mut next_out = phase_us;
        let (mut dropouts, mut max_target) = (0u32, 0.0f32);
        let mut was_dry = false;
        while next_in < end || next_out < end {
            if next_in <= next_out {
                let _ = prod
                    .write_chunk_uninit(480)
                    .map(|c| c.fill_from_iter(packet.iter().copied()));
                k_in += 1;
                // capture: ±2 ms jitter around its own grid
                next_in = k_in * 10_000 + 2 * rand();
            } else {
                let st = r.pull(&mut cons, &mut out);
                let dry = st.underrun_frames > 0;
                if dry && !was_dry && next_out > 60_000_000 {
                    dropouts += 1;
                }
                was_dry = dry;
                max_target = max_target.max(r.target_ms());
                k_out += 1;
                // render clock 100 ppm fast, ±1 ms wake-up jitter
                next_out = phase_us + (k_out as f64 * 10_000.0 / (1.0 + 100e-6)) as i64 + rand();
            }
        }
        (dropouts, r.target_ms(), max_target)
    }

    #[test]
    #[ignore = "slow: two simulated hours per case (run with --ignored --release)"]
    fn two_hours_of_jitter_report() {
        for start in [12.0, 17.0, 20.0] {
            for (i, phase) in [0i64, 1_000, 5_000, 9_000].into_iter().enumerate() {
                let (d, t, m) = simulate(start, phase, 0x9e37_79b9 + i as u64);
                println!("start {start} ms phase {phase} us: {d} dropouts, target {t:.1} ms, max {m:.1} ms");
            }
        }
    }

    #[test]
    fn jitter_does_not_keep_dropping_out() {
        // a shortened (but still long) run of the case above: the buffer may
        // grow once or twice at the start, but must not shrink back into
        // dropouts every few minutes.
        for (i, phase) in [0i64, 5_000, 9_500].into_iter().enumerate() {
            let (d, _, m) = simulate(20.0, phase, 0x1234_5678 + i as u64);
            assert_eq!(d, 0, "phase {phase}: {d} dropouts in two hours");
            assert!(m <= 32.0, "phase {phase}: buffer grew to {m} ms");
        }
    }

    #[test]
    fn gaps_fade_out_and_in_without_clicks() {
        let (mut prod, mut cons) = rtrb::RingBuffer::<f32>::new(48000);
        let mut r = AdaptiveResampler::new(1, 48000, 48000, 20.0, 100.0);
        let mut out = vec![0.0f32; 480];
        let mut all = Vec::new();
        for block in 0..200 {
            // a constant level: any jump in the output is a click
            let stalled = (80..100).contains(&block);
            if !stalled {
                let _ = prod
                    .write_chunk_uninit(480)
                    .map(|c| c.fill_from_iter(std::iter::repeat(0.5f32)));
            }
            r.pull(&mut cons, &mut out);
            all.extend_from_slice(&out);
        }
        let max_step = all.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        assert!(max_step < 0.05, "a click: step of {max_step}");
        assert!(all.iter().any(|&x| x == 0.0), "the stall must be a gap");
    }

    #[test]
    fn startup_backlog_is_dropped_not_played_late() {
        let (mut prod, mut cons) = rtrb::RingBuffer::<f32>::new(48000);
        let mut r = AdaptiveResampler::new(1, 48000, 48000, 20.0, 100.0);
        // 300 ms arrive before the output starts pulling
        let mut phase = 0.0;
        for v in sine(1000.0, 48000.0, 14400, &mut phase) {
            let _ = prod.push(v);
        }
        let mut out = vec![0f32; 480];
        let st = r.pull(&mut cons, &mut out);
        assert!(st.skipped_frames > 13000, "skipped {}", st.skipped_frames);
        // from then on: real-time delivery, latency stays near the target
        let mut last = Vec::new();
        for i in 0..2000 {
            for v in sine(1000.0, 48000.0, 480, &mut phase) {
                let _ = prod.push(v);
            }
            r.pull(&mut cons, &mut out);
            assert!(
                cons.slots() < 48 * 40,
                "buffered {} ms at block {i}",
                cons.slots() / 48
            );
            if i >= 1900 {
                last.extend_from_slice(&out);
            }
        }
        // pitch is right: the tone sits at 1000 Hz
        assert!(tone_level(&last, 1000.0, 48000.0) > 0.4);
        assert!(
            r.drift().drift_ppm().abs() < 100.0,
            "ppm {}",
            r.drift().drift_ppm()
        );
    }

    #[test]
    fn stalled_source_gives_silence_then_recovers() {
        let (mut prod, mut cons) = rtrb::RingBuffer::<f32>::new(48000 * 2);
        let mut r = AdaptiveResampler::new(2, 48000, 48000, 10.0, 100.0);
        let mut out = vec![1f32; 960];
        // nothing delivered yet → silence
        let st = r.pull(&mut cons, &mut out);
        assert_eq!(st.underrun_frames, 480);
        assert!(out.iter().all(|&v| v == 0.0));
        for _ in 0..(48000 / 10) {
            let _ = prod.push(0.25);
            let _ = prod.push(0.25);
        }
        let st = r.pull(&mut cons, &mut out);
        assert_eq!(st.underrun_frames, 0);
        assert!(out[600..].iter().all(|&v| (v - 0.25).abs() < 1e-3));
    }
}
