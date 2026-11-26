//! Ordinary routes (Claude → Discord, Discord → Claude) from a program that
//! pauses: the source is a model of stream.rs `run_capture` (as in
//! late_packets.rs), the output pulls 10 ms blocks on its own clock. A pause
//! is not jitter: the route must not grow its buffer (and its delay) for
//! every pause, and the sound must not drop out.

use ct_engine::mixer::{connect, GainParams, OutputMixer, SourceFanout};
use std::sync::atomic::Ordering;

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

#[derive(Clone, Copy, Debug)]
pub struct Scen {
    pub ppm_src: f64,
    pub ppm_out: f64,
    pub late_prob: f64,
    pub late_ms: (f64, f64),
    pub pause_every_s: f64,
    pub pause_len_s: f64,
    pub lat_us: f64,
    pub seconds: f64,
    pub route_ms: f64,
    pub out_jit_us: f64,
}

pub struct Res {
    /// (render time s, delay ms) of valid blocks
    pub delays: Vec<(f64, f64)>,
    /// blocks with sound expected but invalid output (while source playing)
    pub sound_dropouts: u64,
    pub underrun_frames: u64,
    pub target_ms: f32,
    pub max_target: f32,
    pub ratios: Vec<(f64, f32)>,
}

pub fn run(sc: Scen, seed: u64) -> Res {
    let mut rng = Rng(seed | 1);
    let (mut src_fan, mut src_h) = SourceFanout::new(3, 48000);
    let (mix, mut mh) = OutputMixer::new(1, 1, FRAME);
    let mut mix = mix.without_limiter(); // index-coded samples (limiter only scales)
    let stats = connect(
        &mut src_h,
        &mut mh,
        GainParams::new(0.0, false),
        sc.route_ms,
    );

    let end_us = sc.seconds * 1e6;
    let mut pkts: Vec<(f64, f64, u64)> = Vec::new();
    let mut k = 0u64;
    let mut last_arr = 0.0f64;
    let in_pause = |ts: f64| {
        sc.pause_every_s > 0.0 && {
            let t = ts / 1e6;
            let ph = t % sc.pause_every_s;
            t > 5.0 && ph < sc.pause_len_s
        }
    };
    loop {
        let ts = k as f64 * 10_000.0 * (1.0 + sc.ppm_src * 1e-6) + 3_000.0;
        if ts > end_us {
            break;
        }
        if !in_pause(ts) {
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

    let mut pi = 0usize;
    let mut last_data = 0.0f64;
    let mut pos = 0.0f64;
    let mut timed = false;
    let mut after_silence = false;
    let mut quiet: Option<(f64, u64)> = None;
    let mut cap_wake = 0.0f64;
    let mut pushed_first = false;

    let mut out = vec![0f32; FRAME];
    let mut kout = 0u64;
    let mut last_tg = 0.0f32;
    let mut next_out = 7_300.0;
    let mut res = Res {
        delays: vec![],
        sound_dropouts: 0,
        underrun_frames: 0,
        target_ms: 0.0,
        max_target: 0.0,
        ratios: vec![],
    };

    loop {
        let next_pkt = pkts.get(pi).map(|p| p.0).unwrap_or(f64::INFINITY);
        let cap_next = next_pkt
            .max(cap_wake)
            .min(cap_wake + 5_000.0 + rng.next() * 600.0);
        let t = cap_next.min(next_out);
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
                let dur = FRAME as f64 / 48000.0 * 1e6;
                let contiguous = timed && !after_silence && (qpc - pos).abs() < 3000.0;
                let skip = if timed && !contiguous && pos > qpc + 500.0 {
                    (((pos - qpc) * 48000.0 / 1e6) as u32).min(frames)
                } else {
                    0
                };
                let gap = if timed && !contiguous && qpc > pos + 1000.0 {
                    (((qpc - pos) * 48000.0 / 1e6) as usize).min(4800)
                } else {
                    0
                };
                if gap > 0 {
                    src_fan.push_at(None, 2, gap, (now * 1000.0) as u64);
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
                let end = qpc + dur;
                if contiguous {
                    let pred = pos + dur;
                    pos = pred + 0.05 * (end - pred);
                    after_silence = false;
                } else {
                    after_silence = timed && pos > end;
                    pos = if timed { pos.max(end) } else { end };
                }
                timed = true;
                src_fan.set_quiet_fill(false);
                src_fan.set_reach_ns((pos * 1000.0) as u64);
            }
            if got {
                last_data = now;
                quiet = None;
            } else if pushed_first && now - last_data >= 15_000.0 {
                let (since, sent) = quiet.get_or_insert((pos, 0));
                let due = (((now - 10_000.0) - *since).max(0.0) * 48000.0 / 1e6) as u64;
                let n = due.saturating_sub(*sent).min(4800) as usize;
                if n > 0 {
                    src_fan.set_quiet_fill(true);
                    src_fan.push_at(None, 2, n, (now * 1000.0) as u64);
                    *sent += n as u64;
                    pos = *since + *sent as f64 / 48000.0 * 1e6;
                    timed = true;
                    after_silence = true;
                    src_fan.set_reach_ns((pos * 1000.0) as u64);
                }
            }
        } else {
            // render tick
            mix.render_at(&mut out, (t * 1000.0) as u64);
            let w20 = (1u64 << 20) as f32;
            let wd = |a: f32, b: f32| {
                let mut d = b - a;
                if d < -w20 / 2.0 {
                    d += w20;
                }
                d
            };
            let ok = out.iter().all(|v| *v > 0.5)
                && out.windows(2).all(|p| {
                    (wd(p[0], p[1]) - 1.0).abs() < 0.3
                        || (wd(p[0], p[1]) - 1.0).abs() < 2.0 && (p[0] > w20 - 4.0 || p[1] < 4.0)
                });
            // expected: source time ~ t - route_ms - 10ms is in sound?
            let probe = |dt: f64| {
                let ts = t - dt * 1000.0;
                ts > 200_000.0
                    && !in_pause(ts)
                    && !in_pause(ts - 15_000.0)
                    && !in_pause(ts + 15_000.0)
            };
            if ok {
                // index of last sample → source time
                let v = out[FRAME - 1] as f64 - 1.0;
                let expect = (t - 3000.0 - 25000.0) / 1e6 * 48000.0;
                let w = (1u64 << 20) as f64;
                let last = v + w * ((expect - v) / w).round();
                let ts = last / 48000.0 * 1e6 * (1.0 + sc.ppm_src * 1e-6) + 3_000.0;
                res.delays.push((t / 1e6, (t - ts) / 1000.0));
            } else if probe(25.0)
                && probe(40.0)
                && t > 2e6
                && !(out.iter().cloned().fold(0.0f32, f32::max) > w20 - 600.0
                    && out.iter().cloned().fold(f32::MAX, f32::min) > 0.5)
            {
                res.sound_dropouts += 1;
            }
            let tg = stats.target_ms.load();
            if tg != last_tg {
                last_tg = tg;
            }
            res.max_target = res.max_target.max(tg);
            if kout.is_multiple_of(10) {
                res.ratios.push((t / 1e6, stats.drift_ppm.load()));
            }
            kout += 1;
            next_out = 7_300.0
                + kout as f64 * 10_000.0 / (1.0 + sc.ppm_out * 1e-6)
                + (rng.next() * 2.0 - 1.0) * sc.out_jit_us;
            if kout.is_multiple_of(50) {
                mh.collect_garbage();
                src_h.collect_garbage();
            }
        }
    }
    res.underrun_frames = stats.underrun_frames.load(Ordering::Relaxed);
    res.target_ms = stats.target_ms.load();
    res
}

fn pct(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() - 1) as f64 * p) as usize]
}

#[test]
fn ordinary_routes_do_not_grow_for_pauses() {
    let base = Scen {
        ppm_src: 0.0,
        ppm_out: 0.0,
        late_prob: 0.0,
        late_ms: (0.0, 0.0),
        pause_every_s: 0.0,
        pause_len_s: 0.0,
        lat_us: 200.0,
        seconds: 150.0,
        route_ms: ct_engine::ROUTE_START_MS,
        out_jit_us: 1000.0,
    };
    let cases = [
        ("steady", base),
        (
            "steady +100ppm out",
            Scen {
                ppm_out: 100.0,
                ..base
            },
        ),
        (
            "pause 1.5/6",
            Scen {
                pause_every_s: 6.0,
                pause_len_s: 1.5,
                ..base
            },
        ),
        (
            "pause 1.5/6 +100ppm",
            Scen {
                pause_every_s: 6.0,
                pause_len_s: 1.5,
                ppm_out: 100.0,
                ..base
            },
        ),
        (
            "pause 8/12",
            Scen {
                pause_every_s: 12.0,
                pause_len_s: 8.0,
                ..base
            },
        ),
        (
            "pause 3/10",
            Scen {
                pause_every_s: 10.0,
                pause_len_s: 3.0,
                ..base
            },
        ),
        (
            "pause 0.3/2",
            Scen {
                pause_every_s: 2.0,
                pause_len_s: 0.3,
                ..base
            },
        ),
        (
            "1% late 6-20",
            Scen {
                late_prob: 0.01,
                late_ms: (6.0, 20.0),
                ..base
            },
        ),
    ];
    let mut bad = Vec::new();
    let mut baseline = 0u64;
    for (name, sc) in cases {
        for seed in [1u64, 3, 5] {
            let r = run(sc, seed);
            let mut all: Vec<f64> = r
                .delays
                .iter()
                .filter(|d| d.0 > 30.0)
                .map(|d| d.1)
                .collect();
            // delay in the first 0.5 s after each resume vs. later
            let after_resume: Vec<f64> = r
                .delays
                .iter()
                .filter(|d| {
                    d.0 > 30.0 && sc.pause_every_s > 0.0 && {
                        let ph = d.0 % sc.pause_every_s;
                        ph > sc.pause_len_s && ph < sc.pause_len_s + 0.5
                    }
                })
                .map(|d| d.1)
                .collect();
            let mut ar = after_resume.clone();
            let ppm_min = r
                .ratios
                .iter()
                .filter(|x| x.0 > 30.0)
                .map(|x| x.1)
                .fold(f32::MAX, f32::min);
            let ppm_max = r
                .ratios
                .iter()
                .filter(|x| x.0 > 30.0)
                .map(|x| x.1)
                .fold(f32::MIN, f32::max);
            println!(
                "{name:<22} seed {seed}: median {:.1} ms p95 {:.1} max {:.1}; first 0.5 s after resume median {:.1}; sound dropouts {} underrun frames {} target {:.1} (max {:.1}) ppm {:.0}..{:.0}",
                pct(&mut all.clone(), 0.5),
                pct(&mut all.clone(), 0.95),
                pct(&mut all, 1.0),
                if ar.is_empty() { f64::NAN } else { pct(&mut ar, 0.5) },
                r.sound_dropouts,
                r.underrun_frames,
                r.target_ms,
                r.max_target,
                ppm_min,
                ppm_max
            );
            // Late packets (jitter) may grow the route. Pauses may not (one
            // step at most when the two clocks are far apart, where the drift
            // control overshoots after a resume). The few dropouts the
            // steady case shows are the model's start-up; pauses may add 2.
            let allowed = if sc.ppm_out != 0.0 { 5.1 } else { 0.1 };
            let grew = r.max_target > ct_engine::ROUTE_START_MS as f32 + allowed;
            if name == "steady" {
                baseline = baseline.max(r.sound_dropouts);
            }
            if sc.late_prob == 0.0 && (grew || r.sound_dropouts > baseline + 2) {
                bad.push(format!("{name} seed {seed}"));
            }
        }
    }
    assert!(bad.is_empty(), "route grew or dropped out: {bad:?}");
}
