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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PullStats {
    /// Output frames that had to be filled with silence because the source
    /// did not deliver in time.
    pub underrun_frames: u32,
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
}

impl AdaptiveResampler {
    /// * `in_rate` – the source's sample rate.
    /// * `out_rate` – the output's sample rate (48 000 inside the engine).
    /// * `target_ms` – how much audio to keep buffered (latency vs. safety).
    /// * `blocks_per_second` – how often `pull` is called.
    pub fn new(channels: usize, in_rate: u32, out_rate: u32, target_ms: f64, blocks_per_second: f64) -> Self {
        let nominal = out_rate as f64 / in_rate as f64;
        let inner = Async::<f32>::new_poly(nominal, 1.0 + 0.01, PolynomialDegree::Cubic, CHUNK, channels, FixedAsync::Output)
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
            } else {
                out.fill(0.0);
                stats.underrun_frames = (out.len() / ch) as u32;
                return stats;
            }
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
                    // Underrun: emit silence for the rest and re-prime.
                    out[written..].fill(0.0);
                    stats.underrun_frames += ((wanted - written) / ch) as u32;
                    self.priming = true;
                    return stats;
                }
                let chunk = ring.read_chunk(need * ch).expect("checked slots");
                let (a, b) = chunk.as_slices();
                self.in_buf[..a.len()].copy_from_slice(a);
                self.in_buf[a.len()..a.len() + b.len()].copy_from_slice(b);
                chunk.commit_all();
                let out_frames = self.out_buf.len() / ch;
                let input = InterleavedSlice::new(&self.in_buf[..need * ch], ch, need).expect("in adapter");
                let mut output = InterleavedSlice::new_mut(&mut self.out_buf, ch, out_frames).expect("out adapter");
                let (_, produced) = self.inner.process_into_buffer(&input, &mut output, None).unwrap_or_default();
                self.out_len = produced * ch;
                self.out_pos = 0;
                if produced == 0 {
                    out[written..].fill(0.0);
                    return stats;
                }
            }
            let n = (self.out_len - self.out_pos).min(wanted - written);
            out[written..written + n].copy_from_slice(&self.out_buf[self.out_pos..self.out_pos + n]);
            self.out_pos += n;
            written += n;
        }
        stats
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
        assert!((r.drift().drift_ppm() - 300.0).abs() < 30.0, "ppm {}", r.drift().drift_ppm());
        // the tone is still a clean 1 kHz (other bins far below)
        let main = tone_level(&last, 1000.0 * (1.0 + 300e-6), 48000.0);
        let off = tone_level(&last, 1500.0, 48000.0);
        assert!(main > 0.4, "main {main}");
        assert!(20.0 * (off / main).log10() < -60.0, "spur {} dB", 20.0 * (off / main).log10());
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
