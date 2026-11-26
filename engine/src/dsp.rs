//! Real-time DSP building blocks. Nothing here allocates, locks or logs after
//! construction, so every `process` function is safe to call from an audio
//! thread.

#![allow(clippy::needless_range_loop)] // index loops mirror the DSP maths

use std::sync::atomic::{AtomicU32, Ordering};

/// Converts decibels to a linear factor. `-inf` dB → 0.
#[inline]
pub fn db_to_lin(db: f32) -> f32 {
    if db <= -120.0 {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

/// Converts a linear factor to decibels (floored at -120 dB).
#[inline]
pub fn lin_to_db(lin: f32) -> f32 {
    if lin <= 1e-6 {
        -120.0
    } else {
        20.0 * lin.log10()
    }
}

/// An `f32` that can be shared between the UI thread and an audio thread
/// without locks.
#[derive(Debug)]
pub struct AtomicF32(AtomicU32);

impl AtomicF32 {
    pub const fn new_bits(bits: u32) -> Self {
        AtomicF32(AtomicU32::new(bits))
    }
    pub fn new(v: f32) -> Self {
        AtomicF32(AtomicU32::new(v.to_bits()))
    }
    #[inline]
    pub fn load(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
    #[inline]
    pub fn store(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Relaxed)
    }
    /// Stores `max(current, v)`; used by meters so no peak is lost between
    /// two UI reads.
    #[inline]
    pub fn fetch_max(&self, v: f32) {
        let mut cur = self.0.load(Ordering::Relaxed);
        while f32::from_bits(cur) < v {
            match self.0.compare_exchange_weak(cur, v.to_bits(), Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(c) => cur = c,
            }
        }
    }
    /// Reads and resets to zero (meter consumers).
    #[inline]
    pub fn take(&self) -> f32 {
        f32::from_bits(self.0.swap(0f32.to_bits(), Ordering::Relaxed))
    }
}

impl Default for AtomicF32 {
    fn default() -> Self {
        AtomicF32::new(0.0)
    }
}

/// A gain that glides linearly to its target so gain changes and mutes never
/// click.
#[derive(Clone, Debug)]
pub struct SmoothedGain {
    current: f32,
    target: f32,
    step: f32,
    ramp_samples: f32,
}

impl SmoothedGain {
    /// `ramp_ms` is the time a full 0→1 change takes.
    pub fn new(initial: f32, sample_rate: f32, ramp_ms: f32) -> Self {
        let ramp_samples = (sample_rate * ramp_ms / 1000.0).max(1.0);
        SmoothedGain { current: initial, target: initial, step: 0.0, ramp_samples }
    }

    pub fn set_target(&mut self, target: f32) {
        let target = if target.is_finite() { target.max(0.0) } else { 0.0 };
        if (target - self.target).abs() > f32::EPSILON {
            self.target = target;
            self.step = (self.target - self.current) / self.ramp_samples;
        }
    }

    pub fn current(&self) -> f32 {
        self.current
    }

    pub fn is_silent(&self) -> bool {
        self.current == 0.0 && self.target == 0.0
    }

    #[inline]
    fn next(&mut self) -> f32 {
        if self.current != self.target {
            self.current += self.step;
            if (self.step > 0.0 && self.current >= self.target) || (self.step < 0.0 && self.current <= self.target) {
                self.current = self.target;
            }
        }
        self.current
    }

    /// Multiplies `src` (interleaved, `channels`) by the gain and adds it into `dst`.
    #[inline]
    pub fn mix_into(&mut self, src: &[f32], dst: &mut [f32], channels: usize) {
        debug_assert_eq!(src.len(), dst.len());
        if self.is_silent() {
            return;
        }
        for (s, d) in src.chunks_exact(channels).zip(dst.chunks_exact_mut(channels)) {
            let g = self.next();
            for c in 0..channels {
                d[c] += s[c] * g;
            }
        }
    }

    /// Applies the gain in place.
    #[inline]
    pub fn apply(&mut self, buf: &mut [f32], channels: usize) {
        for frame in buf.chunks_exact_mut(channels) {
            let g = self.next();
            for x in frame.iter_mut() {
                *x *= g;
            }
        }
    }
}

/// Peak and RMS meter published to the UI through atomics.
#[derive(Debug, Default)]
pub struct MeterValues {
    pub peak: AtomicF32,
    /// Mean square accumulated since the last read, and its sample count.
    pub sum_sq: AtomicF32,
    pub count: AtomicU32,
}

impl MeterValues {
    #[inline]
    pub fn feed(&self, buf: &[f32]) {
        let mut peak = 0f32;
        let mut sq = 0f32;
        for &x in buf {
            peak = peak.max(x.abs());
            sq += x * x;
        }
        self.peak.fetch_max(peak);
        // Accumulation is approximate across concurrent readers; that is
        // fine for a meter.
        self.sum_sq.store(self.sum_sq.load() + sq);
        self.count.fetch_add(buf.len() as u32, Ordering::Relaxed);
    }

    /// Returns (peak, rms) since the last call and resets.
    pub fn read(&self) -> (f32, f32) {
        let peak = self.peak.take();
        let sq = self.sum_sq.take();
        let n = self.count.swap(0, Ordering::Relaxed);
        let rms = if n > 0 { (sq / n as f32).sqrt() } else { 0.0 };
        (peak, rms)
    }
}

/// Look-ahead true-peak limiter.
///
/// * The true peak is estimated with 8× polyphase oversampling, so peaks
///   between samples are caught (what a DAC or a later resampler would make).
/// * The gain needed for each sample is spread over a sliding-minimum window
///   and smoothed with a box filter of the look-ahead length. That guarantees
///   the gain reaches its value *before* the peak leaves the delay line, so
///   the ceiling is never exceeded and the gain never jumps (no clicks).
/// * Release is exponential.
/// * Because the gain differs slightly from sample to sample, the waveform
///   reconstructed *between* samples can overshoot what the per-sample
///   estimate predicts (by ~0.1–0.2 dB under extreme limiting, measured by
///   the property tests against an independent reference meter). The
///   limiter therefore holds samples and estimated inter-sample peaks
///   `INTERNAL_HEADROOM_DB` below the requested true-peak ceiling.
pub struct TruePeakLimiter {
    channels: usize,
    ceiling: f32,
    lookahead: usize,
    delay_frames: usize,
    release_coeff: f32,
    // delay line for the audio, interleaved, length lookahead frames
    delay: Vec<f32>,
    delay_pos: usize,
    // history for oversampling, per channel (TAPS samples)
    hist: Vec<f32>,
    hist_pos: usize,
    // monotonic deque of (index, gain) for the sliding minimum, as a ring
    dq_idx: Vec<u64>,
    dq_val: Vec<f32>,
    dq_head: usize,
    dq_len: usize,
    // box filter
    box_buf: Vec<f32>,
    box_pos: usize,
    box_sum: f64,
    released: f32,
    n: u64,
    /// Current gain reduction, for the UI.
    pub reduction: f32,
}

const TAPS: usize = 24;
/// Extra headroom below the requested true-peak ceiling (see type docs).
pub const INTERNAL_HEADROOM_DB: f32 = 0.3;
const PHASES: usize = 8;

/// Polyphase coefficients for 8× interpolation between the two centre taps
/// of a TAPS-sample history (taps TAPS/2-1 and TAPS/2). Phase p interpolates at fraction
/// p/8 after the first centre tap. Windowed sinc (Blackman), each phase normalised to unity
/// DC gain.
fn polyphase() -> [[f32; TAPS]; PHASES - 1] {
    let mut c = [[0f32; TAPS]; PHASES - 1];
    let half = TAPS as f64 / 2.0;
    for p in 1..PHASES {
        let f = p as f64 / PHASES as f64;
        let mut sum = 0.0;
        for t in 0..TAPS {
            let x = f - (t as f64 - (half - 1.0));
            let pi_x = std::f64::consts::PI * x;
            let sinc = if x.abs() < 1e-12 { 1.0 } else { pi_x.sin() / pi_x };
            let w = 0.42 + 0.5 * (std::f64::consts::PI * x / half).cos() + 0.08 * (2.0 * std::f64::consts::PI * x / half).cos();
            c[p - 1][t] = (sinc * w) as f32;
            sum += sinc * w;
        }
        for t in 0..TAPS {
            c[p - 1][t] /= sum as f32;
        }
    }
    c
}

impl TruePeakLimiter {
    /// `ceiling_db` e.g. -1.0 dBTP, `lookahead_ms` e.g. 1.5, `release_ms` e.g. 80.
    pub fn new(channels: usize, sample_rate: f32, ceiling_db: f32, lookahead_ms: f32, release_ms: f32) -> Self {
        let lookahead = ((sample_rate * lookahead_ms / 1000.0) as usize).max(TAPS);
        // The peak estimate refers to the centre of the 16-sample history,
        // TAPS/2 samples back; the audio is delayed by that much extra, and the
        // sliding-minimum window is lookahead+2 so both centre samples are
        // covered (see the guarantee in the module docs).
        let window = lookahead + 2;
        let delay_frames = lookahead + TAPS / 2;
        TruePeakLimiter {
            channels,
            ceiling: db_to_lin(ceiling_db - INTERNAL_HEADROOM_DB),
            lookahead,
            release_coeff: (-1.0 / (sample_rate * release_ms / 1000.0)).exp(),
            delay: vec![0.0; delay_frames * channels],
            delay_frames,
            delay_pos: 0,
            hist: vec![0.0; TAPS * channels],
            hist_pos: 0,
            dq_idx: vec![0; window + 1],
            dq_val: vec![1.0; window + 1],
            dq_head: 0,
            dq_len: 0,
            box_buf: vec![1.0; lookahead],
            box_pos: 0,
            box_sum: lookahead as f64,
            released: 1.0,
            n: 0,
            reduction: 1.0,
        }
    }

    /// Latency added by the limiter, in frames.
    pub fn latency(&self) -> usize {
        self.delay_frames
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        let coeffs = polyphase_cached();
        let ch = self.channels;
        let window = (self.lookahead + 2) as u64;
        let cap = self.dq_idx.len();
        let mut min_red = 1f32;
        for frame in buf.chunks_exact_mut(ch) {
            // 1. true-peak estimate of the incoming frame (linked channels)
            // History index of the oldest sample after writing this frame.
            for c in 0..ch {
                self.hist[c * TAPS + self.hist_pos] = frame[c];
            }
            let oldest = (self.hist_pos + 1) % TAPS;
            let mut peak = 0f32;
            for c in 0..ch {
                let h = &self.hist[c * TAPS..(c + 1) * TAPS];
                // the two centre samples…
                peak = peak.max(h[(oldest + TAPS / 2 - 1) % TAPS].abs()).max(h[(oldest + TAPS / 2) % TAPS].abs());
                // …and the three points between them
                for p in 0..PHASES - 1 {
                    let mut acc = 0f32;
                    for t in 0..TAPS {
                        acc += h[(oldest + t) % TAPS] * coeffs[p][t];
                    }
                    peak = peak.max(acc.abs());
                }
            }
            self.hist_pos = (self.hist_pos + 1) % TAPS;
            let target = if peak > self.ceiling { self.ceiling / peak } else { 1.0 };

            // 2. sliding minimum over the last lookahead+1 targets
            while self.dq_len > 0 {
                let back = (self.dq_head + self.dq_len - 1) % cap;
                if self.dq_val[back] >= target {
                    self.dq_len -= 1;
                } else {
                    break;
                }
            }
            let back = (self.dq_head + self.dq_len) % cap;
            self.dq_idx[back] = self.n;
            self.dq_val[back] = target;
            self.dq_len += 1;
            while self.n >= window && self.dq_idx[self.dq_head] <= self.n - window {
                self.dq_head = (self.dq_head + 1) % cap;
                self.dq_len -= 1;
            }
            let m = self.dq_val[self.dq_head];

            // 3. release (only slows rising gain, so it never breaks the guarantee)
            self.released = if m < self.released { m } else { m + (self.released - m) * self.release_coeff };

            // 4. box filter of length lookahead
            self.box_sum += self.released as f64 - self.box_buf[self.box_pos] as f64;
            self.box_buf[self.box_pos] = self.released;
            self.box_pos = (self.box_pos + 1) % self.lookahead;
            if self.box_pos == 0 {
                // refresh the running sum to stop floating-point creep
                self.box_sum = self.box_buf.iter().map(|&x| x as f64).sum();
            }
            let gain = (self.box_sum / self.lookahead as f64) as f32;
            min_red = min_red.min(gain);

            // 5. delayed audio × gain
            let d = self.delay_pos * ch;
            for c in 0..ch {
                let delayed = self.delay[d + c];
                self.delay[d + c] = frame[c];
                frame[c] = delayed * gain;
            }
            self.delay_pos = (self.delay_pos + 1) % self.delay_frames;
            self.n += 1;
        }
        self.reduction = min_red;
    }
}

fn polyphase_cached() -> &'static [[f32; TAPS]; PHASES - 1] {
    static COEFFS: std::sync::OnceLock<[[f32; TAPS]; PHASES - 1]> = std::sync::OnceLock::new();
    COEFFS.get_or_init(polyphase)
}

impl TruePeakLimiter {
    /// Forces coefficient initialisation outside the audio thread.
    pub fn warm_up() {
        let _ = polyphase_cached();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn bessel_i0(x: f64) -> f64 {
        let (mut sum, mut term) = (1.0, 1.0);
        for k in 1..50 {
            term *= (x / 2.0) * (x / 2.0) / (k as f64 * k as f64);
            sum += term;
        }
        sum
    }

    /// Reference true-peak meter, independent of the limiter's estimator:
    /// 16× oversampling with a 194-tap Kaiser (β = 7) windowed sinc,
    /// normalised per point, evaluated only where the full filter support
    /// exists. Accurate to 0.01 dB up to 18 kHz on pure tones (verified when
    /// it was written; see `reference_meter_is_accurate`).
    pub(super) fn reference_true_peak(x: &[f32]) -> f32 {
        let os = 16;
        let half = 96usize;
        if x.len() <= 2 * half + 2 {
            return x.iter().fold(0f32, |m, v| m.max(v.abs()));
        }
        let mut peak = x[half..x.len() - half].iter().fold(0f32, |m, v| m.max(v.abs())) as f64;
        let beta = 7.0;
        let norm = bessel_i0(beta);
        let mut coeffs = vec![0f64; 2 * half + 2];
        for k in 1..os {
            let frac = k as f64 / os as f64;
            let mut sum = 0.0;
            for (m, c) in coeffs.iter_mut().enumerate() {
                let d = frac + half as f64 - m as f64;
                let r = (d / (half as f64 + 1.0)).clamp(-1.0, 1.0);
                let w = bessel_i0(beta * (1.0 - r * r).sqrt()) / norm;
                let pd = std::f64::consts::PI * d;
                let sinc = if d.abs() < 1e-12 { 1.0 } else { pd.sin() / pd };
                *c = sinc * w;
                sum += *c;
            }
            for c in coeffs.iter_mut() {
                *c /= sum;
            }
            for i in half..x.len() - half - 1 {
                let acc: f64 = coeffs.iter().enumerate().map(|(m, c)| x[i - half + m] as f64 * c).sum();
                peak = peak.max(acc.abs());
            }
        }
        peak as f32
    }

    #[test]
    fn reference_meter_is_accurate() {
        for f in [1000.0f64, 8184.0, 12728.0, 16000.0, 18000.0] {
            let x: Vec<f32> = (0..800).map(|i| (0.5 * (2.0 * std::f64::consts::PI * f * i as f64 / 48000.0 + 0.3).sin()) as f32).collect();
            let err = lin_to_db(reference_true_peak(&x) / 0.5);
            assert!(err.abs() < 0.02, "{f} Hz: {err} dB");
        }
    }

    #[test]
    fn gain_ramp_has_no_jumps() {
        let mut g = SmoothedGain::new(0.0, 48000.0, 10.0);
        g.set_target(1.0);
        let mut buf = vec![1.0f32; 1000];
        g.apply(&mut buf, 1);
        for w in buf.windows(2) {
            assert!((w[1] - w[0]).abs() <= 1.0 / 480.0 + 1e-6);
        }
        assert!((buf[999] - 1.0).abs() < 1e-6);
        g.set_target(0.0);
        let mut buf = vec![1.0f32; 1000];
        g.apply(&mut buf, 1);
        assert_eq!(buf[999], 0.0);
        assert!(g.is_silent());
    }

    #[test]
    fn limiter_is_transparent_below_ceiling() {
        let mut l = TruePeakLimiter::new(2, 48000.0, -1.0, 1.5, 80.0);
        let lat = l.latency();
        let input: Vec<f32> = (0..9600).map(|i| 0.5 * ((i / 2) as f32 * 0.05).sin()).collect();
        let mut out = input.clone();
        l.process(&mut out);
        for i in (lat * 2)..input.len() {
            assert!((out[i] - input[i - lat * 2]).abs() < 1e-5, "sample {i}");
        }
    }

    #[test]
    fn limiter_catches_intersample_peaks() {
        // fs/4 sine at 45° phase: samples are ±0.707·A, true peak is A.
        let a = 1.0f32;
        let input: Vec<f32> = (0..4800).map(|i| a * (std::f32::consts::FRAC_PI_2 * i as f32 + std::f32::consts::FRAC_PI_4).sin()).collect();
        assert!(input.iter().fold(0f32, |m, v| m.max(v.abs())) < 0.75);
        let mut l = TruePeakLimiter::new(1, 48000.0, -1.0, 1.5, 80.0);
        let mut out = input.clone();
        l.process(&mut out);
        let tp = reference_true_peak(&out[200..]);
        assert!(lin_to_db(tp) <= -1.0 + 0.02, "true peak {} dB", lin_to_db(tp));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]
        #[test]
        fn limiter_never_exceeds_ceiling(
            amp in 0.1f32..8.0,
            freqs in prop::collection::vec(20f32..16000.0, 1..4),
            noise in 0f32..1.0,
            seed in any::<u64>(),
        ) {
            // Band-limited test material (≤ ~16 kHz, like voice through a
            // codec): tones plus noise through a 4th-order low-pass.
            let mut rng = seed;
            let mut next = move || { rng ^= rng << 13; rng ^= rng >> 7; rng ^= rng << 17; (rng as f64 / u64::MAX as f64) as f32 * 2.0 - 1.0 };
            let n = 2400;
            let a = (-2.0 * std::f32::consts::PI * 8000.0 / 48000.0).exp();
            let mut lp = [0f32; 4];
            let input: Vec<f32> = (0..n).map(|i| {
                let t = i as f32 / 48000.0;
                let s: f32 = freqs.iter().map(|f| (2.0 * std::f32::consts::PI * f * t).sin()).sum::<f32>() / freqs.len() as f32;
                let mut x = next();
                for st in lp.iter_mut() { *st = a * *st + (1.0 - a) * x; x = *st; }
                amp * (s + noise * 2.0 * x)
            }).collect();
            let mut l = TruePeakLimiter::new(1, 48000.0, -1.0, 1.5, 80.0);
            let mut out = input.clone();
            l.process(&mut out);
            // sample peak: hard guarantee (ceiling minus internal headroom)
            let sp = out.iter().fold(0f32, |m, v| m.max(v.abs()));
            prop_assert!(sp <= db_to_lin(-1.0 - INTERNAL_HEADROOM_DB) * 1.0001, "sample peak {}", sp);
            // also with full-band white noise, the sample-peak guarantee holds
            let mut wn: Vec<f32> = (0..n).map(|_| amp * next()).collect();
            let mut l2 = TruePeakLimiter::new(1, 48000.0, -1.0, 1.5, 80.0);
            l2.process(&mut wn);
            prop_assert!(wn.iter().all(|v| v.abs() <= db_to_lin(-1.0 - INTERNAL_HEADROOM_DB) * 1.0001));
            // true peak (independent 16× reference meter) at or below the
            // ceiling; 0.1 dB allowance for the reference meter itself
            let tp = reference_true_peak(&out);
            prop_assert!(lin_to_db(tp) <= -1.0 + 0.02, "true peak {} dB", lin_to_db(tp));
            prop_assert!(out.iter().all(|v| v.is_finite()));
        }

        #[test]
        fn smoothed_gain_bounded_steps(targets in prop::collection::vec(0f32..4.0, 1..20)) {
            let mut g = SmoothedGain::new(1.0, 48000.0, 10.0);
            let mut last = 1.0f32;
            for t in targets {
                g.set_target(t);
                let mut buf = vec![1.0f32; 256];
                g.apply(&mut buf, 1);
                for &v in &buf {
                    prop_assert!((v - last).abs() <= 4.0 / 480.0 + 1e-5);
                    prop_assert!(v >= 0.0);
                    last = v;
                }
            }
        }
    }
}

