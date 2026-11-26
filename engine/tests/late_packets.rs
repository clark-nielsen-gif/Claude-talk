//! The echo reference for speakers (a loopback source on a fixed route,
//! rendered at each microphone frame's recording time, as in win/host.rs)
//! must keep the same delay when loopback packets come late, the program
//! pauses, or the clocks drift. If the delay moves, echo cancellation loses
//! its lock and the others in Discord hear themselves.
//!
//! The capture loop is a model of win/stream.rs `run_capture`: packets with
//! QPC start times, silence fill after 15 ms without packets, continuing from
//! where the last packet's sound ended, and the overlap of a late packet with
//! that silence skipped. Each reference sample carries its source index, so
//! the delay can be read directly from what the AEC thread receives.

use ct_engine::mixer::{connect_fixed, connect_raw, GainParams, OutputMixer, SourceFanout};

const FRAME: usize = 480;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % 1_000_000) as f64 / 1_000_000.0
    }
}

#[derive(Clone, Copy)]
struct Scen {
    ppm_src: f64,
    ppm_mic: f64,
    late_prob: f64, // probability a packet is late
    late_ms: (f64, f64),
    pause_every_s: f64, // source stops delivering (0 = never)
    pause_len_s: f64,
    mic_burst: usize,
    lat_us: f64, // delivery latency after packet end (loopback)
    seconds: f64,
}

/// Returns (delay samples (time_s, delay_ms) for valid frames, gap frames, skipped)
fn run(sc: Scen, seed: u64) -> (Vec<(f64, f64)>, u64, u64) {
    let mut rng = Rng(seed | 1);
    let (mut src_fan, mut src_h) = SourceFanout::new(3, 48000);
    let (mut mic_fan, mut mic_h) = SourceFanout::new(1, 48000);
    let (mix, mut mh) = OutputMixer::new(9002, 1, FRAME);
    let mut mix = mix.without_limiter();
    let stats = connect_fixed(&mut src_h, &mut mh, GainParams::new(0.0, false), 20.0);
    let mut raw = connect_raw(&mut mic_h, 9001);

    // ---- source packets: (arrival_us, qpc_us (start), first index)
    let end_us = sc.seconds * 1e6;
    let mut pkts: Vec<(f64, f64, u64)> = Vec::new();
    let mut k = 0u64;
    let mut last_arr = 0.0f64;
    loop {
        let ts = k as f64 * 10_000.0 * (1.0 + sc.ppm_src * 1e-6) + 3_000.0;
        if ts > end_us {
            break;
        }
        let in_pause = sc.pause_every_s > 0.0 && {
            let t = ts / 1e6;
            let ph = t % sc.pause_every_s;
            t > 5.0 && ph < sc.pause_len_s
        };
        if !in_pause {
            let mut arr =
                ts + 10_000.0 * (1.0 + sc.ppm_src * 1e-6) + sc.lat_us + rng.next() * 800.0;
            if rng.next() < sc.late_prob {
                arr += sc.late_ms.0 * 1000.0 + rng.next() * (sc.late_ms.1 - sc.late_ms.0) * 1000.0;
            }
            arr = arr.max(last_arr);
            last_arr = arr;
            pkts.push((arr, ts, k * FRAME as u64));
        }
        k += 1;
    }
    // ---- mic packets: (arrival_us, true end time us)
    let mut mics: Vec<(f64, f64)> = Vec::new();
    let mut m = 0u64;
    loop {
        let tend = (m + 1) as f64 * 10_000.0 * (1.0 + sc.ppm_mic * 1e-6) + 7_000.0;
        if tend > end_us {
            break;
        }
        let group_end = ((m / sc.mic_burst as u64) + 1) * sc.mic_burst as u64;
        let tgrp = group_end as f64 * 10_000.0 * (1.0 + sc.ppm_mic * 1e-6) + 7_000.0;
        mics.push((tgrp + 300.0 + rng.next() * 500.0, tend));
        m += 1;
    }

    // ---- capture loop state (as stream.rs)
    let mut pi = 0usize; // next packet to deliver
    let mut last_data = 0.0f64; // us
    let mut last_end = 0.0f64;
    let mut quiet: Option<(f64, u64)> = None;
    let mut silence_until: Option<f64> = None; // us (qpc)
    let mut cap_wake = 0.0f64;

    let mut mi = 0usize;
    let mut mic_queue: std::collections::VecDeque<f64> = Default::default(); // true end times of frames in raw
    let mut mic_last_push = 0u64;
    let mut aec_t = 0.0f64;

    let mut out = vec![0f32; FRAME];
    let mut delays = Vec::new();
    let mut gap_frames = 0u64;
    let mut pushed_first = false;

    // event-driven: capture thread wakes, mic pushes, aec polls (1 ms)
    loop {
        // next capture wake: min(next packet arrival (if after wait start), wait start + 5 ms)
        let next_pkt = pkts.get(pi).map(|p| p.0).unwrap_or(f64::INFINITY);
        let cap_next = next_pkt
            .max(cap_wake)
            .min(cap_wake + 5_000.0 + rng.next() * 600.0);
        let mic_next = mics.get(mi).map(|p| p.0).unwrap_or(f64::INFINITY);
        let aec_next = aec_t + 1_000.0 + rng.next() * 200.0;
        let t = cap_next.min(mic_next).min(aec_next);
        if t > end_us {
            break;
        }
        if t == cap_next {
            cap_wake = t;
            let now = t;
            let mut got = false;
            while pi < pkts.len() && pkts[pi].0 <= now {
                let (_, qpc, first) = pkts[pi];
                pi += 1;
                let frames = FRAME as u32;
                let skip = match silence_until {
                    Some(end) if end > qpc => (((end - qpc) * 48000.0 / 1e6) as u32).min(frames),
                    _ => 0,
                };
                if skip < frames {
                    silence_until = None;
                }
                let keep = (frames - skip) as usize;
                if keep > 0 {
                    let samples: Vec<f32> = (0..FRAME)
                        .flat_map(|i| {
                            let v = ((first + i as u64) % (1 << 20)) as f32 + 1.0;
                            [v, v]
                        })
                        .collect();
                    src_fan.push_at(
                        Some(&samples[skip as usize * 2..]),
                        2,
                        keep,
                        (now * 1000.0) as u64,
                    );
                    pushed_first = true;
                }
                got = true;
                last_end = qpc + FRAME as f64 / 48000.0 * 1e6;
            }
            if got {
                last_data = now;
                quiet = None;
            } else if pushed_first && now - last_data >= 15_000.0 {
                let (since, sent) = quiet.get_or_insert((last_end, 0));
                let due = ((now - *since) * 48000.0 / 1e6) as u64;
                let n = due.saturating_sub(*sent).min(4800) as usize;
                if n > 0 {
                    src_fan.push_at(None, 2, n, (now * 1000.0) as u64);
                    *sent += n as u64;
                    silence_until = Some(now);
                }
            }
        } else if t == mic_next {
            let (_, tend) = mics[mi];
            mi += 1;
            let v = vec![0f32; FRAME];
            mic_fan.push_at(Some(&v), 1, FRAME, (t * 1000.0) as u64);
            mic_last_push = (t * 1000.0) as u64;
            mic_queue.push_back(tend);
        } else {
            aec_t = t;
            while raw.slots() >= FRAME * 2 {
                let c = raw.read_chunk(FRAME * 2).unwrap();
                c.commit_all();
                let tend = mic_queue.pop_front().unwrap();
                let rest = (raw.slots() / 2) as u64;
                let recorded = mic_last_push.saturating_sub(rest * 1_000_000_000 / 48000);
                mix.render_at(&mut out, recorded);
                let v = out[FRAME - 1] as f64 - 1.0;
                let expect = (tend - 3000.0 - 15000.0) / 1e6 * 48000.0;
                let w = (1u64 << 20) as f64;
                let last = v + w * ((expect - v) / w).round();
                let ok = out[FRAME - 1] > 0.5
                    && out[0] > 0.5
                    && out[0] < out[FRAME - 1]
                    && (out[FRAME - 1] - out[FRAME - 2] - 1.0).abs() < 0.3
                    && (out[1] - out[0] - 1.0).abs() < 0.3;
                if ok {
                    // source time of that sample
                    let ts = last / 48000.0 * 1e6 * (1.0 + sc.ppm_src * 1e-6) + 3_000.0;
                    delays.push((tend / 1e6, (tend - ts) / 1000.0));
                } else {
                    gap_frames += 1;
                }
                mh.collect_garbage();
                src_h.collect_garbage();
            }
        }
    }
    let sk = stats
        .skipped_frames
        .load(std::sync::atomic::Ordering::Relaxed);
    let _ = stats
        .underrun_frames
        .load(std::sync::atomic::Ordering::Relaxed);
    (delays, gap_frames, sk)
}

struct Summary {
    off_share: f64,
    max_jump: f64,
    spread: f64,
}

fn summarize(name: &str, sc: Scen) -> Summary {
    let (d, gaps, skipped) = run(sc, 0xabcdef);
    let tail: Vec<f64> = d.iter().filter(|(t, _)| *t > 40.0).map(|x| x.1).collect();
    let n = tail.len() as f64;
    assert!(n > 0.0, "{name}: no reference frames");
    let mut sorted = tail.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let med = sorted[sorted.len() / 2];
    let max_jump = tail
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0, f64::max);
    let off = tail.iter().filter(|v| (**v - med).abs() > 2.0).count() as f64 / n;
    let spread = sorted[sorted.len() - 1] - sorted[0];
    println!(
        "{name:<32} delay {med:6.2} ms, spread {spread:5.2} ms, largest jump {max_jump:5.2} ms, \
         more than 2 ms off {:4.1} %, gap frames {gaps}, skipped {skipped}",
        off * 100.0
    );
    Summary {
        off_share: off,
        max_jump,
        spread,
    }
}

#[test]
fn reference_delay_holds_through_late_packets_pauses_and_drift() {
    let base = Scen {
        ppm_src: 0.0,
        ppm_mic: 0.0,
        late_prob: 0.0,
        late_ms: (0.0, 0.0),
        pause_every_s: 0.0,
        pause_len_s: 0.0,
        mic_burst: 1,
        lat_us: 200.0,
        seconds: 180.0,
    };
    let late = Scen {
        late_prob: 0.01,
        late_ms: (6.0, 20.0),
        ..base
    };
    let cases = [
        ("steady", base),
        (
            "source clock -150 ppm",
            Scen {
                ppm_src: -150.0,
                ..base
            },
        ),
        ("1 % packets 6-20 ms late", late),
        (
            "same, source -150 ppm",
            Scen {
                ppm_src: -150.0,
                ..late
            },
        ),
        (
            "same, source +100 ppm",
            Scen {
                ppm_src: 100.0,
                ..late
            },
        ),
        (
            "5 % packets 3-12 ms late",
            Scen {
                late_prob: 0.05,
                late_ms: (3.0, 12.0),
                ..base
            },
        ),
        (
            "pauses of 1.5 s every 6 s",
            Scen {
                pause_every_s: 6.0,
                pause_len_s: 1.5,
                ..base
            },
        ),
        (
            "3 ms delivery, 1 % late",
            Scen {
                lat_us: 3000.0,
                ..late
            },
        ),
        (
            "mic in pairs, 1 % late",
            Scen {
                mic_burst: 2,
                ..late
            },
        ),
    ];
    let mut bad = Vec::new();
    for (name, sc) in cases {
        let s = summarize(name, sc);
        if s.off_share > 0.01 || s.max_jump > 2.0 || s.spread > 4.0 {
            bad.push(name);
        }
    }
    assert!(bad.is_empty(), "the reference delay moved: {bad:?}");
}
