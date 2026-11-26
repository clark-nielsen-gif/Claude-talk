//! `ct-audiotest` – test tool for real Windows audio devices.
//!
//! ```text
//! ct-audiotest devices
//! ct-audiotest tone --freq 1000 [--device "Speakers"] [--seconds 30] [--amp 0.3] [--burst-ms 20 --period-ms 600]
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
    ct_engine::win::high_res_timer();
    let args: Vec<String> = std::env::args().collect();
    let code = match args.get(1).map(String::as_str) {
        Some("serve-testpage") => serve::run(&args[2..]),
        Some("devices") => imp::devices(),
        Some("tone") => imp::tone(&args[2..]),
        Some("listen") => imp::listen(&args[2..]),
        Some("smoke") => imp::smoke(&args[2..]),
        Some("e2e") => imp::e2e(&args[2..]),
        Some("endurance") => imp::endurance(&args[2..]),
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
        let arg = |n: &str| {
            args.iter()
                .position(|a| a == n)
                .and_then(|i| args.get(i + 1))
                .cloned()
        };
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
        let counter = std::sync::atomic::AtomicU64::new(0);
        let counter = std::sync::Arc::new(counter);
        let out = std::sync::Arc::new(out);
        for conn in listener.incoming().flatten() {
            // One thread per connection: browsers open spare connections that
            // never send anything, which must not block the real requests.
            let counter = counter.clone();
            let out = out.clone();
            std::thread::spawn(move || {
                let _ = conn.set_read_timeout(Some(std::time::Duration::from_secs(10)));
                let n = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                handle(conn, n, &out);
            });
        }
        0
    }

    fn handle(conn: std::net::TcpStream, counter: u64, out: &str) {
        let mut reader = BufReader::new(&conn);
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() {
            return;
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
        if line.starts_with("POST /marks") {
            let mut body = vec![0u8; len];
            let _ = reader.read_exact(&mut body);
            let marks = std::path::Path::new(&out).with_file_name("page-marks.json");
            let _ = std::fs::write(marks, &body);
            let _ = w.write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n");
        } else if line.starts_with("POST /report") {
            let mut body = vec![0u8; len];
            let _ = reader.read_exact(&mut body);
            let _ = std::fs::write(out, &body);
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
            let _ = w.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
        }
    }
}

#[cfg(windows)]
mod imp {
    use ct_engine::analysis::{
        burst_onsets, match_delays, median, mono, tone_peak_near, tone_tolerance, Sine,
    };
    use ct_engine::graph::{OutputId, SourceId};
    use ct_engine::mixer::{connect, GainParams, OutputMixer, SourceFanout};
    use ct_engine::win::devices::{self, DeviceSelector, Flow};
    use ct_engine::win::stream::{
        spawn_capture, spawn_render, CaptureTarget, ProcessTarget, StreamHandle, StreamState,
    };
    use ct_engine::win::ComGuard;
    use ct_engine::{ENGINE_CHANNELS, ENGINE_RATE};
    use serde_json::{json, Value};
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    fn arg<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .map(String::as_str)
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
    fn clocked_source(
        mut fanout: SourceFanout,
        mut gen: impl FnMut(&mut [f32]) + Send + 'static,
        stop: Arc<AtomicBool>,
    ) -> JoinHandle<()> {
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
    fn clocked_sink(
        mut mixer: OutputMixer,
        rec: Arc<Mutex<Vec<f32>>>,
        stop: Arc<AtomicBool>,
    ) -> JoinHandle<()> {
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
        play_pattern(freq, amp, device, None)
    }

    /// `bursts`: (burst length, period) in ms – short tone bursts for
    /// latency measurement instead of a steady tone.
    fn play_pattern(
        freq: f64,
        amp: f32,
        device: DeviceSelector,
        bursts: Option<(f64, f64)>,
    ) -> Player {
        const SRC: SourceId = 1;
        const OUT: OutputId = 1;
        let (fan, mut fan_h) = SourceFanout::new(SRC, ENGINE_RATE);
        let (mix, mut mix_h) = OutputMixer::new(OUT, ENGINE_CHANNELS, 480);
        let tap = connect(&mut fan_h, &mut mix_h, GainParams::new(0.0, false), 30.0);
        let stop = Arc::new(AtomicBool::new(false));
        let mut sine = Sine::new(freq, ENGINE_RATE as f64, amp);
        let mut pos: u64 = 0;
        let gen = clocked_source(
            fan,
            move |b| {
                sine.fill(b, ENGINE_CHANNELS);
                if let Some((len_ms, period_ms)) = bursts {
                    let (len, period) = ((len_ms * 48.0) as u64, (period_ms * 48.0) as u64);
                    for f in b.chunks_exact_mut(ENGINE_CHANNELS) {
                        if pos % period >= len {
                            f.fill(0.0);
                        }
                        pos += 1;
                    }
                }
            },
            stop.clone(),
        );
        let render = spawn_render("tone", device, mix);
        Player {
            stop,
            gen: Some(gen),
            render,
            fan_h,
            mix_h,
            tap,
        }
    }

    pub fn tone(args: &[String]) -> i32 {
        let _com = ComGuard::new();
        let freq: f64 = arg(args, "--freq")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1000.0);
        let amp: f32 = arg(args, "--amp")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.3);
        let secs: f64 = arg(args, "--seconds")
            .and_then(|v| v.parse().ok())
            .unwrap_or(3600.0);
        let dev = match arg(args, "--device") {
            Some(name) => DeviceSelector::NameContains(Flow::Render, name.to_string()),
            None => DeviceSelector::Default(Flow::Render),
        };
        let bursts = match (
            arg(args, "--burst-ms").and_then(|v| v.parse().ok()),
            arg(args, "--period-ms").and_then(|v| v.parse().ok()),
        ) {
            (Some(l), Some(p)) => Some((l, p)),
            _ => None,
        };
        let p = play_pattern(freq, amp, dev, bursts);
        let t0 = Instant::now();
        while t0.elapsed().as_secs_f64() < secs {
            std::thread::sleep(Duration::from_millis(200));
        }
        drop(p);
        0
    }

    /// Records from the default microphone the way Discord and Chrome do
    /// (whatever Windows gives this program as its default), after an
    /// optional delay. Used as the fake programs of the routing test.
    pub fn listen(args: &[String]) -> i32 {
        let _com = ComGuard::new();
        let delay: f64 = arg(args, "--delay")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);
        let secs: f64 = arg(args, "--seconds")
            .and_then(|v| v.parse().ok())
            .unwrap_or(15.0);
        std::thread::sleep(Duration::from_secs_f64(delay));
        let r = record(
            CaptureTarget::Device(DeviceSelector::Default(Flow::Capture)),
            secs,
            |_| {},
        );
        println!("recorded from '{}' ({:?})", r.device, r.state);
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
            hr: format!(
                "0x{:08X}",
                cap.status.last_error.load(Ordering::Relaxed) as u32
            ),
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
            // level, and where the tone actually was (clock differences move it)
            let (db, found) = tone_peak_near(tail, *f, tone_tolerance(*f), ENGINE_RATE as f64);
            v.insert(format!("{f}"), json!((db * 10.0).round() / 10.0));
            v.insert(format!("{f}_hz"), json!((found * 10.0).round() / 10.0));
        }
        Value::Object(v)
    }

    fn lv(v: &Value, f: f64) -> f64 {
        v[format!("{f}")].as_f64().unwrap_or(-200.0)
    }

    fn spawn_tone_process(exe: &std::path::Path, freq: f64, device: &str) -> std::process::Child {
        std::process::Command::new(exe)
            .args([
                "tone",
                "--freq",
                &freq.to_string(),
                "--device",
                device,
                "--seconds",
                "120",
            ])
            .spawn()
            .expect("start tone process")
    }

    fn spawn_burst_process(
        exe: &std::path::Path,
        freq: f64,
        device: &str,
        period_ms: u32,
    ) -> std::process::Child {
        std::process::Command::new(exe)
            .args([
                "tone",
                "--freq",
                &freq.to_string(),
                "--device",
                device,
                "--seconds",
                "120",
                "--amp",
                "0.5",
                "--burst-ms",
                "25",
                "--period-ms",
                &period_ms.to_string(),
            ])
            .spawn()
            .expect("start burst process")
    }

    /// Records several capture targets on one clock. Returns the wall-clock
    /// time (ms since 1970) of sample 0 and, per target, mono samples plus
    /// the average buffering of the recorder's own tap (ms).
    fn record_multi(targets: Vec<CaptureTarget>, secs: f64) -> (f64, Vec<(Vec<f32>, f64)>) {
        let mut parts = Vec::new();
        for (k, t) in targets.into_iter().enumerate() {
            let (fan, mut fan_h) = SourceFanout::new(1, ENGINE_RATE);
            let (mix, mut mix_h) = OutputMixer::new(2 + k as OutputId, ENGINE_CHANNELS, 480);
            let tap = connect(&mut fan_h, &mut mix_h, GainParams::new(0.0, false), 30.0);
            let cap = spawn_capture("rec", t, fan);
            parts.push((mix, tap, cap, fan_h, mix_h, Vec::<f32>::new(), 0f64));
        }
        std::thread::sleep(Duration::from_millis(1500)); // streams start, taps prime
        let start = Instant::now();
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0);
        let mut buf = vec![0f32; 480 * ENGINE_CHANNELS];
        let mut pulled: u64 = 0;
        let mut pulls = 0u64;
        while start.elapsed().as_secs_f64() < secs {
            let due = (start.elapsed().as_secs_f64() * ENGINE_RATE as f64) as u64;
            while pulled + 480 <= due {
                for p in parts.iter_mut() {
                    p.0.render(&mut buf);
                    p.5.extend_from_slice(&buf);
                    p.6 += p.1.buffered_ms.load() as f64;
                }
                pulled += 480;
                pulls += 1;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let out = parts
            .into_iter()
            .map(|p| (mono(&p.5, ENGINE_CHANNELS), p.6 / pulls.max(1) as f64))
            .collect();
        (epoch, out)
    }

    fn start_app(app: &str, echo: &str) -> std::process::Child {
        std::process::Command::new(app)
            .env("CLAUDE_TALK_TEST_URL", "http://127.0.0.1:8765/test.html")
            .env("CLAUDE_TALK_TEST_OPEN_CLAUDE", "1")
            .env("CLAUDE_TALK_TEST_MIC_EXE", "fakemic.exe")
            .env("CLAUDE_TALK_TEST_ECHO", echo)
            .spawn()
            .expect("start app")
    }

    /// Stops the app and its WebView2 processes (they hold the Claude
    /// window's profile).
    fn stop_app(p: &mut std::process::Child) {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &p.id().to_string(), "/T", "/F"])
            .output();
        let _ = p.kill();
        let _ = p.wait();
    }

    fn powershell(cmd: &str) -> String {
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", cmd])
            .output();
        let text = out
            .map(|o| {
                String::from_utf8_lossy(&o.stdout).into_owned()
                    + &String::from_utf8_lossy(&o.stderr)
            })
            .unwrap_or_default();
        println!("powershell: {cmd} -> {}", text.trim());
        text
    }

    /// Full end-to-end test of the app with the Claude window on the CI voice
    /// page. Fake programs (copies of this tool) play the tones:
    /// "Discord" 1000 Hz, microphone 2500 Hz, background 3000 Hz; the page
    /// itself plays Claude's 440 Hz.
    ///
    /// `e2e --app claude-talk.exe --sink "Virtual Audio" --out results`
    pub fn e2e(args: &[String]) -> i32 {
        std::panic::set_hook(Box::new(|info| eprintln!("PANIC: {info}")));
        let _com = ComGuard::new();
        let app = arg(args, "--app").unwrap_or("claude-talk.exe").to_string();
        let sink = arg(args, "--sink").unwrap_or("Virtual Audio").to_string();
        let out = std::path::PathBuf::from(arg(args, "--out").unwrap_or("results"));
        let _ = std::fs::create_dir_all(&out);
        let page_report = out.join("page-report.json");
        let _ = std::fs::remove_file(&page_report);
        let mut report = serde_json::Map::new();
        let mut ok = true;

        // test page server
        let srv_out = page_report.to_string_lossy().into_owned();
        std::thread::spawn(move || {
            super::serve::run(&["--port".into(), "8765".into(), "--out".into(), srv_out])
        });

        // fake programs: copies of this tool under the names the app looks for
        let me = std::env::current_exe().expect("exe");
        let dir = std::env::temp_dir().join("ct-e2e");
        let _ = std::fs::create_dir_all(&dir);
        let mut kids = Vec::new();
        let names = ["Discord.exe", "fakemic.exe", "background.exe"];
        for (name, freq) in names.into_iter().zip([1000.0, 2500.0, 3000.0]) {
            let p = dir.join(name);
            let _ = std::fs::copy(&me, &p);
            kids.push(spawn_tone_process(&p, freq, &sink));
        }
        std::thread::sleep(Duration::from_millis(1500));

        // The app runs twice: headset mode (echo reduction off) and speaker
        // mode (echo reduction on), with the same expectations.
        let log_dir = std::env::var("APPDATA")
            .map(|a| std::path::PathBuf::from(a).join("Claude Talk").join("logs"))
            .ok();
        for echo in ["off", "on"] {
            let _ = std::fs::remove_file(&page_report);
            let mut phase = serde_json::Map::new();
            let mut app_proc = start_app(&app, echo);

            // The page reports after its tests and keeps playing Claude's
            // voice: wait for it (a first WebView2 start can take a while).
            let t0 = Instant::now();
            while !page_report.exists() && t0.elapsed() < Duration::from_secs(60) {
                std::thread::sleep(Duration::from_millis(500));
            }
            std::thread::sleep(Duration::from_millis(500));
            // what Discord hears: record our virtual microphone
            let cable = record(
                CaptureTarget::Device(DeviceSelector::FirstMatching(
                    Flow::Capture,
                    ct_engine::win::host::cable_capture_patterns(),
                )),
                4.0,
                |_| {},
            );
            let l = levels(&cable, 3.0, &[440.0, 2500.0, 1000.0, 3000.0]);
            let (claude, mic, discord, bg) = (
                lv(&l, 440.0),
                lv(&l, 2500.0),
                lv(&l, 1000.0),
                lv(&l, 3000.0),
            );
            let pass = claude > -30.0
                && mic > -30.0
                && discord < claude.min(mic) - 60.0
                && bg < claude.min(mic) - 60.0;
            println!(
                "[{}] echo {echo}: to_discord: {l}",
                if pass { "PASS" } else { "FAIL" }
            );
            phase.insert(
                "to_discord".into(),
                json!({"pass": pass, "levels_db": l, "diag": cable.diag, "device": cable.device}),
            );
            ok &= pass;

            // what Claude hears: the page reports what getUserMedia delivered
            let page: Value = std::fs::read(&page_report)
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or(Value::Null);
            let pl = |f: &str| page["levels"][f].as_f64().unwrap_or(-400.0);
            let (p_discord, p_mic, p_claude, p_bg) =
                (pl("1000"), pl("2500"), pl("440"), pl("3000"));
            let errors = page["errors"].as_array().map(|a| a.len()).unwrap_or(1);
            let pass = errors == 0
                && p_discord > -30.0
                && p_mic > -30.0
                && p_claude < p_discord.min(p_mic) - 60.0
                && p_bg < p_discord.min(p_mic) - 60.0;
            println!(
                "[{}] echo {echo}: to_claude (page): {}",
                if pass { "PASS" } else { "FAIL" },
                page["levels"]
            );
            phase.insert("to_claude".into(), json!({"pass": pass, "page": page}));
            ok &= pass;

            // Latency with tone bursts: the fake microphone (2500 Hz) and
            // Discord (1000 Hz) switch to short bursts.
            for (i, (freq, period)) in [(1000.0, 700u32), (2500.0, 600u32)].into_iter().enumerate()
            {
                let _ = kids[i].kill();
                let _ = kids[i].wait();
                kids[i] = spawn_burst_process(&dir.join(names[i]), freq, &sink, period);
            }
            std::thread::sleep(Duration::from_secs(4)); // the app finds the restarted programs
            let marks_file = page_report.with_file_name("page-marks.json");
            let _ = std::fs::remove_file(&marks_file);
            let (epoch, recs) = record_multi(
                vec![
                    CaptureTarget::Process(ProcessTarget::ExeNames(vec!["fakemic.exe".into()])),
                    CaptureTarget::Device(DeviceSelector::FirstMatching(
                        Flow::Capture,
                        ct_engine::win::host::cable_capture_patterns(),
                    )),
                    CaptureTarget::Process(ProcessTarget::ExeNames(vec!["discord.exe".into()])),
                    // what Claude Talk plays into the cable, before VB-CABLE's own buffer
                    CaptureTarget::DeviceLoopback(DeviceSelector::FirstMatching(
                        Flow::Render,
                        ct_engine::win::host::cable_render_patterns(),
                    )),
                ],
                8.0,
            );
            std::thread::sleep(Duration::from_millis(1500)); // the page posts its marks
            let ms = |idx: Vec<usize>| {
                idx.into_iter()
                    .map(|i| i as f64 / 48.0)
                    .collect::<Vec<f64>>()
            };
            let rate = ENGINE_RATE as f64;
            // Microphone → Discord: both recordings on one clock, each
            // corrected by its own recorder buffering.
            let (mic_rec, mic_buf) = &recs[0];
            let (cable_rec, cable_buf) = &recs[1];
            let a: Vec<f64> = ms(burst_onsets(mic_rec, 2500.0, rate, 0.3, 2400))
                .into_iter()
                .map(|t| t - mic_buf)
                .collect();
            let b: Vec<f64> = ms(burst_onsets(cable_rec, 2500.0, rate, 0.3, 2400))
                .into_iter()
                .map(|t| t - cable_buf)
                .collect();
            let to_discord_total = match_delays(&a, &b, 300.0);
            // Claude Talk's own share: from the microphone into the cable
            // (VB-CABLE then adds its internal buffer, see KNOWN_ISSUES K9)
            let (into_rec, into_buf) = &recs[3];
            let c: Vec<f64> = ms(burst_onsets(into_rec, 2500.0, rate, 0.3, 2400))
                .into_iter()
                .map(|t| t - into_buf)
                .collect();
            let to_discord = match_delays(&a, &c, 300.0);
            let cable_added = median(&match_delays(&c, &b, 300.0));
            // Discord → Claude: wall-clock time the burst entered the
            // recorder (a sample is pulled one 10 ms block after its slot and
            // waited `dc_buf` in the tap), minus one more 10 ms capture
            // period so the estimate errs on the slow side; against the
            // page's arrival times. The two 10 ms terms cancel.
            let (dc_rec, dc_buf) = &recs[2];
            let sent: Vec<f64> = ms(burst_onsets(dc_rec, 1000.0, rate, 0.3, 2400))
                .into_iter()
                .map(|t| epoch + t - dc_buf)
                .collect();
            let marks: Vec<f64> = std::fs::read(&marks_file)
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default();
            let marks: Vec<f64> = marks
                .into_iter()
                .filter(|&m| m >= epoch && m <= epoch + 9000.0)
                .collect();
            let to_claude = match_delays(&sent, &marks, 300.0);
            let (md, mc, mt) = (
                median(&to_discord),
                median(&to_claude),
                median(&to_discord_total),
            );
            let pass = to_discord.len() >= 5
                && to_claude.len() >= 5
                && md.is_some_and(|v| v < 40.0)
                && mc.is_some_and(|v| v < 40.0);
            println!(
                "[{}] echo {echo}: latency Claude Talk → cable {md:?} ms ({} bursts), + VB-CABLE {cable_added:?} ms = {mt:?} ms at Discord; Discord → Claude {mc:?} ms ({} bursts)",
                if pass { "PASS" } else { "FAIL" },
                to_discord.len(),
                to_claude.len()
            );
            phase.insert(
                "latency".into(),
                json!({"pass": pass, "to_discord_ms": md, "vbcable_added_ms": cable_added, "to_discord_total_ms": mt, "to_claude_ms": mc,
                       "to_discord_all": to_discord, "to_claude_all": to_claude,
                       "bursts": {"mic": a.len(), "into_cable": c.len(), "cable": b.len(), "discord": sent.len(), "page": marks.len()},
                       "recorder_buffer_ms": [mic_buf, cable_buf, dc_buf, into_buf]}),
            );
            ok &= pass;
            // back to steady tones for the next mode
            for (i, freq) in [1000.0, 2500.0].into_iter().enumerate() {
                let _ = kids[i].kill();
                let _ = kids[i].wait();
                kids[i] = spawn_tone_process(&dir.join(names[i]), freq, &sink);
            }

            stop_app(&mut app_proc);
            report.insert(format!("echo_{echo}"), Value::Object(phase));
            std::thread::sleep(Duration::from_secs(3));
        }

        // Robustness: the app keeps working when programs restart, the
        // virtual microphone disappears, sample rates change and Windows'
        // audio service restarts.
        {
            let mut rob = serde_json::Map::new();
            let mut app_proc = start_app(&app, "off");
            std::thread::sleep(Duration::from_secs(10));
            let step = |name: &str, rob: &mut serde_json::Map<String, Value>| {
                let r = record(
                    CaptureTarget::Device(DeviceSelector::FirstMatching(
                        Flow::Capture,
                        ct_engine::win::host::cable_capture_patterns(),
                    )),
                    3.0,
                    |_| {},
                );
                let l = levels(&r, 2.0, &[440.0, 2500.0, 1000.0]);
                let (c, m, d) = (lv(&l, 440.0), lv(&l, 2500.0), lv(&l, 1000.0));
                let pass = c > -30.0 && m > -30.0 && d < c.min(m) - 60.0;
                println!(
                    "[{}] robustness {name}: {l}",
                    if pass { "PASS" } else { "FAIL" }
                );
                rob.insert(name.into(), json!({"pass": pass, "levels_db": l, "device": r.device, "state": format!("{:?}", r.state)}));
                pass
            };
            ok &= step("baseline", &mut rob);

            let _ = kids[1].kill();
            let _ = kids[1].wait();
            std::thread::sleep(Duration::from_secs(2));
            kids[1] = spawn_tone_process(&dir.join(names[1]), 2500.0, &sink);
            std::thread::sleep(Duration::from_secs(5));
            ok &= step("microphone_program_restarted", &mut rob);

            powershell("Get-PnpDevice -FriendlyName '*VB-Audio Virtual Cable*' | Disable-PnpDevice -Confirm:$false");
            std::thread::sleep(Duration::from_secs(4));
            powershell("Get-PnpDevice -FriendlyName '*VB-Audio Virtual Cable*' | Enable-PnpDevice -Confirm:$false");
            std::thread::sleep(Duration::from_secs(10));
            ok &= step("virtual_microphone_removed_and_back", &mut rob);

            let cable_ids: Vec<String> = [Flow::Render, Flow::Capture]
                .into_iter()
                .flat_map(|f| devices::list(f).unwrap_or_default())
                .filter(|e| e.name.to_lowercase().contains("claude talk"))
                .map(|e| e.id)
                .collect();
            let set_rate = |ids: &[String], rate: u32| {
                ids.iter()
                    .map(|id| format!("{:?}", ct_engine::win::policy::set_device_rate(id, rate)))
                    .collect::<Vec<_>>()
            };
            rob.insert(
                "cable_rate_change".into(),
                json!(set_rate(&cable_ids, 44100)),
            );
            std::thread::sleep(Duration::from_secs(8));
            ok &= step("virtual_microphone_at_44100", &mut rob);
            set_rate(&cable_ids, 48000);
            std::thread::sleep(Duration::from_secs(5));

            let sink_ids: Vec<String> = devices::list(Flow::Render)
                .unwrap_or_default()
                .into_iter()
                .filter(|e| e.name.to_lowercase().contains(&sink.to_lowercase()))
                .map(|e| e.id)
                .collect();
            rob.insert("sink_rate_change".into(), json!(set_rate(&sink_ids, 44100)));
            std::thread::sleep(Duration::from_secs(8));
            ok &= step("programs_device_at_44100", &mut rob);
            set_rate(&sink_ids, 48000);
            std::thread::sleep(Duration::from_secs(5));

            powershell("Restart-Service Audiosrv -Force");
            std::thread::sleep(Duration::from_secs(15));
            ok &= step("audio_service_restarted", &mut rob);

            let alive = matches!(app_proc.try_wait(), Ok(None));
            println!(
                "[{}] robustness app_still_running",
                if alive { "PASS" } else { "FAIL" }
            );
            rob.insert("app_still_running".into(), json!(alive));
            ok &= alive;
            stop_app(&mut app_proc);
            report.insert("robustness".into(), Value::Object(rob));
        }

        // tidy up
        for mut k in kids {
            let _ = k.kill();
            let _ = k.wait();
        }
        if let Some(d) = log_dir {
            if let Ok(rd) = std::fs::read_dir(d) {
                for e in rd.flatten() {
                    let _ = std::fs::copy(e.path(), out.join(e.file_name()));
                }
            }
        }
        report.insert("pass".into(), json!(ok));
        let _ = std::fs::write(
            out.join("e2e.json"),
            serde_json::to_string_pretty(&Value::Object(report)).unwrap_or_default(),
        );
        if ok {
            0
        } else {
            1
        }
    }

    /// Memory (MB, working set of the app and its child processes) and
    /// total CPU seconds used so far.
    fn app_usage(pid: u32) -> (f64, f64) {
        let cmd = format!(
            "$p = Get-CimInstance Win32_Process | Where-Object {{ $_.ProcessId -eq {pid} -or $_.ParentProcessId -eq {pid} }} | ForEach-Object {{ Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue }}; \
             '{{0}} {{1}}' -f ($p | Measure-Object WorkingSet64 -Sum).Sum, ($p | Measure-Object TotalProcessorTime -Sum).Sum.TotalSeconds"
        );
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &cmd])
            .output();
        let text = out
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();
        let mut it = text
            .split_whitespace()
            .map(|v| v.replace(',', ".").parse::<f64>().unwrap_or(0.0));
        (
            it.next().unwrap_or(0.0) / 1_048_576.0,
            it.next().unwrap_or(0.0),
        )
    }

    /// Long run with the real app (2 hours in CI): every few minutes the
    /// virtual microphone must still carry Claude and the microphone and
    /// nothing else, Discord's bursts must reach the Claude window in under
    /// 40 ms, and memory must not grow.
    ///
    /// `endurance --app claude-talk.exe --sink "Virtual Audio" --minutes 120 --every 5 --out results`
    pub fn endurance(args: &[String]) -> i32 {
        std::panic::set_hook(Box::new(|info| eprintln!("PANIC: {info}")));
        let _com = ComGuard::new();
        let app = arg(args, "--app").unwrap_or("claude-talk.exe").to_string();
        let sink = arg(args, "--sink").unwrap_or("Virtual Audio").to_string();
        let minutes: f64 = arg(args, "--minutes")
            .and_then(|v| v.parse().ok())
            .unwrap_or(120.0);
        let every: f64 = arg(args, "--every")
            .and_then(|v| v.parse().ok())
            .unwrap_or(5.0);
        let out = std::path::PathBuf::from(arg(args, "--out").unwrap_or("results"));
        let _ = std::fs::create_dir_all(&out);
        let page_report = out.join("page-report.json");
        let marks_file = out.join("page-marks.json");
        let srv_out = page_report.to_string_lossy().into_owned();
        std::thread::spawn(move || {
            super::serve::run(&["--port".into(), "8765".into(), "--out".into(), srv_out])
        });

        let me = std::env::current_exe().expect("exe");
        let dir = std::env::temp_dir().join("ct-endurance");
        let _ = std::fs::create_dir_all(&dir);
        for n in ["Discord.exe", "fakemic.exe", "background.exe"] {
            let _ = std::fs::copy(&me, dir.join(n));
        }
        let run_s = (minutes * 60.0 + 600.0).to_string();
        let tone = |name: &str, freq: &str, extra: &[&str]| {
            let mut a = vec![
                "tone",
                "--freq",
                freq,
                "--device",
                sink.as_str(),
                "--seconds",
                run_s.as_str(),
            ];
            a.extend_from_slice(extra);
            std::process::Command::new(dir.join(name))
                .args(a)
                .spawn()
                .expect("start tone")
        };
        let mut kids = [
            tone(
                "Discord.exe",
                "1000",
                &["--amp", "0.5", "--burst-ms", "25", "--period-ms", "700"],
            ),
            tone("fakemic.exe", "2500", &[]),
            tone("background.exe", "3000", &[]),
        ];
        std::thread::sleep(Duration::from_millis(1500));
        let mut app_proc = start_app(&app, "off");
        let pid = app_proc.id();
        std::thread::sleep(Duration::from_secs(20));

        let t0 = Instant::now();
        let mut samples = Vec::new();
        let mut ok = true;
        let mut first_mem = None;
        let mut last_cpu: Option<(f64, Instant)> = None;
        loop {
            let at_min = t0.elapsed().as_secs_f64() / 60.0;
            let cable = record(
                CaptureTarget::Device(DeviceSelector::FirstMatching(
                    Flow::Capture,
                    ct_engine::win::host::cable_capture_patterns(),
                )),
                3.0,
                |_| {},
            );
            let l = levels(&cable, 2.0, &[440.0, 2500.0, 1000.0, 3000.0]);
            let (c, m, d, b) = (
                lv(&l, 440.0),
                lv(&l, 2500.0),
                lv(&l, 1000.0),
                lv(&l, 3000.0),
            );
            let isolation = c > -30.0 && m > -30.0 && d < c.min(m) - 60.0 && b < c.min(m) - 60.0;

            let _ = std::fs::remove_file(&marks_file);
            let (epoch, recs) = record_multi(
                vec![CaptureTarget::Process(ProcessTarget::ExeNames(vec![
                    "discord.exe".into(),
                ]))],
                8.0,
            );
            std::thread::sleep(Duration::from_millis(1500));
            let (dc, dc_buf) = &recs[0];
            let sent: Vec<f64> = burst_onsets(dc, 1000.0, ENGINE_RATE as f64, 0.3, 2400)
                .into_iter()
                .map(|i| epoch + i as f64 / 48.0 - dc_buf)
                .collect();
            let marks: Vec<f64> = std::fs::read(&marks_file)
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default();
            let marks: Vec<f64> = marks
                .into_iter()
                .filter(|&t| t >= epoch && t <= epoch + 9000.0)
                .collect();
            let delays = match_delays(&sent, &marks, 300.0);
            let lat = median(&delays);
            let latency_ok = delays.len() >= 5 && lat.is_some_and(|v| v < 40.0);

            let (mem, cpu_s) = app_usage(pid);
            let now = Instant::now();
            let cpu_pct = last_cpu.map(|(c0, t)| {
                100.0 * (cpu_s - c0)
                    / now.duration_since(t).as_secs_f64()
                    / std::thread::available_parallelism()
                        .map(|n| n.get())
                        .unwrap_or(1) as f64
            });
            last_cpu = Some((cpu_s, now));
            let first = *first_mem.get_or_insert(mem);
            let alive = matches!(app_proc.try_wait(), Ok(None));
            let pass = isolation
                && latency_ok
                && alive
                && mem < 150.0
                && mem < first + 30.0
                && cpu_pct.is_none_or(|p| p < 3.0);
            ok &= pass;
            println!(
                "[{}] {at_min:.0} min: isolation {isolation} {l} | to Claude {lat:?} ms ({} bursts) | {mem:.0} MB, CPU {cpu_pct:?} %",
                if pass { "PASS" } else { "FAIL" },
                delays.len()
            );
            samples.push(json!({"minute": (at_min * 10.0).round() / 10.0, "pass": pass, "isolation": isolation, "levels_db": l,
                "latency_to_claude_ms": lat, "bursts": delays.len(), "memory_mb": (mem * 10.0).round() / 10.0, "cpu_percent": cpu_pct, "alive": alive}));
            let _ = std::fs::write(
                out.join("endurance.json"),
                serde_json::to_string_pretty(&json!({"pass": ok, "samples": samples}))
                    .unwrap_or_default(),
            );
            if !alive || at_min >= minutes {
                break;
            }
            let next = (at_min + every).min(minutes);
            while t0.elapsed().as_secs_f64() / 60.0 < next {
                std::thread::sleep(Duration::from_secs(5));
            }
        }
        stop_app(&mut app_proc);
        for k in kids.iter_mut() {
            let _ = k.kill();
            let _ = k.wait();
        }
        if let Ok(a) = std::env::var("APPDATA") {
            if let Ok(rd) =
                std::fs::read_dir(std::path::PathBuf::from(a).join("Claude Talk").join("logs"))
            {
                for e in rd.flatten() {
                    let _ = std::fs::copy(e.path(), out.join(e.file_name()));
                }
            }
        }
        if ok {
            0
        } else {
            1
        }
    }

    pub fn smoke(args: &[String]) -> i32 {
        std::panic::set_hook(Box::new(|info| eprintln!("PANIC: {info}")));
        let _com = ComGuard::new();
        let cable_in = arg(args, "--cable-in").unwrap_or("CABLE Input").to_string();
        let cable_out = arg(args, "--cable-out")
            .unwrap_or("CABLE Output")
            .to_string();
        let sink = arg(args, "--sink").unwrap_or("Virtual Audio").to_string();
        let out = arg(args, "--out")
            .unwrap_or("smoke-report.json")
            .to_string();
        let mut report = serde_json::Map::new();
        let mut ok = true;
        let mut check =
            |name: &str, pass: bool, detail: Value, report: &mut serde_json::Map<String, Value>| {
                println!("[{}] {name}: {detail}", if pass { "PASS" } else { "FAIL" });
                report.insert(name.to_string(), json!({"pass": pass, "detail": detail}));
                ok &= pass;
            };
        report.insert(
            "windows_build".into(),
            json!(ct_engine::win::windows_build()),
        );
        report.insert(
            "devices_render".into(),
            json!(devices::list(Flow::Render)
                .unwrap_or_default()
                .iter()
                .map(|e| e.name.clone())
                .collect::<Vec<_>>()),
        );
        report.insert(
            "devices_capture".into(),
            json!(devices::list(Flow::Capture)
                .unwrap_or_default()
                .iter()
                .map(|e| e.name.clone())
                .collect::<Vec<_>>()),
        );

        // A. engine render → virtual cable → engine capture
        {
            let player = play(
                1000.0,
                0.3,
                DeviceSelector::NameContains(Flow::Render, cable_in.clone()),
            );
            std::thread::sleep(Duration::from_millis(500));
            let r = record(
                CaptureTarget::Device(DeviceSelector::NameContains(
                    Flow::Capture,
                    cable_out.clone(),
                )),
                3.0,
                |_| {},
            );
            let pl = player.describe();
            drop(player);
            let l = levels(&r, 2.0, &[1000.0, 3000.0]);
            let pass = r.state == StreamState::Running
                && lv(&l, 1000.0) > -16.0
                && lv(&l, 3000.0) < lv(&l, 1000.0) - 60.0;
            check(
                "render_capture_cable",
                pass,
                json!({"levels_db": l, "device": r.device, "state": format!("{:?}", r.state), "hr": r.hr, "player": pl, "samples": r.samples.len(), "diag": r.diag}),
                &mut report,
            );
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
            check(
                "process_loopback_isolation",
                pass,
                json!({"levels_db": l, "pid": r.pid, "state": format!("{:?}", r.state), "hr": r.hr, "diag": r.diag}),
                &mut report,
            );
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
            let r = record(
                CaptureTarget::Process(ProcessTarget::ExeNames(vec!["ct-fakeapp.exe".into()])),
                1.0,
                move |cap| {
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
                },
            );
            let l = levels(&r, 0.9, &[1500.0]);
            let pass = lv(&l, 1500.0) > -20.0 && r.restarts >= 1;
            check(
                "process_restart_recovery",
                pass,
                json!({"levels_db": l, "restarts": r.restarts, "state": format!("{:?}", r.state), "hr": r.hr, "diag": r.diag}),
                &mut report,
            );
            let child = restarted.lock().unwrap().take();
            if let Some(mut c) = child {
                let _ = c.kill();
                let _ = c.wait();
            }
        }

        // D. (information) the microphone the engine would pick never is our
        // own cable, and whether the CI sink's virtual mic echoes its speaker
        {
            let own: Vec<String> = devices::NOT_A_MICROPHONE
                .iter()
                .map(|s| s.to_string())
                .collect();
            let mic = devices::resolve(&DeviceSelector::DefaultExcluding(Flow::Capture, own))
                .map(|(_, e)| e.name)
                .unwrap_or_default();
            let lower = mic.to_lowercase();
            let pass = !devices::OWN_VIRTUAL_DEVICES
                .iter()
                .any(|p| lower.contains(p));
            check(
                "microphone_never_own_cable",
                pass,
                json!({"picked": mic, "default": devices::default_id(Flow::Capture)}),
                &mut report,
            );
            let player = play(
                2500.0,
                0.3,
                DeviceSelector::NameContains(Flow::Render, sink.clone()),
            );
            std::thread::sleep(Duration::from_millis(500));
            let r = record(
                CaptureTarget::Device(DeviceSelector::NameContains(
                    Flow::Capture,
                    "microphone array (virtual audio".into(),
                )),
                2.0,
                |_| {},
            );
            drop(player);
            report.insert("info_sink_mic_echo".into(), json!({"levels_db": levels(&r, 1.5, &[2500.0]), "device": r.device, "state": format!("{:?}", r.state), "hr": r.hr, "diag": r.diag}));
        }

        // E. latency of the engine itself, without the app: a program's
        // bursts are captured (process loopback), routed through the engine
        // (20 ms route buffer) and played into the cable; recorded before the
        // engine (program), after it (cable playback side, loopback) and
        // after VB-CABLE (cable recording side).
        {
            let dir = std::env::temp_dir().join("ct-lat");
            let _ = std::fs::create_dir_all(&dir);
            let prog = dir.join("burster.exe");
            let _ = std::fs::copy(&exe, &prog);
            let mut child = spawn_burst_process(&prog, 2500.0, &sink, 600);
            std::thread::sleep(Duration::from_millis(800));
            let (fan, mut fan_h) = SourceFanout::new(1, ENGINE_RATE);
            let (mix, mut mix_h) = OutputMixer::new(2, ENGINE_CHANNELS, 480);
            let tap = connect(&mut fan_h, &mut mix_h, GainParams::new(0.0, false), 20.0);
            let cap = spawn_capture(
                "lat-src",
                CaptureTarget::Process(ProcessTarget::ExeNames(vec!["burster.exe".into()])),
                fan,
            );
            let render = spawn_render(
                "lat-out",
                DeviceSelector::FirstMatching(
                    Flow::Render,
                    ct_engine::win::host::cable_render_patterns(),
                ),
                mix,
            );
            std::thread::sleep(Duration::from_secs(2));
            let (_, recs) = record_multi(
                vec![
                    CaptureTarget::Process(ProcessTarget::ExeNames(vec!["burster.exe".into()])),
                    CaptureTarget::DeviceLoopback(DeviceSelector::FirstMatching(
                        Flow::Render,
                        ct_engine::win::host::cable_render_patterns(),
                    )),
                    CaptureTarget::Device(DeviceSelector::FirstMatching(
                        Flow::Capture,
                        ct_engine::win::host::cable_capture_patterns(),
                    )),
                ],
                8.0,
            );
            let ms = |r: &(Vec<f32>, f64)| -> Vec<f64> {
                burst_onsets(&r.0, 2500.0, ENGINE_RATE as f64, 0.3, 2400)
                    .into_iter()
                    .map(|i| i as f64 / 48.0 - r.1)
                    .collect()
            };
            let (a, c, b) = (ms(&recs[0]), ms(&recs[1]), ms(&recs[2]));
            let engine = median(&match_delays(&a, &c, 400.0));
            let cable = median(&match_delays(&c, &b, 400.0));
            let total = median(&match_delays(&a, &b, 400.0));
            let pass = engine.is_some_and(|v| v < 40.0);
            check(
                "latency_engine_pipeline",
                pass,
                json!({"engine_ms": engine, "vbcable_ms": cable, "total_ms": total, "bursts": [a.len(), c.len(), b.len()],
                       "route_target_ms": tap.target_ms.load(), "route_buffered_ms": tap.buffered_ms.load(), "route_underrun": tap.underrun_frames.load(Ordering::Relaxed),
                       "render_queue_frames": render.status.period_frames.load(Ordering::Relaxed), "recorder_buffer_ms": [recs[0].1, recs[1].1, recs[2].1]}),
                &mut report,
            );
            drop(cap);
            drop(render);
            let _ = child.kill();
            let _ = child.wait();
            drop((fan_h, mix_h));
        }

        // F. per-app routing: only Discord records from our cable; the
        // setting survives a Discord restart and can be undone.
        {
            use ct_engine::win::{policy, sessions};
            let dir = std::env::temp_dir().join("ct-route");
            let _ = std::fs::create_dir_all(&dir);
            for n in ["Discord.exe", "chrome.exe"] {
                let _ = std::fs::copy(&exe, dir.join(n));
            }
            let listen = |name: &str, delay: &str| {
                std::process::Command::new(dir.join(name))
                    .args(["listen", "--delay", delay, "--seconds", "15"])
                    .spawn()
                    .expect("start listener")
            };
            let discord = vec!["discord.exe".to_string()];
            let cable = policy::cable_capture_id();
            let where_ = |exe: &str| -> Vec<String> {
                sessions::capture_sessions()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|s| s.active && s.exe.eq_ignore_ascii_case(exe))
                    .map(|s| s.endpoint_name)
                    .collect()
            };
            let stop = |mut c: std::process::Child| {
                let _ = c.kill();
                let _ = c.wait();
            };
            // 0. (information) routing a Discord that has not used audio yet
            let d = listen("Discord.exe", "6");
            std::thread::sleep(Duration::from_millis(800));
            let before_audio = format!("{:?}", policy::route_discord_to_cable(&discord));
            stop(d);
            let _ = policy::restore_discord(&discord);
            report.insert("info_route_before_audio".into(), json!(before_audio));

            // 1. Discord and Chrome are recording; Claude Talk routes Discord
            // the way the app does (retrying every 2 s until Windows accepts)
            let d = listen("Discord.exe", "0");
            let c = listen("chrome.exe", "0");
            std::thread::sleep(Duration::from_millis(1500));
            let mut routed = policy::route_discord_to_cable(&discord);
            let mut tries = 1;
            while !matches!(routed, Ok(n) if n > 0) && tries < 4 {
                std::thread::sleep(Duration::from_secs(2));
                routed = policy::route_discord_to_cable(&discord);
                tries += 1;
            }
            std::thread::sleep(Duration::from_secs(4));
            let d_mic = sessions::discord_mic(&discord, cable.as_deref());
            let d_where = where_("discord.exe");
            let c_where = where_("chrome.exe");
            let flag = policy::discord_routed_to_cable(&discord);
            stop(d);
            stop(c);
            let chrome_ok = !c_where.is_empty()
                && !c_where
                    .iter()
                    .any(|n| n.to_lowercase().contains("claude talk"));
            // a running recording moved by itself, or needs Discord restarted
            // (the app then shows "Genstart Discord for mig")
            let moved_live = d_mic == sessions::DiscordMic::Cable;
            let d = listen("Discord.exe", "0");
            std::thread::sleep(Duration::from_secs(4));
            let after_restart = sessions::discord_mic(&discord, cable.as_deref());
            let after_restart_where = where_("discord.exe");
            stop(d);
            check(
                "per_app_routing",
                matches!(routed, Ok(n) if n > 0)
                    && after_restart == sessions::DiscordMic::Cable
                    && chrome_ok
                    && flag == Some(true),
                json!({"routed": format!("{routed:?}"), "tries": tries, "moved_while_recording": moved_live, "discord_records_from": d_where,
                       "after_discord_restart": format!("{after_restart:?}"), "after_restart_records_from": after_restart_where,
                       "chrome_records_from": c_where, "stored": flag}),
                &mut report,
            );
            // 2. Windows remembers it for the program: a new Discord process is routed too
            let d = listen("Discord.exe", "0");
            std::thread::sleep(Duration::from_secs(4));
            let again = sessions::discord_mic(&discord, cable.as_deref());
            let again_where = where_("discord.exe");
            // 3. "Restore normal sound settings" gives Discord its normal microphone back
            let restored_n = policy::restore_discord(&discord);
            stop(d);
            let d = listen("Discord.exe", "0");
            std::thread::sleep(Duration::from_secs(4));
            let after = sessions::discord_mic(&discord, cable.as_deref());
            let after_where = where_("discord.exe");
            stop(d);
            // 4. and also after Discord has been closed (registry entries)
            let d = listen("Discord.exe", "0");
            std::thread::sleep(Duration::from_millis(800));
            let _ = policy::route_discord_to_cable(&discord);
            stop(d);
            // where Windows keeps the setting (diagnostics for the offline restore)
            if let Some(id) = &cable {
                let guid = id
                    .rsplit('.')
                    .next()
                    .unwrap_or(id)
                    .trim_matches(|c| c == '{' || c == '}')
                    .to_string();
                let found = powershell(&format!(
                    "reg query HKCU /s /f '{guid}' 2>&1 | Select-Object -First 40 | Out-String"
                ));
                let found_lm = powershell(&format!("reg query HKLM\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\MMDevices /s /f '{guid}' /d 2>&1 | Select-Object -First 20 | Out-String"));
                report.insert(
                    "info_policy_store".into(),
                    json!({"hkcu": found, "hklm_mmdevices": found_lm}),
                );
            }
            let removed_offline = policy::restore_discord(&discord);
            let d = listen("Discord.exe", "0");
            std::thread::sleep(Duration::from_secs(4));
            let offline_after = sessions::discord_mic(&discord, cable.as_deref());
            stop(d);
            check(
                "per_app_restore",
                // (offline: Windows' audio service keeps the setting in
                // memory; Claude Talk restores it when Discord next runs –
                // reported, not required, see KNOWN_ISSUES K12)
                again == sessions::DiscordMic::Cable && after == sessions::DiscordMic::Other,
                json!({"new_process": format!("{again:?}"), "new_process_records_from": again_where, "restored_entries": restored_n,
                       "after_restore": format!("{after:?}"), "after_restore_records_from": after_where,
                       "removed_while_closed": removed_offline, "after_offline_restore": format!("{offline_after:?}")}),
                &mut report,
            );
        }

        report.insert("pass".into(), json!(ok));
        let _ = std::fs::write(
            &out,
            serde_json::to_string_pretty(&Value::Object(report)).unwrap_or_default(),
        );
        if ok {
            0
        } else {
            1
        }
    }
}
