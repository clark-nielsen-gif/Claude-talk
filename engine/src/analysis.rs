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
        let win = 0.35875 - 0.48829 * t.cos() + 0.14128 * (2.0 * t).cos() - 0.01168 * (3.0 * t).cos();
        wsum += win;
        let s0 = v as f64 * win + c * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    let mag = (s1 * s1 + s2 * s2 - c * s1 * s2).max(0.0).sqrt() * 2.0 / wsum;
    20.0 * mag.max(1e-10).log10()
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
        Sine { phase: 0.0, step: freq / rate, amplitude }
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn onsets_find_impulses() {
        let mut x = vec![0f32; 10000];
        for &i in &[100usize, 4000, 9000] {
            x[i] = 0.9;
            x[i + 1] = -0.5;
        }
        assert_eq!(onsets(&x, 0.3, 1000), vec![100, 4000, 9000]);
    }
}
