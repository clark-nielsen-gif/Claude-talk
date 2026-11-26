//! Clock-drift compensation.
//!
//! Every device has its own crystal. A microphone running at "48 000 Hz" and
//! a virtual cable running at "48 000 Hz" differ by up to a few hundred ppm,
//! so after an hour one of them is ~1 s ahead. Without correction the ring
//! buffer between them would eventually overflow (dropouts) or run dry
//! (gaps). The controller watches the fill level of the ring buffer and nudges
//! the resampling ratio so the fill level stays at its target.

/// Largest pitch correction the proportional term may apply (1000 ppm =
/// 1.7 cents, inaudible; 1.5 Hz on a 1.5 kHz test tone).
const MAX_P: f64 = 1e-3;

/// PI controller that turns "how full is the buffer" into a relative
/// resampling ratio (1.0 = nominal).
#[derive(Clone, Debug)]
pub struct DriftController {
    target: f64,
    kp: f64,
    ki: f64,
    integral: f64,
    filtered: Option<f64>,
    smoothing: f64,
    max_rel: f64,
    ratio: f64,
}

impl DriftController {
    /// * `target_frames` – fill level to hold (the jitter margin).
    /// * `update_hz` – how often `update` is called (blocks per second).
    pub fn new(target_frames: f64, update_hz: f64) -> Self {
        // Devices deliver audio in bursts (e.g. 22 ms packets read every
        // 10 ms), so the fill level has a large saw-tooth that says nothing
        // about drift. The level is therefore averaged over ~2 s, and the
        // proportional term is small (1000 frames of error → 100 ppm) so the
        // remaining ripple cannot frequency-modulate the audio. The integral
        // term absorbs the real, constant drift within a few seconds.
        //
        // Loop design (per update, B = frames consumed per update): the plant
        // integrates B·(drift + u). With natural frequency ω = 1.5e-3 rad per
        // update and damping ≈ 0.7: kp·B = 2ζω ≈ 2.1e-3, ki·B = ω² = 2.25e-6.
        // That settles in ~40 s, keeps the 2 s averaging well inside the loop
        // bandwidth, and turns ±1 frame of residual ripple into < 5 ppm.
        let b = 48_000.0 / update_hz.max(1.0);
        let kp = 2.1e-3 / b;
        let ki = 2.25e-6 / b;
        DriftController {
            target: target_frames,
            kp,
            ki,
            integral: 0.0,
            filtered: None,
            smoothing: (-1.0 / (2.0 * update_hz.max(1.0))).exp(),
            // ±2500 ppm (4.3 cents): above any real device (virtual cables
            // measured ~560 ppm), and inaudible on speech. Bigger jumps in
            // the fill level are handled by dropping/re-priming instead.
            max_rel: 0.0025,
            ratio: 1.0,
        }
    }

    pub fn target(&self) -> f64 {
        self.target
    }

    pub fn set_target(&mut self, target_frames: f64) {
        self.target = target_frames;
    }

    /// Current relative ratio (output/input speed); > 1 means the consumer is
    /// stretching the input (input buffer is running low).
    pub fn ratio(&self) -> f64 {
        self.ratio
    }

    /// Estimated drift in ppm (positive: the source is faster than the sink).
    pub fn drift_ppm(&self) -> f64 {
        (1.0 / self.ratio - 1.0) * 1e6
    }

    /// Feed the fill level measured just before a block is consumed; returns
    /// the relative ratio to use for that block.
    pub fn update(&mut self, fill_frames: f64) -> f64 {
        let f = match self.filtered {
            None => fill_frames,
            Some(prev) => prev * self.smoothing + fill_frames * (1.0 - self.smoothing),
        };
        self.filtered = Some(f);
        // error > 0: too much buffered → consume faster → produce fewer output
        // frames per input frame → ratio < 1.
        let err = f - self.target;
        let new_integral = self.integral + err * self.ki;
        // A sudden step in the fill level (a program restarted) must not
        // become an audible pitch change: the proportional part is capped.
        let p = (err * self.kp).clamp(-MAX_P, MAX_P);
        let raw = 1.0 - (p + new_integral);
        // anti-windup: only integrate while not saturated
        if raw > 1.0 - self.max_rel && raw < 1.0 + self.max_rel {
            self.integral = new_integral;
        }
        self.ratio = raw.clamp(1.0 - self.max_rel, 1.0 + self.max_rel);
        self.ratio
    }

    /// Forget the history (after an underrun or a device change).
    pub fn reset(&mut self) {
        self.filtered = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// Simulates a producer at `ppm` offset from a consumer for `seconds`,
    /// both with 10 ms blocks, and returns (min fill, max fill, fill at end).
    fn simulate(ppm: f64, seconds: f64, jitter: bool) -> (f64, f64, f64, f64) {
        let rate = 48000.0;
        let block = 480.0;
        let target = 960.0;
        let mut c = DriftController::new(target, rate / block);
        let mut fill = target;
        let mut produced_frac = 0.0;
        let mut lo = f64::MAX;
        let mut hi = f64::MIN;
        let blocks = (seconds * rate / block) as u64;
        let mut rng = 0x9E3779B97F4A7C15u64;
        let mut last_before = target;
        for i in 0..blocks {
            // producer: block * (1 + ppm) frames on average, delivered in whole frames
            produced_frac += block * (1.0 + ppm * 1e-6);
            let mut produced = produced_frac.floor();
            produced_frac -= produced;
            if jitter {
                // bursts: sometimes two blocks arrive at once, sometimes none
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                if rng.is_multiple_of(10) && i.is_multiple_of(2) {
                    produced_frac += produced;
                    produced = 0.0;
                }
            }
            fill += produced;
            // consumer: needs `block` output frames, consumes block / ratio input
            // the controller holds the level seen *before* a block is consumed
            let before = fill;
            let ratio = c.update(before);
            fill -= block / ratio;
            lo = lo.min(fill);
            hi = hi.max(before);
            last_before = before;
        }
        (lo, hi, last_before, c.drift_ppm())
    }

    #[test]
    fn holds_fill_level_for_two_hours_at_extreme_drift() {
        for ppm in [-500.0, -100.0, 0.0, 100.0, 500.0] {
            let (lo, hi, end, est) = simulate(ppm, 2.0 * 3600.0, false);
            assert!(lo > 0.0, "ran dry at {ppm} ppm (min fill {lo})");
            assert!(hi < 960.0 * 3.0, "overflowed at {ppm} ppm (max fill {hi})");
            assert!((end - 960.0).abs() < 50.0, "did not settle at {ppm} ppm: {end}");
            assert!((est - ppm).abs() < 5.0, "drift estimate {est} vs {ppm}");
        }
    }

    /// Packets of 1056 frames (22 ms) read in 480-frame blocks: the ratio
    /// must stay smooth (no audible FM) while still tracking the drift.
    #[test]
    fn bursty_device_does_not_modulate_ratio() {
        let mut c = DriftController::new(960.0, 100.0);
        let mut fill = 960.0f64;
        let mut acc = 0.0f64;
        let ppm = -557.0;
        let mut ratios = Vec::new();
        for i in 0..(100 * 600) {
            // producer: one 1056-frame packet every 22 ms of its own clock
            acc += 480.0 * (1.0 + ppm * 1e-6);
            while acc >= 1056.0 {
                fill += 1056.0;
                acc -= 1056.0;
            }
            let r = c.update(fill);
            fill -= 480.0 / r;
            assert!(fill > -480.0, "ran dry at block {i}");
            if i > 100 * 540 {
                ratios.push(r);
            }
        }
        let mean = ratios.iter().sum::<f64>() / ratios.len() as f64;
        let max_dev = ratios.iter().map(|r| (r - mean).abs()).fold(0.0, f64::max);
        // block-to-block change: this is what would be heard as FM
        let step = ratios.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f64::max);
        eprintln!("bursty: mean {:.1} ppm, max dev {:.2} ppm, max step {:.4} ppm", (1.0 / mean - 1.0) * 1e6, max_dev * 1e6, step * 1e6);
        // 200 ppm = 0.35 cent: far below audibility (the old tuning swung by
        // ±2000 ppm and smeared a 1 kHz test tone in CI).
        assert!(max_dev * 1e6 < 200.0, "ratio ripple {} ppm", max_dev * 1e6);
        assert!(step * 1e6 < 50.0, "ratio step {} ppm", step * 1e6);
        assert!(((1.0 / mean - 1.0) * 1e6 - ppm).abs() < 5.0, "mean drift {}", (1.0 / mean - 1.0) * 1e6);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]
        #[test]
        fn never_runs_dry_with_bursty_delivery(ppm in -400.0f64..400.0) {
            let (lo, hi, end, _) = simulate(ppm, 1800.0, true);
            prop_assert!(lo > -480.0, "min {}", lo); // never more than one block short
            prop_assert!(hi < 960.0 * 3.0, "max {}", hi);
            prop_assert!((end - 960.0).abs() < 200.0, "end {}", end);
        }
    }
}
