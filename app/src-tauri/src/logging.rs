//! Log file in `%APPDATA%\Claude Talk\logs` (one file per day, 7 kept).
//! Audio threads never log; they publish counters that the control thread
//! turns into log lines.
use std::io::Write;
use std::sync::Mutex;

struct FileLogger {
    file: Mutex<Option<std::fs::File>>,
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
        if let Ok(mut f) = self.file.lock() {
            if let Some(f) = f.as_mut() {
                let _ = f.write_all(line.as_bytes());
            }
        }
        if cfg!(debug_assertions) {
            eprint!("{line}");
        }
    }
    fn flush(&self) {
        if let Ok(mut f) = self.file.lock() {
            if let Some(f) = f.as_mut() {
                let _ = f.flush();
            }
        }
    }
}

pub fn init() {
    let dir = crate::paths::logs_dir();
    let _ = std::fs::create_dir_all(&dir);
    let day = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86400)
        .unwrap_or(0);
    let path = dir.join(format!("claude-talk-{day}.log"));
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok();
    // keep the 7 newest files
    if let Ok(rd) = std::fs::read_dir(&dir) {
        let mut logs: Vec<_> = rd
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("claude-talk-"))
            .collect();
        logs.sort_by_key(|e| e.file_name());
        while logs.len() > 7 {
            let _ = std::fs::remove_file(logs.remove(0).path());
        }
    }
    let logger = Box::leak(Box::new(FileLogger {
        file: Mutex::new(file),
    }));
    let _ = log::set_logger(logger);
    log::set_max_level(log::LevelFilter::Debug);
    std::panic::set_hook(Box::new(|info| {
        log::error!("panic: {info}");
        log::logger().flush();
    }));
    log::info!("Claude Talk {} starting", env!("CARGO_PKG_VERSION"));
}
