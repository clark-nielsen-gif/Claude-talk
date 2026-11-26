//! `ct-audiotest` – test tool for real Windows audio devices.
//!
//! ```text
//! ct-audiotest devices
//! ct-audiotest tone --freq 1000 [--device "Speakers"] [--seconds 30] [--amp 0.3]
//! ct-audiotest smoke --cable-in "Claude Talk" --cable-out "Claude Talk" --sink "Virtual Audio" --out report.json
//! ```
//! `tone` is used as the fake "Claude", "Discord", microphone and background
//! programs in CI. `smoke` checks render, capture and process loopback
//! through the engine and writes a JSON report (exit code 1 on failure).

#[cfg(not(windows))]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("serve-testpage") {
        std::process::exit(serve::run(&args[2..]));
    }
    eprintln!("ct-audiotest audio commands only run on Windows");
    std::process::exit(2);
}

#[cfg(windows)]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let code = match args.get(1).map(String::as_str) {
        Some("serve-testpage") => serve::run(&args[2..]),
        Some("devices") => imp::devices(),
        Some("tone") => imp::tone(&args[2..]),
        Some("smoke") => imp::smoke(&args[2..]),
        _ => {
            eprintln!("usage: ct-audiotest devices | tone --freq F [--device NAME] [--seconds S] [--amp A] | smoke ...");
            2
        }
    };
    std::process::exit(code);
}

/// Serves the CI voice page with a claude.ai-like strict CSP and stores the
/// page's report. `serve-testpage --port 8765 --out report.json`
mod serve {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    const PAGE: &str = include_str!("../testpage/test.html");

    pub fn run(args: &[String]) -> i32 {
        let arg = |n: &str| args.iter().position(|a| a == n).and_then(|i| args.get(i + 1)).cloned();
        let port = arg("--port").unwrap_or_else(|| "8765".into());
        let out = arg("--out").unwrap_or_else(|| "page-report.json".into());
        let listener = match TcpListener::bind(format!("127.0.0.1:{port}")) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("bind: {e}");
                return 1;
            }
        };
        println!("serving on http://127.0.0.1:{port}/test.html");
        let mut counter = 0u64;
        for conn in listener.incoming().flatten() {
            counter += 1;
            let mut reader = BufReader::new(&conn);
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            let mut len = 0usize;
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() {
                    break;
                }
                if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
            }
            let mut w = &conn;
            if line.starts_with("POST /report") {
                let mut body = vec![0u8; len];
                let _ = reader.read_exact(&mut body);
                let _ = std::fs::write(&out, &body);
                println!("report received ({} bytes)", body.len());
                let _ = w.write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n");
            } else if line.starts_with("GET /test.html") {
                let nonce = format!("ci{counter:08x}{:x}", std::process::id());
                let html = PAGE.replace("__NONCE__", &nonce);
                let csp = format!(
                    "default-src 'none'; script-src 'nonce-{nonce}' 'unsafe-eval'; script-src-attr 'none'; style-src 'unsafe-inline'; \
                     img-src 'self'; connect-src 'self'; media-src blob:; worker-src blob:; frame-src 'self'; base-uri 'self'"
                );
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Security-Policy: {csp}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    html.len()
                );
                let _ = w.write_all(head.as_bytes());
                let _ = w.write_all(html.as_bytes());
            } else {
                let _ = w.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            }
        }
        0
    }
}

#[cfg(windows)]
mod imp {
    use ct_engine::analysis::{mono, tone_db_near, Sine};
    use ct_engine::graph::{OutputId, SourceId};
    use ct_engine::mixer::{connect, GainParams, OutputMixer, SourceFanout};
    use ct_engine::win::devices::{self, DeviceSelector, Flow};
    use ct_engine::win::stream::{spawn_capture, spawn_render, CaptureTarget, ProcessTarget, StreamHandle, StreamState};
    use ct_engine::win::ComGuard;
    use ct_engine::{ENGINE_CHANNELS, ENGINE_RATE};
    use serde_json::{json, Value};
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    fn arg<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
        args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
    }

    pub fn devices() -> i32 {
        let _com = ComGuard::new();
        let mut all = Vec::new();
        for flow in [Flow::Render, Flow::Capture] {
            for e in devices::list(flow).unwrap_or_default() {
                all.push(json!({"flow": format!("{:?}", e.flow), "name": e.name, "id": e.id, "form": format!("{:?}", e.form_factor)}));
            }
        }
        println!("{}", serde_json::to_string_pretty(&all).unwrap_or_default());
        0
    }

    /// Pushes generated audio into a fanout in real time (wall clock).
    fn clocked_source(mut fanout: SourceFanout, mut gen: impl FnMut(&mut [f32]) + Send + 'static, stop: Arc<AtomicBool>) -> JoinHandle<()> {
        std::thread::spawn(move || {
            let start = Instant::now();
            let mut sent: u64 = 0;
            let mut buf = vec![0f32; 4800 * ENGINE_CHANNELS];
            while !stop.load(Ordering::Relaxed) {
                let due = (start.elapsed().as_secs_f64() * ENGINE_RATE as f64) as u64;
                let n = (due.saturating_sub(sent) as usize).min(4800);
                if n > 0 {
                    let b = &mut buf[..n * ENGINE_CHANNELS];
                    gen(b);
                    fanout.push(Some(b), ENGINE_CHANNELS, n);
                    sent += n as u64;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        })
    }

    /// Pulls an output mixer in real time and records what comes out.
    fn clocked_sink(mut mixer: OutputMixer, rec: Arc<Mutex<Vec<f32>>>, stop: Arc<AtomicBool>) -> JoinHandle<()> {
        std::thread::spawn(move || {
            let start = Instant::now();
            let mut pulled: u64 = 0;
            let mut buf = vec![0f32; 480 * ENGINE_CHANNELS];
            while !stop.load(Ordering::Relaxed) {
                let due = (start.elapsed().as_secs_f64() * ENGINE_RATE as f64) as u64;
                while pulled + 480 <= due {
                    mixer.render(&mut buf);
                    rec.lock().unwrap().extend_from_slice(&buf);
                    pulled += 480;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        })
    }

    /// A tone played to a render device through the engine.
    struct Player {
        stop: Arc<AtomicBool>,
        gen: Option<JoinHandle<()>>,
        render: StreamHandle,
        fan_h: ct_engine::mixer::FanoutHandle,
        mix_h: ct_engine::mixer::MixerHandle,
        tap: Arc<ct_engine::mixer::TapStats>,
    }
    impl Player {
        fn describe(&self) -> Value {
            let st = &self.render.status;
            json!({"state": format!("{:?}", st.state()), "hr": format!("0x{:08X}", st.last_error.load(Ordering::Relaxed) as u32),
                   "device": st.device_name.lock().unwrap().clone(), "restarts": st.restarts.load(Ordering::Relaxed),
                   "period": st.period_frames.load(Ordering::Relaxed),
                   "src_frames": self.fan_h.stats.captured_frames.load(Ordering::Relaxed),
                   "src_peak": self.fan_h.stats.meter.read().0,
                   "src_overflow": self.fan_h.stats.overflow_frames.load(Ordering::Relaxed),
                   "out_frames": self.mix_h.stats.rendered_frames.load(Ordering::Relaxed),
                   "out_peak": self.mix_h.stats.meter.read().0,
                   "tap_underrun": self.tap.underrun_frames.load(Ordering::Relaxed),
                   "tap_buffered_ms": self.tap.buffered_ms.load(),
                   "tap_drift_ppm": self.tap.drift_ppm.load()})
        }
    }
    impl Drop for Player {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(g) = self.gen.take() {
                let _ = g.join();
            }
        }
    }

    fn play(freq: f64, amp: f32, device: DeviceSelector) -> Player {
        const SRC: SourceId = 1;
        const OUT: OutputId = 1;
        let (fan, mut fan_h) = SourceFanout::new(SRC, ENGINE_RATE);
        let (mix, mut mix_h) = OutputMixer::new(OUT, ENGINE_CHANNELS, 480);
        let tap = connect(&mut fan_h, &mut mix_h, GainParams::new(0.0, false), 30.0);
        let stop = Arc::new(AtomicBool::new(false));
        let mut sine = Sine::new(freq, ENGINE_RATE as f64, amp);
        let gen = clocked_source(fan, move |b| sine.fill(b, ENGINE_CHANNELS), stop.clone());
        let render = spawn_render("tone", device, mix);
        Player { stop, gen: Some(gen), render, fan_h, mix_h, tap }
    }

    pub fn tone(args: &[String]) -> i32 {
        let _com = ComGuard::new();
        let freq: f64 = arg(args, "--freq").and_then(|v| v.parse().ok()).unwrap_or(1000.0);
        let amp: f32 = arg(args, "--amp").and_then(|v| v.parse().ok()).unwrap_or(0.3);
        let secs: f64 = arg(args, "--seconds").and_then(|v| v.parse().ok()).unwrap_or(3600.0);
        let dev = match arg(args, "--device") {
            Some(name) => DeviceSelector::NameContains(Flow::Render, name.to_string()),
            None => DeviceSelector::Default(Flow::Render),
        };
        let p = play(freq, amp, dev);
        let t0 = Instant::now();
        while t0.elapsed().as_secs_f64() < secs {
            std::thread::sleep(Duration::from_millis(200));
        }
        drop(p);
        0
    }

    /// Records a capture target for `secs` seconds through the engine.
    struct Recording {
        samples: Vec<f32>,
        diag: Value,
        hr: String,
        state: StreamState,
        device: String,
        pid: u32,
        restarts: u32,
    }

    fn record(target: CaptureTarget, secs: f64, during: impl FnOnce(&StreamHandle)) -> Recording {
        let (fan, mut fan_h) = SourceFanout::new(1, ENGINE_RATE);
        let (mix, mut mix_h) = OutputMixer::new(2, ENGINE_CHANNELS, 480);
        let tap = connect(&mut fan_h, &mut mix_h, GainParams::new(0.0, false), 30.0);
        let cap = spawn_capture("rec", target, fan);
        let rec = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let sink = clocked_sink(mix, rec.clone(), stop.clone());
        during(&cap);
        std::thread::sleep(Duration::from_secs_f64(secs));
        stop.store(true, Ordering::Relaxed);
        let _ = sink.join();
        let samples = std::mem::take(&mut *rec.lock().unwrap());
        let raw_peak = samples.iter().fold(0f32, |m, v| m.max(v.abs()));
        let diag = json!({
            "captured_frames": fan_h.stats.captured_frames.load(Ordering::Relaxed),
            "capture_peak_last": fan_h.stats.meter.read().0,
            "overflow": fan_h.stats.overflow_frames.load(Ordering::Relaxed),
            "silence_filled": cap.status.silence_filled_frames.load(Ordering::Relaxed),
            "discontinuities": cap.status.discontinuities.load(Ordering::Relaxed),
            "tap_underrun": tap.underrun_frames.load(Ordering::Relaxed),
            "tap_buffered_ms": tap.buffered_ms.load(),
            "recorded_peak": raw_peak,
        });
        let r = Recording {
            diag,
            samples,
            state: cap.status.state(),
            hr: format!("0x{:08X}", cap.status.last_error.load(Ordering::Relaxed) as u32),
            device: cap.status.device_name.lock().unwrap().clone(),
            pid: cap.status.pid.load(Ordering::Relaxed),
            restarts: cap.status.restarts.load(Ordering::Relaxed),
        };
        drop(cap);
        drop(fan_h);
        drop(mix_h);
        r
    }

    /// Levels (dB) of the given tones in the last `tail_s` seconds.
    fn levels(r: &Recording, tail_s: f64, freqs: &[f64]) -> Value {
        let m = mono(&r.samples, ENGINE_CHANNELS);
        let n = ((tail_s * ENGINE_RATE as f64) as usize).min(m.len());
        let tail = &m[m.len() - n..];
        let mut v = serde_json::Map::new();
        for f in freqs {
            v.insert(format!("{f}"), json!((tone_db_near(tail, *f, 2.0, ENGINE_RATE as f64) * 10.0).round() / 10.0));
        }
        Value::Object(v)
    }

    fn lv(v: &Value, f: f64) -> f64 {
        v[format!("{f}")].as_f64().unwrap_or(-200.0)
    }

    fn spawn_tone_process(exe: &std::path::Path, freq: f64, device: &str) -> std::process::Child {
        std::process::Command::new(exe)
            .args(["tone", "--freq", &freq.to_string(), "--device", device, "--seconds", "120"])
            .spawn()
            .expect("start tone process")
    }

    pub fn smoke(args: &[String]) -> i32 {
        std::panic::set_hook(Box::new(|info| eprintln!("PANIC: {info}")));
        let _com = ComGuard::new();
        let cable_in = arg(args, "--cable-in").unwrap_or("CABLE Input").to_string();
        let cable_out = arg(args, "--cable-out").unwrap_or("CABLE Output").to_string();
        let sink = arg(args, "--sink").unwrap_or("Virtual Audio").to_string();
        let out = arg(args, "--out").unwrap_or("smoke-report.json").to_string();
        let mut report = serde_json::Map::new();
        let mut ok = true;
        let mut check = |name: &str, pass: bool, detail: Value, report: &mut serde_json::Map<String, Value>| {
            println!("[{}] {name}: {detail}", if pass { "PASS" } else { "FAIL" });
            report.insert(name.to_string(), json!({"pass": pass, "detail": detail}));
            ok &= pass;
        };
        report.insert("windows_build".into(), json!(ct_engine::win::windows_build()));
        report.insert("devices_render".into(), json!(devices::list(Flow::Render).unwrap_or_default().iter().map(|e| e.name.clone()).collect::<Vec<_>>()));
        report.insert("devices_capture".into(), json!(devices::list(Flow::Capture).unwrap_or_default().iter().map(|e| e.name.clone()).collect::<Vec<_>>()));

        // A. engine render → virtual cable → engine capture
        {
            let player = play(1000.0, 0.3, DeviceSelector::NameContains(Flow::Render, cable_in.clone()));
            std::thread::sleep(Duration::from_millis(500));
            let r = record(CaptureTarget::Device(DeviceSelector::NameContains(Flow::Capture, cable_out.clone())), 3.0, |_| {});
            let pl = player.describe();
            drop(player);
            let l = levels(&r, 2.0, &[1000.0, 3000.0]);
            let pass = r.state == StreamState::Running && lv(&l, 1000.0) > -16.0 && lv(&l, 3000.0) < lv(&l, 1000.0) - 60.0;
            check("render_capture_cable", pass, json!({"levels_db": l, "device": r.device, "state": format!("{:?}", r.state), "hr": r.hr, "player": pl, "samples": r.samples.len(), "diag": r.diag}), &mut report);
        }

        // B. process loopback isolates one program from another
        let exe = std::env::current_exe().expect("exe path");
        {
            let mut target = spawn_tone_process(&exe, 1500.0, &sink);
            let mut other = spawn_tone_process(&exe, 3000.0, &sink);
            std::thread::sleep(Duration::from_millis(1500));
            let pid = Arc::new(AtomicU32::new(target.id()));
            let r = record(CaptureTarget::Process(ProcessTarget::Pid(pid)), 3.0, |_| {});
            let l = levels(&r, 2.0, &[1500.0, 3000.0]);
            let pass = lv(&l, 1500.0) > -20.0 && lv(&l, 3000.0) < lv(&l, 1500.0) - 60.0;
            check("process_loopback_isolation", pass, json!({"levels_db": l, "pid": r.pid, "state": format!("{:?}", r.state), "hr": r.hr, "diag": r.diag}), &mut report);
            let _ = target.kill();
            let _ = other.kill();
            let _ = target.wait();
            let _ = other.wait();
        }

        // C. program restarts: found again by name, audio returns
        {
            let fake = std::env::temp_dir().join("ct-fakeapp.exe");
            let _ = std::fs::copy(&exe, &fake);
            let mut first = spawn_tone_process(&fake, 1500.0, &sink);
            std::thread::sleep(Duration::from_millis(1500));
            let restarted: Arc<Mutex<Option<std::process::Child>>> = Arc::new(Mutex::new(None));
            let r2 = restarted.clone();
            let fake2 = fake.clone();
            let sink2 = sink.clone();
            let r = record(CaptureTarget::Process(ProcessTarget::ExeNames(vec!["ct-fakeapp.exe".into()])), 1.0, move |cap| {
                std::thread::sleep(Duration::from_millis(1500));
                let first_pid = cap.status.pid.load(Ordering::Relaxed);
                let _ = first.kill();
                let _ = first.wait();
                std::thread::sleep(Duration::from_millis(500));
                *r2.lock().unwrap() = Some(spawn_tone_process(&fake2, 1500.0, &sink2));
                // wait until the stream has found the new process
                let t0 = Instant::now();
                while t0.elapsed() < Duration::from_secs(8) {
                    let p = cap.status.pid.load(Ordering::Relaxed);
                    if p != 0 && p != first_pid && cap.status.state() == StreamState::Running {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                std::thread::sleep(Duration::from_millis(2500));
            });
            let l = levels(&r, 0.9, &[1500.0]);
            let pass = lv(&l, 1500.0) > -20.0 && r.restarts >= 1;
            check("process_restart_recovery", pass, json!({"levels_db": l, "restarts": r.restarts, "state": format!("{:?}", r.state), "hr": r.hr, "diag": r.diag}), &mut report);
            let child = restarted.lock().unwrap().take();
            if let Some(mut c) = child {
                let _ = c.kill();
                let _ = c.wait();
            }
        }

        // D. (information) the microphone the engine would pick never is our
        // own cable, and whether the CI sink's virtual mic echoes its speaker
        {
            let own: Vec<String> = devices::OWN_VIRTUAL_DEVICES.iter().map(|s| s.to_string()).collect();
            let mic = devices::resolve(&DeviceSelector::DefaultExcluding(Flow::Capture, own)).map(|(_, e)| e.name).unwrap_or_default();
            let lower = mic.to_lowercase();
            let pass = !devices::OWN_VIRTUAL_DEVICES.iter().any(|p| lower.contains(p));
            check("microphone_never_own_cable", pass, json!({"picked": mic, "default": devices::default_id(Flow::Capture)}), &mut report);
            let player = play(2500.0, 0.3, DeviceSelector::NameContains(Flow::Render, sink.clone()));
            std::thread::sleep(Duration::from_millis(500));
            let r = record(CaptureTarget::Device(DeviceSelector::NameContains(Flow::Capture, "microphone array (virtual audio".into())), 2.0, |_| {});
            drop(player);
            report.insert("info_sink_mic_echo".into(), json!({"levels_db": levels(&r, 1.5, &[2500.0]), "device": r.device, "state": format!("{:?}", r.state), "hr": r.hr, "diag": r.diag}));
        }

        report.insert("pass".into(), json!(ok));
        let _ = std::fs::write(&out, serde_json::to_string_pretty(&Value::Object(report)).unwrap_or_default());
        if ok {
            0
        } else {
            1
        }
    }
}
