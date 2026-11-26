//! Clock-drift compensation.
//!
//! Every device has its own crystal. A microphone running at "48 000 Hz" and
//! a virtual cable running at "48 000 Hz" differ by up to a few hundred ppm,
//! so after an hour one of them is ~1 s ahead. Without correction the ring
//! buffer between them would eventually overflow (dropouts) or run dry
//! (gaps). The controller watches the fill level of the ring buffer and nudges
//! the resampling ratio so the fill level stays at its target.

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
        // Gains are expressed per update so the loop behaves the same at any
        // block size: the proportional term corrects a 1-frame error in about
        // 4 s (~250 ppm per 1000 frames of error at 48 kHz), the integral term
        // absorbs the constant drift.
        let kp = 2.0e-6;
        let ki = 2.0e-6 / update_hz.max(1.0);
        DriftController {
            target: target_frames,
            kp,
            ki,
            integral: 0.0,
            filtered: None,
            smoothing: (-1.0 / (0.5 * update_hz.max(1.0))).exp(),
            max_rel: 0.005, // ±5000 ppm: far beyond any real crystal, still inaudible
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
        let raw = 1.0 - (err * self.kp + new_integral);
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
