//! Log file in `%LOCALAPPDATA%\Claude Talk\logs`: a new file each day (also
//! while the program keeps running), 7 kept; past 8 MB in a day only
//! information and warnings are written.
//! Audio threads never log; they publish counters that the control thread
//! turns into log lines.
use std::io::Write;
use std::sync::Mutex;

struct FileLogger {
    dir: std::path::PathBuf,
    file: Mutex<LogFile>,
}

struct LogFile {
    day: u64,
    written: u64,
    file: Option<std::fs::File>,
}

const DEBUG_LIMIT: u64 = 8 * 1024 * 1024;

fn today() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86400)
        .unwrap_or(0)
}

/// Opens (appends to) the file for `day` and keeps the 7 newest files.
fn open_day(dir: &std::path::Path, day: u64) -> (Option<std::fs::File>, u64) {
    let path = dir.join(format!("claude-talk-{day}.log"));
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok();
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    if let Ok(rd) = std::fs::read_dir(dir) {
        let mut logs: Vec<_> = rd
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("claude-talk-"))
            .collect();
        logs.sort_by_key(|e| e.file_name());
        while logs.len() > 7 {
            let _ = std::fs::remove_file(logs.remove(0).path());
        }
    }
    (file, size)
}

impl log::Log for FileLogger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info || m.target().starts_with("claude_talk")
    }
    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let line = format!(
            "{}.{:03} {:5} {}: {}\n",
            now.as_secs(),
            now.subsec_millis(),
            r.level(),
            r.target(),
            r.args()
        );
        if let Ok(mut lf) = self.file.lock() {
            let day = now.as_secs() / 86400;
            if day != lf.day {
                let (file, written) = open_day(&self.dir, day);
                *lf = LogFile { day, written, file };
            }
            if lf.written < DEBUG_LIMIT || r.level() <= log::Level::Info {
                lf.written += line.len() as u64;
                if let Some(f) = lf.file.as_mut() {
                    let _ = f.write_all(line.as_bytes());
                }
            }
        }
        if cfg!(debug_assertions) {
            eprint!("{line}");
        }
    }
    fn flush(&self) {
        if let Ok(mut lf) = self.file.lock() {
            if let Some(f) = lf.file.as_mut() {
                let _ = f.flush();
            }
        }
    }
}

pub fn init() {
    init_in(crate::paths::logs_dir());
}

/// Logs into `dir` (the elevated installer modes use an admin-only folder).
pub fn init_in(dir: std::path::PathBuf) {
    let _ = std::fs::create_dir_all(&dir);
    let day = today();
    let (file, written) = open_day(&dir, day);
    let logger = Box::leak(Box::new(FileLogger {
        dir,
        file: Mutex::new(LogFile { day, written, file }),
    }));
    let _ = log::set_logger(logger);
    log::set_max_level(log::LevelFilter::Debug);
    std::panic::set_hook(Box::new(|info| {
        log::error!("panic: {info}");
        log::logger().flush();
        // Only the main thread's panic ends the program (the audio and
        // helper threads are caught and restarted). If it goes down,
        // Discord must not be left on a silent cable.
        #[cfg(windows)]
        if std::thread::current().name() == Some("main") {
            crate::discord::give_up_after_panic();
        }
    }));
    log::info!("Claude Talk {} starting", env!("CARGO_PKG_VERSION"));
}
