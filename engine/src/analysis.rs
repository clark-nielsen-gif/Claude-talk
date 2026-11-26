//! Signal analysis used by the self-test and the CI test tools: tone levels
//! (windowed Goertzel), test-signal generators and impulse detection.

/// Level of `freq` in a mono signal, in dBFS of a sine's peak amplitude
/// (a full-scale sine reads 0 dB). Blackman-Harris window, so tones 60+ dB
/// apart can be told apart even a few hundred Hz away.
pub fn tone_db(x: &[f32], freq: f64, rate: f64) -> f64 {
    if x.len() < 16 {
        return -200.0;
    }
    let n = x.len() as f64;
    let w = 2.0 * std::f64::consts::PI * freq / rate;
    let c = 2.0 * w.cos();
    let (mut s1, mut s2, mut wsum) = (0.0f64, 0.0f64, 0.0f64);
    for (i, &v) in x.iter().enumerate() {
        let t = 2.0 * std::f64::consts::PI * i as f64 / (n - 1.0);
        let win =
            0.35875 - 0.48829 * t.cos() + 0.14128 * (2.0 * t).cos() - 0.01168 * (3.0 * t).cos();
        wsum += win;
        let s0 = v as f64 * win + c * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    let mag = (s1 * s1 + s2 * s2 - c * s1 * s2).max(0.0).sqrt() * 2.0 / wsum;
    20.0 * mag.max(1e-10).log10()
}

/// Highest level of a tone within ±`tolerance_hz` of `freq` (devices run a
/// few hundred ppm off nominal, which moves a test tone by a fraction of a
/// hertz). Searched in 0.1 Hz steps.
pub fn tone_db_near(x: &[f32], freq: f64, tolerance_hz: f64, rate: f64) -> f64 {
    let steps = (tolerance_hz / 0.1).ceil() as i64;
    (-steps..=steps)
        .map(|k| tone_db(x, freq + k as f64 * 0.1, rate))
        .fold(-400.0, f64::max)
}

/// Like `tone_db_near`, but also returns the frequency where the tone was
/// found (to see how far a clock moved it).
pub fn tone_peak_near(x: &[f32], freq: f64, tolerance_hz: f64, rate: f64) -> (f64, f64) {
    let steps = (tolerance_hz / 0.1).ceil() as i64;
    (-steps..=steps)
        .map(|k| {
            let f = freq + k as f64 * 0.1;
            (tone_db(x, f, rate), f)
        })
        .fold(
            (-400.0, freq),
            |best, cur| if cur.0 > best.0 { cur } else { best },
        )
}

/// How far to search around a test tone: ±0.5 % (at least 3 Hz). Drift
/// correction may move a tone by up to ±0.25 % on purpose, and a source
/// generated on a different clock than the device it plays on (test tone
/// programs, self-test tones) is off by that clock difference as well.
pub fn tone_tolerance(freq: f64) -> f64 {
    (freq * 0.005).max(3.0)
}

/// First channel of an interleaved buffer.
pub fn mono(x: &[f32], channels: usize) -> Vec<f32> {
    x.chunks_exact(channels.max(1)).map(|f| f[0]).collect()
}

/// Continuous sine generator.
#[derive(Clone, Debug)]
pub struct Sine {
    phase: f64,
    step: f64,
    pub amplitude: f32,
}

impl Sine {
    pub fn new(freq: f64, rate: f64, amplitude: f32) -> Self {
        Sine {
            phase: 0.0,
            step: freq / rate,
            amplitude,
        }
    }
    /// Fills an interleaved buffer (all channels get the same sample).
    pub fn fill(&mut self, out: &mut [f32], channels: usize) {
        for frame in out.chunks_exact_mut(channels.max(1)) {
            let v = (2.0 * std::f64::consts::PI * self.phase).sin() as f32 * self.amplitude;
            self.phase = (self.phase + self.step).fract();
            frame.fill(v);
        }
    }
}

/// Indices where |x| first rises above `threshold` after at least
/// `min_gap` samples below it (impulse onsets).
pub fn onsets(x: &[f32], threshold: f32, min_gap: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut quiet = min_gap;
    for (i, &v) in x.iter().enumerate() {
        if v.abs() >= threshold {
            if quiet >= min_gap {
                out.push(i);
            }
            quiet = 0;
        } else {
            quiet += 1;
        }
    }
    out
}

/// Band-pass filter (RBJ cookbook, 0 dB peak; the same design as Web
/// Audio's `BiquadFilterNode` "bandpass"), so both ends of a latency
/// measurement can isolate one tone with an identical delay.
pub fn bandpass(x: &[f32], freq: f64, q: f64, rate: f64) -> Vec<f32> {
    let w0 = 2.0 * std::f64::consts::PI * freq / rate;
    let alpha = w0.sin() / (2.0 * q);
    let a0 = 1.0 + alpha;
    let (b0, b2) = (alpha / a0, -alpha / a0);
    let (a1, a2) = (-2.0 * w0.cos() / a0, (1.0 - alpha) / a0);
    let (mut x1, mut x2, mut y1, mut y2) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    x.iter()
        .map(|&v| {
            let v = v as f64;
            let y = b0 * v + b2 * x2 - a1 * y1 - a2 * y2;
            x2 = x1;
            x1 = v;
            y2 = y1;
            y1 = y;
            y as f32
        })
        .collect()
}

/// Onsets of tone bursts at `freq`: band-pass, then the first sample above
/// `rel` × the loudest peak after at least `min_gap` quiet samples.
pub fn burst_onsets(x: &[f32], freq: f64, rate: f64, rel: f32, min_gap: usize) -> Vec<usize> {
    let y = bandpass(x, freq, 10.0, rate);
    let peak = y.iter().fold(0f32, |m, v| m.max(v.abs()));
    if peak < 1e-4 {
        return Vec::new();
    }
    onsets(&y, peak * rel, min_gap)
}

/// Pairs each later event with the latest earlier event no more than
/// `max_ms` before it and returns the delays (ms). Times in ms.
pub fn match_delays(earlier: &[f64], later: &[f64], max_ms: f64) -> Vec<f64> {
    later
        .iter()
        .filter_map(|&t| {
            earlier
                .iter()
                .filter(|&&e| e <= t && t - e <= max_ms)
                .fold(None, |best: Option<f64>, &e| {
                    Some(best.map_or(e, |b| b.max(e)))
                })
                .map(|e| t - e)
        })
        .collect()
}

/// Median of a non-empty list.
pub fn median(v: &[f64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    Some(s[s.len() / 2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_found_when_a_clock_moved_it() {
        let rate = 48000.0;
        let mut x = vec![0f32; 48000 * 2];
        Sine::new(2500.0 * 1.0022, rate, 0.3).fill(&mut x, 1); // 2200 ppm off
        assert!(
            tone_db_near(&x, 2500.0, 3.0, rate) < -60.0,
            "a tight search misses it"
        );
        let (db, f) = tone_peak_near(&x, 2500.0, tone_tolerance(2500.0), rate);
        assert!((db + 10.5).abs() < 0.3, "level {db:.1}");
        assert!((f - 2505.5).abs() < 0.2, "found at {f:.1} Hz");
    }

    /// The same bursts, delayed by 23 ms and mixed with a loud steady tone
    /// at another frequency, are found 23 ms later.
    #[test]
    fn burst_delay_is_measured_through_other_sound() {
        let rate = 48000.0;
        let n = 48000 * 3;
        let mut clean = vec![0f32; n];
        let mut burst = Sine::new(2500.0, rate, 0.5);
        let mut b = vec![0f32; 960];
        for start in (4800..n - 2000).step_by(28800) {
            burst.fill(&mut b, 1);
            clean[start..start + 960].copy_from_slice(&b);
        }
        let delay = 48 * 23;
        let mut other = Sine::new(440.0, rate, 0.3);
        let mut steady = vec![0f32; n];
        other.fill(&mut steady, 1);
        let mut later = steady.clone();
        for i in delay..n {
            later[i] += 0.4 * clean[i - delay];
        }
        let to_ms = |v: Vec<usize>| v.into_iter().map(|i| i as f64 / 48.0).collect::<Vec<_>>();
        let a = to_ms(burst_onsets(&clean, 2500.0, rate, 0.3, 2400));
        let b = to_ms(burst_onsets(&later, 2500.0, rate, 0.3, 2400));
        assert_eq!(a.len(), 5);
        assert_eq!(b.len(), 5);
        let d = median(&match_delays(&a, &b, 300.0)).unwrap();
        assert!((d - 23.0).abs() < 1.0, "measured {d:.2} ms");
    }

    #[test]
    fn tone_level_reads_amplitude_and_separates_tones() {
        let mut a = Sine::new(1000.0, 48000.0, 0.5);
        let mut b = Sine::new(1400.0, 48000.0, 0.0005);
        let mut x = vec![0f32; 48000];
        let mut y = vec![0f32; 48000];
        a.fill(&mut x, 1);
        b.fill(&mut y, 1);
        for (p, q) in x.iter_mut().zip(&y) {
            *p += q;
        }
        assert!((tone_db(&x, 1000.0, 48000.0) - (-6.02)).abs() < 0.1);
        assert!((tone_db(&x, 1400.0, 48000.0) - (-66.02)).abs() < 1.0);
        assert!(tone_db(&x, 2500.0, 48000.0) < -120.0);
    }

    #[test]
    fn near_search_finds_slightly_detuned_tone() {
        let mut a = Sine::new(999.44, 48000.0, 0.5);
        let mut x = vec![0f32; 96000];
        a.fill(&mut x, 1);
        assert!((tone_db_near(&x, 1000.0, 2.0, 48000.0) + 6.02).abs() < 0.3);
    }

    #[test]
    fn onsets_find_impulses() {
        let mut x = vec![0f32; 10000];
        for &i in &[100usize, 4000, 9000] {
            x[i] = 0.9;
            x[i + 1] = -0.5;
        }
        assert_eq!(onsets(&x, 0.3, 1000), vec![100, 4000, 9000]);
    }
}
