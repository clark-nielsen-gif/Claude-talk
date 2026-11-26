//! Echo reduction for the microphone (WebRTC AEC3, Rust port `aec3`).
//!
//! Used only when sound can reach the microphone through the air (speakers).
//! The reference is what Claude and Discord play; AEC3 finds the acoustic
//! delay itself. Works on 10 ms mono frames at 48 kHz. High-pass filter and
//! echo canceller only – no noise suppression or automatic gain, so the
//! user's voice is not altered beyond removing the echo.

use aec3::nodes::audio::AudioFormat;
use aec3::pipelines::linear::{self, LinearPipeline};

pub const FRAME: usize = 480;

pub struct EchoReducer {
    pipeline: LinearPipeline,
    out: Vec<f32>,
}

impl EchoReducer {
    pub fn new() -> Result<Self, String> {
        let format = AudioFormat::ten_ms(48_000, 1);
        let pipeline = linear::builder(format, format)
            .enable_noise_suppression(false)
            .enable_gain_controller2(false)
            .enable_high_pass_filter(true)
            .build()
            .map_err(|e| format!("{e:?}"))?;
        Ok(EchoReducer {
            pipeline,
            out: vec![0.0; FRAME],
        })
    }

    /// `reference`: what is being played (10 ms); `mic`: what was recorded
    /// (10 ms). Returns the microphone with the echo removed.
    pub fn process(&mut self, reference: &[f32], mic: &[f32]) -> &[f32] {
        debug_assert_eq!(reference.len(), FRAME);
        debug_assert_eq!(mic.len(), FRAME);
        if self.pipeline.handle_render_frame(reference).is_err() {
            self.out.copy_from_slice(mic);
            return &self.out;
        }
        match self.pipeline.process_capture_frame(mic, &mut self.out) {
            Ok(true) => {}
            // no output yet (start-up) or an internal error: pass the mic through
            _ => self.out.copy_from_slice(mic),
        }
        &self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{tone_db, Sine};

    /// Speech-like reference (two tones with an on/off pattern) leaks into the
    /// mic 40 ms later at -6 dB, plus a near-end tone the user "speaks".
    #[test]
    fn removes_a_delayed_echo_and_keeps_the_near_end() {
        let mut aec = EchoReducer::new().expect("aec");
        let mut far1 = Sine::new(440.0, 48000.0, 0.3);
        let mut far2 = Sine::new(1180.0, 48000.0, 0.2);
        let mut near = Sine::new(2500.0, 48000.0, 0.2);
        let delay = 48 * 40;
        let mut history = vec![0f32; delay];
        let mut out_tail = Vec::new();
        let mut mic_tail = Vec::new();
        let frames = 100 * 12; // 12 s to converge
        let mut noise = 0x1234_5678u32;
        for i in 0..frames {
            let mut r = vec![0f32; FRAME];
            let mut a = vec![0f32; FRAME];
            let mut b = vec![0f32; FRAME];
            far1.fill(&mut a, 1);
            far2.fill(&mut b, 1);
            let talking = (i / 70) % 2 == 0; // far end talks in bursts
            for k in 0..FRAME {
                noise ^= noise << 13;
                noise ^= noise >> 17;
                noise ^= noise << 5;
                let n = (noise as f32 / u32::MAX as f32 - 0.5) * 0.05;
                r[k] = if talking { a[k] + b[k] + n } else { n };
            }
            history.extend_from_slice(&r);
            let echo: Vec<f32> = history.drain(..FRAME).collect();
            let mut nr = vec![0f32; FRAME];
            near.fill(&mut nr, 1);
            let mic: Vec<f32> = echo.iter().zip(&nr).map(|(e, n)| 0.5 * e + n).collect();
            let out = aec.process(&r, &mic).to_vec();
            if i >= frames - 140 && talking {
                out_tail.extend_from_slice(&out);
                mic_tail.extend_from_slice(&mic);
            }
        }
        let before = tone_db(&mic_tail, 440.0, 48000.0);
        let after = tone_db(&out_tail, 440.0, 48000.0);
        let near_before = tone_db(&mic_tail, 2500.0, 48000.0);
        let near_after = tone_db(&out_tail, 2500.0, 48000.0);
        eprintln!("echo 440 Hz: {before:.1} → {after:.1} dB; near end 2500 Hz: {near_before:.1} → {near_after:.1} dB");
        assert!(
            after < before - 15.0,
            "echo only reduced from {before:.1} to {after:.1} dB"
        );
        assert!(
            near_after > near_before - 6.0,
            "near-end voice damaged: {near_before:.1} → {near_after:.1} dB"
        );
    }
}

#[cfg(test)]
mod passthrough_tests {
    use super::*;
    use crate::analysis::{tone_db, Sine};

    /// Steady far-end tones that never reach the microphone must not make
    /// the reducer swallow the user's (steady) voice.
    #[test]
    fn keeps_the_user_when_there_is_no_echo() {
        let mut aec = EchoReducer::new().expect("aec");
        let mut f1 = Sine::new(440.0, 48000.0, 0.3);
        let mut f2 = Sine::new(1000.0, 48000.0, 0.3);
        let mut near = Sine::new(2500.0, 48000.0, 0.3);
        let mut mic_tail = Vec::new();
        let mut out_tail = Vec::new();
        for i in 0..800 {
            let mut a = vec![0f32; FRAME];
            let mut b = vec![0f32; FRAME];
            f1.fill(&mut a, 1);
            f2.fill(&mut b, 1);
            let r: Vec<f32> = a.iter().zip(&b).map(|(x, y)| x + y).collect();
            let mut m = vec![0f32; FRAME];
            near.fill(&mut m, 1);
            let out = aec.process(&r, &m).to_vec();
            if i >= 600 {
                mic_tail.extend_from_slice(&m);
                out_tail.extend_from_slice(&out);
            }
        }
        let b = tone_db(&mic_tail, 2500.0, 48000.0);
        let a = tone_db(&out_tail, 2500.0, 48000.0);
        eprintln!("user 2500 Hz: {b:.1} → {a:.1} dB");
        assert!(a > b - 6.0, "user's voice removed: {b:.1} → {a:.1} dB");
    }
}

#[cfg(test)]
mod delay_tests {
    use super::*;
    use crate::analysis::{burst_onsets, match_delays, median, Sine};

    /// The user's voice must not be delayed by the echo reducer.
    #[test]
    fn adds_no_delay_to_the_user() {
        let mut aec = EchoReducer::new().expect("aec");
        let mut tone = Sine::new(2500.0, 48000.0, 0.4);
        let mut far = Sine::new(440.0, 48000.0, 0.2);
        let (mut inp, mut out) = (Vec::new(), Vec::new());
        let mut pos = 0usize;
        for _ in 0..500 {
            let mut m = vec![0f32; FRAME];
            tone.fill(&mut m, 1);
            for (k, v) in m.iter_mut().enumerate() {
                if (pos + k) % 28800 >= 1200 {
                    *v = 0.0; // 25 ms bursts every 600 ms
                }
            }
            pos += FRAME;
            let mut r = vec![0f32; FRAME];
            far.fill(&mut r, 1);
            out.extend_from_slice(aec.process(&r, &m));
            inp.extend_from_slice(&m);
        }
        let ms = |x: &[f32]| {
            burst_onsets(x, 2500.0, 48000.0, 0.3, 2400)
                .into_iter()
                .map(|i| i as f64 / 48.0)
                .collect::<Vec<_>>()
        };
        let d = median(&match_delays(&ms(&inp), &ms(&out), 300.0)).expect("bursts found");
        eprintln!("echo reducer delay: {d:.1} ms");
        assert!(d < 12.0, "echo reducer delays the voice by {d:.1} ms");
    }
}
