//! Where Claude Talk keeps its files: `%APPDATA%\Claude Talk`.
use std::path::PathBuf;

pub fn app_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("Claude Talk")
}
pub fn logs_dir() -> PathBuf {
    app_dir().join("logs")
}
pub fn config_file() -> PathBuf {
    app_dir().join("config.json")
}
/// WebView2 user-data folder of the Claude window (its own login session).
/// In `%LOCALAPPDATA%`: browser data must not travel with roaming profiles.
pub fn claude_window_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(app_dir)
        .join("Claude Talk")
        .join("ClaudeWindow")
}
