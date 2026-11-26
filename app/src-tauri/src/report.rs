//! "Save error report": a zip with logs, settings and a status snapshot, on
//! the Desktop. No passwords, cookies or Claude conversations are included
//! (the Claude window's own data folder is never read).
use std::io::Write;
use tauri::AppHandle;

pub fn save(_app: &AppHandle) -> Result<String, Box<dyn std::error::Error>> {
    let desktop = std::env::var_os("USERPROFILE").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Desktop");
    let dir = if desktop.is_dir() { desktop } else { std::env::temp_dir() };
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs();
    let path = dir.join(format!("Claude Talk fejlrapport {stamp}.zip"));
    let file = std::fs::File::create(&path)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    log::logger().flush();
    if let Ok(rd) = std::fs::read_dir(crate::paths::logs_dir()) {
        for e in rd.flatten() {
            if let Ok(bytes) = std::fs::read(e.path()) {
                zip.start_file(format!("logs/{}", e.file_name().to_string_lossy()), opts)?;
                zip.write_all(&bytes)?;
            }
        }
    }
    if let Ok(bytes) = std::fs::read(crate::paths::config_file()) {
        zip.start_file("config.json", opts)?;
        zip.write_all(&bytes)?;
    }
    zip.start_file("system.txt", opts)?;
    let info = crate::commands::app_info();
    writeln!(zip, "Claude Talk {}\nWindows build {}\nsupported: {}\ncable installed: {}", info.version, info.windows_build, info.supported, info.cable_installed)?;
    #[cfg(windows)]
    {
        let _com = ct_engine::win::ComGuard::new();
        for flow in [ct_engine::win::devices::Flow::Render, ct_engine::win::devices::Flow::Capture] {
            for e in ct_engine::win::devices::list(flow).unwrap_or_default() {
                writeln!(zip, "{:?}: {} ({:?})", e.flow, e.name, e.form_factor)?;
            }
        }
    }
    zip.finish()?;
    Ok(path.to_string_lossy().into_owned())
}
