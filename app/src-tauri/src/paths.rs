//! Where Claude Talk keeps its files: `%APPDATA%\Claude Talk`.
use std::path::PathBuf;

pub fn app_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("Claude Talk")
}
/// In `%LOCALAPPDATA%`: logs must not travel with roaming profiles.
pub fn logs_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(|p| PathBuf::from(p).join("Claude Talk"))
        .unwrap_or_else(app_dir)
        .join("logs")
}
/// What Claude Talk itself last gave Discord as its microphone (D28): read
/// back later, it is not the user's own setting. Kept apart from the
/// settings, which the running app may save over at any time.
pub fn given_back_file() -> PathBuf {
    app_dir().join("discord-given-back.txt")
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
