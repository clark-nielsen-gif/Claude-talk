//! A source that generates its own audio in real time (self-test tones,
//! CI fake programs). Runs on its own thread, paced by the system clock.

use crate::mixer::SourceFanout;
use crate::{ENGINE_CHANNELS, ENGINE_RATE};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub struct Generator {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Generator {
    /// `gen` fills interleaved stereo 48 kHz buffers.
    pub fn spawn(
        mut fanout: SourceFanout,
        mut gen: impl FnMut(&mut [f32]) + Send + 'static,
    ) -> Generator {
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        let thread = std::thread::Builder::new()
            .name("generator".into())
            .spawn(move || {
                let start = Instant::now();
                let mut sent: u64 = 0;
                let mut buf = vec![0f32; 4800 * ENGINE_CHANNELS];
                while !s2.load(Ordering::Relaxed) {
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
            .expect("spawn generator");
        Generator {
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for Generator {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
